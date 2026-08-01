use crate::content_utils::contains_ascii_case_insensitive;
use crate::dom::{Document, NodeId, SimpleSelector};
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
}

#[derive(Debug, Clone)]
pub(crate) struct AnalysisNode {
    pub dom_id: NodeId,
    pub parent: Option<usize>,
    pub first_child: Option<usize>,
    pub next_sibling: Option<usize>,
    pub depth: usize,
    pub tag: String,
    attr_start: usize,
    attr_len: usize,
    text_start: Option<usize>,
    text_len: usize,
    pub facts: AnalysisFacts,
}

#[derive(Debug, Clone)]
pub(crate) struct AnalysisSnapshot {
    root: usize,
    source_generation: u64,
    nodes: Vec<AnalysisNode>,
    attributes: Vec<(String, String)>,
    text: String,
    lookup: FxHashMap<NodeId, usize>,
}

impl AnalysisSnapshot {
    pub(crate) fn from_subtree(doc: &Document, root: NodeId) -> Self {
        Self::from_subtree_with_facts(doc, root, true)
    }

    pub(crate) fn from_subtree_topology(doc: &Document, root: NodeId) -> Self {
        Self::from_subtree_with_facts(doc, root, false)
    }

    fn from_subtree_with_facts(doc: &Document, root: NodeId, collect_facts: bool) -> Self {
        let mut snapshot = Self {
            root: 0,
            source_generation: doc.mutation_generation(),
            nodes: Vec::new(),
            attributes: Vec::new(),
            text: String::new(),
            lookup: FxHashMap::default(),
        };
        snapshot.root = snapshot.visit(doc, root, None, 0, collect_facts);
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

    pub(crate) fn contains(&self, ancestor: NodeId, descendant: NodeId) -> bool {
        let Some(&current_index) = self.lookup.get(&descendant) else {
            return false;
        };
        let mut current = current_index;
        let ancestor = self.lookup.get(&ancestor).copied();
        while let Some(index) = Some(current) {
            if Some(index) == ancestor {
                return true;
            }
            let Some(parent) = self.nodes[index].parent else {
                break;
            };
            current = parent;
        }
        false
    }

    pub(crate) fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.lookup
            .get(&node)
            .and_then(|&index| self.nodes[index].parent)
            .map(|parent| self.nodes[parent].dom_id)
    }

    pub(crate) fn is_valid_for(&self, doc: &Document) -> bool {
        self.source_generation == doc.mutation_generation()
    }

    pub(crate) fn select_tag(&self, tag: &str) -> Vec<NodeId> {
        self.nodes
            .iter()
            .filter(|node| node.tag.eq_ignore_ascii_case(tag))
            .map(|node| node.dom_id)
            .collect()
    }

    pub(crate) fn select_selector(&self, selector: &str) -> Option<Vec<NodeId>> {
        if selector.contains(['>', '+', '~']) || selector.contains(":has(") {
            return None;
        }
        let parsed = crate::css_selector::parse(selector);
        if parsed.is_empty() {
            return None;
        }
        let mut results = Vec::new();
        for node in &self.nodes {
            if parsed
                .iter()
                .any(|parts| self.matches_selector_chain(node.dom_id, parts))
            {
                results.push(node.dom_id);
            }
        }
        Some(results)
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
            snapshot: Some(self),
            source_generation: self.source_generation,
            removed: FxHashSet::default(),
            tag_overrides: FxHashMap::default(),
            attr_overrides: FxHashMap::default(),
            text_overrides: FxHashMap::default(),
            invalidated_subtrees: FxHashSet::default(),
            structural_invalidated: false,
            log: Vec::new(),
        }
    }

    fn matches_selector_chain(&self, node: NodeId, parts: &[SimpleSelector]) -> bool {
        let Some(last) = parts.last() else {
            return false;
        };
        if !self.matches_simple(node, last) {
            return false;
        }
        let mut ancestor = self.parent(node);
        for selector in parts[..parts.len() - 1].iter().rev() {
            let mut matched = false;
            while let Some(candidate) = ancestor {
                if self.matches_simple(candidate, selector) {
                    matched = true;
                    ancestor = self.parent(candidate);
                    break;
                }
                ancestor = self.parent(candidate);
            }
            if !matched {
                return false;
            }
        }
        true
    }

    fn matches_simple(&self, node: NodeId, selector: &SimpleSelector) -> bool {
        let Some(&index) = self.lookup.get(&node) else {
            return false;
        };
        let current = &self.nodes[index];
        if current.tag.is_empty() {
            return false;
        }
        if let Some(tag) = &selector.tag
            && !current.tag.eq_ignore_ascii_case(tag)
        {
            return false;
        }
        if let Some(id) = &selector.id
            && self.get_attr(node, "id") != Some(id.as_str())
        {
            return false;
        }
        for class in &selector.classes {
            if !self
                .get_attr(node, "class")
                .is_some_and(|value| value.split_whitespace().any(|item| item == class))
            {
                return false;
            }
        }
        if let Some(name) = &selector.attr_presence
            && self.get_attr(node, name).is_none()
        {
            return false;
        }
        if let Some((name, value, insensitive)) = &selector.attr_equals
            && !self.attr_matches(node, name, insensitive, |actual| {
                if *insensitive {
                    actual.eq_ignore_ascii_case(value)
                } else {
                    actual == value
                }
            })
        {
            return false;
        }
        if let Some((name, value, insensitive)) = &selector.attr_contains
            && !self.attr_matches(node, name, insensitive, |actual| {
                if *insensitive {
                    actual
                        .to_ascii_lowercase()
                        .contains(&value.to_ascii_lowercase())
                } else {
                    actual.contains(value)
                }
            })
        {
            return false;
        }
        if let Some((name, value, insensitive)) = &selector.attr_starts
            && !self.attr_matches(node, name, insensitive, |actual| {
                if *insensitive {
                    actual
                        .to_ascii_lowercase()
                        .starts_with(&value.to_ascii_lowercase())
                } else {
                    actual.starts_with(value)
                }
            })
        {
            return false;
        }
        if let Some((name, value, insensitive)) = &selector.attr_ends
            && !self.attr_matches(node, name, insensitive, |actual| {
                if *insensitive {
                    actual
                        .to_ascii_lowercase()
                        .ends_with(&value.to_ascii_lowercase())
                } else {
                    actual.ends_with(value)
                }
            })
        {
            return false;
        }
        if let Some((name, value, insensitive)) = &selector.attr_word
            && !self.attr_matches(node, name, insensitive, |actual| {
                actual.split_whitespace().any(|word| {
                    if *insensitive {
                        word.eq_ignore_ascii_case(value)
                    } else {
                        word == value
                    }
                })
            })
        {
            return false;
        }
        if selector
            .negation
            .as_ref()
            .is_some_and(|negation| self.matches_simple(node, negation))
        {
            return false;
        }
        true
    }

    fn attr_matches(
        &self,
        node: NodeId,
        name: &str,
        _insensitive: &bool,
        mut predicate: impl FnMut(&str) -> bool,
    ) -> bool {
        self.get_attr(node, name)
            .is_some_and(|value| predicate(value))
    }

    fn get_attr(&self, node: NodeId, name: &str) -> Option<&str> {
        self.lookup
            .get(&node)
            .and_then(|&index| {
                let node = &self.nodes[index];
                self.attributes[node.attr_start..node.attr_start + node.attr_len]
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case(name))
            })
            .map(|(_, value)| value.as_str())
    }

    fn text_value(&self, node: NodeId) -> Option<&str> {
        let &index = self.lookup.get(&node)?;
        let node = &self.nodes[index];
        let start = node.text_start?;
        Some(&self.text[start..start + node.text_len])
    }

    fn visit(
        &mut self,
        doc: &Document,
        node: NodeId,
        parent: Option<usize>,
        depth: usize,
        collect_facts: bool,
    ) -> usize {
        let index = self.nodes.len();
        self.lookup.insert(node, index);
        let attr_start = self.attributes.len();
        if collect_facts {
            for name in doc.attr_names(node) {
                if let Some(value) = doc.get_attr(node, &name) {
                    self.attributes.push((name, value.to_owned()));
                }
            }
        }
        let attr_len = self.attributes.len() - attr_start;
        let (text_start, text_len) = if collect_facts && let Some(text) = doc.text_node_value(node)
        {
            let text_start = self.text.len();
            self.text.push_str(text);
            (Some(text_start), text.len())
        } else {
            (None, 0)
        };
        self.nodes.push(AnalysisNode {
            dom_id: node,
            parent,
            first_child: None,
            next_sibling: None,
            depth,
            tag: doc.tag_name(node).to_owned(),
            attr_start,
            attr_len,
            text_start,
            text_len,
            facts: AnalysisFacts::default(),
        });

        let mut facts = AnalysisFacts::default();
        if let Some(text) = self.text_value(node) {
            facts.text_bytes = text.len();
            facts.comma_count = text.bytes().filter(|&byte| byte == b',').count();
        }
        let mut first_child: Option<usize> = None;
        let mut previous_child: Option<usize> = None;
        for &child in doc.children(node) {
            let child_index = self.visit(doc, child, Some(index), depth + 1, collect_facts);
            if first_child.is_none() {
                first_child = Some(child_index);
            }
            if let Some(previous_child) = previous_child {
                self.nodes[previous_child].next_sibling = Some(child_index);
            }
            previous_child = Some(child_index);
            facts.merge(&self.nodes[child_index].facts);
        }
        self.nodes[index].first_child = first_child;

        if !collect_facts {
            return index;
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
    Text {
        node: NodeId,
        previous: Option<String>,
    },
    Invalidated {
        node: NodeId,
        previous: bool,
    },
    StructuralInvalidation {
        previous: bool,
    },
}

#[derive(Debug)]
pub(crate) struct MutationOverlay<'a> {
    snapshot: Option<&'a AnalysisSnapshot>,
    source_generation: u64,
    removed: FxHashSet<NodeId>,
    tag_overrides: FxHashMap<NodeId, String>,
    attr_overrides: FxHashMap<(NodeId, String), Option<String>>,
    text_overrides: FxHashMap<NodeId, String>,
    invalidated_subtrees: FxHashSet<NodeId>,
    structural_invalidated: bool,
    log: Vec<OverlayMutation>,
}

impl<'a> MutationOverlay<'a> {
    pub(crate) fn empty(doc: &Document) -> MutationOverlay<'static> {
        MutationOverlay {
            snapshot: None,
            source_generation: doc.mutation_generation(),
            removed: FxHashSet::default(),
            tag_overrides: FxHashMap::default(),
            attr_overrides: FxHashMap::default(),
            text_overrides: FxHashMap::default(),
            invalidated_subtrees: FxHashSet::default(),
            structural_invalidated: false,
            log: Vec::new(),
        }
    }

    pub(crate) fn checkpoint(&self) -> usize {
        self.log.len()
    }

    pub(crate) fn rollback(&mut self, checkpoint: usize) {
        assert!(
            checkpoint <= self.log.len(),
            "overlay checkpoint is from the future"
        );
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
                OverlayMutation::Text { node, previous } => {
                    if let Some(previous) = previous {
                        self.text_overrides.insert(node, previous);
                    } else {
                        self.text_overrides.remove(&node);
                    }
                }
                OverlayMutation::Invalidated { node, previous } => {
                    if previous {
                        self.invalidated_subtrees.insert(node);
                    } else {
                        self.invalidated_subtrees.remove(&node);
                    }
                }
                OverlayMutation::StructuralInvalidation { previous } => {
                    self.structural_invalidated = previous;
                }
            }
        }
    }

    pub(crate) fn is_structurally_valid(&self) -> bool {
        !self.structural_invalidated
    }

    pub(crate) fn is_valid_for(&self, doc: &Document) -> bool {
        self.is_structurally_valid()
            && self
                .snapshot
                .is_none_or(|snapshot| snapshot.is_valid_for(doc))
            && self.source_generation == doc.mutation_generation()
    }

    pub(crate) fn invalidate_subtree(&mut self, node: NodeId) {
        let previous = self.invalidated_subtrees.contains(&node);
        self.log
            .push(OverlayMutation::Invalidated { node, previous });
        self.invalidated_subtrees.insert(node);
    }

    pub(crate) fn invalidate_structure(&mut self) {
        let previous = self.structural_invalidated;
        self.log
            .push(OverlayMutation::StructuralInvalidation { previous });
        self.structural_invalidated = true;
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

    pub(crate) fn set_text(&mut self, node: NodeId, text: &str) {
        let previous = self.text_overrides.get(&node).cloned();
        self.log.push(OverlayMutation::Text { node, previous });
        self.text_overrides.insert(node, text.to_owned());
        self.invalidate_subtree(node);
    }

    pub(crate) fn is_visible(&self, node: NodeId) -> bool {
        let Some(snapshot) = self.snapshot else {
            return !self.removed.contains(&node);
        };
        let Some(&current_index) = snapshot.lookup.get(&node) else {
            return false;
        };
        let mut current = current_index;
        loop {
            let current_node = snapshot.nodes[current].dom_id;
            if self.removed.contains(&current_node) {
                return false;
            }
            let Some(parent) = snapshot.nodes[current].parent else {
                return true;
            };
            current = parent;
        }
    }

    pub(crate) fn contains(&self, ancestor: NodeId, descendant: NodeId) -> bool {
        self.snapshot.is_some_and(|snapshot| {
            self.is_visible(descendant) && snapshot.contains(ancestor, descendant)
        })
    }

    pub(crate) fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.snapshot.and_then(|snapshot| snapshot.parent(node))
    }

    pub(crate) fn select_tag(&self, tag: &str) -> Vec<NodeId> {
        let Some(snapshot) = self.snapshot else {
            return Vec::new();
        };
        snapshot
            .nodes
            .iter()
            .filter(|node| {
                self.is_visible(node.dom_id) && self.tag_name(node.dom_id).eq_ignore_ascii_case(tag)
            })
            .map(|node| node.dom_id)
            .collect()
    }

    pub(crate) fn select_selector(&self, selector: &str) -> Option<Vec<NodeId>> {
        if selector.contains(['>', '+', '~']) || selector.contains(":has(") {
            return None;
        }
        let parsed = crate::css_selector::parse(selector);
        if parsed.is_empty() {
            return None;
        }
        Some(
            self.snapshot?
                .nodes
                .iter()
                .filter(|node| {
                    self.is_visible(node.dom_id)
                        && parsed
                            .iter()
                            .any(|parts| self.matches_selector_chain(node.dom_id, parts))
                })
                .map(|node| node.dom_id)
                .collect(),
        )
    }

    pub(crate) fn tag_name(&self, node: NodeId) -> &str {
        self.tag_overrides
            .get(&node)
            .map(String::as_str)
            .or_else(|| {
                self.snapshot.and_then(|snapshot| {
                    snapshot
                        .lookup
                        .get(&node)
                        .map(|&index| snapshot.nodes[index].tag.as_str())
                })
            })
            .unwrap_or("")
    }

    fn matches_selector_chain(&self, node: NodeId, parts: &[SimpleSelector]) -> bool {
        let Some(last) = parts.last() else {
            return false;
        };
        if !self.matches_simple(node, last) {
            return false;
        }
        let mut ancestor = self.parent(node);
        for selector in parts[..parts.len() - 1].iter().rev() {
            let mut matched = false;
            while let Some(candidate) = ancestor {
                if self.matches_simple(candidate, selector) {
                    matched = true;
                    ancestor = self.parent(candidate);
                    break;
                }
                ancestor = self.parent(candidate);
            }
            if !matched {
                return false;
            }
        }
        true
    }

    fn matches_simple(&self, node: NodeId, selector: &SimpleSelector) -> bool {
        let Some(snapshot) = self.snapshot else {
            return false;
        };
        let Some(&index) = snapshot.lookup.get(&node) else {
            return false;
        };
        let current = &snapshot.nodes[index];
        if current.tag.is_empty() {
            return false;
        }
        if let Some(tag) = &selector.tag
            && !self.tag_name(node).eq_ignore_ascii_case(tag)
        {
            return false;
        }
        if let Some(id) = &selector.id
            && self.get_attr(node, "id") != Some(id.as_str())
        {
            return false;
        }
        for class in &selector.classes {
            if !self
                .get_attr(node, "class")
                .is_some_and(|value| value.split_whitespace().any(|item| item == class))
            {
                return false;
            }
        }
        if let Some(name) = &selector.attr_presence
            && self.get_attr(node, name).is_none()
        {
            return false;
        }
        if let Some((name, value, insensitive)) = &selector.attr_equals
            && !self.get_attr(node, name).is_some_and(|actual| {
                if *insensitive {
                    actual.eq_ignore_ascii_case(value)
                } else {
                    actual == value
                }
            })
        {
            return false;
        }
        if let Some((name, value, insensitive)) = &selector.attr_contains
            && !self.get_attr(node, name).is_some_and(|actual| {
                if *insensitive {
                    actual
                        .to_ascii_lowercase()
                        .contains(&value.to_ascii_lowercase())
                } else {
                    actual.contains(value)
                }
            })
        {
            return false;
        }
        if let Some((name, value, insensitive)) = &selector.attr_starts
            && !self.get_attr(node, name).is_some_and(|actual| {
                if *insensitive {
                    actual
                        .to_ascii_lowercase()
                        .starts_with(&value.to_ascii_lowercase())
                } else {
                    actual.starts_with(value)
                }
            })
        {
            return false;
        }
        if let Some((name, value, insensitive)) = &selector.attr_ends
            && !self.get_attr(node, name).is_some_and(|actual| {
                if *insensitive {
                    actual
                        .to_ascii_lowercase()
                        .ends_with(&value.to_ascii_lowercase())
                } else {
                    actual.ends_with(value)
                }
            })
        {
            return false;
        }
        if let Some((name, value, insensitive)) = &selector.attr_word
            && !self.get_attr(node, name).is_some_and(|actual| {
                actual.split_whitespace().any(|word| {
                    if *insensitive {
                        word.eq_ignore_ascii_case(value)
                    } else {
                        word == value
                    }
                })
            })
        {
            return false;
        }
        if selector
            .negation
            .as_ref()
            .is_some_and(|negation| self.matches_simple(node, negation))
        {
            return false;
        }
        let _ = current;
        true
    }

    pub(crate) fn get_attr(&self, node: NodeId, name: &str) -> Option<&str> {
        let key = (node, name.to_ascii_lowercase());
        if let Some(value) = self.attr_overrides.get(&key) {
            return value.as_deref();
        }
        self.snapshot
            .and_then(|snapshot| snapshot.get_attr(node, name))
    }

    pub(crate) fn text_content(&self, node: NodeId) -> String {
        let mut text = String::new();
        self.text_content_into(node, &mut text);
        text
    }

    fn text_content_into(&self, node: NodeId, out: &mut String) {
        let Some(snapshot) = self.snapshot else {
            return;
        };
        if !self.is_visible(node) {
            return;
        }
        let Some(&index) = snapshot.lookup.get(&node) else {
            return;
        };
        if let Some(text) = self.text_overrides.get(&node) {
            out.push_str(text);
        } else if let Some(text) = snapshot.text_value(node) {
            out.push_str(text);
        }
        let mut child = snapshot.nodes[index].first_child;
        while let Some(child_index) = child {
            self.text_content_into(snapshot.nodes[child_index].dom_id, out);
            child = snapshot.nodes[child_index].next_sibling;
        }
    }

    pub(crate) fn facts(&self, node: NodeId) -> Option<AnalysisFacts> {
        let snapshot = self.snapshot?;
        let &index = snapshot.lookup.get(&node)?;
        if !self.invalidated_subtrees.is_empty()
            || !self.removed.is_empty()
            || !self.tag_overrides.is_empty()
            || !self.attr_overrides.is_empty()
            || !self.text_overrides.is_empty()
        {
            return Some(self.compute_facts(index));
        }
        Some(snapshot.nodes[index].facts.clone())
    }

    fn compute_facts(&self, index: usize) -> AnalysisFacts {
        let Some(snapshot) = self.snapshot else {
            return AnalysisFacts::default();
        };
        let node = &snapshot.nodes[index];
        if !self.is_visible(node.dom_id) {
            return AnalysisFacts::default();
        }
        let mut facts = AnalysisFacts::default();
        if let Some(text) = self
            .text_overrides
            .get(&node.dom_id)
            .map(String::as_str)
            .or_else(|| snapshot.text_value(node.dom_id))
        {
            facts.text_bytes = text.len();
            facts.comma_count = text.bytes().filter(|&byte| byte == b',').count();
        }
        let mut child = node.first_child;
        while let Some(child_index) = child {
            facts.merge(&self.compute_facts(child_index));
            child = snapshot.nodes[child_index].next_sibling;
        }
        let tag = self.tag_name(node.dom_id);
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
            if self
                .get_attr(node.dom_id, "alt")
                .is_some_and(|alt| !alt.trim().is_empty())
                && self
                    .parent(node.dom_id)
                    .is_some_and(|parent| self.tag_name(parent).eq_ignore_ascii_case("a"))
                && self
                    .parent(node.dom_id)
                    .and_then(|parent| self.get_attr(parent, "href"))
                    .is_some_and(|href| href.contains("/@"))
            {
                facts.has_author_image_link = true;
            }
        } else if tag.eq_ignore_ascii_case("table") {
            facts.table_count += 1;
        }
        facts.has_gated_content = contains_ascii_case_insensitive(
            self.get_attr(node.dom_id, "class").unwrap_or(""),
            "gated-content",
        ) || self
            .get_attr(node.dom_id, "data-schema")
            .is_some_and(|value| value == "smart-brevity");
        facts
    }

    pub(crate) fn apply_to(&self, doc: &mut Document) {
        assert!(
            self.is_valid_for(doc),
            "cannot apply a mutation overlay to a document changed after snapshot capture"
        );
        for (&node, tag) in &self.tag_overrides {
            if self.is_visible(node) {
                doc.set_tag_name(node, tag);
            }
        }
        for (&node, text) in &self.text_overrides {
            if self.is_visible(node) {
                doc.set_text(node, text);
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
                if let Some(snapshot) = self.snapshot {
                    snapshot
                        .lookup
                        .get(&node)
                        .and_then(|&index| snapshot.nodes[index].parent)
                        .is_none_or(|parent| !self.removed.contains(&snapshot.nodes[parent].dom_id))
                } else {
                    doc.parent(node)
                        .is_none_or(|parent| !self.removed.contains(&parent))
                }
            })
            .collect::<Vec<_>>();
        for node in removals {
            doc.remove(node);
        }
    }

    pub(crate) fn serialize(&self, doc: &Document, root: NodeId) -> String {
        let mut copy = doc.clone();
        self.apply_to(&mut copy);
        copy.outer_html(root)
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
        let root = doc.query_selector_all("article")[0];
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
    fn snapshot_selectors_and_overlay_facts_follow_mutations() {
        let doc = Document::parse(
            r#"<article id="root"><p class="keep">One, two</p><p class="drop">Three</p></article>"#,
        );
        let root = doc.root();
        let snapshot = AnalysisSnapshot::from_subtree(&doc, root);
        let paragraphs = snapshot
            .select_selector("p[class]")
            .expect("supported selector");
        assert_eq!(paragraphs.len(), 2);
        assert!(snapshot.select_selector("article > p").is_none());
        let text_node = snapshot
            .nodes()
            .iter()
            .find(|node| snapshot.text_value(node.dom_id).is_some())
            .expect("text node")
            .dom_id;

        let mut overlay = snapshot.overlay();
        overlay.remove(paragraphs[1]);
        overlay.set_attr(paragraphs[0], "data-kept", "yes");
        overlay.set_text(text_node, "Changed");
        assert_eq!(
            overlay.select_selector("p[data-kept]").unwrap(),
            vec![paragraphs[0]]
        );
        assert_eq!(overlay.text_content(root), "Changed");
        assert_eq!(overlay.facts(root).unwrap().text_bytes, "Changed".len());
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

    #[test]
    fn overlay_serialization_includes_pending_state() {
        let doc = Document::parse("<article><p>Keep</p><p>Drop</p></article>");
        let root = doc.query_selector_all("article")[0];
        let snapshot = AnalysisSnapshot::from_subtree(&doc, root);
        let paragraphs = snapshot.select_tag("p");
        let mut overlay = snapshot.overlay();
        overlay.remove(paragraphs[1]);
        assert_eq!(
            overlay.serialize(&doc, root),
            "<article><p>Keep</p></article>"
        );
    }

    #[test]
    fn empty_overlay_plans_removals_without_snapshot_facts() {
        let mut doc = Document::parse("<article><p>Keep</p><p>Drop</p></article>");
        let paragraph = doc.query_selector_all("p")[1];
        let mut overlay = MutationOverlay::empty(&doc);
        overlay.remove(paragraph);
        overlay.apply_to(&mut doc);
        assert_eq!(doc.query_selector_all("p").len(), 1);
    }

    #[test]
    fn snapshot_detects_dom_mutations_after_capture() {
        let mut doc = Document::parse("<article><p>Keep</p></article>");
        let snapshot = AnalysisSnapshot::from_subtree(&doc, doc.root());
        assert!(snapshot.is_valid_for(&doc));
        let paragraph = doc.query_selector_all("p")[0];
        doc.set_attr(paragraph, "data-seen", "yes");
        assert!(!snapshot.is_valid_for(&doc));
    }
}
