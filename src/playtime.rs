/*
This is my attempt at making a simple tracker that will make Alexander have most hours in league for the server.
It was largely made using
https://github.com/Gummiees/playtime-discord-bot
as a reference
*/

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use poise::serenity_prelude::{self as serenity};
use sqlx::SqlitePool;

use crate::{Context, Error};

const TRACKED_GAME: &str = "League of Legends";

const AUTO_CHECK_INTERVAL: Duration = Duration::from_secs(5 * 60);

// Configuration for the server's leaderboard: channel location, message ID, refresh interval, and enabled status.
struct AutoLeaderboardConfig {
    channel_id: u64,
    message_id: u64,
    interval_seconds: u64,
    enabled: bool,
    next_due: u64,
}

pub struct PlaytimeTracker {
    // when each users session is started
    active_sessions: Mutex<HashMap<u64, Instant>>, // user_id -> session_start_time
}

// This struct will be shared across the bot, so it needs to be thread-safe (hence the Mutex).
impl PlaytimeTracker {
    pub fn new() -> Self {
        Self {
            active_sessions: Mutex::new(HashMap::new()),
        }
    }

    // Handles presence updates to track session start/end, returns elapsed seconds when session ends.
    pub fn handle_presence_update(
        &self,
        user_id: u64,
        activities: &[serenity::Activity],
    ) -> Option<u64> {
        let is_playing_league = activities.iter().any(|activity| {
            activity.kind == serenity::ActivityType::Playing && activity.name == TRACKED_GAME
        });

        // We need to lock the active_sessions mutex to check and update session states.
        let mut sessions = self.active_sessions.lock().unwrap();

        if is_playing_league {
            sessions.entry(user_id).or_insert_with(Instant::now);
            None
        } else if let Some(start_time) = sessions.remove(&user_id) {
            let elapsed = start_time.elapsed().as_secs();
            Some(elapsed)
        } else {
            None
        }
    }
}

// Persists the playtime data from LoL if it's a first time player it just adds a new row for them.
pub async fn record_completed_session(
    pool: &SqlitePool,
    user_id: u64,
    seconds: u64,
) -> Result<(), Error> {
    let user_id = user_id as i64;
    let seconds = seconds as i64;

    sqlx::query(
        "INSERT INTO playtime_totals (user_id, total_seconds) VALUES (?1, ?2)
         ON CONFLICT(user_id) DO UPDATE SET total_seconds = total_seconds + ?2",
    )
    .bind(user_id)
    .bind(seconds)
    .execute(pool)
    .await?;

    Ok(())
}

// Returns 10 (limit) users with the most played sorted with ORDER
async fn get_players(pool: &SqlitePool, limit: i64) -> Result<Vec<(u64, u64)>, Error> {
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT user_id, total_seconds FROM playtime_totals ORDER BY total_seconds DESC LIMIT ?1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(user_id, seconds)| (user_id as u64, seconds as u64))
        .collect())
}

// Formats a number of seconds as something more readable like "3h 27m"
fn format_duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    format!("{hours}h {minutes}m")
}

// returns the current time as a unix timestamp in seconds
fn current_unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/*
This function builds the leaderboard message from the list of users and their playtime.
It then just apirs playtime and fetching so autoleaderboads show exactly the same formatting.
*/

async fn build_leaderboard_content(
    http: &serenity::Http,
    top_players: &[(u64, u64)],
) -> Result<String, Error> {
    if top_players.is_empty() {
        return Ok("No playtime data available yet.".to_string());
    }

    let mut lines = Vec::new();

    for (rank, &(user_id, seconds)) in top_players.iter().enumerate() {
        let user = serenity::UserId::new(user_id).to_user(http).await?;

        lines.push(format!(
            "**{}.** {} — {}",
            rank + 1,
            user.name,
            format_duration(seconds)
        ));
    }

    Ok(format!(
        "**Top 10 most degenerate LoL players so far:**\n{}",
        lines.join("\n")
    ))
}

// Show the Top 10 users
#[poise::command(slash_command)]
pub async fn playtime(ctx: Context<'_>) -> Result<(), Error> {
    let pool = &ctx.data().pool;
    let top_players = get_players(pool, 10).await?;

    if top_players.is_empty() {
        ctx.say("No playtime data available yet.").await?;
        return Ok(());
    }

    let content = build_leaderboard_content(ctx.http(), &top_players).await?;

    let channel_id = ctx.channel_id();
    let channel_id_u64 = channel_id.get();

    // read the old message from database so we can delete it after posting the new one.
    let old_message_id: Option<i64> =
        sqlx::query_scalar("SELECT message_id FROM leaderboard_messages WHERE channel_id = ?1")
            .bind(channel_id_u64 as i64)
            .fetch_optional(pool)
            .await?;

    // post new leaderboard
    let reply = ctx.say(content).await?;
    let new_message = reply.message().await?;

    // now delete the old one
    if let Some(old_message_id) = old_message_id {
        let old_message_id = serenity::MessageId::new(old_message_id as u64);
        if let Err(error) = channel_id.delete_message(ctx.http(), old_message_id).await {
            eprintln!("Failed to delete old leaderboard message: {}", error);
        }
    }

    // Upsert the new leaderboard message ID for this channel.
    sqlx::query(
        "INSERT INTO leaderboard_messages (channel_id, message_id) VALUES (?1, ?2)
         ON CONFLICT(channel_id) DO UPDATE SET message_id = ?2",
    )
    .bind(channel_id_u64 as i64)
    .bind(new_message.id.get() as i64)
    .execute(pool)
    .await?;

    Ok(())
}

// A select list for intervals for playtimeauto
#[derive(Debug, Clone, Copy, poise::ChoiceParameter)]
pub enum Interval {
    #[name = "1 day"]
    OneDay,
    #[name = "7 days"]
    SevenDays,
    #[name = "30 days"]
    ThirtyDays,
}

impl Interval {
    fn as_seconds(&self) -> u64 {
        match self {
            Interval::OneDay => 24 * 3600,
            Interval::SevenDays => 7 * 24 * 3600,
            Interval::ThirtyDays => 30 * 24 * 3600,
        }
    }
}

// Retrives the auto-leaderboard configuration for a guild, if it exists, from the database.
async fn get_auto_leaderboard(
    pool: &SqlitePool,
    guild_id: u64,
) -> Result<Option<AutoLeaderboardConfig>, Error> {
    let row: Option<(i64, i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT channel_id, message_id, interval_seconds, enabled, next_due
         FROM auto_leaderboards WHERE guild_id = ?1",
    )
    .bind(guild_id as i64)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(
        |(channel_id, message_id, interval_seconds, enabled, next_due)| AutoLeaderboardConfig {
            channel_id: channel_id as u64,
            message_id: message_id as u64,
            interval_seconds: interval_seconds as u64,
            enabled: enabled != 0,
            next_due: next_due as u64,
        },
    ))
}

// Saves the auto-leaderboard configuration for a guild to the database, inserting a new row or updating the existing one.
async fn upsert_auto_leaderboard(
    pool: &SqlitePool,
    guild_id: u64,
    config: &AutoLeaderboardConfig,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO auto_leaderboards (guild_id, channel_id, message_id, interval_seconds, enabled, next_due)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(guild_id) DO UPDATE SET
            channel_id = ?2,
            message_id = ?3,
            interval_seconds = ?4,
            enabled = ?5,
            next_due = ?6",
    )
    .bind(guild_id as i64)
    .bind(config.channel_id as i64)
    .bind(config.message_id as i64)
    .bind(config.interval_seconds as i64)
    .bind(config.enabled as i64)
    .bind(config.next_due as i64)
    .execute(pool)
    .await?;

    Ok(())
}

// Updates the message_id for a guild's auto-leaderboard configuration in the database.
async fn update_auto_leaderboard_message_id(
    pool: &SqlitePool,
    guild_id: u64,
    message_id: u64,
) -> Result<(), Error> {
    sqlx::query("UPDATE auto_leaderboards SET message_id = ?1 WHERE guild_id = ?2")
        .bind(message_id as i64)
        .bind(guild_id as i64)
        .execute(pool)
        .await?;

    Ok(())
}

// This auto leaderboard message will be posted in a channel and then updated every X time, where X is the interval the user selects when setting it up.
// it also remembers channel IDs for next time
async fn refresh_auto_leaderboard(
    http: &serenity::Http,
    pool: &SqlitePool,
    guild_id: u64,
) -> Result<(), Error> {
    let top_players = get_players(pool, 10).await?;
    let content = build_leaderboard_content(http, &top_players).await?;

    let Some(config) = get_auto_leaderboard(pool, guild_id).await? else {
        return Ok(());
    };

    let channel_id = serenity::ChannelId::new(config.channel_id);
    let message_id = serenity::MessageId::new(config.message_id);

    // Try editing the existing message first, keeping its content fresh.
    let edit = serenity::EditMessage::new().content(content.clone());

    match channel_id.edit_message(http, message_id, edit).await {
        Ok(_) => {}
        Err(_) => {
            // The old message no longer exists (probably deleted manually).
            // Post a new one and remember its ID for next time.
            let new_message = channel_id
                .send_message(http, serenity::CreateMessage::new().content(content))
                .await?;

            update_auto_leaderboard_message_id(pool, guild_id, new_message.id.get()).await?;
        }
    }

    Ok(())
}

// Now we make the command that does it all
// Running in a new channel moves the leaderboard deleting of course still
// running again in same channel just updates
#[poise::command(slash_command, guild_only)]
pub async fn playtimeauto(
    ctx: Context<'_>,

    #[description = "How often the leaderboard should refresh"] interval: Interval,
    #[description = "Whether automatic refreshing is turned on"] enabled: bool,
) -> Result<(), Error> {
    let pool = &ctx.data().pool;

    let guild_id = ctx
        .guild_id()
        .expect("This command should only be used in a guild")
        .get();
    let channel_id = ctx.channel_id();
    let channel_id_u64 = channel_id.get();

    let top_players = get_players(pool, 10).await?;
    let content = build_leaderboard_content(ctx.http(), &top_players).await?;

    // We read the existing config first rather than holding a lock across all calls below.
    let existing_config = get_auto_leaderboard(pool, guild_id).await?;

    let moving_channels = existing_config
        .as_ref()
        .map(|config| config.channel_id != channel_id_u64)
        .unwrap_or(true);

    let next_due = if enabled {
        current_unix_time() + interval.as_seconds()
    } else {
        0
    };

    if moving_channels {
        // remove old message
        if let Some(old_config) = &existing_config {
            let old_channel_id = serenity::ChannelId::new(old_config.channel_id);
            let old_message_id = serenity::MessageId::new(old_config.message_id);

            if let Err(error) = old_channel_id
                .delete_message(ctx.http(), old_message_id)
                .await
            {
                eprintln!("Failed to delete old auto leaderboard message: {}", error);
            }
        }

        // now post new message
        let reply = ctx.say(content).await?;
        let new_message = reply.message().await?;

        upsert_auto_leaderboard(
            pool,
            guild_id,
            &AutoLeaderboardConfig {
                channel_id: channel_id_u64,
                message_id: new_message.id.get(),
                interval_seconds: interval.as_seconds(),
                enabled,
                next_due,
            },
        )
        .await?;
    } else {
        // same channel, update settings and refresh message
        let mut config = existing_config.expect("moving_channels is false, so a config exists");
        config.interval_seconds = interval.as_seconds();
        config.enabled = enabled;
        config.next_due = next_due;

        let message_id = serenity::MessageId::new(config.message_id);
        let edit = serenity::EditMessage::new().content(content);

        if let Err(error) = channel_id.edit_message(ctx.http(), message_id, edit).await {
            eprintln!("Failed to edit auto leaderboard message: {}", error);
        }

        upsert_auto_leaderboard(pool, guild_id, &config).await?;

        // Acknowledge the command
        ctx.send(
            poise::CreateReply::default()
                .content("Auto leaderboard settings updated!")
                .ephemeral(true),
        )
        .await?;
    }

    Ok(())
}

// Loop leaderbaord in the background periodically.
// update if refresh time
pub async fn run_auto_leaderboard_loop(http: Arc<serenity::Http>, pool: SqlitePool) {
    loop {
        tokio::time::sleep(AUTO_CHECK_INTERVAL).await;

        // This queries the table directly for the enabled rows that are due.
        let now = current_unix_time();

        let due_guilds: Vec<(u64, u64)> = match sqlx::query_as::<_, (i64, i64)>(
            "SELECT guild_id, interval_seconds FROM auto_leaderboards WHERE enabled = 1 AND next_due <= ?1",
        )
        .bind(now as i64)
        .fetch_all(&pool)
        .await
        {
            Ok(rows) => rows
                .into_iter()
                .map(|(guild_id, interval_seconds)| (guild_id as u64, interval_seconds as u64))
                .collect(),
            Err(error) => {
                eprintln!("Failed to check due auto leaderboards: {}", error);
                Vec::new()
            }
        };

        for (guild_id, interval_seconds) in due_guilds {
            if let Err(error) = refresh_auto_leaderboard(&http, &pool, guild_id).await {
                eprintln!(
                    "Failed to refresh auto leaderboard for guild {}: {}",
                    guild_id, error
                );
            }

            // schedule the next refresh no matter what
            if let Err(error) =
                sqlx::query("UPDATE auto_leaderboards SET next_due = ?1 WHERE guild_id = ?2")
                    .bind((current_unix_time() + interval_seconds) as i64)
                    .bind(guild_id as i64)
                    .execute(&pool)
                    .await
            {
                eprintln!(
                    "Failed to schedule next refresh for guild {}: {}",
                    guild_id, error
                );
            }
        }
    }
}
