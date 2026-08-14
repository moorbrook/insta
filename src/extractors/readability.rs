use super::{article_from_html, fetch_html, ExtractedArticle};
use crate::error::ExtractError;
use std::time::Duration;

pub(crate) async fn extract(
    client: &reqwest::Client,
    url_str: &str,
    timeout: Duration,
) -> Result<Option<ExtractedArticle>, ExtractError> {
    let Some(html) = fetch_html(client, url_str, timeout).await? else {
        return Ok(None);
    };
    if html.is_empty() {
        return Ok(None);
    }
    Ok(article_from_html(&html, url_str))
}
