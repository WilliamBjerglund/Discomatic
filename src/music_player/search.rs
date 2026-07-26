/*
src/music_player/search.rs

This file provides a helper function for our player to show search results for the user.
This way when a user does /play <bob marley> we will show them the top results like one love and more.
*/

use poise::serenity_prelude::builder::AutocompleteChoice;
use tokio::process::Command;

use crate::Context;

const MAX_RESULTS: usize = 10;
const MIN_QUERY_LENGTH: usize = 3; // dont search at just 2 letters.

/// This represents the information we want to display per query result.
struct SearchResult {
    title: String,
    artist: String,
    duration: String,
    url: String,
}

/// Autocomplete function for the /play command. It will search youtube for the query and return the top results.
pub async fn autocomplete_song(_ctx: Context<'_>, partial_query: &str) -> Vec<AutocompleteChoice> {
    let trimmed_query = partial_query.trim();

    // If the query is too short, we don't want to search for it.
    if trimmed_query.chars().count() < MIN_QUERY_LENGTH {
        return Vec::new();
    }

    // If the query is a URL, we don't want to search for it.
    if trimmed_query.starts_with("http://") || trimmed_query.starts_with("https://") {
        return Vec::new();
    }

    // Search youtube for the query and return the top results.
    let results = match search_youtube(trimmed_query).await {
        Ok(results) => results,
        Err(error) => {
            tracing::warn!("YouTube autocomplete search failed: {error}");
            return Vec::new();
        }
    };

    // Map the results to autocomplete choices and return them.
    results
        .into_iter()
        .map(|result| {
            let display_name =
                format!("{} — {} • {}", result.title, result.artist, result.duration);

            AutocompleteChoice::new(truncate_discord_label(&display_name), result.url)
        })
        .collect()
}

/// Runs yt-dlp to search for the query and returns the top results.
async fn search_youtube(query: &str) -> Result<Vec<SearchResult>, crate::Error> {
    let search = format!("ytsearch{MAX_RESULTS}:{query}");

    let output = Command::new("yt-dlp")
        .args([
            "--flat-playlist", // We only want the metadata, not the actual video.
            "--ignore-errors", // If a video is unavailable, we don't want to fail the entire search.
            //* Print the metadata in a tab-separated format.
            "--print",
            "%(title)s\t%(channel)s\t%(duration_string)s\t%(webpage_url)s",
            &search,
        ])
        .output()
        .await?;

    // If yt-dlp failed, return an error with the stderr output.
    if !output.status.success() {
        return Err(format!(
            "yt-dlp search failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    // Parse the output into a vector of SearchResult structs.
    let results = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_search_result)
        .collect();

    Ok(results)
}

/// This parses one tab seperated line printed by yt dlp in the expected format "title\tartist\tduration\turl" and returns a SearchResult struct.
fn parse_search_result(line: &str) -> Option<SearchResult> {
    let mut fields = line.splitn(4, '\t');

    let title = usable_field(fields.next()?, "Unknown title");
    let artist = usable_field(fields.next()?, "Unknown artist");
    let duration = usable_field(fields.next()?, "Unknown length");
    let url = fields.next()?.trim();

    if url.is_empty() || url == "NA" {
        return None;
    }

    Some(SearchResult {
        title,
        artist,
        duration,
        url: url.to_string(),
    })
}

/// This returns the value if it is not empty or "NA", otherwise it returns the fallback value.
fn usable_field(value: &str, fallback: &str) -> String {
    let value = value.trim();

    if value.is_empty() || value == "NA" {
        fallback.to_string()
    } else {
        value.to_string()
    }
}

/// This truncates a string to a maximum of 100 characters.
fn truncate_discord_label(value: &str) -> String {
    const MAX_CHARS: usize = 100;

    if value.chars().count() <= MAX_CHARS {
        return value.to_string();
    }

    let mut shortened: String = value.chars().take(MAX_CHARS - 1).collect();
    shortened.push('…');
    shortened
}
