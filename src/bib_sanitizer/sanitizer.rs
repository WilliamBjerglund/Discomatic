use std::collections::HashSet;

use linkify::{LinkFinder, LinkKind};
use poise::serenity_prelude as serenity;
use url::Url;

// This list is all prefixes that are tracking paremeters so things such as utm just goes away...
// in other words no need matching, if found just remove
const TRACKING_PREFIXES: &[&str] = &["utm_", "hsa_"];

// Stanalone parameters
const TRACKING_PARAMETERS: &[&str] = &["gclid", "dclid"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizedUrl {
    pub cleaned_url: String,
    pub removed_parameters: Vec<String>,
}

// Returns true if the parameter name is a tracking parameter that should be removed from the URL.
fn is_tracking_parameter(parameter_name: &str) -> bool {
    let lowercase_name = parameter_name.to_ascii_lowercase();

    let matches_prefix = TRACKING_PREFIXES
        .iter()
        .any(|prefix| lowercase_name.starts_with(prefix));

    let matches_exact_name = TRACKING_PARAMETERS
        .iter()
        .any(|parameter| lowercase_name == *parameter);

    matches_prefix || matches_exact_name
}

// This function sanitizes a URL and returns "Some(SanitizedUrl)" when at least one paremeter is removed, otherwise "None" is returned for already clean.
pub fn sanitize_url(raw_url: &str) -> Option<SanitizedUrl> {
    let mut url = Url::parse(raw_url).ok()?;

    if url.scheme() != "http" && url.scheme() != "https" {
        return None; // Not a valid HTTP/HTTPS URL
    }

    let original_parameters: Vec<(String, String)> = url
        .query_pairs() // Get the original query parameters as a vector of (key, value) pairs
        .map(|(k, v)| (k.to_string(), v.to_string())) // Convert to owned Strings
        .collect();

    let mut kept_parameters = Vec::new();
    let mut removed_parameters: Vec<String> = Vec::new();

    // Iterate through the original parameters and separate them into kept and removed based on tracking criteria
    for (key, value) in original_parameters {
        if is_tracking_parameter(&key) {
            if !removed_parameters
                .iter()
                .any(|removed| removed.eq_ignore_ascii_case(&key))
            {
                removed_parameters.push(key);
            }
        } else {
            kept_parameters.push((key, value));
        }
    }

    if removed_parameters.is_empty() {
        return None; // No tracking parameters found, return None
    }

    // Clear the existing query, then rebuild everything using only kept params
    url.set_query(None);
    if !kept_parameters.is_empty() {
        let mut query = url.query_pairs_mut();

        for (key, value) in kept_parameters {
            query.append_pair(&key, &value);
        }
    }

    Some(SanitizedUrl {
        cleaned_url: url.to_string(),
        removed_parameters,
    })
}

// Finds all URLS in a message and sanitizes everything that contains tracking parameters.
pub fn sanitize_message(message_content: &str) -> Vec<SanitizedUrl> {
    let mut finder = LinkFinder::new();

    // Find URLs only, not email addresses.
    finder.kinds(&[LinkKind::Url]);

    let mut seen_urls = HashSet::new();
    let mut sanitized_urls = Vec::new();

    for link in finder.links(message_content) {
        let Some(sanitized) = sanitize_url(link.as_str()) else {
            continue;
        };

        if seen_urls.insert(sanitized.cleaned_url.clone()) {
            sanitized_urls.push(sanitized);
        }
    }
    sanitized_urls
}

// Finally we handle all incoming Discord Messages
pub async fn handle_message(
    ctx: &serenity::Context,
    message: &serenity::Message,
) -> serenity::Result<()> {
    // Do not react to ourselves or other bots.
    if message.author.bot || message.webhook_id.is_some() {
        return Ok(());
    }

    if message.content.is_empty() {
        return Ok(());
    }

    let sanitized_urls = sanitize_message(&message.content);

    if sanitized_urls.is_empty() {
        return Ok(());
    }

    let heading = if sanitized_urls.len() == 1 {
        "**Sanitized link:**"
    } else {
        "**Sanitized links:**"
    };

    let mut response = String::from(heading);

    for sanitized in sanitized_urls {
        let removed = sanitized
            .removed_parameters
            .iter()
            .map(|parameter| format!("{parameter}"))
            .collect::<Vec<_>>()
            .join(", ");
        let link_section = format!("\n<{}>\nRemoved: {}\n", sanitized.cleaned_url, removed);

        if response.len() + link_section.len() > 1900 {
            response.push_str("\nAdditional sanitized links were excluded due to len limit.");
            break;
        }
        response.push_str(&link_section);
    }

    message.reply(&ctx.http, response).await?;
    Ok(())
}
