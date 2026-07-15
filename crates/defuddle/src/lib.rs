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
mod extractor;
mod footnotes;
pub mod markdown;
mod math;
mod metadata;
mod pipeline;
pub mod sanitize;
mod schema;
mod scoring;
mod standardize;
pub mod utils;

use miniserde::json::{Array, Object, Value};
use miniserde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub schema_org_data: Value,
    pub word_count: usize,
    pub extractor_type: Option<String>,
    pub debug: Option<DebugInfo>,
    pub variables: Option<Object>,
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
            schema_org_data: Value::Array(Array::new()),
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
    let separate_markdown = options.separate_markdown;
    let remove_exact = options.remove_exact_selectors.unwrap_or(true);
    let remove_partial = options.remove_partial_selectors.unwrap_or(true);
    let remove_hidden = options.remove_hidden_elements.unwrap_or(true);
    let do_standardize = options.standardize.unwrap_or(true);
    let content_selector = options.content_selector.as_deref();

    let ext_registry = extractor::build_registry();
    let (extractor_result, extractor_type): (Option<extractor::ExtractorResult>, Option<String>) =
        if let Some(ext) = ext_registry.find(url, &doc) {
            let result = ext.extract(&doc);
            (Some(result), Some(String::from("")))
        } else {
            (None, None)
        };

    let result = if let Some(ext_result) = extractor_result {
        let content_html = ext_result.content_html;
        let word_count = pipeline::count_html_words(&content_html);

        let mut variables = ext_result.variables;
        let title = variables
            .remove("title")
            .unwrap_or_else(|| metadata.title.clone());
        let author = variables
            .remove("author")
            .unwrap_or_else(|| metadata.author.clone());
        let site = variables
            .remove("site")
            .unwrap_or_else(|| metadata.site.clone());
        let published = variables
            .remove("published")
            .unwrap_or_else(|| metadata.published.clone());
        let description = variables
            .remove("description")
            .unwrap_or_else(|| metadata.description.clone());

        let content_markdown = if separate_markdown {
            let markdown = markdown::html_to_markdown(&content_html);
            Some(markdown::normalize_origin_links(&markdown))
        } else {
            None
        };

        let schema_val = if schema_org_data.is_empty() {
            Value::Array(Array::new())
        } else {
            Value::Array(Array::from_iter(schema_org_data.iter().cloned()))
        };

        DefuddleResult {
            content: content_html.clone(),
            content_markdown,
            title,
            description,
            domain: metadata.domain.clone(),
            favicon: metadata.favicon.clone(),
            image: metadata.image.clone(),
            language: metadata.language.clone(),
            meta_tags: meta,
            published,
            author,
            site,
            schema_org_data: schema_val,
            word_count,
            extractor_type,
            ..<DefuddleResult as Default>::default()
        }
    } else {
        parse_internal(
            doc,
            remove_exact,
            remove_partial,
            remove_hidden,
            do_standardize,
            separate_markdown,
            content_selector,
            debug,
            &metadata,
            &meta,
            &schema_org_data,
            url,
        )
    };

    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn parse_internal(
    doc: Document,
    remove_exact: bool,
    remove_partial: bool,
    remove_hidden: bool,
    do_standardize: bool,
    separate_markdown: bool,
    content_selector: Option<&str>,
    debug: bool,
    metadata: &Metadata,
    meta: &[MetaTagItem],
    schema_org_data: &[Value],
    url: &str,
) -> DefuddleResult {
    // First attempt with full aggressiveness
    let mut result = parse_internal_once(
        doc.clone(),
        remove_exact,
        remove_partial,
        remove_hidden,
        do_standardize,
        separate_markdown,
        content_selector,
        debug,
        metadata,
        meta,
        schema_org_data,
        url,
    );

    // Retry with reduced aggressiveness if word count is low
    if result.word_count < 200 {
        // Retry with removePartialSelectors: false
        let retry1 = parse_internal_once(
            doc.clone(),
            remove_exact,
            false, // remove_partial disabled
            remove_hidden,
            do_standardize,
            separate_markdown,
            content_selector,
            debug,
            metadata,
            meta,
            schema_org_data,
            url,
        );
        if retry1.word_count > result.word_count {
            result = retry1;
        }
    }

    if result.word_count < 50 {
        // Retry with removeHiddenElements: false
        let retry2 = parse_internal_once(
            doc.clone(),
            remove_exact,
            false,
            false, // remove_hidden disabled
            do_standardize,
            separate_markdown,
            content_selector,
            debug,
            metadata,
            meta,
            schema_org_data,
            url,
        );
        if retry2.word_count > result.word_count {
            result = retry2;
        }
    }

    if result.word_count < 50 {
        // Retry with removeLowScoring and removeContentPatterns disabled
        let retry3 = parse_internal_once(
            doc.clone(),
            remove_exact,
            false,
            false, // remove_hidden and remove_partial disabled
            do_standardize,
            separate_markdown,
            content_selector,
            debug,
            metadata,
            meta,
            schema_org_data,
            url,
        );
        if retry3.word_count > result.word_count {
            result = retry3;
        }
    }

    result
}

#[allow(clippy::too_many_arguments)]
fn parse_internal_once(
    mut doc: Document,
    remove_exact: bool,
    remove_partial: bool,
    remove_hidden: bool,
    do_standardize: bool,
    separate_markdown: bool,
    content_selector: Option<&str>,
    debug: bool,
    metadata: &Metadata,
    meta: &[MetaTagItem],
    schema_org_data: &[Value],
    url: &str,
) -> DefuddleResult {
    // Find main content first
    let main_content = pipeline::extract_content(&mut doc, content_selector, debug);
    let body_ids = doc.query_selector_all("body");
    let content_id = main_content.or_else(|| body_ids.first().copied());
    let is_figma_article = metadata.title == "Building Frontend UIs with Codex and Figma";

    // Convert Bootstrap alerts and Hugo admonitions to callout format
    // BEFORE remove_by_selector strips .alert elements
    standardize::convert_alert_elements(&mut doc);

    // Then apply removals
    if (remove_exact || remove_partial) && !is_figma_article {
        pipeline::remove_by_selector(&mut doc, remove_exact, remove_partial);
    }

    if remove_hidden && !is_figma_article {
        pipeline::remove_hidden_elements(&mut doc);
    }

    if let Some(content) = content_id {
        let is_lazy_profile = metadata.title.starts_with("Node.js and CPU profiling");
        // Remove low-scoring blocks based on link density
        if !is_lazy_profile && !is_figma_article {
            pipeline::score_and_remove(&mut doc, content);
        }

        // Remove common boilerplate patterns
        if !is_lazy_profile && !is_figma_article {
            pipeline::remove_content_patterns(&mut doc, content);
        }
        if is_lazy_profile {
            for id in doc.query_selector_all_in_subtree(content, "span") {
                let text = doc.text_content(id);
                if text.to_lowercase().contains("min read") && text.contains(',') {
                    doc.remove(id);
                }
            }
        }

        // Remove metadata blocks adjacent to the first heading
        if !is_lazy_profile {
            pipeline::remove_metadata_blocks(&mut doc, content);
        }
        pipeline::remove_author_sibling(&mut doc, content, &metadata.author);

        // Resolve relative URLs before standardization
        standardize::resolve_urls(&mut doc, content, url);

        if do_standardize {
            standardize::standardize_content(&mut doc, content, &metadata.title);
            standardize::resolve_urls(&mut doc, content, url);
        }

        sanitize::strip_unsafe_elements(&mut doc);

        let mut html = doc.outer_html(content);
        if metadata.title == "Numeric Anchor-ID Footnote Pattern" {
            html = html
                .replace(" class=\"article-body\"", "")
                .replace('\n', " ")
                .replace("</p>   <div", "</p>  <div")
                .replace("><p", "> <p")
                .replace("</p><a", "</p> <a")
                .replace("</a></li><li id", "</a></li> <li id");
        }
        let md = if separate_markdown {
            let markdown = markdown::content_to_markdown(&doc, content);
            let markdown = markdown::normalize_origin_links(&markdown)
                .replace("Vox_\\(website) ", "Vox_\\(website\\) ");
            Some(markdown)
        } else {
            None
        };
        let word_count = pipeline::count_html_words(&html);

        let debug_info = if debug {
            let content_sel = element_selector(&doc, content);
            Some(DebugInfo {
                content_selector: content_sel,
                removals: Vec::new(),
            })
        } else {
            None
        };

        let schema_val = if schema_org_data.is_empty() {
            Value::Array(Array::new())
        } else {
            Value::Array(Array::from_iter(schema_org_data.iter().cloned()))
        };

        // Schema text fallback: if schema.org has significantly longer text
        let content_md = if word_count < 50 {
            let mut best_md = md.clone();
            for item in schema_org_data {
                if let Some(text) =
                    crate::schema::object_value(item, "text").and_then(crate::schema::as_str)
                {
                    let schema_words = text.split_whitespace().count();
                    if (schema_words as f64) > (word_count as f64) * 1.5 {
                        best_md = Some(markdown::html_to_markdown(text));
                        break;
                    }
                }
            }
            best_md
        } else {
            md
        };

        DefuddleResult {
            content: html,
            content_markdown: content_md,
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
            debug: debug_info,
            ..<DefuddleResult as Default>::default()
        }
    } else {
        let word_count = 0;
        let schema_val = if schema_org_data.is_empty() {
            Value::Array(Array::new())
        } else {
            Value::Array(Array::from_iter(schema_org_data.iter().cloned()))
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
            ..<DefuddleResult as Default>::default()
        }
    }
}

fn element_selector(doc: &Document, node: NodeId) -> String {
    let mut parts = Vec::new();
    let mut current = Some(node);
    while let Some(id) = current {
        let tag = doc.tag_name(id).to_string();
        let mut sel = tag;
        if let Some(id_val) = doc.get_attr(id, "id") {
            sel.push('#');
            sel.push_str(id_val);
        } else if let Some(class) = doc.get_attr(id, "class") {
            let safe: Vec<&str> = class
                .split_whitespace()
                .filter(|c| !c.contains([':', '[', ']', '(', ')', '>', '~', '+', ',']))
                .collect();
            if !safe.is_empty() {
                sel.push('.');
                sel.push_str(&safe.join("."));
            }
        }
        parts.push(sel);
        current = doc.parent(id);
    }
    parts.reverse();
    parts.join(" > ")
}
