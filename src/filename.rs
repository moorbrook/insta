use regex::Regex;
use sha2::{Digest, Sha256};
use std::sync::LazyLock;

static RE_BAD_CHARS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"[<>:"/\\|?*]"#).unwrap());
static RE_WHITESPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

pub fn get_article_id(url: &str) -> String {
    let hash = Sha256::digest(url.as_bytes());
    hex::encode(&hash[..6]) // first 6 bytes = 12 hex chars
}

pub fn sanitize_filename(text: &str, max_length: usize) -> String {
    let safe = RE_BAD_CHARS.replace_all(text, "");
    let safe = RE_WHITESPACE.replace_all(safe.trim(), "_");
    // Unicode-safe truncation
    let truncated: String = safe.chars().take(max_length).collect();
    if truncated.is_empty() {
        "untitled".to_string()
    } else {
        truncated
    }
}

pub fn make_filename(url: &str, title: &str) -> String {
    let id = get_article_id(url);
    let safe = sanitize_filename(title, 80);
    format!("{id}_{safe}.txt")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finite_text_corpus(max_length: usize) -> Vec<String> {
        let alphabet = ['a', ' ', '\n', ':', '/', '深'];
        let mut corpus = vec![String::new()];
        let mut frontier = vec![String::new()];

        for _ in 0..max_length {
            frontier = frontier
                .iter()
                .flat_map(|prefix| {
                    alphabet.iter().map(move |character| {
                        let mut value = prefix.clone();
                        value.push(*character);
                        value
                    })
                })
                .collect();
            corpus.extend(frontier.iter().cloned());
        }

        corpus
    }

    #[test]
    fn test_article_id_deterministic() {
        let id = get_article_id("https://example.com/article");
        assert_eq!(id.len(), 12);
        assert_eq!(id, get_article_id("https://example.com/article"));
    }

    #[test]
    fn test_sanitize_removes_bad_chars() {
        assert_eq!(sanitize_filename("Hello: World?", 100), "Hello_World");
    }

    #[test]
    fn test_sanitize_truncates() {
        let long = "a".repeat(200);
        assert_eq!(sanitize_filename(&long, 80).len(), 80);
    }

    #[test]
    fn test_sanitize_empty() {
        assert_eq!(sanitize_filename("", 80), "untitled");
    }

    #[test]
    fn test_sanitize_unicode() {
        let result = sanitize_filename("深圳市達妙科技 Hello", 10);
        assert_eq!(result.chars().count(), 10);
    }

    #[test]
    fn test_make_filename() {
        let f = make_filename("https://example.com", "Test Article");
        assert!(std::path::Path::new(&f)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("txt")));
        assert!(f.contains("_Test_Article"));
    }

    #[test]
    fn sanitizer_laws_hold_for_a_finite_boundary_alphabet() {
        for input in finite_text_corpus(4) {
            for max_length in 8..=12 {
                let sanitized = sanitize_filename(&input, max_length);

                assert!(!sanitized.is_empty(), "empty result for {input:?}");
                assert_eq!(
                    sanitize_filename(&sanitized, max_length),
                    sanitized,
                    "not idempotent for {input:?}"
                );
                assert!(
                    sanitized.chars().count() <= max_length,
                    "length bound exceeded for {input:?}"
                );
                assert!(
                    !sanitized.chars().any(char::is_whitespace),
                    "whitespace survived for {input:?}"
                );
                assert!(
                    !sanitized.chars().any(|character| matches!(
                        character,
                        '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                    )),
                    "forbidden character survived for {input:?}"
                );
            }
        }
    }

    #[test]
    fn adding_forbidden_punctuation_is_a_neutral_transformation() {
        assert_eq!(
            sanitize_filename("<Hello>: \tWorld?", 80),
            sanitize_filename("Hello World", 80)
        );
    }

    #[test]
    fn article_ids_are_fixed_width_lowercase_hex() {
        for suffix in finite_text_corpus(3) {
            let id = get_article_id(&format!("https://example.com/{suffix}"));
            assert_eq!(id.len(), 12);
            assert!(id.bytes().all(|byte| byte.is_ascii_hexdigit()));
            assert_eq!(id, id.to_ascii_lowercase());
        }
    }
}
