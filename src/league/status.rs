// status.rs
// Periodically cehcks the discords presence and shows a condensed top 10 playtime summary in the status.

use std::time::Duration;

use poise::serenity_prelude::{self as serenity};
use sqlx::SqlitePool;

use crate::Error;

// How long each leaderboard entry remains visible before advancing.
const STATUS_ROTATE_INTERVAL: Duration = Duration::from_secs(30);

// Number of leaderboard positions to include in the status rotation.
const TOP_RANK_COUNT: usize = 10;

// Longer names are shortened
const MAX_NAME_DISPLAY_LEN: usize = 12;

fn truncate_name(name: &str) -> String {
    if name.chars().count() <= MAX_NAME_DISPLAY_LEN {
        name.to_string()
    } else {
        // Leave one character of space for the ellipsis.
        let truncated: String = name.chars().take(MAX_NAME_DISPLAY_LEN - 1).collect();

        format!("{truncated}…")
    }
}

// Builds the status text for a given leaderboard rank.
async fn build_status_text_for_rank(
    pool: &SqlitePool,
    http: &serenity::Http,
    rank: usize,
) -> Result<Option<String>, Error> {
    // select one leaderboard entry at offset rank
    let row: Option<(i64, i64)> = sqlx::query_as(
        r#"
        SELECT user_id, total_seconds
        FROM playtime_totals
        ORDER BY total_seconds DESC
        LIMIT 1 OFFSET ?1
        "#,
    )
    .bind(rank as i64)
    .fetch_optional(pool)
    .await?;

    // If no more players just return none
    let Some((user_id, seconds)) = row else {
        return Ok(None);
    };

    // Fetch the Discord user so we can get their name
    let user = serenity::UserId::new(user_id as u64).to_user(http).await?;

    let name = truncate_name(&user.name);

    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;

    let displayed_rank = rank + 1;

    Ok(Some(format!("#{displayed_rank} {name} {hours}h{minutes}m")))
}

// The main loop that periodically updates the bot's status with the top 10 playtime leaderboard.
pub async fn run_status_update_loop(ctx: serenity::Context, pool: SqlitePool) {
    // The leaderboard position currently being displayed.
    let mut rank = 0usize;

    loop {
        match build_status_text_for_rank(&pool, &ctx.http, rank).await {
            Ok(Some(text)) => {
                ctx.set_activity(Some(serenity::ActivityData::watching(&text)));

                // Move to next position.
                rank = (rank + 1) % TOP_RANK_COUNT;
                tokio::time::sleep(STATUS_ROTATE_INTERVAL).await;
            }

            Ok(None) if rank == 0 => {
                // No leaderboard entries exist yet.
                ctx.set_activity(Some(serenity::ActivityData::watching(
                    "no playtime tracked yet",
                )));
                tokio::time::sleep(STATUS_ROTATE_INTERVAL).await;
            }

            Ok(None) => {
                // when there are no more players return to #1
                rank = 0;
            }

            Err(error) => {
                eprintln!(
                    "Failed to build status text for rank #{}: {}",
                    rank + 1,
                    error
                );

                // Move to next position
                rank = (rank + 1) % TOP_RANK_COUNT;
                tokio::time::sleep(STATUS_ROTATE_INTERVAL).await;
            }
        }
    }
}
