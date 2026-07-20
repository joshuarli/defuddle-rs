use wasm_bindgen::prelude::*;

#[derive(miniserde::Serialize)]
struct ParseResult {
    content: String,
    content_markdown: Option<String>,
    title: String,
    word_count: usize,
}

#[wasm_bindgen]
pub fn parse_page(html: &str, url: &str) -> Result<String, JsValue> {
    let options = defuddle::DefuddleOptions {
        url: (!url.is_empty()).then(|| url.to_string()),
        separate_markdown: true,
        remove_iframes: true,
        ..Default::default()
    };

    let result = defuddle::parse_html(html, options)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;

    Ok(miniserde::json::to_string(&ParseResult {
        content: result.content,
        content_markdown: result.content_markdown,
        title: result.title,
        word_count: result.word_count,
    }))
}

#[cfg(test)]
mod tests {
    use super::parse_page;

    #[test]
    fn returns_extracted_html_and_markdown_as_json() {
        let html = "<html><head><title>Test</title></head><body><article><h1>Test</h1><p>A readable paragraph.</p></article></body></html>";
        let result = parse_page(html, "https://example.com/article").expect("parse succeeds");

        assert!(result.contains("\"content\":"));
        assert!(result.contains("\"content_markdown\":"));
        assert!(result.contains("\"title\":\"Test\""));
        assert!(result.contains("\"word_count\":"));
        assert!(!result.contains("schema_org_data"));
        assert!(!result.contains("meta_tags"));
    }
}
