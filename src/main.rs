/*
This is my simple rust discord bot for now, i might split into more files as i progress on writing it.
For now it's a dice roller that understands notation like d20, 2d6, or 2d20+4, defaulting to a single d6.
*/

mod db; // Shared SQL database for all modules to use
mod random {
    pub mod dice; // Roll Dice
}
mod league {
    pub mod playtime; // Track playtime in LoL using Discord Presence updates.
}

use std::sync::Arc;

use colored::*;
use poise::serenity_prelude as serenity;
use sqlx;
use sqlx::SqlitePool;

struct Data {
    playtime_tracker: Arc<league::playtime::PlaytimeTracker>,
    pool: SqlitePool,
}

// A catch-all error type.
type Error = Box<dyn std::error::Error + Send + Sync>;
// The context passed to all command functions.
type Context<'a> = poise::Context<'a, Data, Error>;

#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenvy::dotenv().ok();
    let token = std::env::var("DISCORD_TOKEN").expect("Set the DISCORD_TOKEN environment variable");

    let intents =
        serenity::GatewayIntents::non_privileged() | serenity::GatewayIntents::GUILD_PRESENCES;

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                random::dice::roll(),
                league::playtime::playtime(),
                league::playtime::playtimeauto(),
            ],
            event_handler: |ctx, event, framework, data| {
                Box::pin(handle_event(ctx, event, framework, data))
            },
            ..Default::default()
        })
        .setup(|ctx, ready, framework| {
            Box::pin(async move {
                println!("Logged in as {}", ready.user.name);
                // Registers /roll and /playtime with Discord. commands with discord
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;

                let pool = db::init_pool().await?;
                let playtime_tracker = Arc::new(league::playtime::PlaytimeTracker::new());

                // starts the background task
                tokio::spawn(league::playtime::run_auto_leaderboard_loop(
                    ctx.http.clone(),
                    pool.clone(),
                ));

                Ok(Data {
                    playtime_tracker,
                    pool,
                })
            })
        })
        .build();

    let mut client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .await?;

    tokio::select! {
        res = client.start() => {
            res?;
        }
        res = tokio::signal::ctrl_c() => {
            res?;
            println!("{}", "Shutting down...".red());
        }
    }

    Ok(())
}

async fn handle_event(
    _ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    data: &Data,
) -> Result<(), Error> {
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
