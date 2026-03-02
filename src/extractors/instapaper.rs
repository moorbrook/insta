//! Retrieve article text via the Instapaper Full API.
//!
//! Uses OAuth 1.0a authentication and the /api/1/bookmarks/get_text endpoint
//! to fetch the permanently archived article content that Instapaper stored
//! at save time. Requires a Premium subscription.
//!
//! API docs: https://www.instapaper.com/api/full
//!
//! TODO: Implement OAuth xAuth token exchange (insta login)
//! TODO: Implement bookmarks/list to build URL -> bookmark_id mapping
//! TODO: Implement get_text(bookmark_id) to fetch archived content

use super::ExtractedArticle;
use std::time::Duration;

/// Retrieve article text from Instapaper's permanent archive via API.
///
/// Requires a valid OAuth token (obtained via `insta login`) and a
/// bookmark_id (resolved from URL via bookmarks/list).
///
/// Returns None until the API client is implemented.
pub async fn extract(
    _client: &reqwest::Client,
    _url: &str,
    _timeout: Duration,
) -> anyhow::Result<Option<ExtractedArticle>> {
    // TODO: Look up bookmark_id for this URL from cached mapping
    // TODO: Call /api/1/bookmarks/get_text with OAuth signature
    // TODO: Run returned HTML through crate::html_extract::extract()
    Ok(None)
}
