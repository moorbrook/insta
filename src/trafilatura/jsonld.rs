//! Extract article content from JSON-LD structured data (schema.org).

use serde_json::Value;

/// Try to extract articleBody from JSON-LD script tags.
pub fn extract_jsonld_body(html: &str) -> Option<String> {
    let doc = scraper::Html::parse_document(html);
    let selector = scraper::Selector::parse("script[type='application/ld+json']").unwrap();

    for script in doc.select(&selector) {
        let json_text = script.text().collect::<String>();
        if let Some(body) = parse_jsonld_article_body(&json_text) {
            return Some(body);
        }
    }
    None
}

fn parse_jsonld_article_body(json_text: &str) -> Option<String> {
    let value: Value = serde_json::from_str(json_text).ok()?;

    // Could be a single object or an array
    match &value {
        Value::Array(arr) => {
            for item in arr {
                if let Some(body) = extract_body_from_object(item) {
                    return Some(body);
                }
            }
        }
        Value::Object(_) => {
            if let Some(body) = extract_body_from_object(&value) {
                return Some(body);
            }
            // Check @graph array (common pattern)
            if let Some(Value::Array(graph)) = value.get("@graph") {
                for item in graph {
                    if let Some(body) = extract_body_from_object(item) {
                        return Some(body);
                    }
                }
            }
        }
        _ => {}
    }
    None
}

fn extract_body_from_object(obj: &Value) -> Option<String> {
    // Check for articleBody field
    if let Some(body) = obj.get("articleBody").and_then(|v| v.as_str()) {
        let cleaned = body.trim().to_string();
        if !cleaned.is_empty() {
            return Some(cleaned);
        }
    }

    // Check for text field (alternative)
    if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
        let cleaned = text.trim().to_string();
        if cleaned.len() > 200 {
            return Some(cleaned);
        }
    }

    None
}
