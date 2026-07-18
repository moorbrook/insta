pub mod archive;
pub mod github;
pub mod instapaper;
pub mod readability;
pub mod youtube;

pub struct ExtractedArticle {
    pub title: String,
    pub content: String,
}

/// Match an exact domain or one of its subdomains.
///
/// Parsing the host avoids treating a domain mentioned in a path, query,
/// fragment, or username as the destination host.
pub(crate) fn host_is_domain_or_subdomain(url: &str, domain: &str) -> bool {
    let Ok(url) = url::Url::parse(url) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };

    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
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

#[cfg(test)]
mod tests {
    use super::host_is_domain_or_subdomain;

    #[test]
    fn host_matching_accepts_exact_domains_and_subdomains() {
        assert!(host_is_domain_or_subdomain(
            "https://youtube.com/watch?v=1",
            "youtube.com"
        ));
        assert!(host_is_domain_or_subdomain(
            "https://www.youtube.com/watch?v=1",
            "youtube.com"
        ));
        assert!(host_is_domain_or_subdomain(
            "HTTPS://WWW.YOUTUBE.COM/watch?v=1",
            "youtube.com"
        ));
    }

    #[test]
    fn host_matching_rejects_domain_text_outside_the_host() {
        for url in [
            "https://example.com/youtube.com/watch",
            "https://example.com/?next=https://youtube.com",
            "https://youtube.com@example.com/watch",
            "https://notyoutube.com/watch",
            "https://youtube.com.evil.example/watch",
            "not a url containing youtube.com",
        ] {
            assert!(
                !host_is_domain_or_subdomain(url, "youtube.com"),
                "misclassified {url}"
            );
        }
    }
}
