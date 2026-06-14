/*
This is my attempt at making a simple tracker that will make Alexander have most hours in league for the server.
It was largely made using
https://github.com/Gummiees/playtime-discord-bot
as a reference
*/

use std::collections::HashMap;
use std::fs;
use std::sync::Mutex;
use std::time::Instant;

use poise::serenity_prelude as serenity;
use serde::{Deserialize, Serialize};

use crate::{Context, Error};

const DATA_FILE: &str = "playtime_data.json";
const TRACKED_GAME: &str = "League of Legends";

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

pub struct PlaytimeTracker {
    // when each users session is started
    active_sessions: Mutex<HashMap<u64, Instant>>, // user_id -> session_start_time
    // completed playtime for users on storage.
    totals: Mutex<PlaytimeTotals>, // Accumulated playtime totals
}

// This struct will be shared across the bot, so it needs to be thread-safe (hence the Mutex).
impl PlaytimeTracker {
    pub fn new() -> Self {
        Self {
            active_sessions: Mutex::new(HashMap::new()),
            totals: Mutex::new(PlaytimeTotals::load()),
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

// Show the Top 10 users
#[poise::command(slash_command)]
pub async fn playtime(ctx: Context<'_>) -> Result<(), Error> {
    let top_players = ctx.data().playtime_tracker.get_players(10);

    if top_players.is_empty() {
        ctx.say("No playtime data available yet.").await?;

        return Ok(()); // No data to show
    }

    // Build the leaderboard message
    let mut lines = Vec::new();

    for (rank, &(user_id, seconds)) in top_players.iter().enumerate() {
        let user = serenity::UserId::new(user_id).to_user(ctx.http()).await?;

        lines.push(format!(
            "**{}.** {} — {}",
            rank + 1,
            user.name,
            format_duration(seconds)
        ));
    }

    ctx.say(format!(
        "**Top 10 most degenerate LoL players so far:**\n{}",
        lines.join("\n")
    ))
    .await?;

    Ok(())
}
