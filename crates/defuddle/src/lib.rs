mod dom;

use serde::{Deserialize, Serialize};

pub use dom::{Document, NodeId};

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

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, thiserror::Error)]
pub enum DefuddleError {
    #[error("HTML parsing failed: {0}")]
    Parse(String),
}

pub fn parse_html(html: &str, _options: DefuddleOptions) -> Result<DefuddleResult, DefuddleError> {
    let _doc = Document::parse(html);
    Ok(DefuddleResult {
        schema_org_data: serde_json::Value::Array(Vec::new()),
        ..DefuddleResult::default()
    })
}
