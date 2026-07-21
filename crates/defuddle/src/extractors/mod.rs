use rustc_hash::FxHashMap;

use crate::dom::Document;

pub mod chatgpt;
pub mod comments;
pub mod github;
pub mod hackernews;
pub mod mastodon;
pub mod reddit;
pub mod substack;
pub mod twitter;

pub struct ExtractorResult {
    pub content_html: String,
    pub variables: FxHashMap<String, String>,
}

pub trait Extractor {
    fn can_extract(&self, doc: &Document) -> bool;
    fn extract(&self, doc: &Document) -> ExtractorResult;
}

type ExtractorFactory = fn(doc: &Document, url: &str) -> Option<Box<dyn Extractor>>;

pub struct Registry {
    mappings: Vec<(String, String, ExtractorFactory)>,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

impl Registry {
    pub fn new() -> Self {
        Registry {
            mappings: Vec::new(),
        }
    }

    pub fn register(&mut self, domain: &str, name: &str, factory: ExtractorFactory) {
        self.mappings
            .push((domain.to_string(), name.to_string(), factory));
    }

    pub fn find(&self, url: &str, doc: &Document) -> Option<(String, Box<dyn Extractor>)> {
        let hostname = crate::utils::extract_domain(url);
        for (domain, name, factory) in &self.mappings {
            if (hostname.contains(domain.as_str())
                || url.contains(domain.as_str())
                || domain.is_empty())
                && let Some(extractor) = factory(doc, url)
                && extractor.can_extract(doc)
            {
                return Some((name.clone(), extractor));
            }
        }
        None
    }

    pub fn find_named(
        &self,
        name: &str,
        url: &str,
        doc: &Document,
    ) -> Option<(String, Box<dyn Extractor>)> {
        self.mappings
            .iter()
            .find(|(_, registered_name, _)| registered_name.eq_ignore_ascii_case(name))
            .and_then(|(_, registered_name, factory)| {
                factory(doc, url)
                    .filter(|extractor| extractor.can_extract(doc))
                    .map(|extractor| (registered_name.clone(), extractor))
            })
    }

    pub fn names(&self) -> Vec<&str> {
        let mut seen = std::collections::HashSet::new();
        self.mappings
            .iter()
            .filter_map(|(_, name, _)| {
                if seen.insert(name.as_str()) {
                    Some(name.as_str())
                } else {
                    None
                }
            })
            .collect()
    }
}

pub fn build_registry() -> Registry {
    let mut registry = Registry::new();

    registry.register("news.ycombinator.com", "hackernews", hackernews::create);
    registry.register("old.reddit.com", "reddit", reddit::create);
    registry.register("reddit.com", "reddit", reddit::create);
    registry.register("github.com", "github", github::create);
    registry.register("chatgpt.com", "chatgpt", chatgpt::create);
    registry.register("substack.com", "substack", substack::create);
    registry.register("mastodon", "mastodon", mastodon::create);
    registry.register("x.com", "twitter", twitter::create);
    registry.register("twitter.com", "twitter", twitter::create);

    registry
}
