//! Math standardization: convert KaTeX, MathJax, and raw LaTeX to $...$ notation.

use crate::dom::{Document, NodeId};

pub fn standardize_math(doc: &mut Document, content_id: NodeId) {
    standardize_data_math(doc, content_id);
    standardize_katex(doc, content_id);
    standardize_mathjax_tex(doc, content_id);
    standardize_raw_latex(doc, content_id);
    standardize_mathjax_svg(doc, content_id);
}

/// Extract LaTeX from .math-inline[data-math] and .math-block[data-math] elements.
/// Used by Gemini and other services that embed math in data attributes.
fn standardize_data_math(doc: &mut Document, content_id: NodeId) {
    for sel in &[".math-inline[data-math]", ".math-block[data-math]"] {
        let els = doc.query_selector_all_in_subtree(content_id, sel);
        for el_id in els {
            if has_closest(doc, el_id, &["pre", "code"]) {
                continue;
            }
            if let Some(latex) = doc.get_attr(el_id, "data-math")
                && !latex.is_empty()
            {
                let is_block = is_block_math(doc, el_id);
                let replacement = if is_block {
                    format!("$$\n{latex}\n$$")
                } else {
                    format!("${latex}$")
                };
                doc.replace_with_html(el_id, &replacement);
            }
        }
    }
}

/// Extract LaTeX from KaTeX spans: look for <annotation encoding="application/x-tex">
/// inside .katex elements, or data-latex attributes.
fn standardize_katex(doc: &mut Document, content_id: NodeId) {
    let katex_els = doc.query_selector_all_in_subtree(content_id, ".katex");
    for el_id in katex_els {
        if has_closest(doc, el_id, &["pre", "code"]) {
            continue;
        }
        // Try data-latex attribute directly on .katex first
        let latex = if let Some(dl) = doc.get_attr(el_id, "data-latex")
            && !dl.trim().is_empty()
        {
            dl.trim().to_string()
        } else {
            // Try annotation element
            let annotations = doc.query_selector_all_in_subtree(el_id, "annotation");
            let mut latex = String::new();
            for ann_id in annotations {
                if let Some(enc) = doc.get_attr(ann_id, "encoding")
                    && enc == "application/x-tex"
                {
                    latex = doc.text_content(ann_id).trim().to_string();
                    break;
                }
            }
            if latex.is_empty() {
                // Try data-math on parent .math-inline / .math-block
                if let Some(parent) = doc.closest(el_id, ".math-inline,.math-block") {
                    latex = doc
                        .get_attr(parent, "data-math")
                        .map(|s| s.trim().to_string())
                        .unwrap_or_default();
                }
            }
            latex
        };
        if latex.is_empty() {
            continue;
        }
        let is_block = is_block_math(doc, el_id);
        let replacement = if is_block {
            format!("$$\n{latex}\n$$")
        } else {
            format!("${latex}$")
        };
        let target = doc.closest(el_id, ".math").unwrap_or(el_id);
        doc.replace_with_html(target, &replacement);
    }
}

/// Extract LaTeX from MathJax <script type="math/tex"> elements.
fn standardize_mathjax_tex(doc: &mut Document, content_id: NodeId) {
    let scripts = doc.query_selector_all_in_subtree(content_id, "script");
    for script_id in scripts {
        if has_closest(doc, script_id, &["pre", "code"]) {
            continue;
        }
        let type_attr = doc.get_attr(script_id, "type").unwrap_or("");
        if !type_attr.starts_with("math/tex") {
            continue;
        }
        let tex = doc.text_content(script_id).trim().to_string();
        if tex.is_empty() {
            continue;
        }
        let is_block = type_attr.contains("mode=display");
        let replacement = if is_block {
            format!("$$\n{tex}\n$$")
        } else {
            format!("${tex}$")
        };
        doc.replace_with_html(script_id, &replacement);
    }
}

/// Convert raw LaTeX delimiters: \(...\) → $...$, $$...$$ stays as $$...$$, \[...\] → $$...$$
fn standardize_raw_latex(doc: &mut Document, content_id: NodeId) {
    let all_els = doc.query_selector_all_in_subtree(content_id, "p,div,span,li,td,th,figcaption");
    for el_id in all_els {
        if has_closest(doc, el_id, &["pre", "code"]) {
            continue;
        }
        let text = doc.text_content(el_id);
        if !text.contains("\\(") && !text.contains("\\[") && !text.contains("$$") {
            continue;
        }
        if text.contains("$\\displaystyle") || text.contains("$\\") {
            continue;
        }
        let mut changed = false;
        let mut new_text = text.to_string();

        // \[...\] block LaTeX → $$...$$
        if let Ok(re_block) = regex_lite::Regex::new(r"\\\[([^\]]*)\\\]")
            && re_block.is_match(&new_text) {
                new_text = re_block.replace_all(&new_text, "$$\n$1\n$$").to_string();
                changed = true;
            }

        // \(...\) inline LaTeX → $...$
        if let Ok(re_inline) = regex_lite::Regex::new(r"\\\(([^)]*)\\\)")
            && re_inline.is_match(&new_text) {
                new_text = re_inline.replace_all(&new_text, "$$1$").to_string();
                changed = true;
            }

        if changed && new_text != text && !new_text.contains("$\\displaystyle") {
            doc.replace_with_html(el_id, &new_text);
        }
    }
}

/// Extract LaTeX from MathJax SVG (data-tex) and CHTML (data-latex).
/// Also handle mjx-container elements that embed MathML without LaTeX source.
fn standardize_mathjax_svg(doc: &mut Document, content_id: NodeId) {
    for sel in &["mjx-container", "mjx-math"] {
        let els = doc.query_selector_all_in_subtree(content_id, sel);
        for el_id in els {
            if has_closest(doc, el_id, &["pre", "code"]) {
                continue;
            }
            let latex = doc
                .get_attr(el_id, "data-tex")
                .or_else(|| doc.get_attr(el_id, "data-latex"));
            if let Some(data_tex) = latex
                && !data_tex.is_empty()
            {
                let is_block = is_block_mathjax(doc, el_id);
                let replacement = if is_block {
                    format!("$$\n{data_tex}\n$$")
                } else {
                    format!("${data_tex}$")
                };
                doc.replace_with_html(el_id, &replacement);
            }
        }
    }
}

/// Check if a .katex element (or its wrapper) is block-display math.
fn is_block_math(doc: &Document, node: NodeId) -> bool {
    // Check for displaystyle="true" on any child
    let has_ds = doc.query_selector_all_in_subtree(node, "[displaystyle=\"true\"]");
    if !has_ds.is_empty() {
        return true;
    }
    // Check if parent or grandparent has .math-block class
    let mut current = Some(node);
    for _ in 0..3 {
        if let Some(id) = current {
            if let Some(class) = doc.get_attr(id, "class") {
                let class_lower = class.to_lowercase();
                if class_lower.contains("math-block") || class_lower.contains("display") {
                    return true;
                }
            }
            current = doc.parent(id);
        } else {
            break;
        }
    }
    false
}

/// Check if a MathJax element is block display.
fn is_block_mathjax(doc: &Document, node: NodeId) -> bool {
    if let Some(display) = doc.get_attr(node, "display")
        && (display == "true" || display == "block") {
            return true;
        }
    false
}

fn has_closest(doc: &Document, node: NodeId, selectors: &[&str]) -> bool {
    selectors.iter().any(|s| doc.closest(node, s).is_some())
}
