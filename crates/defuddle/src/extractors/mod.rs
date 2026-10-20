use rustc_hash::FxHashMap;

use crate::dom::Document;
use crate::pipeline::{ExtractedContent, ExtractionContext};

pub mod chatgpt;
pub mod comments;
mod dom_builder;
pub mod github;
pub mod gmail;
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
    pub extracted_content: ExtractedContent,
    pub variables: FxHashMap<String, String>,
}

pub(crate) trait SiteAdapter {
    fn matches(&self, context: &ExtractionContext<'_>) -> bool;
    fn prepare(&self, _context: &mut ExtractionContext<'_>) {}
    fn extract(&self, context: &mut ExtractionContext<'_>) -> SiteExtraction;
}

type AdapterCandidate = (&'static str, fn() -> Box<dyn SiteAdapter>);

pub(crate) fn site_extraction_from_html(
    content_html: String,
    variables: FxHashMap<String, String>,
) -> SiteExtraction {
    let document = Document::parse(&content_html);
    let root = document.root();
    SiteExtraction {
        extracted_content: ExtractedContent { document, root },
        variables,
    }
}

pub(crate) fn find_site_adapter(
    context: &ExtractionContext<'_>,
    requested_name: Option<&str>,
) -> Option<(&'static str, Box<dyn SiteAdapter>)> {
    let candidates: &[AdapterCandidate] = &[
        ("hackernews", hackernews::adapter),
        ("substack", substack::adapter),
        ("reddit", reddit::adapter),
        ("github", github::adapter),
        ("gmail", gmail::adapter),
        ("chatgpt", chatgpt::adapter),
        ("mastodon", mastodon::adapter),
        ("twitter", twitter::adapter),
    ];
    candidates.iter().find_map(|(name, factory)| {
        if requested_name.is_some_and(|requested| !requested.eq_ignore_ascii_case(name)) {
            return None;
        }
        let domain_matches = match *name {
            "hackernews" => context.page_url.contains("news.ycombinator.com"),
            "substack" => context.page_url.contains("substack.com"),
            "reddit" => context.page_url.contains("reddit.com"),
            "github" => context.page_url.contains("github.com"),
            "gmail" => context.page_url.contains("mail.google.com"),
            "chatgpt" => context.page_url.contains("chatgpt.com"),
            "mastodon" => context.page_url.contains("mastodon."),
            "twitter" => {
                context.page_url.contains("x.com") || context.page_url.contains("twitter.com")
            }
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
        "gmail",
        "chatgpt",
        "mastodon",
        "twitter",
    ]
}
