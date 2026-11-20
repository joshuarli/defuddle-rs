use std::borrow::Cow;

use crate::dom::{Document, NodeId};

pub(crate) fn resolve_urls(doc: &mut Document, content_id: NodeId, page_url: &str) {
    if !page_url.contains("://") {
        return;
    }

    let url_attrs = ["src", "href", "srcset", "poster", "data-src", "data-srcset"];
    let all_els = doc.query_selector_all_in_subtree(content_id, "*");

    for id in all_els {
        for attr_name in &url_attrs {
            if let Some(value) = doc.get_attr(id, attr_name) {
                let trimmed = value.trim();
                if trimmed.is_empty()
                    || trimmed.starts_with("data:")
                    || trimmed.starts_with("javascript:")
                {
                    continue;
                }
                if let Some(resolved) = resolve_url(trimmed, page_url) {
                    doc.set_attr(id, attr_name, &resolved);
                }
            }
        }
    }
}

fn resolve_url(url: &str, base: &str) -> Option<String> {
    let (path, suffix) = url.split_at(url.find(['?', '#']).unwrap_or(url.len()));
    if let Some((scheme, _)) = path.split_once("://")
        && !scheme.contains('/')
    {
        if path.matches('/').count() == 2 && suffix.is_empty() {
            return Some(format!("{url}/"));
        }
        return Some(url.to_string());
    }
    if url.starts_with("//") {
        let (scheme, _) = base.split_once("://")?;
        return Some(format!("{scheme}:{url}"));
    }
    if path.is_empty() || url.starts_with("mailto:") {
        return Some(url.to_string());
    }

    let base = base.split(['?', '#']).next()?;
    let (scheme, authority_and_path) = base.split_once("://")?;
    let (authority, base_path) = authority_and_path
        .split_once('/')
        .unwrap_or((authority_and_path, ""));
    let merged = match (path.strip_prefix('/'), base_path.rsplit_once('/')) {
        (Some(rooted), _) => Cow::Borrowed(rooted),
        (None, Some((directory, _))) => Cow::Owned(format!("{directory}/{path}")),
        (None, None) => Cow::Borrowed(path),
    };

    let mut segments = Vec::new();
    for segment in merged.split('/') {
        match segment {
            "." => {}
            ".." => {
                segments.pop();
            }
            _ => segments.push(segment),
        }
    }
    if matches!(merged.rsplit('/').next(), Some("." | "..")) {
        segments.push("");
    }
    Some(format!("{scheme}://{authority}/{}{suffix}", segments.join("/")))
}
