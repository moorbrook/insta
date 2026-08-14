//! Multi-tier HTML content extraction pipeline.
//!
//! Ported to Rust from [trafilatura](https://github.com/adbar/trafilatura)
//! by Adrien Barbaresi, licensed under Apache 2.0.
//!
//! Extraction order:
//! 1. JSON-LD articleBody (structured data)
//! 2. Custom extraction: CSS-targeted content areas + boilerplate removal + link density filtering
//! 3. Readability fallback (Mozilla algorithm)
//! 4. Baseline fallback (all text from `<body>`)

mod cleaning;
mod jsonld;
mod scoring;

use cleaning::find_boilerplate_ids;
use jsonld::extract_jsonld_body;
use scoring::extract_main_content;
use std::collections::HashSet;

/// Result of HTML content extraction.
#[derive(Debug)]
pub(crate) struct ExtractionResult {
    pub title: String,
    pub text: String,
}

/// Extract article content from HTML using a multi-tier approach.
pub(crate) fn extract(html: &str, url: &str) -> Option<ExtractionResult> {
    // Parse HTML once for all tiers
    let doc = scraper::Html::parse_document(html);

    let title = extract_title(&doc);

    // Tier 1: JSON-LD articleBody
    if let Some(body) = extract_jsonld_body(&doc) {
        if body.len() > 100 {
            return Some(ExtractionResult {
                title: title.clone(),
                text: body,
            });
        }
    }

    // Build boilerplate exclusion set (DOM-based, replaces regex cleaning)
    let boilerplate_ids = find_boilerplate_ids(&doc);

    // Tier 2: Custom extraction with content scoring
    if let Some(candidate) = extract_main_content(&doc, &boilerplate_ids) {
        if candidate.text.len() > 50 {
            let final_title = if candidate.title.is_empty() {
                title.clone()
            } else {
                candidate.title
            };
            return Some(ExtractionResult {
                title: final_title,
                text: candidate.text,
            });
        }
    }

    // Tier 3: Readability fallback (uses its own HTML parser)
    if let Some(result) = try_readability(html, url) {
        return Some(result);
    }

    // Tier 4: Baseline - extract all visible text from body, excluding boilerplate
    let baseline = extract_baseline(&doc, &boilerplate_ids);
    if baseline.len() > 50 {
        return Some(ExtractionResult {
            title,
            text: baseline,
        });
    }

    None
}

/// Extract title from parsed HTML document.
fn extract_title(doc: &scraper::Html) -> String {
    use std::sync::LazyLock;

    static TITLE_SEL: LazyLock<scraper::Selector> = LazyLock::new(|| {
        #[allow(clippy::unwrap_used, reason = "hardcoded CSS selector must parse")]
        scraper::Selector::parse("title").unwrap()
    });
    static H1_SEL: LazyLock<scraper::Selector> = LazyLock::new(|| {
        #[allow(clippy::unwrap_used, reason = "hardcoded CSS selector must parse")]
        scraper::Selector::parse("h1").unwrap()
    });

    // Try <title> first
    if let Some(el) = doc.select(&TITLE_SEL).next() {
        let title = el.text().collect::<String>().trim().to_string();
        if !title.is_empty() {
            // Clean up common title patterns: "Article Title | Site Name" -> "Article Title"
            let cleaned = title
                .split(" | ")
                .next()
                .unwrap_or(&title)
                .split(" - ")
                .next()
                .unwrap_or(&title)
                .split(" — ")
                .next()
                .unwrap_or(&title)
                .trim()
                .to_string();
            if !cleaned.is_empty() {
                return cleaned;
            }
        }
    }

    // Try <h1>
    if let Some(el) = doc.select(&H1_SEL).next() {
        let h1 = el.text().collect::<String>().trim().to_string();
        if !h1.is_empty() {
            return h1;
        }
    }

    "Untitled".to_string()
}

/// Tier 3: Try readability crate as fallback.
fn try_readability(html: &str, url_str: &str) -> Option<ExtractionResult> {
    use std::io::Cursor;
    let parsed_url = url::Url::parse(url_str).ok()?;
    let mut cursor = Cursor::new(html.as_bytes());
    let product = readability::extractor::extract(&mut cursor, &parsed_url).ok()?;
    let text = product.text.trim().to_string();
    if text.is_empty() {
        return None;
    }
    let title = if product.title.is_empty() {
        "Untitled".to_string()
    } else {
        product.title
    };
    Some(ExtractionResult { title, text })
}

/// Tier 4: Baseline extraction - get all visible text from body, excluding boilerplate.
fn extract_baseline(doc: &scraper::Html, exclude_ids: &HashSet<ego_tree::NodeId>) -> String {
    use std::sync::LazyLock;

    static BODY_SEL: LazyLock<scraper::Selector> = LazyLock::new(|| {
        #[allow(clippy::unwrap_used, reason = "hardcoded CSS selector must parse")]
        scraper::Selector::parse("body").unwrap()
    });

    if let Some(body) = doc.select(&BODY_SEL).next() {
        let mut parts = Vec::new();
        scoring::collect_text_excluding(&body, exclude_ids, &mut parts);
        let text: String = parts.join(" ");
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::print_stdout,
        clippy::print_stderr
    )]

    use super::*;

    const PARAGRAPHS: &str = r"
        <p>Rust makes important state transitions explicit and reviewable.</p>
        <p>Independent tests should try to falsify each consequential invariant.</p>
        <p>Bounded concurrency keeps resource use predictable under large inputs.</p>
        <p>Clear failure semantics keep missing data distinct from broken storage.</p>
    ";

    fn article_document(inserted_inside_article: &str, inserted_outside_article: &str) -> String {
        format!(
            r"
            <html>
              <head><title>Evidence First | Example Site</title></head>
              <body>
                {inserted_outside_article}
                <article>
                  <h1>Evidence First</h1>
                  {inserted_inside_article}
                  {PARAGRAPHS}
                </article>
              </body>
            </html>
            "
        )
    }

    #[test]
    fn inserting_boilerplate_is_a_neutral_transformation() {
        let base = extract(&article_document("", ""), "https://example.com/article").unwrap();
        let with_boilerplate = extract(
            &article_document(
                "<aside>INSIDE NOISE</aside><div class='newsletter'>MORE NOISE</div>",
                "<nav>OUTSIDE NOISE</nav><footer>FOOTER NOISE</footer>",
            ),
            "https://example.com/article",
        )
        .unwrap();

        assert_eq!(with_boilerplate.title, base.title);
        assert_eq!(with_boilerplate.text, base.text);
        assert!(!with_boilerplate.text.contains("NOISE"));
    }

    #[test]
    fn json_ld_article_body_takes_precedence_over_html_candidates() {
        let structured_body = "Structured evidence wins over fallback content. ".repeat(4);
        let html = format!(
            r#"
            <html>
              <head>
                <title>Structured Title | Site Name</title>
                <script type="application/ld+json">
                  {{"@type":"Article","articleBody":{structured_body:?}}}
                </script>
              </head>
              <body><article><p>{PARAGRAPHS}</p></article></body>
            </html>
            "#
        );

        let result = extract(&html, "https://example.com/structured").unwrap();
        assert_eq!(result.title, "Structured Title");
        assert_eq!(result.text, structured_body.trim());
        assert!(!result.text.contains("Bounded concurrency"));
    }

    #[test]
    fn malformed_json_ld_falls_back_without_leaking_script_text() {
        let html = article_document(
            r#"<script type="application/ld+json">{"articleBody": invalid}</script>"#,
            "",
        );

        let result = extract(&html, "https://example.com/fallback").unwrap();
        assert_eq!(result.title, "Evidence First");
        assert!(result.text.contains("Bounded concurrency"));
        assert!(!result.text.contains("articleBody"));
        assert!(!result.text.contains("invalid"));
    }

    #[test]
    fn title_site_suffixes_have_the_same_article_title() {
        for separator in [" | ", " - ", " — "] {
            let html = article_document("", "").replace(
                "Evidence First | Example Site",
                &format!("Evidence First{separator}Example Site"),
            );
            let result = extract(&html, "https://example.com/title").unwrap();
            assert_eq!(result.title, "Evidence First");
        }
    }
}
