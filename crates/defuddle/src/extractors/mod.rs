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

struct AdapterDef {
    name: &'static str,
    factory: fn() -> Box<dyn SiteAdapter>,
}

const ADAPTERS: &[AdapterDef] = &[
    AdapterDef {
        name: "hackernews",
        factory: hackernews::adapter,
    },
    AdapterDef {
        name: "substack",
        factory: substack::adapter,
    },
    AdapterDef {
        name: "reddit",
        factory: reddit::adapter,
    },
    AdapterDef {
        name: "github",
        factory: github::adapter,
    },
    AdapterDef {
        name: "gmail",
        factory: gmail::adapter,
    },
    AdapterDef {
        name: "chatgpt",
        factory: chatgpt::adapter,
    },
    AdapterDef {
        name: "mastodon",
        factory: mastodon::adapter,
    },
    AdapterDef {
        name: "twitter",
        factory: twitter::adapter,
    },
];

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
    ADAPTERS.iter().find_map(|adapter| {
        if requested_name
            .is_some_and(|requested| !requested.eq_ignore_ascii_case(adapter.name))
        {
            return None;
        }
        let domain_matches = match adapter.name {
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
        let instance = (adapter.factory)();
        instance
            .matches(context)
            .then_some((adapter.name, instance))
    })
}

pub fn adapter_names() -> Vec<&'static str> {
    ADAPTERS.iter().map(|adapter| adapter.name).collect()
}

#[cfg(test)]
mod tests {
    use super::ADAPTERS;

    #[test]
    fn adapter_names_match_registry_order() {
        let names = super::adapter_names();
        let registry: Vec<&'static str> =
            ADAPTERS.iter().map(|adapter| adapter.name).collect();
        assert_eq!(names, registry);
    }
}
