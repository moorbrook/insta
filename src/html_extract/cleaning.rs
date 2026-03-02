//! HTML cleaning and boilerplate removal.
//!
//! Identifies known boilerplate elements (nav, footer, ads, cookie banners, etc.)
//! for exclusion during content extraction. Uses DOM traversal for correctness
//! with nested and malformed HTML.

use scraper::{Html, Selector};
use std::collections::HashSet;
use std::sync::LazyLock;

/// Tags that are always non-content (scripts, forms, media, etc.)
const REMOVE_TAGS: &[&str] = &[
    "script", "style", "noscript", "iframe", "object", "embed", "applet",
    "svg", "canvas", "map", "audio", "video", "source", "track",
    "input", "button", "select", "option", "textarea", "fieldset",
    "form", "label", "datalist", "output",
];

/// CSS selectors for structural boilerplate elements.
const BOILERPLATE_SELECTOR_STRS: &[&str] = &[
    "nav",
    "footer",
    "aside",
    // Site-level headers
    "header[class*='site']",
    "header[id*='site']",
    "header[class*='main']",
    "header[id*='main']",
    "header[class*='global']",
    "header[id*='global']",
    "header[class*='page']",
    "header[id*='page']",
    // Boilerplate divs by class
    "div[class*='sidebar']",
    "div[id*='sidebar']",
    "div[class*='widget']",
    "div[id*='widget']",
    "div[class*='cookie']",
    "div[id*='cookie']",
    "div[class*='consent']",
    "div[id*='consent']",
    "div[class*='banner']",
    "div[id*='banner']",
    "div[class*='modal']",
    "div[id*='modal']",
    "div[class*='popup']",
    "div[id*='popup']",
    "div[class*='overlay']",
    "div[id*='overlay']",
    "div[class*='social-share']",
    "div[class*='social_share']",
    "div[class*='sharing']",
    "div[id*='sharing']",
    "div[class*='newsletter']",
    "div[id*='newsletter']",
    "div[class*='subscribe']",
    "div[id*='subscribe']",
    "div[class*='ad-container']",
    "div[class*='ad_container']",
    "div[class*='ad-wrapper']",
    "div[class*='ad_wrapper']",
    "div[class*='advertisement']",
    "div[id*='advertisement']",
    "div[class*='sponsor']",
    "div[id*='sponsor']",
    "div[class*='promo']",
    "div[id*='promo']",
];

/// Pre-parsed selectors for non-content tags.
static REMOVE_TAG_SELECTORS: LazyLock<Vec<Selector>> = LazyLock::new(|| {
    REMOVE_TAGS
        .iter()
        .filter_map(|s| Selector::parse(s).ok())
        .collect()
});

/// Pre-parsed selectors for boilerplate elements.
static BOILERPLATE_SELECTORS: LazyLock<Vec<Selector>> = LazyLock::new(|| {
    BOILERPLATE_SELECTOR_STRS
        .iter()
        .filter_map(|s| Selector::parse(s).ok())
        .collect()
});

/// Identify all boilerplate node IDs in the document.
///
/// Returns a set of node IDs (element + all descendants) that should be
/// excluded during content extraction.
pub fn find_boilerplate_ids(doc: &Html) -> HashSet<ego_tree::NodeId> {
    let mut ids = HashSet::new();

    for sel in REMOVE_TAG_SELECTORS.iter() {
        for el in doc.select(sel) {
            add_subtree(&mut ids, &el);
        }
    }

    for sel in BOILERPLATE_SELECTORS.iter() {
        for el in doc.select(sel) {
            add_subtree(&mut ids, &el);
        }
    }

    ids
}

fn add_subtree(ids: &mut HashSet<ego_tree::NodeId>, el: &scraper::ElementRef) {
    ids.insert(el.id());
    for desc in el.descendants() {
        ids.insert(desc.id());
    }
}
