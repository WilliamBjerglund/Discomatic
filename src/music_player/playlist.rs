/* playlist.rs

By default Songbird sets the --no-playlist flag for yt-dlp meaning if a user provides a playlist they get just the first song.

I want to fix that by making a workaround that allows the user to provide a playlist and have it play all the songs in the playlist.
*/

use std::sync::Arc;

use songbird::{Call, input::YoutubeDl};
use tokio::{process::Command, sync::Mutex};

use crate::{Context, Error};

// from what i can see most if not all youtube playlist urls has a "list=" path we can use to see if it is a playlist or not.
pub fn is_playlist_url(url: &str) -> bool {
    url.contains("list=")
}

// Function to get the entries of a playlist using yt-dlp
async fn playlist_entries(url: &str) -> Result<Vec<(String, String)>, Error> {
    // Use yt-dlp to get the playlist entries
    let output = Command::new("yt-dlp")
        .args([
            "--flat-playlist",
            "--ignore-errors", // skip unavailable/private/deleted entries
            "--print",
            "%(id)s\t%(title)s",
            url,
        ])
        .output()
        .await?;

    // Check if yt-dlp was successful and return error if not
    if !output.status.success() {
        return Err(format!(
            "yt-dlp failed to get playlist: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    // Parse the output into a vector of (id, title) tuples
    let entries = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (id, title) = line.split_once('\t')?;
            (!id.is_empty() && id != "NA").then(|| (id.to_string(), title.to_string()))
        })
        .collect();

    Ok(entries)
}

pub async fn queue_playlist(
    ctx: Context<'_>,
    call_lock: Arc<Mutex<Call>>,
    url: &str,
) -> Result<(), Error> {
    let entries = playlist_entries(url).await?;

    if entries.is_empty() {
        ctx.say("No valid entries found in the playlist.").await?;
        return Ok(());
    }

    // queue first song and then rest while its playing
    let (was_queued, first_title) = {
        let mut call = call_lock.lock().await;
        let was_queued = !call.queue().is_empty();

        let (first_id, first_title) = &entries[0];
        let first_url = format!("https://www.youtube.com/watch?v={first_id}");
        let source = YoutubeDl::new(ctx.data().http_client.clone(), first_url);
        call.enqueue_input(source.into()).await;

        (was_queued, first_title.clone())
    };

    if was_queued {
        ctx.say(format!(
            "Queuing {} tracks from the playlist...",
            entries.len()
        ))
        .await?;
    } else {
        ctx.say(format!(
            "Now playing **{first_title}** — queuing {} more from the playlist in the background.",
            entries.len() - 1
        ))
        .await?;
    }

    // Queue the rest of the playlist in the background, one at time.
    if entries.len() > 1 {
        let http_client = ctx.data().http_client.clone();
        let rest = entries[1..].to_vec();

        tokio::spawn(async move {
            for (id, _title) in rest {
                let video_url = format!("https://www.youtube.com/watch?v={id}");
                let source = YoutubeDl::new(http_client.clone(), video_url);

                let mut call = call_lock.lock().await;
                call.enqueue_input(source.into()).await;
            }
        });
    }

    Ok(())
}
