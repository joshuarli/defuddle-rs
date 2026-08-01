use crate::content_utils::contains_ascii_case_insensitive;
use crate::dom::{Document, NodeId};
use rustc_hash::FxHashMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AnalysisFacts {
    pub text_bytes: usize,
    pub comma_count: usize,
    pub paragraph_count: usize,
    pub link_count: usize,
    pub unordered_list_count: usize,
    pub ordered_list_count: usize,
    pub image_count: usize,
    pub table_count: usize,
    pub link_text_bytes: usize,
    pub has_gated_content: bool,
    pub has_author_image_link: bool,
}

impl AnalysisFacts {
    fn merge(&mut self, child: &Self) {
        self.text_bytes += child.text_bytes;
        self.comma_count += child.comma_count;
        self.paragraph_count += child.paragraph_count;
        self.link_count += child.link_count;
        self.unordered_list_count += child.unordered_list_count;
        self.ordered_list_count += child.ordered_list_count;
        self.image_count += child.image_count;
        self.table_count += child.table_count;
        self.link_text_bytes += child.link_text_bytes;
        self.has_gated_content |= child.has_gated_content;
        self.has_author_image_link |= child.has_author_image_link;
    }

    pub(crate) fn descendant_count(
        &self,
        doc: &Document,
        node: NodeId,
        total: usize,
        tag: &str,
    ) -> usize {
        total.saturating_sub(usize::from(doc.tag_name_is(node, tag)))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AnalysisNode {
    pub dom_id: NodeId,
    pub parent: Option<usize>,
    pub depth: usize,
    pub tag: String,
    pub facts: AnalysisFacts,
}

#[derive(Debug, Clone)]
pub(crate) struct AnalysisSnapshot {
    root: usize,
    nodes: Vec<AnalysisNode>,
    lookup: FxHashMap<NodeId, usize>,
}

impl AnalysisSnapshot {
    pub(crate) fn from_subtree(doc: &Document, root: NodeId) -> Self {
        let mut snapshot = Self {
            root: 0,
            nodes: Vec::new(),
            lookup: FxHashMap::default(),
        };
        snapshot.root = snapshot.visit(doc, root, None, 0);
        snapshot
    }

    pub(crate) fn root(&self) -> NodeId {
        self.nodes[self.root].dom_id
    }

    pub(crate) fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub(crate) fn nodes(&self) -> &[AnalysisNode] {
        &self.nodes
    }

    pub(crate) fn facts(&self, node: NodeId) -> Option<&AnalysisFacts> {
        self.lookup
            .get(&node)
            .map(|&index| &self.nodes[index].facts)
    }

    pub(crate) fn select_tag(&self, tag: &str) -> Vec<NodeId> {
        self.nodes
            .iter()
            .filter(|node| node.tag.eq_ignore_ascii_case(tag))
            .map(|node| node.dom_id)
            .collect()
    }

    pub(crate) fn descendant_count(&self, node: NodeId, tag: &str) -> Option<usize> {
        let facts = self.facts(node)?;
        let total = match tag {
            "p" => facts.paragraph_count,
            "a" => facts.link_count,
            "ul" => facts.unordered_list_count,
            "ol" => facts.ordered_list_count,
            "img" => facts.image_count,
            "table" => facts.table_count,
            _ => return None,
        };
        let self_count = self
            .lookup
            .get(&node)
            .and_then(|&index| self.nodes.get(index))
            .is_some_and(|current| current.tag.eq_ignore_ascii_case(tag));
        Some(total.saturating_sub(usize::from(self_count)))
    }

    fn visit(
        &mut self,
        doc: &Document,
        node: NodeId,
        parent: Option<usize>,
        depth: usize,
    ) -> usize {
        let index = self.nodes.len();
        self.lookup.insert(node, index);
        self.nodes.push(AnalysisNode {
            dom_id: node,
            parent,
            depth,
            tag: doc.tag_name(node).to_owned(),
            facts: AnalysisFacts::default(),
        });

        let mut facts = AnalysisFacts::default();
        if let Some(text) = doc.text_node_value(node) {
            facts.text_bytes = text.len();
            facts.comma_count = text.bytes().filter(|&byte| byte == b',').count();
        }
        for &child in doc.children(node) {
            let child_index = self.visit(doc, child, Some(index), depth + 1);
            facts.merge(&self.nodes[child_index].facts);
        }

        let tag = self.nodes[index].tag.as_str();
        if tag.eq_ignore_ascii_case("p") {
            facts.paragraph_count += 1;
        } else if tag.eq_ignore_ascii_case("a") {
            facts.link_count += 1;
            facts.link_text_bytes += facts.text_bytes;
        } else if tag.eq_ignore_ascii_case("ul") {
            facts.unordered_list_count += 1;
        } else if tag.eq_ignore_ascii_case("ol") {
            facts.ordered_list_count += 1;
        } else if tag.eq_ignore_ascii_case("img") {
            facts.image_count += 1;
            if doc
                .get_attr(node, "alt")
                .is_some_and(|alt| !alt.trim().is_empty())
                && doc.closest(node, "a").is_some_and(|anchor| {
                    doc.get_attr(anchor, "href")
                        .is_some_and(|href| href.contains("/@"))
                })
            {
                facts.has_author_image_link = true;
            }
        } else if tag.eq_ignore_ascii_case("table") {
            facts.table_count += 1;
        }

        facts.has_gated_content = contains_ascii_case_insensitive(
            doc.get_attr(node, "class").unwrap_or(""),
            "gated-content",
        ) || doc
            .get_attr(node, "data-schema")
            .is_some_and(|value| value == "smart-brevity");
        self.nodes[index].facts = facts;
        index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_matches_dom_subtree_facts() {
        let doc = Document::parse(
            r#"<article class="content"><h2>Related links</h2><p>One, two</p><p><a href="/@person">Profile</a></p><ul><li>Item</li></ul><a href="/@person"><img alt="portrait"></a></article>"#,
        );
        let root = doc.root();
        let snapshot = AnalysisSnapshot::from_subtree(&doc, root);

        for node in snapshot.nodes() {
            let mut text = String::new();
            doc.text_content_into(node.dom_id, &mut text);
            let facts = snapshot.facts(node.dom_id).expect("snapshot facts");
            assert_eq!(facts.text_bytes, text.len(), "node {:?}", node.dom_id);
            assert_eq!(
                facts.comma_count,
                text.bytes().filter(|&byte| byte == b',').count(),
                "node {:?}",
                node.dom_id
            );
        }

        for tag in ["p", "a", "ul", "img"] {
            assert_eq!(
                snapshot.select_tag(tag).len(),
                doc.query_selector_all_in_subtree(root, tag).len(),
                "tag {tag}"
            );
        }
        let root_facts = snapshot.facts(root).expect("root facts");
        assert_eq!(root_facts.paragraph_count, 2);
        assert_eq!(root_facts.link_count, 2);
        assert_eq!(root_facts.unordered_list_count, 1);
        assert_eq!(root_facts.image_count, 1);
        assert_eq!(root_facts.link_text_bytes, "Profile".len());
        assert!(root_facts.has_author_image_link);
    }

    #[test]
    fn snapshot_construction_cost_is_visible() {
        let html =
            include_str!("../tests/fixtures/Claude Code is the Inflection Point.substack.html");
        let doc = Document::parse(html);
        let root = doc
            .query_selector_all("body")
            .first()
            .copied()
            .unwrap_or(doc.root());
        let start = std::time::Instant::now();
        let mut node_count = 0;
        for _ in 0..10 {
            node_count =
                std::hint::black_box(AnalysisSnapshot::from_subtree(&doc, root).node_count());
        }
        eprintln!(
            "analysis snapshot: {} nodes, {:.3} ms per build",
            node_count,
            start.elapsed().as_secs_f64() * 100.0
        );
        assert!(node_count > 0);
    }
}
