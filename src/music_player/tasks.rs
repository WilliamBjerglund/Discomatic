/*
src/music_player/tasks.rs
This is a background task for the music player module. It handles Discord events for the music_player module.
*/

use std::time::Duration;

use poise::serenity_prelude::GuildId;
use songbird::Songbird;
use std::sync::Arc;
use tokio::time::{Instant, sleep};

const IDLE_TIMEOUT: Duration = Duration::from_secs(15 * 60); // 15 minutes
const CHECK_INTERVAL: Duration = Duration::from_secs(60); // 1 minute

pub fn idle_timer(songbird: Arc<Songbird>, guild_id: GuildId) {
    // Spawn a background task to monitor the voice channel for inactivity.
    tokio::spawn(async move {
        let mut idle_since = None; // Track when the bot became idle.

        loop {
            sleep(CHECK_INTERVAL).await;

            let Some(call_lock) = songbird.get(guild_id) else {
                // Bot is no longer connected.
                break;
            };

            let queue_is_empty = {
                let call = call_lock.lock().await;
                call.queue().is_empty()
            };

            if queue_is_empty {
                let started = idle_since.get_or_insert_with(Instant::now);

                if started.elapsed() >= IDLE_TIMEOUT {
                    if let Err(error) = songbird.remove(guild_id).await {
                        eprintln!("Failed to disconnect from VC in {guild_id}: {error}");
                    }

                    break;
                }
            } else {
                // reset timer if new shit happens.
                idle_since = None;
            }
        }
    });
}
