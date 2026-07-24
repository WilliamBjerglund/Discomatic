/*
src/waifu/nekos.rs
This file is a very quick and dirty little tool to get waifu pics i plan on making more API's into it because of David so this is a mess for now.
*/

use std::collections::HashMap;

use reqwest::Client;
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::{Context, Error};

const BASE_URL: &str = "https://api.nekosapi.com/v5"; // endpoint for the Nekos API
const TAG_PAGE_SIZE: usize = 100; // max allowed tags in single request
const AUTOCOMPLETE_LIMIT: usize = 25; // max allowed autocomplete cause discord lowkey dont like us

/// This is just a tag returned by the API so whatever ID it has and whether it is nsfw
#[derive(Debug, Clone, Deserialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub is_nsfw: bool,
}

/// This is a image returned by the API so just the URL of the image
#[derive(Debug, Deserialize)]
struct Image {
    url: String,
}

/// this one is a just a way to get all the tags by the endpoint.
#[derive(Debug, Deserialize)]
struct TagPage {
    items: Vec<Tag>,
    total: usize,
}

/// Simple cache to index the name of all tags
#[derive(Debug, Default)]
pub struct TagCache {
    by_name: RwLock<HashMap<String, Tag>>,
}

impl TagCache {
    /// Replaces the entire cache with a new set of tags.
    pub async fn replace(&self, tags: Vec<Tag>) {
        let tags = tags
            .into_iter()
            .map(|tag| (normalize_tag_name(&tag.name), tag))
            .collect();

        *self.by_name.write().await = tags;
    }

    pub async fn is_empty(&self) -> bool {
        self.by_name.read().await.is_empty()
    }

    /// Find a tag by its name dont care about case
    pub async fn get_by_name(&self, name: &str) -> Option<Tag> {
        let normalized_name = normalize_tag_name(name);
        let cache = self.by_name.read().await;

        cache.get(&normalized_name).cloned()
    }

    // Search the cached tags for the best match and reutrn it
    pub async fn search(&self, query: &str, limit: usize) -> Vec<Tag> {
        let query = normalize_tag_name(query);
        let cache = self.by_name.read().await;
        let mut matches = Vec::new();

        // Filter tags that contain the query string, case-insensitive
        for tag in cache.values() {
            let normalized_name = normalize_tag_name(&tag.name);

            if normalized_name.contains(&query) {
                matches.push(tag.clone());
            }
        }

        matches.sort_by_key(|tag| {
            let name = normalize_tag_name(&tag.name);

            let priority = if name == query {
                0
            } else if name.starts_with(&query) {
                1
            } else {
                2
            };

            (priority, name)
        });

        matches.truncate(limit);
        matches
    }
}

fn normalize_tag_name(name: &str) -> String {
    name.trim().to_lowercase()
}

/// This function makes a API call and requests one random image depending on if its safe only and if it has a tag or not.
async fn random_image(
    client: &Client,
    safe_only: bool,
    tag: Option<&str>,
) -> Result<Image, reqwest::Error> {
    let mut request = client.get(format!("{BASE_URL}/images/random"));

    if safe_only {
        request = request.query(&[("rating", "safe")]);
    }

    if let Some(tag) = tag {
        request = request.query(&[("tag", tag)]);
    }

    request.send().await?.error_for_status()?.json().await
}

/// Redownload all tags from API replace local cache with new tags and return the number of tags downloaded
pub async fn refresh_tag_cache(client: &Client, cache: &TagCache) -> Result<usize, reqwest::Error> {
    let mut tags = Vec::new();
    let mut offset = 0;

    loop {
        // Request a page of tags from the API
        let request = client
            .get(format!("{BASE_URL}/tags"))
            .query(&[("limit", TAG_PAGE_SIZE), ("offset", offset)]);

        // Send the request and handle errors
        let raw_response = request.send().await?;
        let checked_response = raw_response.error_for_status()?;
        let page = checked_response.json::<TagPage>().await?; // Deserialize the response into a TagPage struct

        // Extract the items and total count from the page
        let items = page.items;
        let total = page.total;

        if items.is_empty() {
            break;
        }

        offset += items.len();
        tags.extend(items);

        if offset >= total {
            break;
        }
    }

    let count = tags.len();
    cache.replace(tags).await;

    Ok(count)
}

/// Function is in the name it just autocompletes a list of tags for the user to select from.
/// It filters out NSFW tags if the channel is not NSFW.
async fn autocomplete_tag(ctx: Context<'_>, partial: &str) -> impl Iterator<Item = String> {
    let channel_is_nsfw = ctx
        .guild_channel() // Get the channel the command was invoked in
        .await
        .map(|channel| channel.nsfw) // Check if the channel is NSFW
        .unwrap_or(false); // Default to false if the channel is not found

    ctx.data()
        .tag_cache
        .search(partial, 100) // search for up to 100 tags
        .await
        .into_iter() // interate over the results
        .filter(move |tag| channel_is_nsfw || !tag.is_nsfw) // filter out NSFW tags if the channel is not NSFW
        .take(AUTOCOMPLETE_LIMIT) // stop when we have 25 tags
        .map(|tag| tag.name)
}

/// This is the command Waifu that gets a random anime image optionally filtered by a tag.
#[poise::command(slash_command)]
pub async fn waifu(
    ctx: Context<'_>,

    #[description = "Optional tag; begin typing to search"]
    #[autocomplete = "autocomplete_tag"]
    tag: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;

    let channel_is_nsfw = ctx
        .guild_channel()
        .await
        .map(|channel| channel.nsfw)
        .unwrap_or(false);

    let selected_tag = match tag.as_deref() {
        Some(input) => {
            if ctx.data().tag_cache.is_empty().await {
                ctx.say("The tag cache is loading.").await?;

                return Ok(());
            }

            let Some(tag) = ctx.data().tag_cache.get_by_name(input).await else {
                ctx.say(format!(
                    "Unknown tag `{input}`. Select a tag from autocomplete you dingus."
                ))
                .await?;

                return Ok(());
            };

            if tag.is_nsfw && !channel_is_nsfw {
                ctx.say("That tag belongs only in the dark caves of NSFW channels.")
                    .await?;

                return Ok(());
            }

            Some(tag)
        }

        None => None,
    };

    // If a tag was provided, we use its ID in the API request; otherwise, we request a random image without a tag.
    let tag_id = selected_tag.as_ref().map(|tag| tag.id.as_str());

    // Normal channels request safe images; NSFW channels allow every rating.
    let image = random_image(&ctx.data().http_client, !channel_is_nsfw, tag_id).await?;

    ctx.send(poise::CreateReply::default().content(image.url))
        .await?;

    Ok(())
}
