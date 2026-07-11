// metadata.rs
// This file contains the logic for fetching metadata from external sources (CrossRef and arXiv) and formatting it as BibTeX entries.
// It also includes functions for extracting citation sources from messages and handling Discord messages to provide BibTeX citations.

use linkify::{LinkFinder, LinkKind};
use poise::serenity_prelude as serenity;
use regex::Regex;

// CrossRef gives us metadata for anything with a DOI.
const CROSSREF_API: &str = "https://api.crossref.org/works/"; // Public endpoint

// arXiv has its own API for preprints.
const ARXIV_API: &str = "https://export.arxiv.org/api/query?id_list="; // Public endpoint

#[derive(Debug)]
enum CitationSource {
    Doi(String),
    Arxiv(String),
}

// this function scans a message for atm DOI and arXiv links then returns a vector of all citation sources.
fn find_citation_sources(message: &str) -> Vec<CitationSource> {
    let mut sources = Vec::new();

    let mut finder = LinkFinder::new();
    finder.kinds(&[LinkKind::Url]);

    let doi_path_re = Regex::new(r"doi\.org/(.+)").unwrap();
    let arxiv_re = Regex::new(r"arxiv\.org/abs/([\w.]+)").unwrap();

    for link in finder.links(message) {
        let url = link.as_str();

        if let Some(cap) = doi_path_re.captures(url) {
            sources.push(CitationSource::Doi(cap[1].to_string()));
            continue;
        }

        if let Some(cap) = arxiv_re.captures(url) {
            sources.push(CitationSource::Arxiv(cap[1].to_string()));
            continue;
        }
    }

    // catch bare DOIs (10.xxxx/...) that are not actually a link just the end part.
    let bare_doi_re = Regex::new(r"\b(10\.\d{4,}/\S+)").unwrap();
    for cap in bare_doi_re.captures_iter(message) {
        let doi = cap[1].to_string();
        // only add it if we have not yet seen it to avoid duplicates.
        let already_found = sources
            .iter()
            .any(|s| matches!(s, CitationSource::Doi(d) if d == &doi));
        if !already_found {
            sources.push(CitationSource::Doi(doi));
        }
    }

    sources
}

// This function makes a bixtex key atm from the first authors last name and year like bibtex extension does.
// if it cannot find a author it fallsback to "unknown{year}".
fn make_bibtex_key(author: &str, year: &str) -> String {
    let last_name = author
        .split(',')
        .next()
        .unwrap_or("unknown")
        .split_whitespace()
        .last()
        .unwrap_or("unknown")
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_alphabetic())
        .collect::<String>();

    format!("{}{}", last_name, year)
}

// Fetch metadata from CrossRef and format as BibTeX.
async fn doi_to_bibtex(doi: &str) -> Option<String> {
    let url = format!("{}{}", CROSSREF_API, doi);
    let client = reqwest::Client::new();

    let response = client
        .get(&url)
        .header("User-Agent", "Discomatic/1.0 (discord citation bot)")
        .send()
        .await
        .ok()?;

    let json: serde_json::Value = response.json().await.ok()?;
    let work = json.get("message")?;

    // Entry type depends on the CrossRef type field.
    let entry_type = match work["type"].as_str().unwrap_or("") {
        "journal-article" => "article",
        "proceedings-article" => "inproceedings",
        "book" => "book",
        "book-chapter" => "incollection",
        _ => "misc",
    };

    let title = work["title"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown Title")
        .to_string();

    // Authors come back as [{family: "Smith", given: "John"}] basic standard english report name formatting.
    let authors: Vec<String> = work["author"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    let family = a["family"].as_str()?;
                    let given = a["given"].as_str().unwrap_or("");
                    Some(format!("{}, {}", family, given))
                })
                .collect()
        })
        .unwrap_or_default();

    let author_str = if authors.is_empty() {
        "Unknown".to_string()
    } else {
        authors.join(" and ")
    };

    let year = work["published"]["date-parts"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|a| a.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_u64())
        .map(|y| y.to_string())
        .unwrap_or_else(|| "n.d.".to_string());

    let journal = work["container-title"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let volume = work["volume"].as_str().unwrap_or("").to_string();
    let issue = work["issue"].as_str().unwrap_or("").to_string();
    let pages = work["page"].as_str().unwrap_or("").to_string();

    let key = make_bibtex_key(
        authors.first().map(|s| s.as_str()).unwrap_or("unknown"),
        &year,
    );

    let mut bib = format!("@{}{{{},\n", entry_type, key);
    bib.push_str(&format!("  author = {{{}}},\n", author_str));
    bib.push_str(&format!("  title  = {{{}}},\n", title));
    bib.push_str(&format!("  year   = {{{}}},\n", year));

    if !journal.is_empty() {
        bib.push_str(&format!("  journal = {{{}}},\n", journal));
    }
    if !volume.is_empty() {
        bib.push_str(&format!("  volume = {{{}}},\n", volume));
    }
    if !issue.is_empty() {
        bib.push_str(&format!("  number = {{{}}},\n", issue));
    }
    if !pages.is_empty() {
        bib.push_str(&format!("  pages  = {{{}}},\n", pages));
    }
    bib.push_str(&format!("  doi    = {{{}}},\n", doi));
    bib.push('}');

    Some(bib)
}

// the function fetches metadata from the arXiv API and formats as BibTex using one rule
// arXiv preprints are always @misc with a note pointing to the abs URL.
async fn arxiv_to_bibtex(arxiv_id: &str) -> Option<String> {
    let url = format!("{}{}", ARXIV_API, arxiv_id);
    let response = reqwest::get(&url).await.ok()?;
    let body = response.text().await.ok()?;

    // arXiv gives us XML text. We will just use regular text search to find what we need.
    let title = extract_xml_tag(&body, "title")
        .into_iter()
        .nth(1) // first <title> is the feed title, second is the paper title
        .unwrap_or_else(|| "Unknown Title".to_string())
        .replace('\n', " ")
        .trim()
        .to_string();

    let authors: Vec<String> = {
        let name_re = Regex::new(r"<name>([^<]+)</name>").unwrap();
        name_re
            .captures_iter(&body)
            .map(|cap| {
                // arXiv gives "First Last", we want "Last, First" for BibTeX.
                let name = cap[1].trim();
                let parts: Vec<&str> = name.rsplitn(2, ' ').collect();
                if parts.len() == 2 {
                    format!("{}, {}", parts[0], parts[1])
                } else {
                    name.to_string()
                }
            })
            .collect()
    };

    let author_str = if authors.is_empty() {
        "Unknown".to_string()
    } else {
        authors.join(" and ")
    };

    // published date is given as year, month, day, time so we just grab the first 4 characters for the year.
    let year = extract_xml_tag(&body, "published")
        .into_iter()
        .next()
        .and_then(|d| d.get(..4).map(|s| s.to_string()))
        .unwrap_or_else(|| "n.d.".to_string());

    let key = make_bibtex_key(
        authors.first().map(|s| s.as_str()).unwrap_or("unknown"),
        &year,
    );

    let bib = format!(
        "@misc{{{key},\n  author = {{{author}}},\n  title  = {{{{{title}}}}},\n  year   = {{{year}}},\n  note   = {{arXiv:{arxiv_id}}},\n  url    = {{https://arxiv.org/abs/{arxiv_id}}},\n}}",
        key = key,
        author = author_str,
        title = title,
        year = year,
        arxiv_id = arxiv_id,
    );

    Some(bib)
}

// Pull all text contents of a given XML tag from a string.
fn extract_xml_tag(xml: &str, tag: &str) -> Vec<String> {
    let re = Regex::new(&format!(r"<{tag}[^>]*>([^<]+)</{tag}>")).unwrap();
    re.captures_iter(xml)
        .map(|cap| cap[1].trim().to_string())
        .collect()
}

// Handle an incoming Discord message, scanning for citable links and replying with BibTeX.
pub async fn handle_message(
    ctx: &serenity::Context,
    message: &serenity::Message,
) -> serenity::Result<()> {
    if message.author.bot || message.webhook_id.is_some() {
        return Ok(());
    }

    if message.content.is_empty() {
        return Ok(());
    }

    let sources = find_citation_sources(&message.content);
    if sources.is_empty() {
        return Ok(());
    }

    let heading = "**BibTeX citation:**";

    for source in sources {
        let result = match source {
            CitationSource::Doi(ref doi) => doi_to_bibtex(doi).await,
            CitationSource::Arxiv(ref id) => arxiv_to_bibtex(id).await,
        };

        let response = match result {
            Some(bib) => format!("{heading}\n```bibtex\n{bib}\n```"),
            None => {
                let label = match source {
                    CitationSource::Doi(doi) => format!("DOI `{doi}`"),
                    CitationSource::Arxiv(id) => format!("arXiv `{id}`"),
                };

                format!("Could not resolve {label}.")
            }
        };

        message.reply(&ctx.http, response).await?;
    }

    Ok(())
}
