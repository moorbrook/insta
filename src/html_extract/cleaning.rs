//! HTML cleaning and boilerplate removal.
//!
//! Removes known boilerplate elements (nav, footer, ads, cookie banners, etc.)
//! before content extraction.

use regex::Regex;
use std::sync::LazyLock;

// Tags to completely remove (with all content)
const REMOVE_TAGS: &[&str] = &[
    "script", "style", "noscript", "iframe", "object", "embed", "applet",
    "svg", "canvas", "map", "audio", "video", "source", "track",
    "input", "button", "select", "option", "textarea", "fieldset",
    "form", "label", "datalist", "output",
];

/// Remove boilerplate elements from HTML, returning cleaned HTML string.
pub fn clean_html(html: &str) -> String {
    let mut result = html.to_string();

    // Remove entire tags with content
    for tag in REMOVE_TAGS {
        let re = Regex::new(&format!(r"(?si)<{tag}[\s>].*?</{tag}>")).unwrap();
        result = re.replace_all(&result, "").to_string();
    }

    // Remove HTML comments
    static COMMENT_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?s)<!--.*?-->").unwrap());
    result = COMMENT_RE.replace_all(&result, "").to_string();

    // Remove structural boilerplate tags
    static NAV_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?si)<nav[\s>].*?</nav>").unwrap());
    result = NAV_RE.replace_all(&result, "").to_string();

    static FOOTER_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?si)<footer[\s>].*?</footer>").unwrap());
    result = FOOTER_RE.replace_all(&result, "").to_string();

    static ASIDE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?si)<aside[\s>].*?</aside>").unwrap());
    result = ASIDE_RE.replace_all(&result, "").to_string();

    // Remove site-level headers (those with class/id containing site/main/global/page)
    static HEADER_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?si)<header[^>]*(?:class|id)\s*=\s*["'][^"']*(?:site|main|global|page)[^"']*["'][^>]*>.*?</header>"#).unwrap()
    });
    result = HEADER_RE.replace_all(&result, "").to_string();

    // Remove divs with boilerplate class/id names
    static BOILERPLATE_DIV_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?si)<div[^>]*(?:class|id)\s*=\s*["'][^"']*(?:sidebar|widget|cookie|consent|banner|modal|popup|overlay|social[-_]?share|sharing|newsletter|subscribe|ad[-_]container|ad[-_]wrapper|advertisement|sponsor|promo)[^"']*["'][^>]*>.*?</div>"#).unwrap()
    });
    result = BOILERPLATE_DIV_RE.replace_all(&result, "").to_string();

    result
}
