/*
This is my simple rust discord bot for now, i might split into more files as i progress on writing it.
For now it's a dice roller that understands notation like d20, 2d6, or 2d20+4, defaulting to a single d6.
*/

mod dice; // Roll Dice
mod playtime; // Track playtime in LoL

use colored::*;
use poise::serenity_prelude as serenity;

struct Data {
    playtime_tracker: playtime::PlaytimeTracker,
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
            commands: vec![dice::roll(), playtime::playtime()],
            event_handler: |ctx, event, framework, data| {
                Box::pin(handle_event(ctx, event, framework, data))
            },
            ..Default::default()
        })
        .setup(|ctx, ready, framework| {
            Box::pin(async move {
                println!("Logged in as {}", ready.user.name);
                // Registers /roll and /playtime with Discord.
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data {
                    playtime_tracker: playtime::PlaytimeTracker::new(),
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
        let activity_names: Vec<String> = new_data
            .activities
            .iter()
            .map(|activity| format!("{:?}: {}", activity.kind, activity.name))
            .collect();

        println!(
            "Presence update for user {}: [{}]",
            new_data.user.id.get(),
            activity_names.join(", ")
        );

        data.playtime_tracker
            .handle_presence_update(new_data.user.id.get(), &new_data.activities);
    }

    Ok(())
}
