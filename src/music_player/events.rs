/*
src/music_player/events.rs
This file handles Discord events for the music player module.
*/

use poise::serenity_prelude::{Context as SerenityContext, FullEvent};

use crate::{Data, Error};

/// Handles voice state updates related to the music player.
pub async fn handle(ctx: &SerenityContext, event: &FullEvent, data: &Data) -> Result<(), Error> {
    // only handle voice state updates
    let FullEvent::VoiceStateUpdate { old, new } = event else {
        return Ok(());
    };

    // We just want voice state for this bot
    if new.user_id != ctx.cache.current_user().id {
        return Ok(());
    }

    let Some(guild_id) = new.guild_id else {
        return Ok(());
    };

    let was_connected = old.as_ref().and_then(|state| state.channel_id).is_some();
    let is_disconnected = new.channel_id.is_none();

    if was_connected && is_disconnected {
        // the bot was disconnected through discord manually
        super::tasks::disconnect_and_cleanup(&data.songbird, guild_id).await;
    }

    Ok(())
}
