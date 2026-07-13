use poise::serenity_prelude as serenity;

use crate::{Data, Error};

pub async fn handle(event: &serenity::FullEvent, data: &Data) -> Result<(), Error> {
    let serenity::FullEvent::PresenceUpdate { new_data } = event else {
        return Ok(());
    };

    let elapsed = data
        .playtime_tracker
        .handle_presence_update(new_data.user.id.get(), &new_data.activities);

    let Some(seconds) = elapsed else {
        return Ok(());
    };

    if let Err(error) = sqlx::query(
        r#"
        INSERT INTO playtime_totals (
            user_id,
            total_seconds
        )
        VALUES (?1, ?2)
        ON CONFLICT(user_id)
        DO UPDATE SET
            total_seconds = total_seconds + ?2
        "#,
    )
    .bind(new_data.user.id.get() as i64)
    .bind(seconds as i64)
    .execute(&data.pool)
    .await
    {
        tracing::error!(
            user_id = new_data.user.id.get(),
            seconds,
            %error,
            "Failed to record League playtime"
        );
    }

    Ok(())
}
