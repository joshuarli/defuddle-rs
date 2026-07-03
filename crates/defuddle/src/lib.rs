//! # Defuddle
//!
//! A Rust library for extracting clean, readable content from HTML documents.
//! Defuddle identifies the main article content, strips clutter (ads, navigation,
//! sidebars), extracts metadata, and optionally converts the result to Markdown.
//!
//! This is a port of the [Defuddle](https://github.com/kepano/defuddle)
//! TypeScript library, targeting static HTML parity.
//!
//! ## Quick start
//!
//! ```rust
//! use defuddle::{DefuddleOptions, parse_html};
//!
//! let html = r#"
//! <html><head><meta property="og:title" content="Hello World"></head>
//! <body><article><h1>Hello World</h1><p>This is an article.</p></article></body>
//! </html>"#;
//!
//! let result = parse_html(html, DefuddleOptions {
//!     url: Some("https://example.com/article".into()),
//!     ..Default::default()
//! }).unwrap();
//!
//! assert_eq!(result.title, "Hello World");
//! assert_eq!(result.domain, "example.com");
//! ```
//!
//! ## Features
//!
//! - HTML parsing via `html5ever`
//! - CSS selector-based content extraction
//! - Metadata extraction (title, author, date, site, etc.)
//! - Schema.org JSON-LD support
//! - Dangerous element/attribute sanitization
//! - HTML-to-Markdown conversion via `html-to-markdown-rs`
//! - No network, TLS, proxy, or async runtime dependencies
//!
//! ## Limitations
//!
//! This crate accepts static HTML input only. It does not:
//! - Fetch URLs over HTTP
//! - Execute JavaScript
//! - Evaluate live browser CSS/layout
//! - Access shadow DOM from extension isolated worlds
//! - Support async/streaming extraction (future work)

mod constants;
mod dom;
mod markdown;
mod metadata;
mod pipeline;
pub mod sanitize;
mod schema;
mod scoring;
mod standardize;
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

    let debug = options.debug;
    let remove_exact = options.remove_exact_selectors.unwrap_or(true);
    let remove_partial = options.remove_partial_selectors.unwrap_or(true);
    let remove_hidden = options.remove_hidden_elements.unwrap_or(true);
    let do_standardize = options.standardize.unwrap_or(true);
    let content_selector = options.content_selector.as_deref();

    let result = parse_internal(
        doc,
        remove_exact,
        remove_partial,
        remove_hidden,
        do_standardize,
        content_selector,
        debug,
        &metadata,
        &meta,
        &schema_org_data,
        url,
    );

    Ok(result)
}

fn parse_internal(
    mut doc: Document,
    remove_exact: bool,
    remove_partial: bool,
    remove_hidden: bool,
    do_standardize: bool,
    content_selector: Option<&str>,
    debug: bool,
    metadata: &Metadata,
    meta: &[MetaTagItem],
    schema_org_data: &[serde_json::Value],
    _url: &str,
) -> DefuddleResult {
    if remove_exact || remove_partial {
        pipeline::remove_by_selector(&mut doc, remove_exact, remove_partial);
    }

    if remove_hidden {
        pipeline::remove_hidden_elements(&mut doc);
    }

    let main_content = pipeline::extract_content(&mut doc, content_selector, debug);
    let body_ids = doc.query_selector_all("body");
    let content_id = main_content.or_else(|| body_ids.first().copied());

    if let Some(content) = content_id {
        if do_standardize {
            standardize::standardize_content(&mut doc, content);
        }

        let html = doc.inner_html(content);
        let md = markdown::content_to_markdown(&doc, content);
        let word_count = pipeline::count_html_words(&html);

        let schema_val = if schema_org_data.is_empty() {
            serde_json::Value::Array(Vec::new())
        } else {
            serde_json::Value::Array(schema_org_data.to_vec())
        };

        DefuddleResult {
            content: html,
            content_markdown: Some(md),
            title: metadata.title.clone(),
            description: metadata.description.clone(),
            domain: metadata.domain.clone(),
            favicon: metadata.favicon.clone(),
            image: metadata.image.clone(),
            language: metadata.language.clone(),
            meta_tags: meta.to_vec(),
            published: metadata.published.clone(),
            author: metadata.author.clone(),
            site: metadata.site.clone(),
            schema_org_data: schema_val,
            word_count,
            ..DefuddleResult::default()
        }
    } else {
        let word_count = 0;
        let schema_val = if schema_org_data.is_empty() {
            serde_json::Value::Array(Vec::new())
        } else {
            serde_json::Value::Array(schema_org_data.to_vec())
        };

        DefuddleResult {
            title: metadata.title.clone(),
            description: metadata.description.clone(),
            domain: metadata.domain.clone(),
            favicon: metadata.favicon.clone(),
            image: metadata.image.clone(),
            language: metadata.language.clone(),
            meta_tags: meta.to_vec(),
            published: metadata.published.clone(),
            author: metadata.author.clone(),
            site: metadata.site.clone(),
            schema_org_data: schema_val,
            word_count,
            ..DefuddleResult::default()
        }
    }
}
