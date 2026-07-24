/*
src/waifu/tasks.rs
This file contains background tasks for the waifu module, such as refreshing the tag cache from the Nekos API.
*/

use std::{sync::Arc, time::Duration};

use reqwest::Client;
use tokio::time::{MissedTickBehavior, interval};
use tracing::{error, info};

use super::nekos::{self, TagCache};

const TAG_REFRESH_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

/// This starts a task that refreshes the tag cache from the Nekos API every six hours.
///
/// It also performs an immediate refresh when the task is started.
pub fn start_tag_cache_refresh(
    client: Client,
    cache: Arc<TagCache>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Fetch immediately instead of waiting six hours for the first tick.
        refresh_once(&client, &cache).await;

        let mut ticker = interval(TAG_REFRESH_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

        // the interval() from tokio makes a first tick immediately, so we consume it here to avoid double-refreshing.
        ticker.tick().await;

        loop {
            ticker.tick().await;
            refresh_once(&client, &cache).await;
        }
    })
}

/// This refreshes the tag cache from the Nekos API once, logging the result.
async fn refresh_once(client: &Client, cache: &TagCache) {
    match nekos::refresh_tag_cache(client, cache).await {
        Ok(count) => {
            info!(tag_count = count, "Refreshed Nekos API tag cache");
        }
        Err(error) => {
            // Keep the previous cache on failure.
            error!(%error, "Could not refresh Nekos API tag cache");
        }
    }
}
