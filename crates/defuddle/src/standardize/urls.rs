use crate::dom::{Document, NodeId};

pub fn resolve_urls(doc: &mut Document, content_id: NodeId, page_url: &str) {
    if page_url.is_empty() {
        return;
    }

    let base_url = page_url.to_string();
    if !base_url.contains("://") {
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
                if let Some(resolved) = resolve_url(trimmed, &base_url) {
                    doc.set_attr(id, attr_name, &resolved);
                }
            }
        }
    }
}

// --- Individual standardization steps ---

/// Replace &nbsp; with regular spaces in text nodes.

pub(crate) fn resolve_url(url: &str, base: &str) -> Option<String> {
    if url.contains("://") {
        if url.matches('/').count() == 2 && !url.contains(['?', '#']) {
            return Some(format!("{url}/"));
        }
        return Some(url.to_string());
    }
    if url.starts_with("//") {
        let proto = base.split("://").next()?;
        return Some(format!("{}:{}", proto, url));
    }
    if url.starts_with('/') {
        if let Some(end) = base.find("://") {
            let after = &base[end + 3..];
            if let Some(slash) = after.find('/') {
                return Some(format!("{}{}", &base[..end + 3 + slash], url));
            }
            return Some(format!("{}{}", base, url));
        }
        return None;
    }
    if url.starts_with('#') || url.starts_with('?') || url.starts_with("mailto:") {
        return Some(url.to_string());
    }
    let last_slash = base.rfind('/');
    if let Some(pos) = last_slash
        && pos > base.find("://").unwrap_or(0) + 2
    {
        let base_dir = &base[..pos + 1];
        return Some(resolve_dot_segments(&format!("{}{}", base_dir, url)));
    }
    Some(format!("{}/{}", base.trim_end_matches('/'), url))
}

pub(crate) fn resolve_dot_segments(path: &str) -> String {
    let mut segs: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                segs.pop();
            }
            _ => segs.push(seg),
        }
    }
    segs.join("/")
}

// --- Remaining Phase 2 functions ---
