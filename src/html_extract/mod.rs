//! Multi-tier HTML content extraction pipeline.
//!
//! Ported to Rust from [trafilatura](https://github.com/adbar/trafilatura)
//! by Adrien Barbaresi, licensed under Apache 2.0.
//!
//! Extraction order:
//! 1. JSON-LD articleBody (structured data)
//! 2. Custom extraction: CSS-targeted content areas + boilerplate removal + link density filtering
//! 3. Readability fallback (Mozilla algorithm)
//! 4. Baseline fallback (all text from <body>)

mod cleaning;
mod jsonld;
mod scoring;

use cleaning::find_boilerplate_ids;
use jsonld::extract_jsonld_body;
use scoring::extract_main_content;
use std::collections::HashSet;

/// Result of HTML content extraction.
pub struct ExtractionResult {
    pub title: String,
    pub text: String,
}

/// Extract article content from HTML using a multi-tier approach.
pub fn extract(html: &str, url: &str) -> Option<ExtractionResult> {
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
            let final_title = if !candidate.title.is_empty() {
                candidate.title
            } else {
                title.clone()
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

    static TITLE_SEL: LazyLock<scraper::Selector> =
        LazyLock::new(|| scraper::Selector::parse("title").unwrap());
    static H1_SEL: LazyLock<scraper::Selector> =
        LazyLock::new(|| scraper::Selector::parse("h1").unwrap());

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

    static BODY_SEL: LazyLock<scraper::Selector> =
        LazyLock::new(|| scraper::Selector::parse("body").unwrap());

    if let Some(body) = doc.select(&BODY_SEL).next() {
        let mut parts = Vec::new();
        scoring::collect_text_excluding(&body, exclude_ids, &mut parts);
        let text: String = parts.join(" ");
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        String::new()
    }
}
