/*
This is my attempt at making a simple tracker that will make Alexander have most hours in league for the server.
It was largely made using
https://github.com/Gummiees/playtime-discord-bot
as a reference
*/

use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use poise::serenity_prelude::{self as serenity};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex as AsyncMutex;

use crate::{Context, Error};

const DATA_FILE: &str = "playtime_data.json";
const MESSAGE_DATA_FILE: &str = "message_data.json";
const AUTO_DATA_FILE: &str = "auto_data.json";
const TRACKED_GAME: &str = "League of Legends";

const AUTO_CHECK_INTERVAL: Duration = Duration::from_secs(5 * 60);

// Accumulated playtime per user, stores as seconds
#[derive(Default, Serialize, Deserialize)]
struct PlaytimeTotals {
    seconds_per_user: HashMap<u64, u64>, // user_id -> total_seconds (u64 for simplicity)
}

impl PlaytimeTotals {
    // Load playtime data from file, or return an empty tracker if the file doesn't exist or is invalid.
    fn load() -> Self {
        match fs::read_to_string(DATA_FILE) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    // Save playtime data to file.
    fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(DATA_FILE, json);
        }
    }

    // Add playtime for a user.
    fn add_seconds(&mut self, user_id: u64, seconds: u64) {
        *self.seconds_per_user.entry(user_id).or_insert(0) += seconds;
        self.save()
    }
}

#[derive(Default, Serialize, Deserialize)]
struct LeaderboardMessages {
    message_per_channel: HashMap<u64, u64>, // channel_id -> message_id
}

impl LeaderboardMessages {
    fn load() -> Self {
        match fs::read_to_string(MESSAGE_DATA_FILE) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    fn save(&self) {
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(error) = fs::write(MESSAGE_DATA_FILE, json) {
                    eprintln!("Failed to save message data: {}", error);
                }
            }
            Err(error) => {
                eprintln!("Failed to serialize message data: {}", error)
            }
        }
    }
}

// The servers Leaderboard setup, where does it live and how often does it refresh and is it on
#[derive(Clone, Serialize, Deserialize)]
struct AutoLeaderboardConfig {
    channel_id: u64,
    message_id: u64,
    interval_seconds: u64,
    enabled: bool,
    next_due: u64,
}

// Onee leaderboard config per server keyed by its guild id
#[derive(Default, Serialize, Deserialize)]
struct AutoLeaderboards {
    config_per_guild: HashMap<u64, AutoLeaderboardConfig>,
}

// Configuration for automatic leaderboard updates
impl AutoLeaderboards {
    fn load() -> Self {
        match fs::read_to_string(AUTO_DATA_FILE) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    fn save(&self) {
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(error) = fs::write(AUTO_DATA_FILE, json) {
                    eprintln!("Failed to save auto-leaderboard data: {}", error);
                }
            }
            Err(error) => {
                eprintln!("Failed to serialize auto-leaderboard data: {}", error)
            }
        }
    }
}

pub struct PlaytimeTracker {
    // when each users session is started
    active_sessions: Mutex<HashMap<u64, Instant>>, // user_id -> session_start_time
    // completed playtime for users on storage.
    totals: Mutex<PlaytimeTotals>, // Accumulated playtime
    // channel_id -> last leaderboard message_id
    last_messages: AsyncMutex<LeaderboardMessages>,
    // guild_id -> auto leaderboard config
    auto_leaderboards: AsyncMutex<AutoLeaderboards>,
}

// This struct will be shared across the bot, so it needs to be thread-safe (hence the Mutex).
impl PlaytimeTracker {
    pub fn new() -> Self {
        Self {
            active_sessions: Mutex::new(HashMap::new()),
            totals: Mutex::new(PlaytimeTotals::load()),
            last_messages: AsyncMutex::new(LeaderboardMessages::load()),
            auto_leaderboards: AsyncMutex::new(AutoLeaderboards::load()),
        }
    }

    // Now we need a call for every "precense" update discord has, essetially we check when sessions start and end.
    // based on whether the individuals activity list contains the game we want to track.
    pub fn handle_presence_update(&self, user_id: u64, activities: &[serenity::Activity]) {
        let is_playing_league = activities.iter().any(|activity| {
            activity.kind == serenity::ActivityType::Playing && activity.name == TRACKED_GAME
        });

        // We need to lock the active_sessions mutex to check and update session states.
        let mut sessions = self.active_sessions.lock().unwrap();

        if is_playing_league {
            sessions.entry(user_id).or_insert_with(Instant::now);
        } else if let Some(start_time) = sessions.remove(&user_id) {
            let elapsed = start_time.elapsed().as_secs();
            drop(sessions); // Release the lock before updating totals.
            self.totals.lock().unwrap().add_seconds(user_id, elapsed);
        }
    }

    // returns up to 10 users with the most playtime, sorted by playtime 1-10
    pub fn get_players(&self, limit: usize) -> Vec<(u64, u64)> {
        let totals = self.totals.lock().unwrap();

        let mut entries: Vec<(u64, u64)> = totals
            .seconds_per_user
            .iter()
            .map(|(&user_id, &seconds)| (user_id, seconds))
            .collect();
        entries.sort_by(|left, right| right.1.cmp(&left.1)); // Sort by playtime in descending order
        entries.truncate(limit);
        entries
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
    let top_players = ctx.data().playtime_tracker.get_players(10);

    if top_players.is_empty() {
        ctx.say("No playtime data available yet.").await?;
        return Ok(());
    }

    let content = build_leaderboard_content(ctx.http(), &top_players).await?;

    let channel_id = ctx.channel_id();
    let channel_id_u64 = channel_id.get();

    let mut message_data = ctx.data().playtime_tracker.last_messages.lock().await;

    // Look up the last message ID without removing it yet, thus message data persist if new message fails.
    let old_message_id = message_data
        .message_per_channel
        .get(&channel_id_u64)
        .copied();

    // post new leaderboard
    let reply = ctx.say(content).await?;
    let new_message = reply.message().await?;

    // now delete the old one
    if let Some(old_message_id) = old_message_id {
        let old_message_id = serenity::MessageId::new(old_message_id);
        if let Err(error) = channel_id.delete_message(ctx.http(), old_message_id).await {
            eprintln!("Failed to delete old leaderboard message: {}", error);
        }
    }

    // store the new ID
    message_data
        .message_per_channel
        .insert(channel_id_u64, new_message.id.get());

    // finally we persist the updated IDS to our JSON
    message_data.save();

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

// This auto leaderboard message will be posted in a channel and then updated every X time, where X is the interval the user selects when setting it up.
// it also remembers channel IDs for next time
async fn refresh_auto_leaderboard(
    http: &serenity::Http,
    tracker: &PlaytimeTracker,
    guild_id: u64,
) -> Result<(), Error> {
    let top_players = tracker.get_players(10);
    let content = build_leaderboard_content(http, &top_players).await?;

    let mut auto_data = tracker.auto_leaderboards.lock().await;

    let Some(config) = auto_data.config_per_guild.get_mut(&guild_id) else {
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

            config.message_id = new_message.id.get();
        }
    }

    auto_data.save();

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
    let guild_id = ctx
        .guild_id()
        .expect("This command should only be used in a guild")
        .get();
    let channel_id = ctx.channel_id();
    let channel_id_u64 = channel_id.get();

    let top_players = ctx.data().playtime_tracker.get_players(10);
    let content = build_leaderboard_content(ctx.http(), &top_players).await?;

    let mut auto_data = ctx.data().playtime_tracker.auto_leaderboards.lock().await;

    // Clone any existing config and mutate the map below without fighting a held ref.
    let existing_config = auto_data.config_per_guild.get(&guild_id).cloned();

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
        if let Some(old_config) = existing_config {
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

        auto_data.config_per_guild.insert(
            guild_id,
            AutoLeaderboardConfig {
                channel_id: channel_id_u64,
                message_id: new_message.id.get(),
                interval_seconds: interval.as_seconds(),
                enabled,
                next_due,
            },
        );
    } else {
        // same channel, update settings and refresh message
        let config = auto_data.config_per_guild.get_mut(&guild_id).unwrap();
        config.interval_seconds = interval.as_seconds();
        config.enabled = enabled;
        config.next_due = next_due;

        let message_id = serenity::MessageId::new(config.message_id);
        let edit = serenity::EditMessage::new().content(content);

        if let Err(error) = channel_id.edit_message(ctx.http(), message_id, edit).await {
            eprintln!("Failed to edit auto leaderboard message: {}", error);
        }

        // Acknowledge the command
        ctx.send(
            poise::CreateReply::default()
                .content("Auto leaderboard settings updated!")
                .ephemeral(true),
        )
        .await?;
    }

    auto_data.save();

    Ok(())
}

// Loop leaderbaord in the background periodically.
// update if refresh time
pub async fn run_auto_leaderboard_loop(http: Arc<serenity::Http>, tracker: Arc<PlaytimeTracker>) {
    loop {
        tokio::time::sleep(AUTO_CHECK_INTERVAL).await;

        // figure out which guilds need refreshing
        let due_guilds: Vec<u64> = {
            let auto_data = tracker.auto_leaderboards.lock().await;
            let now = current_unix_time();

            auto_data
                .config_per_guild
                .iter()
                .filter(|(_, config)| config.enabled && now >= config.next_due)
                .map(|(&guild_id, _)| guild_id)
                .collect()
        };

        for guild_id in due_guilds {
            if let Err(error) = refresh_auto_leaderboard(&http, &tracker, guild_id).await {
                eprintln!(
                    "Failed to refresh auto leaderboard for guild {}: {}",
                    guild_id, error
                );
            }

            // schedule the next refresh no matter what
            let mut auto_data = tracker.auto_leaderboards.lock().await;
            if let Some(config) = auto_data.config_per_guild.get_mut(&guild_id) {
                config.next_due = current_unix_time() + config.interval_seconds;
            }
            auto_data.save();
        }
    }
}
