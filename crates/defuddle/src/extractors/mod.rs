use rustc_hash::FxHashMap;

use crate::dom::Document;
use crate::pipeline::{ContentResult, PipelineContext};

pub mod chatgpt;
pub mod comments;
mod dom_builder;
pub mod github;
pub mod hackernews;
pub mod mastodon;
pub mod reddit;
pub mod substack;
pub mod twitter;

pub(crate) struct HtmlExtraction {
    pub content_html: String,
    pub variables: FxHashMap<String, String>,
}

pub(crate) struct SiteExtraction {
    pub content: ContentResult,
    pub variables: FxHashMap<String, String>,
}

pub(crate) trait SiteAdapter {
    fn matches(&self, context: &PipelineContext<'_>) -> bool;
    fn prepare(&self, _context: &mut PipelineContext<'_>) {}
    fn extract(&self, context: &mut PipelineContext<'_>) -> SiteExtraction;
}

pub(crate) fn site_extraction_from_html(
    content_html: String,
    variables: FxHashMap<String, String>,
) -> SiteExtraction {
    let document = Document::parse(&content_html);
    let root = document.root();
    SiteExtraction {
        content: ContentResult { document, root },
        variables,
    }
}

pub(crate) fn find_site_adapter(
    context: &PipelineContext<'_>,
    requested_name: Option<&str>,
) -> Option<(&'static str, Box<dyn SiteAdapter>)> {
    let candidates: &[(&str, fn() -> Box<dyn SiteAdapter>)] = &[
        ("hackernews", hackernews::adapter),
        ("substack", substack::adapter),
        ("reddit", reddit::adapter),
        ("github", github::adapter),
        ("chatgpt", chatgpt::adapter),
        ("mastodon", mastodon::adapter),
        ("twitter", twitter::adapter),
    ];
    candidates.iter().find_map(|(name, factory)| {
        if requested_name.is_some_and(|requested| !requested.eq_ignore_ascii_case(name)) {
            return None;
        }
        let domain_matches = match *name {
            "hackernews" => context.url.contains("news.ycombinator.com"),
            "substack" => context.url.contains("substack.com"),
            "reddit" => context.url.contains("reddit.com"),
            "github" => context.url.contains("github.com"),
            "chatgpt" => context.url.contains("chatgpt.com"),
            "mastodon" => context.url.contains("mastodon."),
            "twitter" => context.url.contains("x.com") || context.url.contains("twitter.com"),
            _ => false,
        };
        if requested_name.is_none() && !domain_matches {
            return None;
        }
        let adapter = factory();
        adapter.matches(context).then_some((*name, adapter))
    })
}

pub fn adapter_names() -> &'static [&'static str] {
    &[
        "hackernews",
        "substack",
        "reddit",
        "github",
        "chatgpt",
        "mastodon",
        "twitter",
    ]
}
