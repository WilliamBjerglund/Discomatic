/*
src/music_player/player.rs
This is a attempt at making a Voice playblack / Discord Music Bot.

The idea: It should join a channel, play music in accordance with a queue via yt-dlp.

inspiration is drawn from:
https://github.com/phoxwupsh/turto
*/

use poise::serenity_prelude::ChannelId;
use songbird::input::{Compose, YoutubeDl};

use crate::music_player::{playlist, search, tasks};
use crate::{Context, Error};

// Finds the voice channel of the user
fn user_voice_channel(ctx: Context<'_>) -> Option<ChannelId> {
    ctx.guild()?.voice_states.get(&ctx.author().id)?.channel_id
}

// Command to join the VC user is currently in
#[poise::command(slash_command, guild_only, category = "Music")]
pub async fn join(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().expect("Guild only");

    // Case 1: User is not in a voice channel
    let Some(channel_id) = user_voice_channel(ctx) else {
        ctx.say("You are not in a voice channel").await?;
        return Ok(());
    };

    // Case 2: Join the voice channel if the bot is not already connected
    let already_connected = ctx.data().songbird.get(guild_id).is_some();
    ctx.data().songbird.join(guild_id, channel_id).await?;

    // Start the idle timer if the bot is not already connected to a voice channel
    if !already_connected {
        tasks::idle_timer(
            ctx.data().songbird.clone(),
            guild_id,
            ctx.serenity_context().cache.clone(),
        );
    }

    ctx.say(format!("Joined <#{channel_id}>")).await?;

    Ok(())
}

fn format_duration(duration: std::time::Duration) -> String {
    let total_seconds = duration.as_secs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

// Function to play a Youtube link or search Youtube using supplied text by user.
#[poise::command(slash_command, guild_only, category = "Music")]
pub async fn play(
    ctx: Context<'_>,
    #[description = "Youtube link or search query or playlist link"]
    #[autocomplete = "search::autocomplete_song"]
    query: String,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().expect("Guild only");

    // Case 1: User is not in a voice channel
    let Some(channel_id) = user_voice_channel(ctx) else {
        ctx.say("You are not in a voice channel").await?;
        return Ok(());
    };

    // Since lookup can take time we defer our response and acknowledge the command.
    ctx.defer().await?;

    // Case 2: If the bot is not already in the VC join automatically.
    let already_connected = ctx.data().songbird.get(guild_id).is_some();
    let call_lock = ctx.data().songbird.join(guild_id, channel_id).await?;

    if !already_connected {
        tasks::idle_timer(
            ctx.data().songbird.clone(),
            guild_id,
            ctx.serenity_context().cache.clone(),
        );
    }

    let is_url = query.starts_with("http://") || query.starts_with("https://");

    if is_url && playlist::is_playlist_url(&query) {
        return playlist::queue_playlist(ctx, call_lock, &query).await;
    }

    let mut source = if is_url {
        // if the query is a URL just use it.
        YoutubeDl::new(ctx.data().http_client.clone(), query.clone())
    } else {
        // if the query is not a URL search Youtube for it.
        YoutubeDl::new_search(ctx.data().http_client.clone(), query.clone())
    };

    // Ask for the title and confirm the source exists
    let metadata = match source.aux_metadata().await {
        Ok(metadata) => metadata,
        Err(error) => {
            ctx.say(format!("Failed to find song: {}", error)).await?;
            return Ok(());
        }
    };

    // If the title is not found, use the query as a fallback
    let title = metadata.title.unwrap_or_else(|| query.clone());

    let duration = metadata
        .duration
        .map(format_duration)
        .unwrap_or_else(|| "unknown length".to_string());

    let display = format!("**{title}** `[{duration}]`");

    // play first song and queue rest.
    let was_queued = {
        let mut call = call_lock.lock().await;
        let was_queued = !call.queue().is_empty();

        let _track_handle = call.enqueue_input(source.into()).await;

        was_queued
    };

    if was_queued {
        ctx.say(format!("Added to queue: {display}")).await?;
    } else {
        ctx.say(format!("Now playing: {display}")).await?;
    }

    Ok(())
}

#[poise::command(slash_command, guild_only, category = "Music")]
pub async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().expect("Guild only");

    // Bot is not in a voice channel
    let Some(call_lock) = ctx.data().songbird.get(guild_id) else {
        ctx.say("I am not connected to a voice channel.").await?;
        return Ok(());
    };

    // get lock stop queue drop lock
    call_lock.lock().await.stop();

    ctx.say("Stopped playback and cleared the queue.").await?;

    Ok(())
}

#[poise::command(slash_command, guild_only, category = "Music")]
pub async fn skip(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();

    let Some(call_lock) = ctx.data().songbird.get(guild_id) else {
        ctx.say("Nothing is playing.").await?;
        return Ok(());
    };

    let call = call_lock.lock().await;
    call.queue().skip()?;
    ctx.say("Skipped.").await?;
    Ok(())
}
