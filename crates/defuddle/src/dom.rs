use html5ever::interface::{ElementFlags, NodeOrText, QuirksMode};
use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::tree_builder::{ElemName, TreeSink};
use html5ever::{Attribute, LocalName, Namespace, QualName};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::cell::RefCell;
use std::mem;
use std::num::NonZeroUsize;
use std::sync::Arc;

#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    static PARSE_COUNT: Cell<usize> = const { Cell::new(0) };
    static SERIALIZE_COUNT: Cell<usize> = const { Cell::new(0) };
}

type ParsedSelector = Vec<Vec<SimpleSelector>>;

fn parse_selector(selector: &str) -> ParsedSelector {
    selector
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.split_whitespace()
                .filter_map(crate::css_selector::parse_one)
                .collect()
        })
        .filter(|parts: &Vec<SimpleSelector>| !parts.is_empty())
        .collect()
}

#[derive(Debug)]
pub struct Document {
    nodes: Vec<Node>,
    root: NodeId,
    selector_cache: RefCell<FxHashMap<String, Arc<ParsedSelector>>>,
}

impl Clone for Document {
    fn clone(&self) -> Self {
        Self {
            nodes: self.nodes.clone(),
            root: self.root,
            selector_cache: RefCell::new(FxHashMap::default()),
        }
    }
}

#[derive(Default)]
struct TrimmedTextStats {
    byte_len: usize,
    word_count: usize,
    pending_whitespace: usize,
    has_text: bool,
    in_word: bool,
}

impl Document {
    pub(crate) fn empty() -> Self {
        Self {
            nodes: vec![Node {
                parent: NodeParent::new(None),
                children: SmallVec::new(),
                kind: NodeKind::Document,
            }],
            root: NodeId(0),
            selector_cache: RefCell::new(FxHashMap::default()),
        }
    }

    #[cfg(test)]
    pub(crate) fn reset_test_counters() {
        PARSE_COUNT.with(|count| count.set(0));
        SERIALIZE_COUNT.with(|count| count.set(0));
    }

    #[cfg(test)]
    pub(crate) fn test_parse_count() -> usize {
        PARSE_COUNT.with(Cell::get)
    }

    #[cfg(test)]
    pub(crate) fn test_serialize_count() -> usize {
        SERIALIZE_COUNT.with(Cell::get)
    }

    fn parsed_selector(&self, selector: &str) -> Arc<ParsedSelector> {
        if let Some(parsed) = self.selector_cache.borrow().get(selector).cloned() {
            return parsed;
        }

        let parsed = Arc::new(parse_selector(selector));
        self.selector_cache
            .borrow_mut()
            .entry(selector.to_string())
            .or_insert_with(|| Arc::clone(&parsed))
            .clone()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub usize);

#[derive(Debug, Clone)]
enum NodeKind {
    Document,
    Doctype,
    Element {
        name: LocalName,
        attrs: Vec<(LocalName, String)>,
    },
    Text(String),
    Comment(String),
    ProcessingInstruction,
    Removed,
}

struct ParseSink {
    document: RefCell<Document>,
}

impl ParseSink {
    fn new() -> Self {
        Self::with_node_capacity(0)
    }

    fn with_node_capacity(node_capacity: usize) -> Self {
        let mut document = Document {
            nodes: Vec::with_capacity(node_capacity),
            root: NodeId(0),
            selector_cache: RefCell::new(FxHashMap::default()),
        };
        let root = document.push_node(NodeKind::Document, None);
        document.root = root;
        Self {
            document: RefCell::new(document),
        }
    }

    fn append_node(&self, parent: NodeId, child: NodeId) {
        let mut document = self.document.borrow_mut();
        if let Some(old_parent) = document.node(child).parent.get() {
            document
                .node_mut(old_parent)
                .children
                .retain(|id| *id != child);
        }
        let child_text = match document.node(child).kind {
            NodeKind::Text(_) => Some(mem::replace(
                &mut document.node_mut(child).kind,
                NodeKind::Removed,
            )),
            _ => None,
        };
        if let Some(NodeKind::Text(text)) = child_text {
            if let Some(&last) = document.node(parent).children.last()
                && last != child
                && let NodeKind::Text(last_text) = &mut document.node_mut(last).kind
            {
                last_text.push_str(&text);
                return;
            }
            document.node_mut(child).kind = NodeKind::Text(text);
        }
        document.node_mut(child).parent = NodeParent::new(Some(parent));
        document.node_mut(parent).children.push(child);
    }

    fn append_text(&self, parent: NodeId, text: &str) {
        let mut document = self.document.borrow_mut();
        if let Some(&last) = document.node(parent).children.last()
            && let NodeKind::Text(last_text) = &mut document.node_mut(last).kind
        {
            last_text.push_str(text);
            return;
        }
        let child = document.push_node(NodeKind::Text(text.to_string()), Some(parent));
        debug_assert_eq!(document.node(parent).children.last(), Some(&child));
    }

    fn append_before(&self, sibling: NodeId, child: NodeId) {
        let mut document = self.document.borrow_mut();
        let Some(parent) = document.node(sibling).parent.get() else {
            return;
        };
        if let Some(old_parent) = document.node(child).parent.get() {
            document
                .node_mut(old_parent)
                .children
                .retain(|id| *id != child);
        }
        let Some(position) = document
            .node(parent)
            .children
            .iter()
            .position(|&id| id == sibling)
        else {
            return;
        };
        document.node_mut(child).parent = NodeParent::new(Some(parent));
        document.node_mut(parent).children.insert(position, child);
    }
}

#[derive(Debug)]
struct OwnedElemName {
    namespace: Namespace,
    local: LocalName,
}

impl ElemName for OwnedElemName {
    fn ns(&self) -> &Namespace {
        &self.namespace
    }

    fn local_name(&self) -> &LocalName {
        &self.local
    }
}

impl TreeSink for ParseSink {
    type Handle = NodeId;
    type Output = Document;
    type ElemName<'a> = OwnedElemName;

    fn finish(self) -> Document {
        self.document.into_inner()
    }

    fn parse_error(&self, _msg: std::borrow::Cow<'static, str>) {}

    fn get_document(&self) -> NodeId {
        self.document.borrow().root
    }

    fn elem_name<'a>(&'a self, target: &'a NodeId) -> OwnedElemName {
        let document = self.document.borrow();
        OwnedElemName {
            namespace: html5ever::ns!(html).clone(),
            local: document.tag_name(*target).into(),
        }
    }

    fn create_element(
        &self,
        name: QualName,
        attrs: Vec<Attribute>,
        _flags: ElementFlags,
    ) -> NodeId {
        let attrs = attrs
            .into_iter()
            .map(|attr| (attr.name.local, attr.value.to_string()))
            .collect();
        self.document.borrow_mut().push_node(
            NodeKind::Element {
                name: name.local,
                attrs,
            },
            None,
        )
    }

    fn create_comment(&self, text: StrTendril) -> NodeId {
        self.document
            .borrow_mut()
            .push_node(NodeKind::Comment(text.to_string()), None)
    }

    fn create_pi(&self, _target: StrTendril, _data: StrTendril) -> NodeId {
        self.document
            .borrow_mut()
            .push_node(NodeKind::ProcessingInstruction, None)
    }

    fn append(&self, parent: &NodeId, child: NodeOrText<NodeId>) {
        match child {
            NodeOrText::AppendNode(child) => self.append_node(*parent, child),
            NodeOrText::AppendText(text) => self.append_text(*parent, &text),
        }
    }

    fn append_based_on_parent_node(
        &self,
        element: &NodeId,
        prev_element: &NodeId,
        child: NodeOrText<NodeId>,
    ) {
        let parent = self
            .document
            .borrow()
            .node(*element)
            .parent
            .get()
            .or_else(|| self.document.borrow().node(*prev_element).parent.get());
        if let Some(parent) = parent {
            self.append(&parent, child);
        }
    }

    fn append_doctype_to_document(
        &self,
        _name: StrTendril,
        _public_id: StrTendril,
        _system_id: StrTendril,
    ) {
        let root = self.get_document();
        let node = self
            .document
            .borrow_mut()
            .push_node(NodeKind::Doctype, None);
        self.append_node(root, node);
    }

    fn get_template_contents(&self, target: &NodeId) -> NodeId {
        *target
    }

    fn same_node(&self, x: &NodeId, y: &NodeId) -> bool {
        x == y
    }

    fn set_quirks_mode(&self, _mode: QuirksMode) {}

    fn append_before_sibling(&self, sibling: &NodeId, new_node: NodeOrText<NodeId>) {
        match new_node {
            NodeOrText::AppendNode(child) => self.append_before(*sibling, child),
            NodeOrText::AppendText(text) => {
                let mut document = self.document.borrow_mut();
                let Some(parent) = document.node(*sibling).parent.get() else {
                    return;
                };
                let Some(position) = document
                    .node(parent)
                    .children
                    .iter()
                    .position(|&id| id == *sibling)
                else {
                    return;
                };
                let child = document.push_node(NodeKind::Text(text.to_string()), Some(parent));
                let end = document.node(parent).children.len() - 1;
                document.node_mut(parent).children.remove(end);
                document.node_mut(parent).children.insert(position, child);
            }
        }
    }

    fn add_attrs_if_missing(&self, target: &NodeId, attrs: Vec<Attribute>) {
        let mut document = self.document.borrow_mut();
        let NodeKind::Element {
            attrs: existing, ..
        } = &mut document.node_mut(*target).kind
        else {
            return;
        };
        for attr in attrs {
            if !existing
                .iter()
                .any(|(name, _)| atom_str(name).eq_ignore_ascii_case(atom_str(&attr.name.local)))
            {
                existing.push((attr.name.local, attr.value.to_string()));
            }
        }
    }

    fn remove_from_parent(&self, target: &NodeId) {
        let mut document = self.document.borrow_mut();
        if let Some(parent) = document.node(*target).parent.get() {
            document
                .node_mut(parent)
                .children
                .retain(|id| *id != *target);
        }
        document.node_mut(*target).parent = NodeParent::new(None);
    }

    fn reparent_children(&self, node: &NodeId, new_parent: &NodeId) {
        let children = self.document.borrow().node(*node).children.clone();
        for child in children {
            self.append(new_parent, NodeOrText::AppendNode(child));
        }
        self.document.borrow_mut().node_mut(*node).children.clear();
    }
}

#[derive(Debug, Clone)]
struct Node {
    parent: NodeParent,
    children: SmallVec<[NodeId; 1]>,
    kind: NodeKind,
}

#[derive(Debug, Clone, Copy)]
struct NodeParent(Option<NonZeroUsize>);

impl NodeParent {
    fn new(parent: Option<NodeId>) -> Self {
        Self(parent.map(|parent| NonZeroUsize::new(parent.0 + 1).unwrap()))
    }

    fn get(self) -> Option<NodeId> {
        self.0.map(|parent| NodeId(parent.get() - 1))
    }
}

impl Document {
    pub fn parse(html: &str) -> Self {
        #[cfg(test)]
        PARSE_COUNT.with(|count| count.set(count.get() + 1));
        let node_capacity = html
            .as_bytes()
            .windows(2)
            .filter(|pair| {
                pair[0] == b'<' && (pair[1].is_ascii_alphabetic() || b"/!?".contains(&pair[1]))
            })
            .count()
            .saturating_add(4)
            * 95
            / 100;
        let sink = if node_capacity > 2_048 {
            ParseSink::with_node_capacity(node_capacity)
        } else {
            ParseSink::new()
        };
        html5ever::parse_document(sink, Default::default()).one(html)
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    pub fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.node(node).parent.get()
    }

    pub fn children(&self, node: NodeId) -> &[NodeId] {
        &self.node(node).children
    }

    pub fn append_node(&mut self, parent: NodeId, child: NodeId) {
        if self.is_removed(parent)
            || self.is_removed(child)
            || parent == child
            || self.contains(child, parent)
        {
            return;
        }
        if let Some(old_parent) = self.node(child).parent.get() {
            self.node_mut(old_parent)
                .children
                .retain(|id| *id != child);
        }
        self.node_mut(child).parent = NodeParent::new(Some(parent));
        self.node_mut(parent).children.push(child);
    }

    pub fn append_text(&mut self, parent: NodeId, text: &str) {
        if self.is_removed(parent) || text.is_empty() {
            return;
        }
        if let Some(&last) = self.node(parent).children.last()
            && let NodeKind::Text(last_text) = &mut self.node_mut(last).kind
        {
            last_text.push_str(text);
            return;
        }
        let child = self.push_node(NodeKind::Text(text.to_string()), Some(parent));
        debug_assert_eq!(self.node(parent).children.last(), Some(&child));
    }

    pub(crate) fn create_element(
        &mut self,
        parent: NodeId,
        name: &str,
        attrs: &[(&str, &str)],
    ) -> NodeId {
        let node = self.push_node(
            NodeKind::Element {
                name: name.into(),
                attrs: attrs
                    .iter()
                    .map(|(name, value)| ((*name).into(), (*value).to_string()))
                    .collect(),
            },
            Some(parent),
        );
        debug_assert_eq!(self.parent(node), Some(parent));
        node
    }

    pub(crate) fn clone_node_from(
        &mut self,
        source: &Document,
        source_id: NodeId,
        parent: NodeId,
    ) -> NodeId {
        self.clone_subtree_from(source, source_id, Some(parent))
    }

    pub(crate) fn clone_subtree(source: &Document, source_id: NodeId) -> (Self, NodeId) {
        let mut document = Self::empty();
        let root = document.clone_subtree_from(source, source_id, None);
        document.root = root;
        (document, root)
    }

    pub fn tag_name(&self, node: NodeId) -> &str {
        match &self.node(node).kind {
            NodeKind::Element { name, .. } => atom_str(name),
            _ => "",
        }
    }

    /// Case-insensitive tag name comparison. Returns true if the element's
    /// tag name equals `name` ignoring ASCII case.
    pub fn tag_name_is(&self, node: NodeId, name: &str) -> bool {
        match &self.node(node).kind {
            NodeKind::Element { name: n, .. } => atom_str(n).eq_ignore_ascii_case(name),
            _ => false,
        }
    }

    /// Case-insensitive tag name check against multiple candidates.
    /// Returns true if the element's tag name matches any of `names`.
    pub fn tag_name_is_any(&self, node: NodeId, names: &[&str]) -> bool {
        match &self.node(node).kind {
            NodeKind::Element { name: n, .. } => {
                names
                    .iter()
                    .any(|&name| atom_str(n).eq_ignore_ascii_case(name))
            }
            _ => false,
        }
    }

    pub fn is_comment(&self, node: NodeId) -> bool {
        matches!(&self.node(node).kind, NodeKind::Comment(_))
    }

    pub fn is_text(&self, node: NodeId) -> bool {
        matches!(&self.node(node).kind, NodeKind::Text(_))
    }

    pub fn is_element(&self, node: NodeId) -> bool {
        matches!(&self.node(node).kind, NodeKind::Element { .. })
    }

    pub fn is_removed(&self, node: NodeId) -> bool {
        matches!(self.node(node).kind, NodeKind::Removed)
    }

    pub fn query_selector_all(&self, selector: &str) -> Vec<NodeId> {
        let mut results: Vec<NodeId> = Vec::new();
        self.query_selector_all_into(selector, &mut results);
        results
    }

    pub(crate) fn query_selector_all_into(&self, selector: &str, results: &mut Vec<NodeId>) {
        results.clear();
        for parts in self.parsed_selector(selector).iter() {
            results.extend(
                self.live_node_ids()
                    .filter(|&id| self.matches_selector_chain(id, parts)),
            );
        }
    }

    pub(crate) fn query_selector_union_optimized_into(
        &self,
        selectors: &[&str],
        results: &mut Vec<NodeId>,
    ) {
        results.clear();
        let parsed = selectors
            .iter()
            .map(|selector| self.parsed_selector(selector))
            .collect::<Vec<_>>();
        let mut wildcard: Vec<&[SimpleSelector]> = Vec::new();
        let mut by_tag: FxHashMap<String, Vec<&[SimpleSelector]>> = FxHashMap::default();
        for selector in &parsed {
            for parts in selector.iter() {
                if let Some(tag) = parts.last().and_then(|part| part.tag.as_ref()) {
                    by_tag
                        .entry(tag.to_ascii_lowercase())
                        .or_default()
                        .push(parts);
                } else {
                    wildcard.push(parts);
                }
            }
        }
        for id in self.live_node_ids() {
            let NodeKind::Element { name, .. } = &self.node(id).kind else {
                continue;
            };
            let tag_candidates = by_tag
                .get(atom_str(name))
                .into_iter()
                .flat_map(|parts| parts.iter().copied());
            if wildcard
                .iter()
                .copied()
                .chain(tag_candidates)
                .any(|parts| self.matches_selector_chain(id, parts))
            {
                results.push(id);
            }
        }
    }

    pub(crate) fn element_ids_with_any_attribute_into(
        &self,
        attributes: &[&str],
        results: &mut Vec<NodeId>,
    ) {
        results.clear();
        for id in self.live_node_ids() {
            if !self.is_element(id) {
                continue;
            }
            if attributes
                .iter()
                .any(|&attribute| self.get_attr(id, attribute).is_some())
            {
                results.push(id);
            }
        }
    }

    /// Like query_selector_all but only searches within a subtree.
    pub fn query_selector_all_in_subtree(&self, root: NodeId, selector: &str) -> Vec<NodeId> {
        let mut results: Vec<NodeId> = Vec::new();
        self.query_selector_all_in_subtree_into(root, selector, &mut results);
        results
    }

    pub(crate) fn query_selector_all_in_subtree_into(
        &self,
        root: NodeId,
        selector: &str,
        results: &mut Vec<NodeId>,
    ) {
        results.clear();
        for parts in self.parsed_selector(selector).iter() {
            self.collect_matching_subtree(root, parts, results);
        }
    }

    pub fn has_selector_in_subtree(&self, root: NodeId, selector: &str) -> bool {
        self.parsed_selector(selector)
            .iter()
            .any(|parts| self.subtree_has_match(root, parts))
    }

    pub fn count_selector_in_subtree(&self, root: NodeId, selector: &str) -> usize {
        self.parsed_selector(selector)
            .iter()
            .map(|parts| self.subtree_match_count(root, parts))
            .sum()
    }

    pub fn closest(&self, node: NodeId, selector: &str) -> Option<NodeId> {
        let selector = crate::css_selector::parse_one(selector)?;
        let mut current = Some(node);
        while let Some(id) = current {
            if self.matches_simple(id, &selector) {
                return Some(id);
            }
            current = self.node(id).parent.get();
        }
        None
    }

    pub fn contains(&self, ancestor: NodeId, descendant: NodeId) -> bool {
        let mut current = Some(descendant);
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            current = self.node(id).parent.get();
        }
        false
    }

    pub fn next_element_sibling(&self, node: NodeId) -> Option<NodeId> {
        if let Some(parent) = self.parent(node) {
            let children = self.children(parent);
            let pos = children.iter().position(|&c| c == node)?;
            children
                .iter()
                .skip(pos + 1)
                .copied()
                .find(|id| self.is_element(*id))
        } else {
            None
        }
    }

    pub fn prev_element_sibling(&self, node: NodeId) -> Option<NodeId> {
        let parent = self.parent(node)?;
        let children = self.children(parent);
        let pos = children.iter().position(|&c| c == node)?;
        children[..pos]
            .iter()
            .rev()
            .copied()
            .find(|id| self.is_element(*id))
    }

    pub fn has_closest(&self, node: NodeId, selectors: &[&str]) -> bool {
        selectors
            .iter()
            .any(|selector| self.closest(node, selector).is_some())
    }

    pub fn text_content(&self, node: NodeId) -> String {
        let mut out = String::new();
        self.text_content_into(node, &mut out);
        out
    }

    pub(crate) fn text_content_len(&self, node: NodeId) -> usize {
        if self.is_removed(node) {
            return 0;
        }
        let own_len = self.text_node_value(node).map_or(0, str::len);
        own_len
            + self
                .node(node)
                .children
                .iter()
                .copied()
                .map(|child| self.text_content_len(child))
                .sum::<usize>()
    }

    pub(crate) fn trimmed_text_stats(&self, node: NodeId) -> (usize, usize) {
        let mut stats = TrimmedTextStats::default();
        self.accumulate_trimmed_text_stats(node, &mut stats);
        (stats.byte_len, stats.word_count)
    }

    fn accumulate_trimmed_text_stats(&self, node: NodeId, stats: &mut TrimmedTextStats) {
        if self.is_removed(node) {
            return;
        }
        if let Some(text) = self.text_node_value(node) {
            for character in text.chars() {
                if character.is_whitespace() {
                    if stats.has_text {
                        stats.pending_whitespace += character.len_utf8();
                    }
                    stats.in_word = false;
                } else {
                    if stats.has_text {
                        stats.byte_len += stats.pending_whitespace;
                    }
                    stats.pending_whitespace = 0;
                    stats.byte_len += character.len_utf8();
                    stats.has_text = true;
                    if !stats.in_word {
                        stats.word_count += 1;
                    }
                    stats.in_word = true;
                }
            }
        }
        for &child in &self.node(node).children {
            self.accumulate_trimmed_text_stats(child, stats);
        }
    }

    pub(crate) fn text_node_value(&self, node: NodeId) -> Option<&str> {
        match &self.node(node).kind {
            NodeKind::Text(text) => Some(text),
            _ => None,
        }
    }

    pub fn get_attr(&self, node: NodeId, name: &str) -> Option<&str> {
        match &self.node(node).kind {
            NodeKind::Element { attrs, .. } => attrs
                .iter()
                .find(|(attr_name, _)| atom_str(attr_name).eq_ignore_ascii_case(name))
                .map(|(_, value)| value.as_str()),
            _ => None,
        }
    }

    pub fn set_attr(&mut self, node: NodeId, name: &str, value: &str) {
        if let NodeKind::Element { attrs, .. } = &mut self.node_mut(node).kind {
            if let Some((_, existing)) = attrs
                .iter_mut()
                .find(|(attr_name, _)| atom_str(attr_name).eq_ignore_ascii_case(name))
            {
                *existing = value.to_string();
            } else {
                attrs.push((name.into(), value.to_string()));
            }
        }
    }

    pub fn remove_attr(&mut self, node: NodeId, name: &str) {
        if let NodeKind::Element { attrs, .. } = &mut self.node_mut(node).kind {
            attrs.retain(|(attr_name, _)| !atom_str(attr_name).eq_ignore_ascii_case(name));
        }
    }

    pub(crate) fn retain_attributes(
        &mut self,
        node: NodeId,
        mut keep: impl FnMut(&str) -> bool,
    ) {
        if let NodeKind::Element { attrs, .. } = &mut self.node_mut(node).kind {
            attrs.retain(|(name, _)| keep(name));
        }
    }

    /// Remove every `on*` attribute without copying the element's attribute names.
    /// Sanitization must cover handlers added by browsers after this crate's known list.
    pub(crate) fn remove_event_handler_attrs(&mut self, node: NodeId) {
        if let NodeKind::Element { attrs, .. } = &mut self.node_mut(node).kind {
            attrs.retain(|(name, _)| {
                !name
                    .get(..2)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("on"))
            });
        }
    }

    /// Get all attribute names on an element.
    pub fn attr_names(&self, node: NodeId) -> Vec<String> {
        match &self.node(node).kind {
            NodeKind::Element { attrs, .. } => {
                attrs.iter().map(|(name, _)| name.to_string()).collect()
            }
            _ => Vec::new(),
        }
    }

    /// Set the tag name of an element.
    pub fn set_tag_name(&mut self, node: NodeId, new_name: &str) {
        if let NodeKind::Element { name, .. } = &mut self.node_mut(node).kind {
            *name = new_name.into();
        }
    }

    /// Set the text content of a text node.
    pub fn set_text(&mut self, node: NodeId, text: &str) {
        if let NodeKind::Text(content) = &mut self.node_mut(node).kind {
            *content = text.to_string();
        }
    }

    /// Get mutable text content of a text node.
    pub fn text_content_mut(&mut self, node: NodeId) -> Option<&mut String> {
        match &mut self.node_mut(node).kind {
            NodeKind::Text(content) => Some(content),
            _ => None,
        }
    }

    /// Check if an element is an SVG element (or nested within one).
    pub fn is_svg_element(&self, node: NodeId) -> bool {
        let name = self.tag_name(node);
        name.eq_ignore_ascii_case("svg")
            || name.eq_ignore_ascii_case("g")
            || name.eq_ignore_ascii_case("path")
            || name.eq_ignore_ascii_case("circle")
            || name.eq_ignore_ascii_case("rect")
            || name.eq_ignore_ascii_case("line")
            || name.eq_ignore_ascii_case("polygon")
            || name.eq_ignore_ascii_case("polyline")
            || name.eq_ignore_ascii_case("ellipse")
            || name.eq_ignore_ascii_case("defs")
            || name.eq_ignore_ascii_case("use")
            || name.eq_ignore_ascii_case("mask")
            || name.eq_ignore_ascii_case("pattern")
            || name.eq_ignore_ascii_case("clipPath")
            || name.eq_ignore_ascii_case("text")
            || name.eq_ignore_ascii_case("tspan")
            || name.eq_ignore_ascii_case("stop")
            || name.eq_ignore_ascii_case("linearGradient")
            || name.eq_ignore_ascii_case("radialGradient")
    }

    pub fn remove(&mut self, node: NodeId) {
        if node == self.root || self.is_removed(node) {
            return;
        }
        if let Some(parent) = self.node(node).parent.get() {
            self.node_mut(parent)
                .children
                .retain(|child| *child != node);
        }
        self.mark_removed(node);
    }

    pub fn unwrap(&mut self, node: NodeId) {
        if node == self.root || self.is_removed(node) {
            return;
        }
        let Some(parent) = self.node(node).parent.get() else {
            return;
        };
        let children = self.node(node).children.clone();
        let Some(position) = self
            .node(parent)
            .children
            .iter()
            .position(|&child| child == node)
        else {
            return;
        };

        {
            let parent_node = self.node_mut(parent);
            parent_node
                .children
            .remove(position);
            for (offset, child) in children.iter().copied().enumerate() {
                parent_node.children.insert(position + offset, child);
            }
        }
        for child in children {
            self.node_mut(child).parent = NodeParent::new(Some(parent));
        }
        self.node_mut(node).children.clear();
        self.mark_removed(node);
    }

    pub fn append_html(&mut self, parent: NodeId, html: &str) {
        let fragment = Self::parse(html);
        let fragment_children = fragment.body_children_or_root_children();
        for child in fragment_children {
            self.clone_subtree_from(&fragment, child, Some(parent));
        }
    }

    pub fn replace_with_html(&mut self, node: NodeId, html: &str) {
        if node == self.root || self.is_removed(node) {
            return;
        }
        let Some(parent) = self.node(node).parent.get() else {
            return;
        };
        let Some(position) = self
            .node(parent)
            .children
            .iter()
            .position(|&child| child == node)
        else {
            return;
        };

        let fragment = Self::parse(html);
        let fragment_children = fragment.body_children_or_root_children();
        let mut new_children = Vec::new();
        for child in fragment_children {
            let new_child = self.clone_subtree_from(&fragment, child, None);
            self.node_mut(new_child).parent = NodeParent::new(Some(parent));
            new_children.push(new_child);
        }

        {
            let parent_node = self.node_mut(parent);
            parent_node.children.remove(position);
            for (offset, child) in new_children.into_iter().enumerate() {
                parent_node.children.insert(position + offset, child);
            }
        }
        self.mark_removed(node);
    }

    pub fn outer_html(&self, node: NodeId) -> String {
        #[cfg(test)]
        SERIALIZE_COUNT.with(|count| count.set(count.get() + 1));
        let mut out = String::new();
        self.serialize_node(node, &mut out);
        out
    }

    pub fn inner_html(&self, node: NodeId) -> String {
        let mut out = String::new();
        for child in self.node(node).children.iter().copied() {
            self.serialize_node(child, &mut out);
        }
        out
    }

    fn push_node(&mut self, kind: NodeKind, parent: Option<NodeId>) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(Node {
            parent: NodeParent::new(parent),
            children: SmallVec::new(),
            kind,
        });
        if let Some(parent) = parent {
            self.nodes[parent.0].children.push(id);
        }
        id
    }

    fn body_children_or_root_children(&self) -> SmallVec<[NodeId; 1]> {
        if let Some(body) = self
            .live_node_ids()
            .find(|&id| matches!(&self.node(id).kind, NodeKind::Element { name, .. } if atom_str(name).eq_ignore_ascii_case("body")))
        {
            return self.node(body).children.clone();
        }
        self.node(self.root).children.clone()
    }

    fn clone_subtree_from(
        &mut self,
        source: &Document,
        source_id: NodeId,
        parent: Option<NodeId>,
    ) -> NodeId {
        let new_id = self.push_node(source.node(source_id).kind.clone(), parent);
        for child in source.node(source_id).children.iter().copied() {
            if !source.is_removed(child) {
                self.clone_subtree_from(source, child, Some(new_id));
            }
        }
        new_id
    }

    fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0]
    }

    fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id.0]
    }

    fn live_node_ids(&self) -> impl Iterator<Item = NodeId> + '_ {
        (0..self.nodes.len())
            .map(NodeId)
            .filter(|&id| !self.is_removed(id))
    }

    fn collect_matching_subtree(
        &self,
        node: NodeId,
        selector: &[SimpleSelector],
        results: &mut Vec<NodeId>,
    ) {
        if self.is_removed(node) {
            return;
        }
        if self.matches_selector_chain(node, selector) {
            results.push(node);
        }
        for &child in &self.node(node).children {
            self.collect_matching_subtree(child, selector, results);
        }
    }

    fn subtree_has_match(&self, node: NodeId, selector: &[SimpleSelector]) -> bool {
        if self.is_removed(node) {
            return false;
        }
        if self.matches_selector_chain(node, selector) {
            return true;
        }
        self.node(node)
            .children
            .iter()
            .any(|&child| self.subtree_has_match(child, selector))
    }

    fn subtree_match_count(&self, node: NodeId, selector: &[SimpleSelector]) -> usize {
        if self.is_removed(node) {
            return 0;
        }
        let own_match = usize::from(self.matches_selector_chain(node, selector));
        own_match
            + self
                .node(node)
                .children
                .iter()
                .map(|&child| self.subtree_match_count(child, selector))
                .sum::<usize>()
    }

    fn mark_removed(&mut self, node: NodeId) {
        if self.is_removed(node) {
            return;
        }
        let children = mem::take(&mut self.node_mut(node).children);
        let kind = mem::replace(&mut self.node_mut(node).kind, NodeKind::Removed);
        self.node_mut(node).parent = NodeParent::new(None);
        drop(kind);
        for child in children {
            self.mark_removed(child);
        }
    }

    fn matches_selector_chain(&self, node: NodeId, parts: &[SimpleSelector]) -> bool {
        let Some(last) = parts.last() else {
            return false;
        };
        if !self.matches_simple(node, last) {
            return false;
        }

        let mut ancestor = self.node(node).parent.get();
        for selector in parts[..parts.len() - 1].iter().rev() {
            let mut matched = None;
            while let Some(candidate) = ancestor {
                if self.matches_simple(candidate, selector) {
                    matched = Some(candidate);
                    ancestor = self.node(candidate).parent.get();
                    break;
                }
                ancestor = self.node(candidate).parent.get();
            }
            if matched.is_none() {
                return false;
            }
        }
        true
    }

    fn matches_simple(&self, node: NodeId, selector: &SimpleSelector) -> bool {
        let NodeKind::Element { name, attrs } = &self.node(node).kind else {
            return false;
        };
        if let Some(tag) = &selector.tag
            && !atom_str(name).eq_ignore_ascii_case(tag)
        {
            return false;
        }
        if let Some(id) = &selector.id
            && !attrs
                .iter()
                .any(|(aname, aval)| {
                    atom_str(aname).eq_ignore_ascii_case("id") && aval == id.as_ref()
                })
        {
            return false;
        }
        if !selector.classes.is_empty() {
            for class in &selector.classes {
                if !attrs.iter().any(|(aname, aval)| {
                    atom_str(aname).eq_ignore_ascii_case("class")
                        && aval
                            .split_whitespace()
                            .any(|value| value == class.as_ref())
                }) {
                    return false;
                }
            }
        }
        for attribute in &selector.attributes {
            let matches = match attribute {
                AttributeSelector::Presence(name) => attrs
                    .iter()
                    .any(|(attr_name, _)| eq_ignore_case(atom_str(attr_name), name)),
                AttributeSelector::Equals(name, value, case_insensitive) => {
                    attr_match(attrs, name, |actual| {
                        if *case_insensitive {
                            actual.eq_ignore_ascii_case(value)
                        } else {
                            actual == value
                        }
                    })
                }
                AttributeSelector::Contains(name, value, case_insensitive) => {
                    attr_match(attrs, name, |actual| {
                        if *case_insensitive {
                            crate::content_utils::contains_ascii_case_insensitive(actual, value)
                        } else {
                            actual.contains(value.as_str())
                        }
                    })
                }
                AttributeSelector::Starts(name, value, case_insensitive) => {
                    attr_match(attrs, name, |actual| {
                        if *case_insensitive {
                            starts_ascii_case_insensitive(actual, value)
                        } else {
                            actual.starts_with(value.as_str())
                        }
                    })
                }
                AttributeSelector::Ends(name, value, case_insensitive) => {
                    attr_match(attrs, name, |actual| {
                        if *case_insensitive {
                            ends_ascii_case_insensitive(actual, value)
                        } else {
                            actual.ends_with(value.as_str())
                        }
                    })
                }
                AttributeSelector::Word(name, value, case_insensitive) => {
                    attr_match(attrs, name, |actual| {
                        if *case_insensitive {
                            actual
                                .split_whitespace()
                                .any(|word| word.eq_ignore_ascii_case(value))
                        } else {
                            actual.split_whitespace().any(|word| word == value.as_str())
                        }
                    })
                }
            };
            if !matches {
                return false;
            }
        }
        if let Some(ref negation) = selector.negation
            && self.matches_simple(node, negation)
        {
            return false;
        }
        if let Some(ref has) = selector.extra
            && !self.has_descendant_matching(node, has)
        {
            return false;
        }
        true
    }

    fn has_descendant_matching(&self, node: NodeId, selector: &SimpleSelector) -> bool {
        for &child in &self.node(node).children {
            if self.matches_simple(child, selector) || self.has_descendant_matching(child, selector)
            {
                return true;
            }
        }
        false
    }

    pub(crate) fn text_content_into(&self, node: NodeId, out: &mut String) {
        if self.is_removed(node) {
            return;
        }
        if let NodeKind::Text(text) = &self.node(node).kind {
            out.push_str(text);
        }
        for child in self.node(node).children.iter().copied() {
            self.text_content_into(child, out);
        }
    }

    pub(crate) fn text_content_with_br_into(&self, node: NodeId, out: &mut String) {
        if self.is_removed(node) {
            return;
        }
        match &self.node(node).kind {
            NodeKind::Text(text) => out.push_str(text),
            NodeKind::Element { name, .. } if atom_str(name).eq_ignore_ascii_case("br") => {
                out.push('\n');
            }
            _ => {
                for child in self.node(node).children.iter().copied() {
                    self.text_content_with_br_into(child, out);
                }
            }
        }
    }

    pub fn text_content_with_br(&self, node: NodeId) -> String {
        let mut out = String::new();
        self.text_content_with_br_into(node, &mut out);
        out
    }

    pub fn replace_with_node(&mut self, target: NodeId, replacement: NodeId) {
        if target == self.root || self.is_removed(target) {
            return;
        }
        if target == replacement || self.contains(replacement, target) {
            return;
        }
        let target_parent = self.node(target).parent.get();
        let target_pos =
            target_parent.and_then(|p| self.node(p).children.iter().position(|&c| c == target));

        if let Some(rp) = self.node(replacement).parent.get() {
            self.node_mut(rp)
                .children
                .retain(|c| *c != replacement);
        }

        if let (Some(parent), Some(pos)) = (target_parent, target_pos) {
            self.node_mut(parent).children[pos] = replacement;
            self.node_mut(replacement).parent = NodeParent::new(target_parent);
        }

        for child in self.node(target).children.clone() {
            self.node_mut(child).parent = NodeParent::new(None);
        }
        self.node_mut(target).children.clear();
        self.mark_removed(target);
    }

    fn serialize_node(&self, node: NodeId, out: &mut String) {
        if self.is_removed(node) {
            return;
        }
        match &self.node(node).kind {
            NodeKind::Document => {
                for child in self.node(node).children.iter().copied() {
                    self.serialize_node(child, out);
                }
            }
            NodeKind::Doctype => out.push_str("<!DOCTYPE html>"),
            NodeKind::Text(text) => escape_text_into(text, out),
            NodeKind::ProcessingInstruction => {}
            NodeKind::Comment(text) => {
                out.push_str("<!--");
                out.push_str(text);
                out.push_str("-->");
            }
            NodeKind::Element { name, attrs } => {
                out.push('<');
                out.push_str(name);
                for (attr_name, value) in attrs {
                    out.push(' ');
                    out.push_str(attr_name);
                    out.push_str("=\"");
                    escape_attr_into(value, out);
                    out.push('"');
                }
                out.push('>');
                if !is_void_element(name) {
                    for child in self.node(node).children.iter().copied() {
                        self.serialize_node(child, out);
                    }
                    out.push_str("</");
                    out.push_str(name);
                    out.push('>');
                }
            }
            NodeKind::Removed => {}
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct SimpleSelector {
    pub(crate) tag: Option<Box<str>>,
    pub(crate) id: Option<Box<str>>,
    pub(crate) classes: Vec<Box<str>>,
    pub(crate) attributes: Vec<AttributeSelector>,
    pub(crate) negation: Option<Box<SimpleSelector>>,
    pub(crate) extra: Option<Box<SimpleSelector>>,
}

#[derive(Debug)]
pub(crate) enum AttributeSelector {
    Presence(String),
    Equals(String, String, bool),
    Contains(String, String, bool),
    Starts(String, String, bool),
    Ends(String, String, bool),
    Word(String, String, bool),
}

fn eq_ignore_case(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.chars()
            .zip(b.chars())
            .all(|(ac, bc)| ac.eq_ignore_ascii_case(&bc))
}

fn atom_str(atom: &LocalName) -> &str {
    atom
}

fn starts_ascii_case_insensitive(value: &str, prefix: &str) -> bool {
    value
        .get(..prefix.len())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(prefix))
}

fn ends_ascii_case_insensitive(value: &str, suffix: &str) -> bool {
    value
        .get(value.len().saturating_sub(suffix.len())..)
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(suffix))
}

fn attr_match<F: FnMut(&str) -> bool>(
    attrs: &[(LocalName, String)],
    name: &str,
    mut predicate: F,
) -> bool {
    attrs
        .iter()
        .any(|(aname, aval)| eq_ignore_case(atom_str(aname), name) && predicate(aval))
}

fn is_void_element(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

fn escape_text_into(input: &str, out: &mut String) {
    for character in input.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            character => out.push(character),
        }
    }
}

fn escape_attr_into(input: &str, out: &mut String) {
    for character in input.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            character => out.push(character),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Document;

    #[test]
    fn trimmed_text_stats_match_trimmed_text_content() {
        let doc = Document::parse("<article> \nAlpha <b>Beta</b> \n CJK　終わり</article>");
        let article = doc.query_selector_all("article")[0];
        let text = doc.text_content(article);
        assert_eq!(
            doc.trimmed_text_stats(article),
            (text.trim().len(), text.split_whitespace().count())
        );
    }

    #[test]
    fn selector_matches_each_attribute_constraint() {
        let doc = Document::parse(
            "<div data-first='yes' data-second='yes'></div><div data-first='yes'></div>",
        );
        let matches = doc.query_selector_all("div[data-first][data-second]");
        assert_eq!(matches.len(), 1);
        assert_eq!(doc.get_attr(matches[0], "data-second"), Some("yes"));
    }

    #[test]
    fn dom_adapter_spike_covers_required_operations() {
        let mut doc = Document::parse(
            r#"
            <!doctype html>
            <html>
                <body>
                    <main id="main">
                        <article class="post featured"><h1>Title</h1><p class="intro">Hello <em>world</em>.</p></article>
                        <aside>Remove me</aside>
                    </main>
                </body>
            </html>
            "#,
        );

        let article = doc.query_selector_all("main article.post")[0];
        let intro = doc.query_selector_all(".intro")[0];
        let em = doc.query_selector_all("em")[0];
        assert_eq!(doc.closest(em, "article"), Some(article));
        assert!(doc.contains(article, em));
        assert_eq!(
            doc.text_content(article).replace('\n', "").trim(),
            "TitleHello world."
        );

        doc.set_attr(article, "data-seen", "yes");
        assert_eq!(doc.get_attr(article, "data-seen"), Some("yes"));

        doc.unwrap(em);
        assert!(doc.inner_html(intro).contains("Hello world."));

        let aside = doc.query_selector_all("aside")[0];
        doc.remove(aside);
        assert!(!doc.outer_html(doc.root()).contains("Remove me"));

        doc.append_html(article, "<footer>Tail</footer>");
        assert!(doc.outer_html(article).contains("<footer>Tail</footer>"));

        let heading = doc.query_selector_all("h1")[0];
        doc.replace_with_html(heading, "<h2>New title</h2>");
        let cloned = doc.clone();
        assert!(cloned.outer_html(article).contains("data-seen=\"yes\""));
        assert!(cloned.outer_html(article).contains("<h2>New title</h2>"));
    }
}

#[test]
fn test_inner_html_after_removals() {
    let doc = Document::parse(
        r#"<!DOCTYPE html><html><body><article><h1>Title</h1><p>Text with <strong>bold</strong> inside.</p></article></body></html>"#,
    );
    let articles = doc.query_selector_all("article");
    assert_eq!(articles.len(), 1);

    let article = articles[0];
    eprintln!(
        "article children after parse: {}",
        doc.children(article).len()
    );
    let html = doc.inner_html(article);
    eprintln!("inner_html: '{}'", html);
    assert!(html.contains("<h1>Title</h1>"));
    assert!(html.contains("<strong>bold</strong>"));
}
