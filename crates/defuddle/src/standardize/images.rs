use crate::dom::{Document, NodeId};
use rustc_hash::FxHashSet;
use std::sync::OnceLock;

static NOSCRIPT_IMG_SRC_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static IMG_ALT_RE: OnceLock<regex_lite::Regex> = OnceLock::new();

pub(crate) fn recover_noscript_images(doc: &mut Document, content_id: NodeId) {
    let noscripts = doc.query_selector_all_in_subtree(content_id, "noscript");
    for ns_id in noscripts {
        let imgs = doc.query_selector_all_in_subtree(ns_id, "img");
        if imgs.is_empty() {
            let raw = crate::utils::decode_html_entities(&doc.inner_html(ns_id));
            let Some(caps) = NOSCRIPT_IMG_SRC_RE
                .get_or_init(|| {
                    regex_lite::Regex::new(r#"<img[^>]*\bsrc="([^"]+)"[^>]*>"#).unwrap()
                })
                .captures(&raw)
            else {
                continue;
            };
            let src = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            if src.is_empty() || src.starts_with("data:") {
                continue;
            }
            let alt = IMG_ALT_RE
                .get_or_init(|| regex_lite::Regex::new(r#"\balt="([^"]*)""#).unwrap())
                .captures(&raw)
                .and_then(|m| m.get(1))
                .map(|m| m.as_str())
                .unwrap_or("");
            let has_caption = doc.closest(ns_id, "figure").is_some_and(|figure| {
                !doc.query_selector_all_in_subtree(figure, "figcaption")
                    .is_empty()
            });
            let caption = if !has_caption && !alt.is_empty() {
                format!("<p>{}</p>", crate::utils::escape_html(alt))
            } else {
                String::new()
            };
            doc.replace_with_html(
                ns_id,
                &format!(
                    r#"<img src="{}" alt="{}">{}"#,
                    crate::utils::escape_html(src),
                    crate::utils::escape_html(alt),
                    caption
                ),
            );
            continue;
        }
        let ns_img = imgs[0];
        let ns_src = doc.get_attr(ns_img, "src").unwrap_or("").to_string();
        let ns_alt = doc.get_attr(ns_img, "alt").unwrap_or("").to_string();
        if ns_src.is_empty() || ns_src.starts_with("data:") {
            continue;
        }
        let parent = doc.parent(ns_id);
        if let Some(pid) = parent {
            let siblings = doc.children(pid).to_vec();
            for sib in siblings {
                if sib == ns_id || !doc.is_element(sib) {
                    continue;
                }
                if !doc.tag_name_is(sib, "img") {
                    for img_id in doc.query_selector_all_in_subtree(sib, "img") {
                        replace_if_placeholder(doc, img_id, &ns_src, &ns_alt);
                    }
                    continue;
                }
                replace_if_placeholder(doc, sib, &ns_src, &ns_alt);
            }
            let has_caption = doc.closest(ns_id, "figure").is_some_and(|figure| {
                !doc.query_selector_all_in_subtree(figure, "figcaption")
                    .is_empty()
            });
            if !has_caption && !ns_alt.is_empty() {
                doc.append_html(
                    pid,
                    &format!("<p>{}</p>", crate::utils::escape_html(&ns_alt)),
                );
            }
        }
        doc.remove(ns_id);
    }
}

pub(crate) fn replace_if_placeholder(doc: &mut Document, img_id: NodeId, new_src: &str, new_alt: &str) {
    let src = doc.get_attr(img_id, "src").unwrap_or("");
    let is_placeholder = src.is_empty() || src.starts_with("data:image/");
    if !is_placeholder {
        return;
    }
    doc.set_attr(img_id, "src", new_src);
    if !new_alt.is_empty() {
        doc.set_attr(img_id, "alt", new_alt);
    }
    doc.remove_attr(img_id, "data-nimg");
}


pub(crate) fn recover_lazy_images(doc: &mut Document, content_id: NodeId) {
    let lazy_attrs = ["data-image-loader", "data-src", "data-original-src"];
    let imgs = doc.query_selector_all_in_subtree(content_id, "img");
    for id in imgs {
        let src = doc.get_attr(id, "src").unwrap_or("").to_string();
        if src.is_empty() || !src.starts_with("data:image/") {
            continue;
        }
        for attr in &lazy_attrs {
            if let Some(real_url) = doc.get_attr(id, attr)
                && !real_url.is_empty()
                && !real_url.starts_with("data:")
            {
                let url = real_url.to_string();
                doc.set_attr(id, "src", &url);
                break;
            }
        }
        if doc
            .get_attr(id, "src")
            .is_none_or(|s| s.starts_with("data:"))
            && let Some(pid) = doc.parent(id)
            && doc.tag_name(pid).eq_ignore_ascii_case("picture")
        {
            for &source_id in &doc.query_selector_all_in_subtree(pid, "source") {
                let srcset = doc
                    .get_attr(source_id, "srcSet")
                    .or_else(|| doc.get_attr(source_id, "srcset"))
                    .unwrap_or("");
                if !srcset.is_empty()
                    && let Some(best) = get_best_src_url(srcset)
                {
                    doc.set_attr(id, "src", &best);
                    break;
                }
            }
        }
    }
}


pub(crate) fn normalize_srcset_attributes(doc: &mut Document, content_id: NodeId) {
    let elements = doc.query_selector_all_in_subtree(content_id, "source,img");
    for id in elements {
        if let Some(srcset) = doc.get_attr(id, "srcSet") {
            let val = srcset.to_string();
            doc.remove_attr(id, "srcSet");
            doc.set_attr(id, "srcset", &val);
        }
    }
}

pub(crate) fn get_best_src_url(srcset: &str) -> Option<String> {
    if let Some(url) = crate::markdown::get_best_image_src(srcset) {
        return Some(url);
    }
    let mut best: Option<(f32, String)> = None;
    for candidate in srcset.split(',') {
        let mut parts = candidate.split_whitespace();
        let Some(url) = parts.next() else { continue };
        let Some(descriptor) = parts.next() else {
            continue;
        };
        let Some(density) = descriptor
            .strip_suffix('x')
            .and_then(|v| v.parse::<f32>().ok())
        else {
            continue;
        };
        if best.as_ref().is_none_or(|(current, _)| density > *current) {
            best = Some((density, url.to_string()));
        }
    }
    best.map(|(_, url)| url)
}


pub(crate) fn remove_base64_placeholder_images(doc: &mut Document, content_id: NodeId) {
    let lazy_attrs = ["data-src", "data-image-loader", "data-original-src"];
    let imgs = doc.query_selector_all_in_subtree(content_id, "img");
    for id in imgs {
        let src = doc.get_attr(id, "src").unwrap_or("");
        if !src.starts_with("data:image/") {
            continue;
        }
        if src.len() > 10_000 {
            continue;
        }
        let alt = doc.get_attr(id, "alt").unwrap_or("").to_lowercase();
        if src.starts_with("data:image/png")
            && alt.len() > 20
            && !alt.contains("placeholder")
            && !alt.contains("unresolvable")
        {
            continue;
        }
        // Don't remove if has a picture parent with sources
        let mut has_fallback = false;
        if let Some(pid) = doc.parent(id)
            && doc.tag_name(pid).eq_ignore_ascii_case("picture")
            && !doc.query_selector_all_in_subtree(pid, "source").is_empty()
        {
            has_fallback = true;
        }
        if has_fallback {
            continue;
        }
        let has_lazy = lazy_attrs.iter().any(|attr| {
            doc.get_attr(id, attr)
                .is_some_and(|v| !v.is_empty() && !v.starts_with("data:"))
        });
        if has_lazy {
            continue;
        }
        doc.remove(id);
    }
}


pub(crate) fn remove_empty_video_elements(doc: &mut Document, content_id: NodeId) {
    for id in doc.query_selector_all_in_subtree(content_id, "video") {
        let has_src = doc.get_attr(id, "src").is_some_and(|s| !s.is_empty());
        let has_source = !doc.query_selector_all_in_subtree(id, "source").is_empty();
        if !has_src && !has_source {
            doc.remove(id);
        }
    }
}


pub(crate) fn dedup_images(doc: &mut Document, content_id: NodeId) {
    for parent_id in doc.query_selector_all_in_subtree(content_id, "figure,div,p") {
        let imgs = if doc.tag_name(parent_id).eq_ignore_ascii_case("figure") {
            doc.query_selector_all_in_subtree(parent_id, "img")
        } else {
            doc.children(parent_id)
                .iter()
                .copied()
                .filter(|id| doc.is_element(*id) && doc.tag_name(*id).eq_ignore_ascii_case("img"))
                .collect()
        };
        if imgs.len() < 2 {
            continue;
        }
        let mut seen: FxHashSet<String> = FxHashSet::default();
        let mut to_remove: Vec<NodeId> = Vec::new();
        for &img_id in &imgs {
            let src = doc.get_attr(img_id, "src").unwrap_or("").to_string();
            let alt = doc.get_attr(img_id, "alt").unwrap_or("").to_string();
            let key = if !alt.is_empty() && !is_generic_alt(&alt) {
                format!("A:{alt}")
            } else if doc.tag_name(parent_id).eq_ignore_ascii_case("figure") {
                "FIGURE-GENERIC".to_string()
            } else {
                format!("U:{src}")
            };
            if seen.contains(&key) {
                to_remove.push(img_id);
            } else {
                seen.insert(key);
            }
        }
        for img_id in to_remove {
            doc.remove(img_id);
        }
    }
    lightbox_image_dedup(doc, content_id);
}

pub(crate) fn lightbox_image_dedup(doc: &mut Document, content_id: NodeId) {
    let all_imgs = doc.query_selector_all_in_subtree(content_id, "img");
    let mut linked_urls: FxHashSet<String> = FxHashSet::default();
    for &img_id in &all_imgs {
        if let Some(pid) = doc.parent(img_id)
            && doc.tag_name(pid).eq_ignore_ascii_case("a")
            && let Some(href) = doc.get_attr(pid, "href")
            && !href.is_empty()
            && !href.starts_with("javascript:")
        {
            linked_urls.insert(href.to_string());
        }
    }
    let mut to_remove: Vec<NodeId> = Vec::new();
    for &img_id in &all_imgs {
        let src = doc.get_attr(img_id, "src").unwrap_or("").to_string();
        if src.is_empty() || doc.closest(img_id, "a").is_some() {
            continue;
        }
        if linked_urls.contains(&src) {
            to_remove.push(img_id);
        }
    }
    for img_id in to_remove {
        doc.remove(img_id);
    }
}

pub(crate) fn is_generic_alt(alt: &str) -> bool {
    let a = alt.trim().to_lowercase();
    a.is_empty() || a == "image" || a == "picture" || a == "photo"
}

