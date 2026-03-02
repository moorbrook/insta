//! Content extraction with scoring heuristics.
//!
//! Identifies main content areas using CSS selectors targeting common content
//! containers, then scores candidates by text density vs link density.
//! Ported from trafilatura's main_extractor.py logic.

use scraper::{Html, Selector};
use std::collections::HashSet;

pub struct ContentCandidate {
    pub title: String,
    pub text: String,
}

/// CSS selectors for likely content containers, in priority order.
/// Based on trafilatura's BODY_XPATH patterns.
const CONTENT_SELECTORS: &[&str] = &[
    // High confidence: specific article content containers
    "article",
    "[itemprop='articleBody']",
    "[class*='article-body']",
    "[class*='article-content']",
    "[class*='article_body']",
    "[class*='article_content']",
    "[class*='post-content']",
    "[class*='post-body']",
    "[class*='post_content']",
    "[class*='post_body']",
    "[class*='entry-content']",
    "[class*='entry-body']",
    "[class*='entry_content']",
    "[class*='story-body']",
    "[class*='story-content']",
    "[class*='blog-post']",
    "[class*='blog-content']",
    // Medium confidence: general content areas
    "[role='main']",
    "main",
    "[id='content']",
    "[id='main-content']",
    "[id='main_content']",
    "[id='article']",
    "[id='post']",
    "[class*='content-area']",
    "[class*='main-content']",
    "[class*='page-content']",
    "[class*='single-content']",
    "[class='content']",
    "[class='post']",
    "[class='text']",
    "[class='body']",
    // Lower confidence: broader containers
    ".hentry",
    "[class*='wysiwyg']",
    "[class*='markdown']",
    "[class*='prose']",
    "[class*='rich-text']",
];

/// Selectors for elements that should be excluded from text extraction
/// even within content areas (trafilatura's prune_unwanted_sections).
const UNWANTED_WITHIN_CONTENT: &[&str] = &[
    "nav",
    "footer",
    "aside",
    "[class*='sidebar']",
    "[class*='share']",
    "[class*='social']",
    "[class*='related']",
    "[class*='comment']",
    "[class*='author-bio']",
    "[class*='newsletter']",
    "[class*='subscribe']",
    "[class*='advertisement']",
    "[class*='promo']",
    "[class*='recommendation']",
    "[class*='trending']",
    "[class*='popular']",
    "[class*='breadcrumb']",
    "[class*='pagination']",
    "figcaption",
];

/// Extract main content from cleaned HTML.
pub fn extract_main_content(html: &str) -> Option<ContentCandidate> {
    let doc = Html::parse_document(html);

    // Build set of unwanted node IDs (pre-compute once)
    let unwanted_ids = build_unwanted_ids(&doc);

    // Try each content selector in priority order
    let mut best: Option<(String, f64)> = None;
    let mut best_title = String::new();

    for &selector_str in CONTENT_SELECTORS {
        let selector = match Selector::parse(selector_str) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for element in doc.select(&selector) {
            let text = extract_clean_text(&element, &unwanted_ids);
            if text.len() < 50 {
                continue;
            }

            let score = score_content(&element, &text);

            if let Some((ref best_text, best_score)) = best {
                if score > best_score
                    || (score > best_score * 0.9 && text.len() > best_text.len() * 2)
                {
                    best_title = extract_content_title(&element);
                    best = Some((text, score));
                }
            } else {
                best_title = extract_content_title(&element);
                best = Some((text, score));
            }
        }
    }

    // If no content selector matched, try extracting from <body> with scoring
    if best.is_none() {
        let body_sel = Selector::parse("body").unwrap();
        if let Some(body) = doc.select(&body_sel).next() {
            let text = extract_clean_text(&body, &unwanted_ids);
            if text.len() > 100 {
                let score = score_content(&body, &text);
                best = Some((text, score));
            }
        }
    }

    best.map(|(text, _)| ContentCandidate {
        title: best_title,
        text,
    })
}

/// Build a set of node IDs that should be excluded from content.
fn build_unwanted_ids(doc: &Html) -> HashSet<ego_tree::NodeId> {
    let mut ids = HashSet::new();
    for selector_str in UNWANTED_WITHIN_CONTENT {
        if let Ok(sel) = Selector::parse(selector_str) {
            for el in doc.select(&sel) {
                // Add this element and all its descendants
                ids.insert(el.id());
                for desc in el.descendants() {
                    ids.insert(desc.id());
                }
            }
        }
    }
    ids
}

/// Extract text from an element, excluding unwanted sub-elements.
fn extract_clean_text(element: &scraper::ElementRef, exclude_ids: &HashSet<ego_tree::NodeId>) -> String {
    let mut text_parts = Vec::new();
    collect_text_excluding(element, exclude_ids, &mut text_parts);
    let raw = text_parts.join(" ");
    normalize_text(&raw)
}

/// Recursively collect text, skipping nodes in the exclusion set.
fn collect_text_excluding(
    element: &scraper::ElementRef,
    exclude_ids: &HashSet<ego_tree::NodeId>,
    parts: &mut Vec<String>,
) {
    for child in element.children() {
        if exclude_ids.contains(&child.id()) {
            continue;
        }

        match child.value() {
            scraper::node::Node::Text(text) => {
                let t = text.text.trim();
                if !t.is_empty() {
                    parts.push(t.to_string());
                }
            }
            scraper::node::Node::Element(el) => {
                if let Some(child_ref) = scraper::ElementRef::wrap(child) {
                    let tag = el.name();
                    let is_block = matches!(
                        tag,
                        "p" | "div" | "br" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                            | "blockquote" | "pre" | "li" | "tr" | "section" | "article"
                    );
                    if is_block && !parts.is_empty() {
                        parts.push("\n".to_string());
                    }
                    collect_text_excluding(&child_ref, exclude_ids, parts);
                    if is_block {
                        parts.push("\n".to_string());
                    }
                }
            }
            _ => {}
        }
    }
}

/// Score a content candidate based on text quality heuristics.
fn score_content(element: &scraper::ElementRef, text: &str) -> f64 {
    let text_len = text.len() as f64;
    if text_len == 0.0 {
        return 0.0;
    }

    // Base score: text length (more text = likely more content)
    let mut score = text_len.ln();

    // Link density penalty
    let link_density = calculate_link_density(element);
    if link_density > 0.5 {
        score *= 0.1;
    } else if link_density > 0.3 {
        score *= 0.5;
    } else if link_density > 0.1 {
        score *= 0.8;
    }

    // Paragraph density bonus
    let p_sel = Selector::parse("p").unwrap();
    let p_count = element.select(&p_sel).count() as f64;
    if p_count > 3.0 {
        score *= 1.0 + (p_count.ln() * 0.2);
    }

    // Heading bonus
    let heading_sel = Selector::parse("h1, h2, h3").unwrap();
    let heading_count = element.select(&heading_sel).count();
    if heading_count > 0 && heading_count < 10 {
        score *= 1.1;
    }

    // Penalty for too many list items (likely navigation)
    let li_sel = Selector::parse("li").unwrap();
    let li_count = element.select(&li_sel).count() as f64;
    if li_count > 0.0 && p_count > 0.0 && li_count / p_count > 5.0 {
        score *= 0.5;
    }

    // Word count quality check
    let word_count = text.split_whitespace().count() as f64;
    let avg_word_len = if word_count > 0.0 {
        text_len / word_count
    } else {
        0.0
    };
    if avg_word_len < 3.0 {
        score *= 0.5;
    }

    score
}

/// Calculate ratio of link text to total text within an element.
fn calculate_link_density(element: &scraper::ElementRef) -> f64 {
    let total_text: String = element.text().collect();
    let total_len = total_text.trim().len() as f64;
    if total_len == 0.0 {
        return 1.0;
    }

    let a_sel = Selector::parse("a").unwrap();
    let link_text_len: usize = element
        .select(&a_sel)
        .map(|a| a.text().collect::<String>().trim().len())
        .sum();

    link_text_len as f64 / total_len
}

/// Try to extract a title from within the content element.
fn extract_content_title(element: &scraper::ElementRef) -> String {
    let h1_sel = Selector::parse("h1").unwrap();
    if let Some(h1) = element.select(&h1_sel).next() {
        let title = h1.text().collect::<String>().trim().to_string();
        if !title.is_empty() {
            return title;
        }
    }
    String::new()
}

/// Normalize text: collapse whitespace, preserve paragraph structure.
fn normalize_text(raw: &str) -> String {
    let mut result = String::with_capacity(raw.len());
    let mut prev_newline = false;

    for line in raw.split('\n') {
        let trimmed: String = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if trimmed.is_empty() {
            if !prev_newline && !result.is_empty() {
                result.push('\n');
                prev_newline = true;
            }
        } else {
            if !result.is_empty() && !prev_newline {
                result.push('\n');
            }
            result.push_str(&trimmed);
            prev_newline = false;
        }
    }

    result.trim().to_string()
}
