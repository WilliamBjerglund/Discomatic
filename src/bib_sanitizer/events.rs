/*
src/bib_sanitizer/events.rs
This file contains the event handling logic for the bib_sanitizer module.
*/

use poise::serenity_prelude as serenity;

use crate::Error;

pub async fn handle(ctx: &serenity::Context, event: &serenity::FullEvent) -> Result<(), Error> {
    let serenity::FullEvent::Message { new_message } = event else {
        return Ok(());
    };

    super::sanitizer::handle_message(ctx, new_message).await?;
    super::metadata::handle_message(ctx, new_message).await?;

    Ok(())
}
