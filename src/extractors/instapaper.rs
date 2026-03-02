//! Extract article text from Instapaper's text cache.
//!
//! Instapaper caches a clean text version of every saved article.
//! We can fetch it from their public text endpoint.

use super::ExtractedArticle;
use std::time::Duration;

const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

pub async fn extract(
    client: &reqwest::Client,
    url: &str,
    timeout: Duration,
) -> anyhow::Result<Option<ExtractedArticle>> {
    let encoded_url = urlencoding::encode(url);
    let instapaper_url = format!("https://www.instapaper.com/text?u={encoded_url}");

    let response = client
        .get(&instapaper_url)
        .timeout(timeout)
        .header("User-Agent", USER_AGENT)
        .send()
        .await?;

    if !response.status().is_success() {
        return Ok(None);
    }

    let html = response.text().await?;
    if html.is_empty() {
        return Ok(None);
    }

    // Instapaper returns a clean HTML page - extract with our trafilatura pipeline
    match crate::trafilatura::extract(&html, &instapaper_url) {
        Some(result) => {
            if result.text.len() < 50 {
                return Ok(None);
            }
            Ok(Some(ExtractedArticle {
                title: result.title,
                content: result.text,
            }))
        }
        None => Ok(None),
    }
}
