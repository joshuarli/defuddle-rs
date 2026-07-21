use crate::dom::{Document, NodeId};

pub(crate) fn target_url(src: &str) -> Option<String> {
    let lower = src.to_ascii_lowercase();
    let prefix_len = [
        "https://www.youtube.com/embed/",
        "http://www.youtube.com/embed/",
        "https://youtube-nocookie.com/embed/",
        "https://www.youtube-nocookie.com/embed/",
    ]
    .into_iter()
    .find_map(|prefix| lower.strip_prefix(prefix).map(|_| prefix.len()))?;
    let video_id = src
        .get(prefix_len..)?
        .split(['?', '&'])
        .next()
        .filter(|id| !id.is_empty())?;
    Some(format!("https://www.youtube.com/watch?v={video_id}"))
}

pub(crate) fn standardize_lite_youtube(doc: &mut Document, content_id: NodeId) {
    for id in doc.query_selector_all_in_subtree(content_id, "lite-youtube") {
        let Some(video_id) = doc.get_attr(id, "videoid").map(str::to_owned) else {
            continue;
        };
        doc.set_tag_name(id, "iframe");
        doc.set_attr(
            id,
            "src",
            &format!("https://www.youtube.com/embed/{video_id}"),
        );
        doc.set_attr(id, "frameborder", "0");
        doc.set_attr(id, "allowfullscreen", "");
    }
}
