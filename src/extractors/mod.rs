pub mod archive;
pub mod github;
pub mod instapaper;
pub mod readability;
pub mod youtube;

pub struct ExtractedArticle {
    pub title: String,
    pub content: String,
}

/// Maximum response body size (50 MB). Prevents memory exhaustion from hostile pages.
const MAX_BODY_BYTES: u64 = 50 * 1024 * 1024;

/// Read a response body as text with a size limit.
///
/// Rejects responses with a known Content-Length over the limit.
/// Uses reqwest's charset-aware decoding (respects Content-Type headers).
pub async fn read_body(response: reqwest::Response) -> anyhow::Result<String> {
    if let Some(len) = response.content_length() {
        if len > MAX_BODY_BYTES {
            anyhow::bail!("Response too large: {len} bytes (limit: {MAX_BODY_BYTES})");
        }
    }
    // text() respects charset from Content-Type headers
    Ok(response.text().await?)
}
