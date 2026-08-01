use crate::content_utils::contains_ascii_case_insensitive;
use crate::dom::{Document, NodeId};
use rustc_hash::{FxHashMap, FxHashSet};

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

    pub(crate) fn overlay(&self) -> MutationOverlay<'_> {
        MutationOverlay {
            snapshot: self,
            removed: FxHashSet::default(),
            tag_overrides: FxHashMap::default(),
            attr_overrides: FxHashMap::default(),
            log: Vec::new(),
        }
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

#[derive(Debug, Clone)]
enum OverlayMutation {
    Removed {
        node: NodeId,
        previous: bool,
    },
    Tag {
        node: NodeId,
        previous: Option<String>,
    },
    Attribute {
        node: NodeId,
        name: String,
        previous: Option<Option<String>>,
    },
}

#[derive(Debug)]
pub(crate) struct MutationOverlay<'a> {
    snapshot: &'a AnalysisSnapshot,
    removed: FxHashSet<NodeId>,
    tag_overrides: FxHashMap<NodeId, String>,
    attr_overrides: FxHashMap<(NodeId, String), Option<String>>,
    log: Vec<OverlayMutation>,
}

impl<'a> MutationOverlay<'a> {
    pub(crate) fn checkpoint(&self) -> usize {
        self.log.len()
    }

    pub(crate) fn rollback(&mut self, checkpoint: usize) {
        while self.log.len() > checkpoint {
            match self.log.pop().expect("overlay log length checked") {
                OverlayMutation::Removed { node, previous } => {
                    if previous {
                        self.removed.insert(node);
                    } else {
                        self.removed.remove(&node);
                    }
                }
                OverlayMutation::Tag { node, previous } => {
                    if let Some(previous) = previous {
                        self.tag_overrides.insert(node, previous);
                    } else {
                        self.tag_overrides.remove(&node);
                    }
                }
                OverlayMutation::Attribute {
                    node,
                    name,
                    previous,
                } => {
                    let key = (node, name);
                    if let Some(previous) = previous {
                        self.attr_overrides.insert(key, previous);
                    } else {
                        self.attr_overrides.remove(&key);
                    }
                }
            }
        }
    }

    pub(crate) fn remove(&mut self, node: NodeId) {
        let previous = self.removed.contains(&node);
        self.log.push(OverlayMutation::Removed { node, previous });
        self.removed.insert(node);
    }

    pub(crate) fn set_tag_name(&mut self, node: NodeId, tag: &str) {
        let previous = self.tag_overrides.get(&node).cloned();
        self.log.push(OverlayMutation::Tag { node, previous });
        self.tag_overrides.insert(node, tag.to_owned());
    }

    pub(crate) fn set_attr(&mut self, node: NodeId, name: &str, value: &str) {
        let key = (node, name.to_ascii_lowercase());
        let previous = self.attr_overrides.get(&key).cloned();
        self.log.push(OverlayMutation::Attribute {
            node,
            name: key.1.clone(),
            previous,
        });
        self.attr_overrides.insert(key, Some(value.to_owned()));
    }

    pub(crate) fn remove_attr(&mut self, node: NodeId, name: &str) {
        let key = (node, name.to_ascii_lowercase());
        let previous = self.attr_overrides.get(&key).cloned();
        self.log.push(OverlayMutation::Attribute {
            node,
            name: key.1.clone(),
            previous,
        });
        self.attr_overrides.insert(key, None);
    }

    pub(crate) fn is_visible(&self, node: NodeId) -> bool {
        let Some(&current_index) = self.snapshot.lookup.get(&node) else {
            return false;
        };
        let mut current = current_index;
        loop {
            let current_node = self.snapshot.nodes[current].dom_id;
            if self.removed.contains(&current_node) {
                return false;
            }
            let Some(parent) = self.snapshot.nodes[current].parent else {
                return true;
            };
            current = parent;
        }
    }

    pub(crate) fn select_tag(&self, tag: &str) -> Vec<NodeId> {
        self.snapshot
            .nodes
            .iter()
            .filter(|node| {
                self.is_visible(node.dom_id) && self.tag_name(node.dom_id).eq_ignore_ascii_case(tag)
            })
            .map(|node| node.dom_id)
            .collect()
    }

    pub(crate) fn tag_name(&self, node: NodeId) -> &str {
        self.tag_overrides
            .get(&node)
            .map(String::as_str)
            .or_else(|| {
                self.snapshot
                    .lookup
                    .get(&node)
                    .map(|&index| self.snapshot.nodes[index].tag.as_str())
            })
            .unwrap_or("")
    }

    pub(crate) fn apply_to(&self, doc: &mut Document) {
        for (&node, tag) in &self.tag_overrides {
            if self.is_visible(node) {
                doc.set_tag_name(node, tag);
            }
        }
        for (&(node, ref name), value) in &self.attr_overrides {
            if self.is_visible(node) {
                if let Some(value) = value {
                    doc.set_attr(node, name, value);
                } else {
                    doc.remove_attr(node, name);
                }
            }
        }
        let removals = self
            .removed
            .iter()
            .copied()
            .filter(|&node| {
                self.snapshot
                    .lookup
                    .get(&node)
                    .and_then(|&index| self.snapshot.nodes[index].parent)
                    .is_none_or(|parent| {
                        !self.removed.contains(&self.snapshot.nodes[parent].dom_id)
                    })
            })
            .collect::<Vec<_>>();
        for node in removals {
            doc.remove(node);
        }
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

    #[test]
    fn mutation_overlay_checkpoints_and_applies() {
        let mut doc =
            Document::parse("<article><h2>Title</h2><p id=keep>Keep</p><p>Drop</p></article>");
        let root = doc.root();
        let snapshot = AnalysisSnapshot::from_subtree(&doc, root);
        let paragraphs = snapshot.select_tag("p");
        let heading = snapshot.select_tag("h2").into_iter().next().unwrap();
        let checkpoint;

        let mut overlay = snapshot.overlay();
        checkpoint = overlay.checkpoint();
        overlay.remove(paragraphs[1]);
        overlay.set_tag_name(heading, "h3");
        overlay.set_attr(paragraphs[0], "data-kept", "yes");
        assert!(!overlay.is_visible(paragraphs[1]));
        assert_eq!(overlay.tag_name(heading), "h3");
        assert_eq!(overlay.select_tag("p").len(), 1);

        overlay.rollback(checkpoint);
        assert!(overlay.is_visible(paragraphs[1]));
        assert_eq!(overlay.tag_name(heading), "h2");
        assert_eq!(overlay.select_tag("p").len(), 2);

        overlay.remove(paragraphs[1]);
        overlay.set_tag_name(heading, "h3");
        overlay.set_attr(paragraphs[0], "data-kept", "yes");
        overlay.apply_to(&mut doc);
        assert!(doc.query_selector_all("h3").len() == 1);
        assert!(doc.query_selector_all("#keep[data-kept=\"yes\"]").len() == 1);
        assert!(doc.query_selector_all("p").len() == 1);
    }
}
