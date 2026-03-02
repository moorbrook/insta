use super::ExtractedArticle;
use std::time::Duration;

const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

pub async fn extract(
    client: &reqwest::Client,
    url_str: &str,
    timeout: Duration,
) -> anyhow::Result<Option<ExtractedArticle>> {
    let response = client
        .get(url_str)
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

    // Use our trafilatura-style multi-tier extraction
    match crate::trafilatura::extract(&html, url_str) {
        Some(result) => {
            let title = if result.title.is_empty() || result.title == "Untitled" {
                "Untitled".to_string()
            } else {
                result.title
            };
            Ok(Some(ExtractedArticle {
                title,
                content: result.text,
            }))
        }
        None => Ok(None),
    }
}
