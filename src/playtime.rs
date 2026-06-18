/*
This is my attempt at making a simple tracker that will make Alexander have most hours in league for the server.
It was largely made using
https://github.com/Gummiees/playtime-discord-bot
as a reference
*/

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use poise::serenity_prelude::{self as serenity};
use sqlx::SqlitePool;

use crate::{Context, Error};

const TRACKED_GAME: &str = "League of Legends";

// How often the background task checks whether any auto-leaderboard is due for a refresh.
const AUTO_CHECK_INTERVAL: Duration = Duration::from_secs(5 * 60);

pub struct PlaytimeTracker {
    // when each users session is started
    active_sessions: Mutex<HashMap<u64, Instant>>, // user_id -> session_start_time
}

// This struct is shared across the bot.
impl PlaytimeTracker {
    pub fn new() -> Self {
        Self {
            active_sessions: Mutex::new(HashMap::new()),
        }
    }

    // Now we need a call for every "precense" update discord has, essetially we check when sessions start and end.
    // based on whether the individuals activity list contains the game we want to track.
    // RETURNS: elapsed seconds if a session ended.
    pub fn handle_presence_update(
        &self,
        user_id: u64,
        activities: &[serenity::Activity],
    ) -> Option<u64> {
        let is_playing_league = activities
            .iter()
            .any(|a| a.kind == serenity::ActivityType::Playing && a.name == TRACKED_GAME);

        let mut sessions = self.active_sessions.lock().unwrap();

        if is_playing_league {
            sessions.entry(user_id).or_insert_with(Instant::now);
            None
        } else {
            sessions
                .remove(&user_id)
                .map(|start| start.elapsed().as_secs())
        }
    }
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
It then just pairs playtime fetching with formatting so both /playtime and the auto-leaderboard show exactly the same output.
*/
async fn build_leaderboard_message(
    pool: &SqlitePool,
    http: &serenity::Http,
) -> Result<String, Error> {
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT user_id, total_seconds FROM playtime_totals ORDER BY total_seconds DESC LIMIT 10",
    )
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok("No playtime data available yet.".to_string());
    }

    let mut lines = Vec::new();

    for (rank, &(user_id, seconds)) in rows.iter().enumerate() {
        let user = serenity::UserId::new(user_id as u64).to_user(http).await?;

        lines.push(format!(
            "**{}.** {} — {}",
            rank + 1,
            user.name,
            format_duration(seconds as u64)
        ));
    }

    Ok(format!(
        "**Top 10 most degenerate LoL players so far:**\n{}",
        lines.join("\n")
    ))
}

// attempts deletion of message if it cant it logs.
async fn try_delete_message(http: &serenity::Http, channel_id: u64, message_id: u64) {
    let channel = serenity::ChannelId::new(channel_id);
    let message = serenity::MessageId::new(message_id);

    if let Err(error) = channel.delete_message(http, message).await {
        eprintln!("Failed to delete leaderboard message: {}", error);
    }
}

// Refreshes the leaderboard: deletes the old message, posts a new one at the bottom of the channel, and updates the tracked message_id in the database.
// THAT IS WHAT I HOPE AT LEAST LAST TIME IT FUCKED UP BADLY.
async fn refresh_leaderboard(
    pool: &SqlitePool,
    http: &serenity::Http,
    channel_id: u64,
    guild_id: u64,
) -> Result<(), Error> {
    let content = build_leaderboard_message(pool, http).await?;
    let serenity_channel = serenity::ChannelId::new(channel_id);

    // Delete the old message if we have one tracked for this channel.
    if let Some(old_message_id) = sqlx::query_scalar::<_, i64>(
        "SELECT message_id FROM channel_leaderboards WHERE channel_id = ?1",
    )
    .bind(channel_id as i64)
    .fetch_optional(pool)
    .await?
    {
        try_delete_message(http, channel_id, old_message_id as u64).await;
    }

    // Post the new message at the bottom of the channel.
    let new_message = serenity_channel
        .send_message(
            http,
            serenity::CreateMessage::new()
                .content(content)
                .flags(serenity::MessageFlags::SUPPRESS_NOTIFICATIONS),
        )
        .await?;

    // Store the new message_id, preserving whatever auto settings already exist for this channel.
    sqlx::query(
        "INSERT INTO channel_leaderboards (channel_id, guild_id, message_id, auto_enabled, interval_seconds, next_due)
         VALUES (?1, ?2, ?3, 0, 0, 0)
         ON CONFLICT(channel_id) DO UPDATE SET message_id = ?3",
    )
    .bind(channel_id as i64)
    .bind(guild_id as i64)
    .bind(new_message.id.get() as i64)
    .execute(pool)
    .await?;

    Ok(())
}

// Show the Top 10 users
#[poise::command(slash_command, guild_only)]
pub async fn playtime(ctx: Context<'_>) -> Result<(), Error> {
    // simple ACK for interaction
    ctx.defer().await?;

    refresh_leaderboard(
        &ctx.data().pool,
        ctx.http(),
        ctx.channel_id().get(),
        ctx.guild_id()
            .expect("guild_only ensures this is set")
            .get(),
    )
    .await
}

// A select list for intervals for playtimeauto
#[derive(Debug, Clone, Copy, poise::ChoiceParameter)]
pub enum Interval {
    #[name = "TEST"]
    HalfAMinute,
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
            Interval::HalfAMinute => 30,
            Interval::OneDay => 24 * 3600,
            Interval::SevenDays => 7 * 24 * 3600,
            Interval::ThirtyDays => 30 * 24 * 3600,
        }
    }
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
    let channel_id = ctx.channel_id().get();

    ctx.defer().await?;

    // If this guild already has an auto-leaderboard in a different channel, remove it first.
    if let Some((old_channel_id, old_message_id)) = sqlx::query_as::<_, (i64, i64)>(
        "SELECT channel_id, message_id FROM channel_leaderboards
         WHERE guild_id = ?1 AND auto_enabled = 1 AND channel_id != ?2",
    )
    .bind(guild_id as i64)
    .bind(channel_id as i64)
    .fetch_optional(pool)
    .await?
    {
        try_delete_message(ctx.http(), old_channel_id as u64, old_message_id as u64).await;

        // remove old message
        sqlx::query("DELETE FROM channel_leaderboards WHERE channel_id = ?1")
            .bind(old_channel_id)
            .execute(pool)
            .await?;
    }

    // Post fresh leaderboard in the target channel, then set the auto settings in one upsert.
    refresh_leaderboard(pool, ctx.http(), channel_id, guild_id).await?;

    let next_due = if enabled {
        current_unix_time() + interval.as_seconds()
    } else {
        0
    };

    sqlx::query(
        "UPDATE channel_leaderboards
         SET auto_enabled = ?1, interval_seconds = ?2, next_due = ?3
         WHERE channel_id = ?4",
    )
    .bind(enabled as i64)
    .bind(interval.as_seconds() as i64)
    .bind(next_due as i64)
    .bind(channel_id as i64)
    .execute(pool)
    .await?;

    // Acknowledge the command
    ctx.send(
        poise::CreateReply::default()
            .content(if enabled {
                "Auto leaderboard is now active in this channel!"
            } else {
                "Auto leaderboard has been turned off."
            })
            .ephemeral(true),
    )
    .await?;

    Ok(())
}

// Loop leaderbaord in the background periodically.
// update if refresh time
pub async fn run_auto_leaderboard_loop(http: Arc<serenity::Http>, pool: SqlitePool) {
    loop {
        tokio::time::sleep(AUTO_CHECK_INTERVAL).await;

        // figure out which guilds need refreshing
        let now = current_unix_time();

        let due: Vec<(i64, i64, i64)> = match sqlx::query_as(
            "SELECT channel_id, guild_id, interval_seconds
             FROM channel_leaderboards
             WHERE auto_enabled = 1 AND next_due <= ?1",
        )
        .bind(now as i64)
        .fetch_all(&pool)
        .await
        {
            Ok(rows) => rows,
            Err(error) => {
                eprintln!("Failed to check due auto leaderboards: {}", error);
                continue;
            }
        };

        for (channel_id, guild_id, interval_seconds) in due {
            if let Err(error) =
                refresh_leaderboard(&pool, &http, channel_id as u64, guild_id as u64).await
            {
                eprintln!(
                    "Failed to refresh auto leaderboard for channel {}: {}",
                    channel_id, error
                );
            }

            // schedule the next refresh no matter what
            if let Err(error) =
                sqlx::query("UPDATE channel_leaderboards SET next_due = ?1 WHERE channel_id = ?2")
                    .bind((current_unix_time() + interval_seconds as u64) as i64)
                    .bind(channel_id)
                    .execute(&pool)
                    .await
            {
                eprintln!(
                    "Failed to schedule next refresh for channel {}: {}",
                    channel_id, error
                );
            }
        }
    }
}
