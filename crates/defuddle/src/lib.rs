mod constants;
mod dom;
mod metadata;
pub mod sanitize;
mod schema;
pub mod utils;

use serde::{Deserialize, Serialize};

pub use dom::{Document, NodeId};
pub use metadata::Metadata;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DefuddleOptions {
    pub debug: bool,
    pub url: Option<String>,
    pub markdown: bool,
    pub separate_markdown: bool,
    pub remove_exact_selectors: Option<bool>,
    pub remove_partial_selectors: Option<bool>,
    pub remove_images: bool,
    pub remove_hidden_elements: Option<bool>,
    pub remove_low_scoring: Option<bool>,
    pub remove_small_images: Option<bool>,
    pub standardize: Option<bool>,
    pub remove_content_patterns: Option<bool>,
    pub content_selector: Option<String>,
    pub language: Option<String>,
    pub profile: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MetaTagItem {
    pub name: Option<String>,
    pub property: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DebugRemoval {
    pub step: String,
    pub selector: Option<String>,
    pub reason: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DebugInfo {
    pub content_selector: String,
    pub removals: Vec<DebugRemoval>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DefuddleResult {
    pub content: String,
    pub content_markdown: Option<String>,
    pub title: String,
    pub description: String,
    pub domain: String,
    pub favicon: String,
    pub image: String,
    pub language: String,
    pub meta_tags: Vec<MetaTagItem>,
    pub parse_time: u64,
    pub published: String,
    pub author: String,
    pub site: String,
    pub schema_org_data: serde_json::Value,
    pub word_count: usize,
    pub extractor_type: Option<String>,
    pub debug: Option<DebugInfo>,
    pub variables: Option<serde_json::Map<String, serde_json::Value>>,
}

impl Default for DefuddleResult {
    fn default() -> Self {
        Self {
            content: String::new(),
            content_markdown: None,
            title: String::new(),
            description: String::new(),
            domain: String::new(),
            favicon: String::new(),
            image: String::new(),
            language: String::new(),
            meta_tags: Vec::new(),
            parse_time: 0,
            published: String::new(),
            author: String::new(),
            site: String::new(),
            schema_org_data: serde_json::Value::Array(Vec::new()),
            word_count: 0,
            extractor_type: None,
            debug: None,
            variables: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DefuddleError {
    #[error("HTML parsing failed: {0}")]
    Parse(String),
}

pub fn parse_html(html: &str, options: DefuddleOptions) -> Result<DefuddleResult, DefuddleError> {
    let doc = Document::parse(html);

    let url = options.url.as_deref().unwrap_or("");
    let schema_org_data = schema::extract_schema_org_data(&doc);
    let meta_tags_raw = schema::collect_meta_tags(&doc);

    let meta: Vec<MetaTagItem> = meta_tags_raw
        .iter()
        .map(|(name, property, content)| MetaTagItem {
            name: name.clone(),
            property: property.clone(),
            content: content.clone(),
        })
        .collect();

    let metadata = metadata::extract_metadata(&doc, &schema_org_data, &meta_tags_raw, url);

    let content = doc.outer_html(doc.root());

    let schema_val = if schema_org_data.is_empty() {
        serde_json::Value::Array(Vec::new())
    } else {
        serde_json::Value::Array(schema_org_data.clone())
    };

    let word_count = utils::count_words(&content);

    Ok(DefuddleResult {
        content,
        title: metadata.title,
        description: metadata.description,
        domain: metadata.domain,
        favicon: metadata.favicon,
        image: metadata.image,
        language: metadata.language,
        meta_tags: meta,
        published: metadata.published,
        author: metadata.author,
        site: metadata.site,
        schema_org_data: schema_val,
        word_count,
        ..DefuddleResult::default()
    })
}
