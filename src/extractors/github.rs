use super::ExtractedArticle;
use base64::Engine;
use regex::Regex;
use serde::Deserialize;
use std::sync::LazyLock;
use std::time::Duration;

static GITHUB_REPO_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://(?:www\.)?github\.com/([^/]+)/([^/]+)$").unwrap());

static GITHUB_BLOB_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"https?://(?:www\.)?github\.com/([^/]+)/([^/]+)/blob/([^/]+)/(.+)").unwrap()
});

#[derive(Deserialize)]
struct GitHubReadme {
    content: String,
}

pub fn is_github(url: &str) -> bool {
    url.contains("github.com") && !url.contains("gist.")
}

pub async fn extract(
    client: &reqwest::Client,
    url: &str,
    timeout: Duration,
) -> anyhow::Result<Option<ExtractedArticle>> {
    // Try blob path first (more specific)
    if let Some(article) = extract_blob(client, url, timeout).await? {
        return Ok(Some(article));
    }

    // Then try repo README
    if let Some(article) = extract_readme(client, url, timeout).await? {
        return Ok(Some(article));
    }

    Ok(None)
}

/// Extract a specific file from a GitHub blob URL via raw.githubusercontent.com
async fn extract_blob(
    client: &reqwest::Client,
    url: &str,
    timeout: Duration,
) -> anyhow::Result<Option<ExtractedArticle>> {
    let caps = match GITHUB_BLOB_RE.captures(url) {
        Some(c) => c,
        None => return Ok(None),
    };

    let owner = &caps[1];
    let repo = &caps[2];
    let branch = &caps[3];
    let path = &caps[4];

    // Fetch raw content from raw.githubusercontent.com
    let raw_url = format!("https://raw.githubusercontent.com/{owner}/{repo}/{branch}/{path}");

    let response = client
        .get(&raw_url)
        .timeout(timeout)
        .header("User-Agent", "Mozilla/5.0")
        .send()
        .await?;

    if !response.status().is_success() {
        return Ok(None);
    }

    let content = response.text().await?;
    if content.is_empty() {
        return Ok(None);
    }

    // Use filename as part of title
    let filename = path.rsplit('/').next().unwrap_or(path);
    let title = format!("{owner}/{repo}: {filename}");

    Ok(Some(ExtractedArticle { title, content }))
}

/// Extract README from a GitHub repo root URL via the API.
async fn extract_readme(
    client: &reqwest::Client,
    url: &str,
    timeout: Duration,
) -> anyhow::Result<Option<ExtractedArticle>> {
    let caps = match GITHUB_REPO_RE.captures(url) {
        Some(c) => c,
        None => return Ok(None),
    };

    let owner = &caps[1];
    let repo = &caps[2];
    let api_url = format!("https://api.github.com/repos/{owner}/{repo}/readme");

    let response = client
        .get(&api_url)
        .timeout(timeout)
        .header("User-Agent", "Mozilla/5.0")
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await?;

    if !response.status().is_success() {
        return Ok(None);
    }

    let readme: GitHubReadme = response.json().await?;

    let cleaned = readme.content.replace('\n', "");
    let decoded = base64::engine::general_purpose::STANDARD.decode(cleaned)?;
    let content = String::from_utf8(decoded)?;

    Ok(Some(ExtractedArticle {
        title: format!("{owner}/{repo}"),
        content,
    }))
}
