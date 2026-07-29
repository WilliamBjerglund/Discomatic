/*
src/github/requests.rs

This file is a fun project attempting to learn and use the Github API to create issues on my Discord bots repository.
The idea is that users can just type /request in Discord type in what they want and i will have it on my Github to do later.
*/

use std::env;

use poise::Modal;
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};

use crate::{ApplicationContext, Error};

const GITHUB_API_VERSION: &str = "2026-03-10";

// Labels to attach to the issue
const USER_LABEL: &str = "user-request";
const NEEDS_REVIEW: &str = "needs-review";

/// This struct is the way you make Modals using serenity.
#[derive(Modal, Debug)]
#[name = "feature_request"]
struct FeatureRequestModal {
    #[name = "Short Summary / Title"]
    #[placeholder = "A short summary of your request / bug report"]
    #[min_length = 5]
    #[max_length = 250]
    summary: String,

    #[name = "Additional Details (Optional)"]
    #[placeholder = "Any additional details you want to add, this is optional."]
    #[paragraph]
    #[max_length = 2000]
    details: Option<String>,
}

/// This is the information we want about the issue so who made the issue which serve everything.
struct RequestInfo {
    requester: String,
    guild_name: Option<String>,
    channel_id: u64,
}

/// Githubs endpoint expects a JSON body so we make one for the endpoint.
#[derive(Serialize, Debug)]
struct GithubIssue {
    title: String,
    body: String,
    labels: Vec<String>,
}

/// Now we need to do the same but for a created issue response.
#[derive(Debug, Deserialize)]
struct GithubIssueResponse {
    title: String,
    number: u64,
    html_url: String,
}

/// Finally we can make the API client we use to create our issues.
pub struct GithubClient {
    http: reqwest::Client,
    token: String,
    owner: String,
    repo: String,
}

#[rustfmt::skip]
impl GithubClient {
    /// Initializes a new GithubClient with the given token, owner, and repo.
    pub fn new() -> Result<Self, Error> {
        let token = env::var("GITHUB_TOKEN")?;
        let owner = env::var("GITHUB_OWNER")?;
        let repo = env::var("GITHUB_REPO")?;

        let mut headers = HeaderMap::new();

        // Tell Github to return JSON
        headers.insert(ACCEPT, HeaderValue::from_static("application/vnd.github+json"));
        // Tell Github which API version we want to use
        headers.insert("X-GitHub-Api-Version", HeaderValue::from_static(GITHUB_API_VERSION));

        let http = reqwest::Client::builder().default_headers(headers).user_agent("Discord Bot").build()?;

        Ok(Self {
            http,
            token,
            owner,
            repo,
        })
    }

    /// This creates a new issue on the given repository with the given title and body.
    #[rustfmt::skip]
    async fn create_issue(&self, title: &str, body: &str, labels: Vec<String>) -> Result<GithubIssueResponse, Error> {
        
        let url = format!("https://api.github.com/repos/{}/{}/issues", self.owner, self.repo);

        let issue = GithubIssue {
            title: title.to_string(),
            body: body.to_string(),
            labels,
        };

        // Send the POST request to create the issue.
        let response = self.http.post(&url).bearer_auth(&self.token).json(&issue).send().await?;
        let status = response.status();

        // If not successful, return error.
        if !status.is_success() {
        let response_body = response.text().await.unwrap_or_else(|_| "Failed to read response body".to_string());
            return Err(format!("Failed to create issue: HTTP {status} - {response_body}").into());
        }

        let issue_response = response.json::<GithubIssueResponse>().await?;
        Ok(issue_response)
    }
}

#[poise::command(slash_command, category = "Github", guild_only)]
pub async fn request(ctx: ApplicationContext<'_>) -> Result<(), Error> {
    let Some(modal) = FeatureRequestModal::execute(ctx).await? else {
        return Ok(());
    };

    ctx.defer_ephemeral().await?; // Defer the response to give us time to create the issue.

    let source = RequestInfo {
        requester: ctx.author().name.clone(),
        guild_name: ctx.guild().map(|guild| guild.name.clone()),
        channel_id: ctx.channel_id().get(),
    };

    // Create the issue on Github with the given title and body.
    let title = create_issue_title(&modal.summary);
    let body = create_issue_body(&modal.details, &source);
    let labels = vec![USER_LABEL.to_string(), NEEDS_REVIEW.to_string()];
    let result = ctx
        .data()
        .github_issues
        .create_issue(&title, &body, labels)
        .await;

    // If created succesfully, say that otherwise say that it failed.
    match result {
        Ok(issue) => {
            ctx.say(format!(
                "Your request was submitted successfully!\n\n\
                 **#{} — {}**\n\
                 {}",
                issue.number, issue.title, issue.html_url
            ))
            .await?;
        }
        Err(error) => {
            tracing::error!("Failed to create Github issue: {}", error);
            ctx.say("Failed to create your request. Please try again later or contact William.")
                .await?;
        }
    }
    Ok(())
}

/// Creates the title displayed on GitHub.
fn create_issue_title(summary: &str) -> String {
    format!("[Request] {}", summary.trim())
}

/// Prevents submitted text from mentioning GitHub users.
fn github_mentions(text: &str) -> String {
    text.replace('@', "@\u{200B}")
}

/// Creates the body displayed on GitHub.
fn create_issue_body(details: &Option<String>, source: &RequestInfo) -> String {
    let requester = github_mentions(&source.requester);
    let guild_name = source
        .guild_name
        .as_deref()
        .map(github_mentions)
        .unwrap_or_else(|| "Unknown guild".to_string());

    let details = match details {
        Some(details) => {
            let details = details.trim();
            if details.is_empty() {
                "No additional details provided.".to_string()
            } else {
                github_mentions(details)
            }
        }
        None => "No additional details provided.".to_string(),
    };

    format!(
        "## Requested feature\n\
         \n\
         {details}\n\
         \n\
         ---\n\
         \n\
         ## Request information\n\
         \n\
         - **Requested by:** {requester}\n\
         - **Server:** {guild_name}\n\
         - **Discord channel ID:** `{channel_id}`",
        channel_id = source.channel_id,
    )
}
