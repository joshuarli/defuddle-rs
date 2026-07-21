use wasm_bindgen::prelude::*;

#[derive(miniserde::Serialize)]
struct ParseResult {
    content: String,
    content_markdown: Option<String>,
    title: String,
    author: String,
    description: String,
    published: String,
    site: String,
    domain: String,
    extractor_type: Option<String>,
}

#[wasm_bindgen]
pub fn parse_page(html: &str, url: &str, extractor_name: &str) -> Result<String, JsValue> {
    let options = defuddle::DefuddleOptions {
        url: (!url.is_empty()).then(|| url.to_string()),
        separate_markdown: true,
        remove_iframes: true,
        extractor: (!extractor_name.is_empty()).then(|| extractor_name.to_string()),
        ..Default::default()
    };

    let result = defuddle::parse_html(html, options)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;

    Ok(miniserde::json::to_string(&ParseResult {
        content: result.content,
        content_markdown: result.content_markdown,
        title: result.title,
        author: result.author,
        description: result.description,
        published: result.published,
        site: result.site,
        domain: result.domain,
        extractor_type: result.extractor_type,
    }))
}

#[wasm_bindgen]
pub fn list_extractors() -> String {
    defuddle::extractors::adapter_names().join("\n")
}

#[cfg(test)]
mod tests {
    use super::parse_page;

    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn parses_minimal_html_in_wasm() {
        let html = "<html><head><title>Test</title></head><body><article><p>Readable content.</p></article></body></html>";
        let result = parse_page(html, "https://example.com/article", "").expect("parse succeeds");

        assert!(result.contains("Readable content."));
    }

    #[test]
    fn returns_extracted_html_and_markdown_as_json() {
        let html = "<html><head><title>Test</title></head><body><article><h1>Test</h1><p>A readable paragraph.</p></article></body></html>";
        let result = parse_page(html, "https://example.com/article", "").expect("parse succeeds");

        assert!(result.contains("\"content\":"));
        assert!(result.contains("\"content_markdown\":"));
        assert!(result.contains("\"title\":\"Test\""));
        assert!(!result.contains("schema_org_data"));
        assert!(!result.contains("meta_tags"));
    }
}
