/*
src/music_player/tasks.rs
This is a background task for the music player module.
*/

use std::{sync::Arc, time::Duration};

use poise::serenity_prelude::{Cache, GuildId};
use songbird::{Songbird, tracks::PlayMode};
use tokio::time::{Instant, sleep};

const IDLE_TIMEOUT: Duration = Duration::from_secs(15 * 60); // 15 minutes
const ALONE_TIMEOUT: Duration = Duration::from_secs(5 * 60); // 5 minutes
const PAUSED_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60); // 2 hours
const CHECK_INTERVAL: Duration = Duration::from_secs(60); // 1 minute

/// This background task will monitor the VC the bot is in if the bot is not doing anything for 15 minutes it will disconnect and cleanup.
/// Also if the bot is alone in the VC for 5 minutes it will disconnect and cleanup.
pub fn idle_timer(songbird: Arc<Songbird>, guild_id: GuildId, cache: Arc<Cache>) {
    // Spawn a background task to monitor the voice channel for inactivity.
    tokio::spawn(async move {
        let mut idle_since = None; // Track when the bot became idle.
        let mut alone_since = None; // Track when the bot is alone in vc.
        let mut paused_since = None; // Track when the bot is paused.

        loop {
            sleep(CHECK_INTERVAL).await;

            let Some(call_lock) = songbird.get(guild_id) else {
                // Bot is no longer connected.
                break;
            };

            let (queue_is_empty, current_track) = {
                let call = call_lock.lock().await;
                (call.queue().is_empty(), call.queue().current())
            };

            let playback_paused = match current_track {
                Some(track) => match track.get_info().await {
                    Ok(info) => info.playing == PlayMode::Pause,
                    Err(error) => {
                        tracing::debug!(
                            "could not read current track in guild {}: {}",
                            guild_id,
                            error
                        );
                        false
                    }
                },
                None => false,
            };

            // check whether a real user is in the vc with us.
            let bot_is_alone = is_bot_alone(&cache, guild_id);

            if queue_is_empty {
                idle_since.get_or_insert_with(Instant::now);
            } else {
                idle_since = None;
            }

            // check if the bot is alone in the vc.
            if bot_is_alone {
                alone_since.get_or_insert_with(Instant::now);
            } else {
                alone_since = None;
            }

            // paused playback timer
            if playback_paused {
                paused_since.get_or_insert_with(Instant::now);
            } else {
                paused_since = None;
            }

            let idle_timed_out =
                idle_since.is_some_and(|started| started.elapsed() >= IDLE_TIMEOUT);
            let alone_timed_out =
                alone_since.is_some_and(|started| started.elapsed() >= ALONE_TIMEOUT);
            let paused_timed_out =
                paused_since.is_some_and(|started| started.elapsed() >= PAUSED_TIMEOUT);

            if idle_timed_out || alone_timed_out || paused_timed_out {
                disconnect_and_cleanup(&songbird, guild_id).await;
                break;
            }
        }
    });
}

/// This function simply returns true when the bot is alone in a VC.
fn is_bot_alone(cache: &Cache, guild_id: GuildId) -> bool {
    let Some(guild) = cache.guild(guild_id) else {
        return false; // If we can't find the guild, we can't determine if the bot is alone.
    };

    let bot_id = cache.current_user().id.clone();

    let Some(bot_channel_id) = guild.voice_states.get(&bot_id).and_then(|vs| vs.channel_id) else {
        return false; // If we can't find the bot's voice state, the bot is not connected.
    };

    let members_in_channel = guild.voice_states.iter().any(|(user_id, voice_state)| {
        if *user_id == bot_id || voice_state.channel_id != Some(bot_channel_id) {
            return false;
        }

        // ignore other bots.
        guild
            .members
            .get(user_id)
            .map_or(false, |member| !member.user.bot)
    });

    !members_in_channel
}

/// This function is the disconnection that stops all playback and removes the bot from the VC.
pub async fn disconnect_and_cleanup(songbird: &Songbird, guild_id: GuildId) {
    let Some(call_lock) = songbird.get(guild_id) else {
        return; // Bot is not connected to a voice channel.
    };

    // stop the active track and clear the queue.
    {
        let mut call = call_lock.lock().await;
        call.stop();
    }

    if let Err(error) = songbird.remove(guild_id).await {
        tracing::warn!(
            "Failed to remove bot from voice channel in guild {}: {}",
            guild_id,
            error
        );
    }
}
