// status.rs
// Periodically cehcks the discords presence and shows a condensed top 3 playtime summary in the status.

use std::time::Duration;

use poise::serenity_prelude::{self as serenity};
use sqlx::SqlitePool;

use crate::Error;

// How often to check whether the top-3 status needs to be recomputed.
const STATUS_CHECK_INTERVAL: Duration = Duration::from_secs(4 * 60 * 60);

const RANK_LABELS: [&str; 3] = ["#1", "#2", "#3"];
const MAX_NAME_DISPLAY_LEN: usize = 12;

fn truncate_name(name: &str) -> String {
    if name.chars().count() <= MAX_NAME_DISPLAY_LEN {
        name.to_string()
    } else {
        let truncated: String = name.chars().take(MAX_NAME_DISPLAY_LEN - 1).collect();
        format!("{truncated}…")
    }
}

//  build a short "top 3" status string.
async fn build_status_text(pool: &SqlitePool, http: &serenity::Http) -> Result<String, Error> {
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT user_id, total_seconds FROM playtime_totals ORDER BY total_seconds DESC LIMIT 3",
    )
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok("no playtime tracked yet".to_string());
    }

    let mut parts = Vec::with_capacity(rows.len());
    for (i, &(user_id, seconds)) in rows.iter().enumerate() {
        let user = serenity::UserId::new(user_id as u64).to_user(http).await?;
        let rank = RANK_LABELS.get(i).copied().unwrap_or("");
        let name = truncate_name(&user.name);
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        parts.push(format!("{rank} {name} {hours}h{minutes}m"));
    }

    Ok(parts.join(" · "))
}

// Every interval we recompute the top 3 string and push a presence update if it changed.
pub async fn run_status_update_loop(ctx: serenity::Context, pool: SqlitePool) {
    let mut last_status: Option<String> = None;

    loop {
        match build_status_text(&pool, &ctx.http).await {
            Ok(text) => {
                if last_status.as_deref() != Some(text.as_str()) {
                    ctx.set_activity(Some(serenity::ActivityData::watching(&text)));
                    last_status = Some(text);
                }
            }
            Err(error) => {
                eprintln!("Failed to build status text: {}", error);
            }
        }

        tokio::time::sleep(STATUS_CHECK_INTERVAL).await;
    }
}
