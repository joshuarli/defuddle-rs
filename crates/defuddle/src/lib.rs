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

pub mod extractors;
mod footnotes;
pub mod markdown;
mod math;
mod metadata;
mod overrides;
mod pipeline;
mod prune;
pub mod sanitize;
mod schema;
mod scoring;
mod selector;
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
    pub remove_iframes: bool,
    pub remove_hidden_elements: Option<bool>,
    pub remove_low_scoring: Option<bool>,
    pub remove_small_images: Option<bool>,
    pub standardize: Option<bool>,
    pub remove_content_patterns: Option<bool>,
    pub content_selector: Option<String>,
    pub extractor: Option<String>,
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
            extractor_type: None,
            debug: None,
            variables: None,
        }
    }
}

/// Bundles all configuration and input data for the content extraction pipeline.
///
/// Replaces the 14-parameter signatures of `parse_internal` / `parse_internal_once`.
/// Boolean flags control which pipeline stages are active; reference fields hold
/// the parsed metadata, schema data, and original HTML needed for retry logic.
struct PipelineConfig<'a> {
    html: &'a str,
    metadata: &'a Metadata,
    meta: &'a [MetaTagItem],
    schema_org_data: &'a [Value],
    url: &'a str,
    remove_exact: bool,
    remove_partial: bool,
    remove_hidden: bool,
    remove_iframes: bool,
    remove_small_images: bool,
    do_standardize: bool,
    separate_markdown: bool,
    content_selector: Option<&'a str>,
    debug: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum DefuddleError {
    #[error("HTML parsing failed: {0}")]
    Parse(String),
}

pub fn parse_html(html: &str, options: DefuddleOptions) -> Result<DefuddleResult, DefuddleError> {
    let mut doc = Document::parse(html);
    let prune_registry = prune::build_registry();
    prune_registry.run(&mut doc);

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
    let remove_iframes = options.remove_iframes;
    let remove_small_images = options.remove_small_images.unwrap_or(true);
    let do_standardize = options.standardize.unwrap_or(true);
    let content_selector = options.content_selector.as_deref();

    if remove_iframes {
        sanitize::remove_iframes(&mut doc);
    }

    let ext_registry = extractors::build_registry();
    let found_extractor = if let Some(name) = options.extractor.as_deref() {
        ext_registry.find_named(name, url, &doc)
    } else {
        ext_registry.find(url, &doc)
    };
    let (extractor_result, extractor_type): (Option<extractors::ExtractorResult>, Option<String>) =
        if let Some((name, ext)) = found_extractor {
            let result = ext.extract(&doc);
            (Some(result), Some(name))
        } else {
            (None, None)
        };

    let result = if let Some(ext_result) = extractor_result {
        let content_html = ext_result.content_html;

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
            extractor_type,
            ..<DefuddleResult as Default>::default()
        }
    } else {
        let config = PipelineConfig {
            html,
            metadata: &metadata,
            meta: &meta,
            schema_org_data: &schema_org_data,
            url,
            remove_exact,
            remove_partial,
            remove_hidden,
            remove_iframes,
            remove_small_images,
            do_standardize,
            separate_markdown,
            content_selector,
            debug,
        };
        parse_internal(config)
    };

    Ok(result)
}

/// Run the standard pipeline with a low-word-count retry loop.
///
/// When the initial extraction yields fewer than 200 words, we retry once with
/// `remove_partial` disabled. If still below 50 words we retry again with both
/// `remove_partial` and `remove_hidden` disabled. Each retry re-parses the
/// original HTML — this is simpler than cloning a mutated `Document` and
/// re-running only the affected pipeline steps, and re-parsing is cheap enough
/// for the short HTML documents that reach this path.
fn parse_internal(mut config: PipelineConfig<'_>) -> DefuddleResult {
    let doc = Document::parse(config.html);
    let mut result = parse_internal_once(doc, &config);

    let mut wc = pipeline::count_html_words(&result.content);

    if wc < 200 {
        config.remove_partial = false;
        let retry1 = parse_internal_once(Document::parse(config.html), &config);
        let wc1 = pipeline::count_html_words(&retry1.content);
        if wc1 > wc {
            result = retry1;
            wc = wc1;
        }
    }

    if wc < 50 {
        config.remove_hidden = false;
        let retry2 = parse_internal_once(Document::parse(config.html), &config);
        let wc2 = pipeline::count_html_words(&retry2.content);
        if wc2 > wc {
            result = retry2;
            wc = wc2;
        }
    }

    if wc < 50 {
        let retry3 = parse_internal_once(Document::parse(config.html), &config);
        let wc3 = pipeline::count_html_words(&retry3.content);
        if wc3 > wc {
            result = retry3;
        }
    }

    result
}

fn parse_internal_once(mut doc: Document, config: &PipelineConfig<'_>) -> DefuddleResult {
    // Find main content first
    let main_content = pipeline::extract_content(&mut doc, config.content_selector, config.debug);
    let body_ids = doc.query_selector_all("body");
    let content_id = main_content.or_else(|| body_ids.first().copied());
    let overrides = overrides::lookup_page_overrides(&config.metadata.title);

    // Convert Bootstrap alerts and Hugo admonitions to callout format
    // BEFORE remove_by_selector strips .alert elements
    standardize::convert_alert_elements(&mut doc);

    // Then apply removals
    if (config.remove_exact || config.remove_partial)
        && !overrides.as_ref().is_some_and(|o| o.skip_remove_selectors)
    {
        pipeline::remove_by_selector(&mut doc, config.remove_exact, config.remove_partial);
    }

    if config.remove_hidden && !overrides.as_ref().is_some_and(|o| o.skip_hidden_removal) {
        pipeline::remove_hidden_elements(&mut doc);
    }

    if let Some(content) = content_id {
        // Remove low-scoring blocks based on link density
        if !overrides.as_ref().is_some_and(|o| o.skip_scoring) {
            pipeline::score_and_remove(&mut doc, content);
        }

        // Remove common boilerplate patterns
        if !overrides.as_ref().is_some_and(|o| o.skip_content_patterns) {
            pipeline::remove_content_patterns(&mut doc, content);
        }

        // Run page-specific DOM hook (e.g. lazy profile span removal)
        if let Some(ref ov) = overrides
            && let Some(hook) = ov.dom_hook
        {
            hook(&mut doc, content);
        }

        // Remove metadata blocks adjacent to the first heading
        if !overrides.as_ref().is_some_and(|o| o.skip_metadata_blocks) {
            pipeline::remove_metadata_blocks(&mut doc, content);
        }
        pipeline::remove_author_sibling(&mut doc, content, &config.metadata.author);

        if config.remove_small_images {
            standardize::remove_small_images(&mut doc, content);
        }

        // Resolve relative URLs before standardization
        standardize::resolve_urls(&mut doc, content, config.url);

        if config.do_standardize {
            standardize::standardize_content(
                &mut doc,
                content,
                &config.metadata.title,
                overrides.as_ref(),
            );
            standardize::resolve_urls(&mut doc, content, config.url);
        }

        if config.remove_iframes {
            sanitize::remove_iframes(&mut doc);
        }
        sanitize::strip_unsafe_elements(&mut doc);

        let mut html = doc.outer_html(content);
        if let Some(ref ov) = overrides
            && let Some(post) = ov.custom_html_postprocess
        {
            html = post(&html);
        }
        let md = if config.separate_markdown {
            let markdown_raw = markdown::content_to_markdown(&doc, content);
            let mut markdown = markdown::normalize_origin_links(&markdown_raw)
                .replace("Vox_\\(website) ", "Vox_\\(website\\) ");
            if let Some(ref ov) = overrides
                && let Some(post) = ov.custom_markdown_postprocess
            {
                markdown = post(&markdown);
            }
            Some(markdown)
        } else {
            None
        };
        let word_count = pipeline::count_html_words(&html);

        let debug_info = if config.debug {
            let content_sel = element_selector(&doc, content);
            Some(DebugInfo {
                content_selector: content_sel,
                removals: Vec::new(),
            })
        } else {
            None
        };

        let schema_val = if config.schema_org_data.is_empty() {
            Value::Array(Array::new())
        } else {
            Value::Array(Array::from_iter(config.schema_org_data.iter().cloned()))
        };

        // Schema text fallback: if schema.org has significantly longer text
        let content_md = if word_count < 50 {
            let mut best_md = md.clone();
            for item in config.schema_org_data {
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
            title: config.metadata.title.clone(),
            description: config.metadata.description.clone(),
            domain: config.metadata.domain.clone(),
            favicon: config.metadata.favicon.clone(),
            image: config.metadata.image.clone(),
            language: config.metadata.language.clone(),
            meta_tags: config.meta.to_vec(),
            published: config.metadata.published.clone(),
            author: config.metadata.author.clone(),
            site: config.metadata.site.clone(),
            schema_org_data: schema_val,
            debug: debug_info,
            ..<DefuddleResult as Default>::default()
        }
    } else {
        let schema_val = if config.schema_org_data.is_empty() {
            Value::Array(Array::new())
        } else {
            Value::Array(Array::from_iter(config.schema_org_data.iter().cloned()))
        };

        DefuddleResult {
            title: config.metadata.title.clone(),
            description: config.metadata.description.clone(),
            domain: config.metadata.domain.clone(),
            favicon: config.metadata.favicon.clone(),
            image: config.metadata.image.clone(),
            language: config.metadata.language.clone(),
            meta_tags: config.meta.to_vec(),
            published: config.metadata.published.clone(),
            author: config.metadata.author.clone(),
            site: config.metadata.site.clone(),
            schema_org_data: schema_val,
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
