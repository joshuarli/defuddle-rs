use crate::dom::{Document, NodeId};
use crate::extractors::dom_builder::Builder;
use crate::extractors::{SiteAdapter, SiteExtraction};
use crate::pipeline::ExtractionContext;
use rustc_hash::FxHashMap;

pub(crate) fn adapter() -> Box<dyn SiteAdapter> {
    Box::new(SubstackAdapter)
}

struct SubstackAdapter;

impl SiteAdapter for SubstackAdapter {
    fn matches(&self, context: &ExtractionContext<'_>) -> bool {
        detect_mode(&context.doc).is_some()
    }

    fn extract(&self, context: &mut ExtractionContext<'_>) -> SiteExtraction {
        let mode = detect_mode(&context.doc).unwrap_or(SubstackMode::Body);
        let metadata = metadata(&context.doc);
        let mut builder = Builder::new();
        let html = builder.element(builder.root, "html", &[]);
        builder.element(html, "head", &[]);
        let body = builder.element(html, "body", &[]);

        let (title, author, site, source) = match mode {
            SubstackMode::Article => {
                let source = context
                    .doc
                    .query_selector_all("div.body.markup")
                    .first()
                    .copied();
                let title = metadata.title_or_document_title(&context.doc);
                let author = if metadata.author.is_empty() {
                    context
                        .doc
                        .query_selector_all("a[href*=\"substack.com/@\"]")
                        .first()
                        .map(|id| context.doc.text_content(*id).trim().to_string())
                        .unwrap_or_default()
                } else {
                    metadata.author.clone()
                };
                let site = metadata.site_or_default();
                (
                    title,
                    author,
                    site,
                    source.map(|id| ContentSource::Borrowed(&context.doc, id)),
                )
            }
            SubstackMode::Note => {
                let source = note_content_node(&context.doc);
                let author = if metadata.author.is_empty() {
                    metadata
                        .title
                        .rsplit(" (@")
                        .next()
                        .unwrap_or(&metadata.title)
                        .trim()
                        .to_string()
                } else {
                    metadata.author.clone()
                };
                (
                    metadata.title.clone(),
                    author,
                    "Substack".to_string(),
                    source.map(|id| ContentSource::Borrowed(&context.doc, id)),
                )
            }
            SubstackMode::Body => {
                let source = context.doc.query_selector_all("body").first().copied();
                let title = metadata.title_or_document_title(&context.doc);
                let site = metadata.site_or_default();
                (
                    title,
                    metadata.author.clone(),
                    site,
                    source.map(|id| ContentSource::Borrowed(&context.doc, id)),
                )
            }
            SubstackMode::Preload => {
                let (body_html, title, author, site) = substack_preload(&context.doc);
                let source = if body_html.is_empty() {
                    None
                } else {
                    let document = Document::parse(&body_html);
                    let root = document.root();
                    Some(ContentSource::Owned(document, root))
                };
                (title, author, site, source)
            }
        };

        if let Some(source) = source {
            match source {
                ContentSource::Borrowed(document, source_node) => {
                    builder.clone_children(document, source_node, body)
                }
                ContentSource::Owned(document, source_node) => {
                    builder.clone_children(&document, source_node, body)
                }
            }
        }
        cleanup_body_dom(&mut builder.document, body);

        let mut variables = FxHashMap::from_iter([
            ("title".into(), title),
            ("author".into(), author),
            ("site".into(), site),
        ]);
        if !metadata.published.is_empty() {
            variables.insert("published".into(), metadata.published);
        }
        SiteExtraction {
            extracted_content: builder.finish(html),
            variables,
        }
    }
}

#[derive(Clone, Copy)]
enum SubstackMode {
    Article,
    Preload,
    Note,
    Body,
}

enum ContentSource<'a> {
    Borrowed(&'a Document, NodeId),
    Owned(Document, NodeId),
}

fn detect_mode(doc: &Document) -> Option<SubstackMode> {
    if !doc.query_selector_all("div.body.markup").is_empty() {
        return Some(SubstackMode::Article);
    }
    if doc.query_selector_all("script").into_iter().any(|sc| {
        let text = doc.text_content(sc);
        text.contains("window._preloads") && text.contains("body_html")
    }) {
        return Some(SubstackMode::Preload);
    }
    if !doc
        .query_selector_all(".ProseMirror, .FeedProseMirror")
        .is_empty()
    {
        return Some(SubstackMode::Note);
    }
    Some(SubstackMode::Body)
}

struct MetadataValues {
    title: String,
    site: String,
    author: String,
    published: String,
}

impl MetadataValues {
    fn title_or_document_title(&self, doc: &Document) -> String {
        if !self.title.is_empty() {
            return self.title.clone();
        }
        doc.query_selector_all("title")
            .first()
            .map(|id| doc.text_content(*id).trim().to_string())
            .unwrap_or_default()
    }

    fn site_or_default(&self) -> String {
        if self.site.is_empty() {
            "Substack".to_string()
        } else {
            self.site.clone()
        }
    }
}

fn metadata(doc: &Document) -> MetadataValues {
    let content = |selector: &str| {
        doc.query_selector_all(selector)
            .first()
            .and_then(|id| doc.get_attr(*id, "content"))
            .unwrap_or("")
            .to_string()
    };
    MetadataValues {
        title: content("meta[property=\"og:title\"]"),
        site: content("meta[property=\"og:site_name\"]"),
        author: content("meta[name=\"author\"]"),
        published: content("meta[property=\"article:published_time\"]"),
    }
}

fn note_content_node(doc: &Document) -> Option<NodeId> {
    let canonical = doc.query_selector_all("link").into_iter().find_map(|id| {
        let rel = doc.get_attr(id, "rel").unwrap_or("");
        let href = doc.get_attr(id, "href").unwrap_or("");
        (rel.eq_ignore_ascii_case("canonical") || href.contains("/note/c-"))
            .then(|| href.to_string())
    });
    let note_key = canonical
        .as_deref()
        .and_then(|url| url.rsplit('/').next())
        .filter(|key| key.starts_with("c-"));
    if let Some(key) = note_key {
        for link in doc.query_selector_all("a") {
            if doc
                .get_attr(link, "href")
                .is_some_and(|href| href.contains(key))
            {
                let mut current = link;
                loop {
                    if let Some(note) = doc
                        .query_selector_all_in_subtree(current, "div.ProseMirror.FeedProseMirror")
                        .first()
                        .copied()
                    {
                        return Some(note);
                    }
                    current = doc.parent(current)?;
                }
            }
        }
    }
    doc.query_selector_all("[class*=\"feedPermalinkUnit\"]")
        .first()
        .and_then(|id| {
            doc.query_selector_all_in_subtree(*id, "div.ProseMirror.FeedProseMirror")
                .first()
                .copied()
        })
        .or_else(|| {
            doc.query_selector_all("div.ProseMirror.FeedProseMirror")
                .first()
                .copied()
        })
}

fn cleanup_body_dom(doc: &mut Document, root: NodeId) {
    for id in doc.query_selector_all_in_subtree(root, "p.button-wrapper") {
        doc.remove(id);
    }
    for selector in [
        "div[data-component-name=\"SubscribeWidgetToDOM\"]",
        "div[data-component-name=\"DigestPostEmbed\"]",
    ] {
        for id in doc.query_selector_all_in_subtree(root, selector) {
            doc.remove(id);
        }
    }
    for id in doc.query_selector_all_in_subtree(root, "a") {
        if doc
            .text_content(id)
            .trim()
            .eq_ignore_ascii_case("Read full story")
        {
            doc.remove(id);
        }
    }
    for id in doc.query_selector_all_in_subtree(root, "div") {
        if doc.attr_names(id).is_empty()
            && !doc.children(id).iter().any(|child| doc.is_element(*child))
            && is_substack_date_text(&doc.text_content(id))
        {
            doc.remove(id);
        }
    }
    for id in doc.query_selector_all_in_subtree(root, "p") {
        if !doc.text_content(id).trim().starts_with("For More") {
            continue;
        }
        let Some(parent) = doc.parent(id) else {
            continue;
        };
        let siblings = doc.children(parent).to_vec();
        let Some(position) = siblings.iter().position(|child| *child == id) else {
            continue;
        };
        let Some(previous) = siblings[..position]
            .iter()
            .rev()
            .copied()
            .find(|child| doc.is_element(*child))
        else {
            continue;
        };
        if doc.outer_html(previous).trim() == "<div><hr></div>" {
            for child in siblings.into_iter().skip(
                doc.children(parent)
                    .iter()
                    .position(|child| *child == previous)
                    .unwrap_or(position),
            ) {
                doc.remove(child);
            }
        }
    }
}

fn substack_preload(doc: &Document) -> (String, String, String, String) {
    let scripts = doc.query_selector_all("script");
    for sc in scripts {
        let text = doc.text_content(sc);
        if !text.contains("window._preloads") || !text.contains("body_html") {
            continue;
        }
        if let Some(start) = text.find("JSON.parse(\"") {
            let start = start + "JSON.parse(\"".len();
            let mut i = start;
            let chars: Vec<char> = text.chars().collect();
            while i < chars.len() {
                if chars[i] == '\\' {
                    i += 2;
                } else if chars[i] == '"' {
                    break;
                } else {
                    i += 1;
                }
            }
            let inner: String = chars[start..i].iter().collect();
            if let Ok(json_str) = miniserde::json::from_str::<String>(&format!("\"{}\"", inner))
                && let Ok(data) = miniserde::json::from_str::<miniserde::json::Value>(&json_str)
                && let Some(post) = crate::schema::object_value(&data, "feedData")
                    .and_then(|f| crate::schema::object_value(f, "initialPost"))
                    .and_then(|p| crate::schema::object_value(p, "post"))
            {
                let title = crate::schema::object_value(post, "title")
                    .and_then(crate::schema::as_str)
                    .unwrap_or("")
                    .to_string();
                let author = crate::schema::object_value(post, "publishedBylines")
                    .and_then(crate::schema::as_array)
                    .and_then(|arr| arr.first())
                    .and_then(|b| crate::schema::object_value(b, "name"))
                    .and_then(crate::schema::as_str)
                    .unwrap_or("")
                    .to_string();
                let body = crate::schema::object_value(post, "body_html")
                    .and_then(crate::schema::as_str)
                    .unwrap_or("")
                    .to_string();
                if !body.is_empty() {
                    return (body, title, author, "Substack".to_string());
                }
            }
            break;
        }
    }
    (
        String::new(),
        String::new(),
        String::new(),
        "Substack".to_string(),
    )
}
pub fn register_prune(registry: &mut crate::prune::Registry) {
    registry.register(|doc| {
        for id in doc.query_selector_all("button[aria-label=\"Link\"]") {
            doc.remove(id);
        }
        for id in doc.query_selector_all("button[class*=\"icon-container restack-image\"]") {
            doc.remove(id);
        }
        for id in doc.query_selector_all("button[class*=\"icon-container view-image\"]") {
            doc.remove(id);
        }
        for id in doc.query_selector_all("p[class*=\"button-wrapper\"]") {
            doc.remove(id);
        }
        for id in doc.query_selector_all("div[data-component-name=\"SubscribeWidgetToDOM\"]") {
            doc.remove(id);
        }
        for id in doc.query_selector_all("div[data-component-name=\"DigestPostEmbed\"]") {
            doc.remove(id);
        }
        for id in doc.query_selector_all("a") {
            if doc
                .text_content(id)
                .trim()
                .eq_ignore_ascii_case("Read full story")
            {
                doc.remove(id);
            }
        }
        // Remove standalone date divs (e.g. "<div>December 14, 2023</div>").
        // Only match bare <div> elements with text-only content — the
        // original regex targets unadorned date containers typical of
        // Substack.
        for id in doc.query_selector_all("div") {
            if !doc.attr_names(id).is_empty() {
                continue;
            }
            // Only leaf elements: no element children, just text.
            if doc.children(id).iter().any(|c| doc.is_element(*c)) {
                continue;
            }
            let text = doc.text_content(id).trim().to_string();
            if is_substack_date_text(&text) {
                doc.remove(id);
            }
        }
        // Remove "For More …" footer sections.
        for id in doc.query_selector_all("p") {
            let text = doc.text_content(id).trim().to_string();
            if text.starts_with("For More")
                && let Some(parent) = doc.parent(id)
            {
                let siblings = doc.children(parent).to_vec();
                if let Some(pos) = siblings.iter().position(|&s| s == id)
                    && pos > 0
                {
                    let prev = siblings[pos - 1];
                    let prev_html = doc.outer_html(prev);
                    if prev_html.trim() == "<div><hr></div>" {
                        // Remove the <div><hr></div> and the <p>For More…</p>
                        // plus everything after it (same parent).
                        let remove_from = prev;
                        let children = doc.children(parent).to_vec();
                        if let Some(start_pos) = children.iter().position(|&c| c == remove_from) {
                            for sibling in children.into_iter().skip(start_pos) {
                                doc.remove(sibling);
                            }
                        }
                    }
                }
            }
        }
    });
}

/// Check whether text looks like a standalone Substack date line.
fn is_substack_date_text(text: &str) -> bool {
    const MONTHS: &[&str] = &[
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let text = text.trim();
    for month in MONTHS {
        if let Some(rest) = text.strip_prefix(month) {
            let rest = rest.trim();
            // Expect ", DD, YYYY" or " DD, YYYY"
            let rest = rest.trim_start_matches(',');
            let rest = rest.trim();
            // Must start with one or two digits
            let after_day = rest.trim_start_matches(|c: char| c.is_ascii_digit());
            if after_day.len() < rest.len() && after_day.trim_start().starts_with(',') {
                let after_comma = after_day.trim_start_matches(',').trim();
                if after_comma.len() == 4 && after_comma.chars().all(|c| c.is_ascii_digit()) {
                    return true;
                }
            }
        }
    }
    false
}
