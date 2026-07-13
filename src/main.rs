/*
main.rs
This is my simple rust discord bot for now, i might split into more files as i progress on writing it.
For now it's a dice roller that understands notation like d20, 2d6, or 2d20+4, defaulting to a single d6.
*/

mod db; // Shared SQL database for all modules to use
mod random {
    pub mod commands;
    pub mod dice; // Roll Dice // Random commands
}
mod league {
    pub mod commands;
    pub mod playtime; // Track playtime in LoL using Discord Presence updates.
    pub mod status; // Periodically checks the discords presence and shows a condensed top 3 playtime summary in the status. // League commands
}

mod music_player {
    pub mod commands;
    pub mod player; // Music player using songbird and yt-dlp // Music player commands
}

// This path thing seems to have fixed my IDE issue of graying shit out but sadly hints from rust analyzer is gone so kinda shit.
#[path = "bib_sanitizer/metadata.rs"]
mod metadata;
#[path = "bib_sanitizer/sanitizer.rs"]
mod sanitizer;

use std::sync::Arc;

use colored::*;
use poise::serenity_prelude as serenity;
use songbird::SerenityInit;
use sqlx;
use sqlx::SqlitePool;

struct Data {
    playtime_tracker: Arc<league::playtime::PlaytimeTracker>,
    pool: SqlitePool,
    // dependencies for Music player
    songbird: Arc<songbird::Songbird>,
    http_client: reqwest::Client,
}

// A catch-all error type.
type Error = Box<dyn std::error::Error + Send + Sync>;
// The context passed to all command functions.
type Context<'a> = poise::Context<'a, Data, Error>;

// helper function that takes all functions and makes them available
fn build_commands() -> Vec<poise::Command<Data, Error>> {
    let mut commands = Vec::new();

    commands.extend(music_player::commands::all());
    commands.extend(league::commands::all());
    commands.extend(random::commands::all());

    commands
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenvy::dotenv().ok();
    let token = std::env::var("DISCORD_TOKEN").expect("Set the DISCORD_TOKEN environment variable");

    // ! LOGGING TEST
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    // ! LOGGING TEST END

    let songbird = songbird::Songbird::serenity(); // Initialize songbird for voice support
    let framework_songbird = songbird.clone();

    let intents = serenity::GatewayIntents::non_privileged()
        | serenity::GatewayIntents::GUILD_PRESENCES
        | serenity::GatewayIntents::MESSAGE_CONTENT
        | serenity::GatewayIntents::GUILD_VOICE_STATES;

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: build_commands(),

            event_handler: |ctx, event, framework, data| {
                Box::pin(handle_event(ctx, event, framework, data))
            },

            ..Default::default()
        })
        .setup(move |ctx, ready, framework| {
            let songbird = framework_songbird.clone();

            Box::pin(async move {
                println!("Logged in as {}", ready.user.name);
                // Registers slash commands with Discord.
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;

                let pool = db::init_pool().await?;
                let playtime_tracker = Arc::new(league::playtime::PlaytimeTracker::new());

                // background task for the automatic playtime leaderboard updates.
                tokio::spawn(league::playtime::run_auto_leaderboard_loop(
                    ctx.http.clone(),
                    pool.clone(),
                ));

                // starts the background task that updates the bot status.
                tokio::spawn(league::status::run_status_update_loop(
                    ctx.clone(),
                    pool.clone(),
                ));

                Ok(Data {
                    playtime_tracker,
                    pool,
                    songbird,
                    http_client: reqwest::Client::new(),
                })
            })
        })
        .build();

    let mut client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .register_songbird_with(songbird)
        .await?;

    tokio::select! {
        result = client.start() => {
            result?;
        }
        result = tokio::signal::ctrl_c() => {
            result?;
            println!("{}", "Shutting down...".red());
        }
    }

    Ok(())
}

async fn handle_event(
    ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    data: &Data,
) -> Result<(), Error> {
    // Handle newly created Discord messages.
    if let serenity::FullEvent::Message { new_message } = event {
        sanitizer::handle_message(ctx, new_message).await?;
        metadata::handle_message(ctx, new_message).await?;
    }

    // Handle League presence updates.
    if let serenity::FullEvent::PresenceUpdate { new_data } = event {
        let elapsed = data
            .playtime_tracker
            .handle_presence_update(new_data.user.id.get(), &new_data.activities);

        if let Some(seconds) = elapsed {
            if let Err(error) = sqlx::query(
                "INSERT INTO playtime_totals (user_id, total_seconds) VALUES (?1, ?2)
                 ON CONFLICT(user_id) DO UPDATE SET total_seconds = total_seconds + ?2",
            )
            .bind(new_data.user.id.get() as i64)
            .bind(seconds as i64)
            .execute(&data.pool)
            .await
            {
                eprintln!("Failed to record playtime: {}", error);
            }
        }
    }

    Ok(())
}
