use wasm_bindgen::prelude::*;

#[derive(miniserde::Serialize)]
struct WasmParseResult {
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
pub fn parse_page(
    page_html: &str,
    page_url: &str,
    extractor_name: &str,
) -> Result<String, JsValue> {
    let options = defuddle::DefuddleOptions {
        url: (!page_url.is_empty()).then(|| page_url.to_string()),
        separate_markdown: true,
        remove_iframes: true,
        extractor: (!extractor_name.is_empty()).then(|| extractor_name.to_string()),
        ..Default::default()
    };

    let parsed_page = defuddle::parse_html(page_html, options)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;

    Ok(miniserde::json::to_string(&WasmParseResult {
        content: parsed_page.content,
        content_markdown: parsed_page.content_markdown,
        title: parsed_page.title,
        author: parsed_page.author,
        description: parsed_page.description,
        published: parsed_page.published,
        site: parsed_page.site,
        domain: parsed_page.domain,
        extractor_type: parsed_page.extractor_type,
    }))
}

#[wasm_bindgen]
pub fn list_extractors() -> String {
    defuddle::adapter_names().join("\n")
}

#[cfg(test)]
mod tests {
    use super::parse_page;

    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn parses_minimal_html_in_wasm() {
        let page_html = "<html><head><title>Test</title></head><body><article><p>Readable content.</p></article></body></html>";
        let serialized_page =
            parse_page(page_html, "https://example.com/article", "").expect("parse succeeds");

        assert!(serialized_page.contains("Readable content."));
    }

    #[test]
    fn returns_extracted_html_and_markdown_as_json() {
        let page_html = "<html><head><title>Test</title></head><body><article><h1>Test</h1><p>A readable paragraph.</p></article></body></html>";
        let serialized_page =
            parse_page(page_html, "https://example.com/article", "").expect("parse succeeds");

        assert!(serialized_page.contains("\"content\":"));
        assert!(serialized_page.contains("\"content_markdown\":"));
        assert!(serialized_page.contains("\"title\":\"Test\""));
        assert!(!serialized_page.contains("schema_org_data"));
        assert!(!serialized_page.contains("meta_tags"));
    }
}
