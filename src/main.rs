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
    pub mod events;
    pub mod playtime; // Track playtime in LoL using Discord Presence updates.
    pub mod status; // Periodically checks the discords presence and shows a condensed top 3 playtime summary in the status. // League commands
    pub mod tasks; // Background tasks for the league module // Handle Discord events for the league module
}

mod music_player {
    pub mod commands;
    pub mod player; // Music player using songbird and yt-dlp // Music player commands
    pub mod playlist; // Handle Discord events for the music_player module
}

mod bib_sanitizer {
    pub mod events;
    pub mod metadata;
    pub mod sanitizer; // Handle Discord events for the bib_sanitizer module
}

use std::sync::Arc;

use colored::*;
use poise::serenity_prelude as serenity;
use songbird::SerenityInit;
use sqlx::SqlitePool;

struct Data {
    playtime_tracker: Arc<league::playtime::PlaytimeTracker>,
    pool: SqlitePool,
    // dependencies for Music player
    songbird: Arc<songbird::Songbird>,
    http_client: reqwest::Client,
    // Waifu tag cache
    //tag_cache: Arc<waifu::nekos::TagCache>,
}

// A catch-all error type.
type Error = Box<dyn std::error::Error + Send + Sync>;
// The context passed to all command functions.
type Context<'a> = poise::Context<'a, Data, Error>;

// =====================
// ! Helper Functions
// =====================

fn build_commands() -> Vec<poise::Command<Data, Error>> {
    let mut commands = Vec::new();

    commands.extend(music_player::commands::all());
    commands.extend(league::commands::all());
    commands.extend(random::commands::all());
    //commands.extend(waifu::commands::all());

    commands
}

fn get_discord_token() -> String {
    std::env::var("DISCORD_TOKEN").expect("Set the DISCORD_TOKEN environment variable")
}

fn init_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
}

fn gateway_intents() -> serenity::GatewayIntents {
    serenity::GatewayIntents::non_privileged()
        | serenity::GatewayIntents::GUILD_PRESENCES
        | serenity::GatewayIntents::MESSAGE_CONTENT
        | serenity::GatewayIntents::GUILD_VOICE_STATES
}

async fn initialize_data(
    ctx: &serenity::Context,
    songbird: Arc<songbird::Songbird>,
) -> Result<Data, Error> {
    let pool = db::init_pool().await?;

    let playtime_tracker = Arc::new(league::playtime::PlaytimeTracker::new());

    let http_client = reqwest::Client::builder()
        .user_agent(concat!(
            "Discomatic/",
            env!("CARGO_PKG_VERSION"),
            " Discord bot"
        ))
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(20))
        .build()?;

    //let tag_cache = Arc::new(waifu::nekos::TagCache::new());

    league::tasks::start(ctx, pool.clone());

    //waifu::tasks::start_tag_cache_refresh(http_client.clone(), tag_cache.clone());

    Ok(Data {
        playtime_tracker,
        pool,
        songbird,
        http_client,
        //tag_cache,
    })
}

/*  This builds the Poise framework.
    It configures:
    - The commands
    - Discord event handling
    - shared bot data created when discord connects
*/
fn build_framework(songbird: Arc<songbird::Songbird>) -> poise::Framework<Data, Error> {
    poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: build_commands(),

            // Forwad every event to our central event dispatcher.
            event_handler: |ctx, event, framework, data| {
                Box::pin(handle_event(ctx, event, framework, data))
            },

            ..Default::default()
        })
        // Setup function that runs when the bot connects to Discord.
        .setup(move |ctx, ready, framework| {
            // Clone songbird for use in the async block
            let songbird = songbird.clone();

            Box::pin(async move {
                println!("Logged in as {}", ready.user.name);

                poise::builtins::register_globally(ctx, &framework.options().commands).await?;

                // initialize DB, trackers, HTTP client(s), songbird and background tasks.
                initialize_data(ctx, songbird).await
            })
        })
        .build()
}

async fn build_client(
    token: String,
    framework: poise::Framework<Data, Error>,
    songbird: Arc<songbird::Songbird>,
) -> Result<serenity::Client, Error> {
    let client = serenity::ClientBuilder::new(token, gateway_intents())
        .framework(framework)
        .register_songbird_with(songbird)
        .await?;

    Ok(client)
}

async fn run_client(mut client: serenity::Client) -> Result<(), Error> {
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

// =====================
// ! Main Function
// =====================
#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenvy::dotenv().ok();
    init_logging();
    let token = get_discord_token();

    let songbird = songbird::Songbird::serenity(); // Initialize songbird for voice support

    let framework = build_framework(songbird.clone());

    let client = build_client(token, framework, songbird).await?;

    run_client(client).await
}

async fn handle_event(
    ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    data: &Data,
) -> Result<(), Error> {
    bib_sanitizer::events::handle(ctx, event).await?;
    league::events::handle(event, data).await?;

    Ok(())
}
