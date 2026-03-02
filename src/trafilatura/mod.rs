//! Rust port of trafilatura's multi-tier extraction pipeline.
//!
//! Extraction order:
//! 1. JSON-LD articleBody (structured data)
//! 2. Custom extraction: CSS-targeted content areas + boilerplate removal + link density filtering
//! 3. Readability fallback (Mozilla algorithm)
//! 4. Baseline fallback (all text from <body>)

mod cleaning;
mod jsonld;
mod scoring;

use cleaning::clean_html;
use jsonld::extract_jsonld_body;
use scoring::extract_main_content;

/// Result of trafilatura-style extraction.
pub struct ExtractionResult {
    pub title: String,
    pub text: String,
}

/// Extract article content from HTML using a multi-tier approach.
pub fn extract(html: &str, url: &str) -> Option<ExtractionResult> {
    let title = extract_title(html);

    // Tier 1: JSON-LD articleBody
    if let Some(body) = extract_jsonld_body(html) {
        if body.len() > 100 {
            return Some(ExtractionResult {
                title: title.clone(),
                text: body,
            });
        }
    }

    // Clean the HTML (remove scripts, styles, known boilerplate)
    let cleaned = clean_html(html);

    // Tier 2: Custom extraction with content scoring
    if let Some(candidate) = extract_main_content(&cleaned) {
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

    // Tier 3: Readability fallback
    if let Some(result) = try_readability(html, url) {
        return Some(result);
    }

    // Tier 4: Baseline - extract all visible text from body
    let baseline = extract_baseline(&cleaned);
    if baseline.len() > 50 {
        return Some(ExtractionResult {
            title,
            text: baseline,
        });
    }

    None
}

/// Extract title from HTML <title> tag or <h1>.
fn extract_title(html: &str) -> String {
    let doc = scraper::Html::parse_document(html);

    // Try <title> first
    let title_sel = scraper::Selector::parse("title").unwrap();
    if let Some(el) = doc.select(&title_sel).next() {
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
    let h1_sel = scraper::Selector::parse("h1").unwrap();
    if let Some(el) = doc.select(&h1_sel).next() {
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

/// Tier 4: Baseline extraction - just get all text from the body.
fn extract_baseline(html: &str) -> String {
    let doc = scraper::Html::parse_document(html);
    let body_sel = scraper::Selector::parse("body").unwrap();

    if let Some(body) = doc.select(&body_sel).next() {
        let text: String = body.text().collect::<Vec<_>>().join(" ");
        // Normalize whitespace
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        String::new()
    }
}
