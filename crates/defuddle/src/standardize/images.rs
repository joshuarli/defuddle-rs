use crate::dom::{Document, NodeId};
use rustc_hash::FxHashSet;
pub(crate) fn recover_noscript_images(doc: &mut Document, content_id: NodeId) {
    let noscripts = doc.query_selector_all_in_subtree(content_id, "noscript");
    for ns_id in noscripts {
        let imgs = doc.query_selector_all_in_subtree(ns_id, "img");
        if imgs.is_empty() {
            let raw = crate::utils::decode_html_entities(&doc.inner_html(ns_id));
            let parsed = Document::parse(&raw);
            let Some(img_id) = parsed.query_selector_all("img").first().copied() else {
                continue;
            };
            let src = parsed.get_attr(img_id, "src").unwrap_or("");
            if src.is_empty() || src.starts_with("data:") {
                continue;
            }
            let alt = parsed.get_attr(img_id, "alt").unwrap_or("");
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

pub(crate) fn replace_if_placeholder(
    doc: &mut Document,
    img_id: NodeId,
    new_src: &str,
    new_alt: &str,
) {
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

const MIN_DIMENSION: f64 = 33.0;

pub(crate) fn remove_small_images(doc: &mut Document, content_id: NodeId) {
    let elements: Vec<NodeId> = doc
        .query_selector_all_in_subtree(content_id, "img, svg")
        .into_iter()
        .filter(|&id| !doc.is_removed(id))
        .collect();

    for id in elements {
        if doc.is_removed(id) {
            continue;
        }

        let tag = doc.tag_name(id);
        let is_img = tag.eq_ignore_ascii_case("img");
        let is_svg = tag.eq_ignore_ascii_case("svg");

        if is_img {
            let src = doc.get_attr(id, "src").unwrap_or("");
            let has_alt_src = !doc.get_attr(id, "srcset").unwrap_or("").is_empty()
                || !doc.get_attr(id, "data-src").unwrap_or("").is_empty()
                || !doc.get_attr(id, "data-srcset").unwrap_or("").is_empty()
                || !doc.get_attr(id, "data-lazy-src").unwrap_or("").is_empty()
                || !doc.get_attr(id, "data-original").unwrap_or("").is_empty();

            if src.is_empty() && !has_alt_src {
                doc.remove(id);
                continue;
            }

            if !has_alt_src
                && !doc.closest(id, "picture").is_some()
                && src.starts_with("data:image/")
                && !src.starts_with("data:image/svg+xml")
            {
                let b64start = src.find(',').map(|i| i + 1).unwrap_or(src.len());
                let b64len = src.len() - b64start;
                if b64len < 133 {
                    doc.remove(id);
                    continue;
                }
            }
        }

        let attr_w: f64 = doc
            .get_attr(id, "width")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);
        let attr_h: f64 = doc
            .get_attr(id, "height")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);

        let style = doc.get_attr(id, "style").unwrap_or("");
        let mut style_w = 0.0;
        let mut style_h = 0.0;
        for part in style.split(';') {
            let trimmed = part.trim();
            if let Some(val) = trimmed
                .strip_prefix("width:")
                .or_else(|| trimmed.strip_prefix("width :"))
            {
                style_w = val
                    .trim()
                    .trim_end_matches("px")
                    .trim()
                    .parse()
                    .unwrap_or(0.0);
            }
            if let Some(val) = trimmed
                .strip_prefix("height:")
                .or_else(|| trimmed.strip_prefix("height :"))
            {
                style_h = val
                    .trim()
                    .trim_end_matches("px")
                    .trim()
                    .parse()
                    .unwrap_or(0.0);
            }
        }

        let mut viewbox_w = 0.0;
        let mut viewbox_h = 0.0;
        if is_svg && let Some(vb) = doc.get_attr(id, "viewBox") {
            let parts: Vec<&str> = vb.split(|c: char| c.is_whitespace() || c == ',').collect();
            if parts.len() == 4 {
                viewbox_w = parts[2].parse().unwrap_or(0.0);
                viewbox_h = parts[3].parse().unwrap_or(0.0);
            }
        }

        let mut url_w = 0.0;
        if is_img
            && attr_w == 0.0
            && attr_h == 0.0
            && style_w == 0.0
            && style_h == 0.0
            && let Some(srcset) = doc.get_attr(id, "srcset")
        {
            for part in srcset.split(',') {
                let trimmed = part.trim();
                if trimmed.ends_with("1x") {
                    let url_part = trimmed.rsplit_once(' ').map(|x| x.0).unwrap_or("");
                    for seg in url_part.split(['?', '&', '/']) {
                        if let Some(w) = seg
                            .strip_prefix("w=")
                            .or_else(|| seg.strip_prefix("width="))
                            .or_else(|| seg.strip_prefix("w:"))
                            .or_else(|| seg.strip_prefix("width:"))
                            .or_else(|| seg.strip_prefix("w_"))
                        {
                            url_w = w.trim().parse().unwrap_or(0.0);
                            if url_w > 0.0 {
                                break;
                            }
                        }
                    }
                    if url_w > 0.0 {
                        break;
                    }
                }
            }
        }

        let widths: Vec<f64> = [attr_w, style_w, viewbox_w, url_w]
            .into_iter()
            .filter(|&d| d > 0.0)
            .collect();
        let heights: Vec<f64> = [attr_h, style_h, viewbox_h]
            .into_iter()
            .filter(|&d| d > 0.0)
            .collect();

        if widths.is_empty() && heights.is_empty() {
            continue;
        }

        let effective_w = widths.iter().cloned().fold(f64::INFINITY, f64::min);
        let effective_h = heights.iter().cloned().fold(f64::INFINITY, f64::min);

        if effective_w >= MIN_DIMENSION && effective_h >= MIN_DIMENSION {
            continue;
        }

        doc.remove(id);
    }
}
