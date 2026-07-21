pub struct CommentData {
    pub author: String,
    pub date: String,
    pub content: String,
    pub depth: u32,
    pub score: String,
    pub url: String,
}

pub fn build_comment(comment: &CommentData) -> String {
    let author = format!(
        "<span class=\"comment-author\"><strong>{}</strong></span>",
        crate::utils::escape_html(&comment.author)
    );
    let date_html = if !comment.url.is_empty() {
        format!(
            "<a href=\"{}\" class=\"comment-link\">{}</a>",
            crate::utils::escape_html(&comment.url),
            crate::utils::escape_html(&comment.date)
        )
    } else {
        format!(
            "<span class=\"comment-date\">{}</span>",
            crate::utils::escape_html(&comment.date)
        )
    };
    let score_html = if !comment.score.is_empty() {
        format!(
            " · <span class=\"comment-points\">{}</span>",
            crate::utils::escape_html(&comment.score)
        )
    } else {
        String::new()
    };

    format!(
        "<div class=\"comment\">\n\t<div class=\"comment-metadata\">\n\t\t{} · {}{}\n\t</div>\n\t<div class=\"comment-content\">{}</div>\n</div>",
        author, date_html, score_html, comment.content
    )
}

pub fn build_comment_tree(comments: &[CommentData]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut blockquote_stack: Vec<u32> = Vec::new();

    for comment in comments {
        let depth = comment.depth;
        if depth == 0 {
            while !blockquote_stack.is_empty() {
                parts.push("</blockquote>".into());
                blockquote_stack.pop();
            }
            parts.push("<blockquote>".into());
            blockquote_stack.push(0);
        } else {
            let current = blockquote_stack.last().copied().unwrap_or(0);
            if depth < current {
                while blockquote_stack.last().copied().unwrap_or(0) >= depth {
                    parts.push("</blockquote>".into());
                    blockquote_stack.pop();
                }
            }
            let new_current = blockquote_stack.last().copied().unwrap_or(0);
            if depth > new_current {
                parts.push("<blockquote>".into());
                blockquote_stack.push(depth);
            }
        }

        parts.push(build_comment(comment));
    }

    while !blockquote_stack.is_empty() {
        parts.push("</blockquote>".into());
        blockquote_stack.pop();
    }

    parts.join("")
}

pub fn build_content_html(site: &str, post_content: &str, comments: &str) -> String {
    let comments_section = if comments.is_empty() {
        String::new()
    } else {
        format!(
            "\n\t\t\t<hr>\n\t\t\t<div class=\"{site} comments\">\n\t\t\t\t<h2>Comments</h2>\n\t\t\t\t{comments}\n\t\t\t</div>"
        )
    };

    format!(
        "<article data-defuddle>\n\t\t\t<div class=\"{site} post\">\n\t\t\t\t<div class=\"post-content\">\n\t\t\t\t\t{post_content}\n\t\t\t\t</div>\n\t\t\t</div>{}</article>",
        comments_section
    )
}
