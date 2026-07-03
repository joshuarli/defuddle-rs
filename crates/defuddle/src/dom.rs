use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub usize);

#[derive(Debug, Clone)]
enum NodeKind {
    Document,
    Doctype,
    Element {
        name: String,
        attrs: Vec<(String, String)>,
    },
    Text(String),
    Comment(String),
}

#[derive(Debug, Clone)]
struct Node {
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    kind: NodeKind,
    removed: bool,
}

#[derive(Debug, Clone)]
pub struct Document {
    nodes: Vec<Node>,
    root: NodeId,
}

impl Document {
    pub fn parse(html: &str) -> Self {
        let rcdom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
        let mut doc = Self {
            nodes: Vec::new(),
            root: NodeId(0),
        };
        let root = doc.push_node(NodeKind::Document, None);
        doc.root = root;
        doc.import_rcdom_children(&rcdom.document, root);
        doc
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    pub fn query_selector_all(&self, selector: &str) -> Vec<NodeId> {
        let parts: Vec<SimpleSelector> = selector
            .split_whitespace()
            .filter_map(SimpleSelector::parse)
            .collect();
        if parts.is_empty() {
            return Vec::new();
        }

        self.live_node_ids()
            .filter(|&id| self.matches_selector_chain(id, &parts))
            .collect()
    }

    pub fn closest(&self, node: NodeId, selector: &str) -> Option<NodeId> {
        let selector = SimpleSelector::parse(selector)?;
        let mut current = Some(node);
        while let Some(id) = current {
            if self.matches_simple(id, &selector) {
                return Some(id);
            }
            current = self.node(id).parent;
        }
        None
    }

    pub fn contains(&self, ancestor: NodeId, descendant: NodeId) -> bool {
        let mut current = Some(descendant);
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            current = self.node(id).parent;
        }
        false
    }

    pub fn text_content(&self, node: NodeId) -> String {
        let mut out = String::new();
        self.text_content_into(node, &mut out);
        out
    }

    pub fn get_attr(&self, node: NodeId, name: &str) -> Option<&str> {
        match &self.node(node).kind {
            NodeKind::Element { attrs, .. } => attrs
                .iter()
                .find(|(attr_name, _)| attr_name.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.as_str()),
            _ => None,
        }
    }

    pub fn set_attr(&mut self, node: NodeId, name: &str, value: &str) {
        if let NodeKind::Element { attrs, .. } = &mut self.node_mut(node).kind {
            if let Some((_, existing)) = attrs
                .iter_mut()
                .find(|(attr_name, _)| attr_name.eq_ignore_ascii_case(name))
            {
                *existing = value.to_string();
            } else {
                attrs.push((name.to_string(), value.to_string()));
            }
        }
    }

    pub fn remove(&mut self, node: NodeId) {
        if node == self.root || self.node(node).removed {
            return;
        }
        if let Some(parent) = self.node(node).parent {
            self.node_mut(parent)
                .children
                .retain(|&child| child != node);
        }
        self.mark_removed(node);
    }

    pub fn unwrap(&mut self, node: NodeId) {
        if node == self.root || self.node(node).removed {
            return;
        }
        let Some(parent) = self.node(node).parent else {
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
                .splice(position..=position, children.clone());
        }
        for child in children {
            self.node_mut(child).parent = Some(parent);
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
        if node == self.root || self.node(node).removed {
            return;
        }
        let Some(parent) = self.node(node).parent else {
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
            new_children.push(self.clone_subtree_from(&fragment, child, Some(parent)));
        }

        {
            let parent_node = self.node_mut(parent);
            parent_node
                .children
                .splice(position..=position, new_children);
        }
        self.mark_removed(node);
    }

    pub fn outer_html(&self, node: NodeId) -> String {
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
            parent,
            children: Vec::new(),
            kind,
            removed: false,
        });
        if let Some(parent) = parent {
            self.nodes[parent.0].children.push(id);
        }
        id
    }

    fn import_rcdom_children(&mut self, handle: &Handle, parent: NodeId) {
        for child in handle.children.borrow().iter() {
            let Some(kind) = Self::kind_from_rcdom(child) else {
                self.import_rcdom_children(child, parent);
                continue;
            };
            let id = self.push_node(kind, Some(parent));
            self.import_rcdom_children(child, id);
        }
    }

    fn kind_from_rcdom(handle: &Handle) -> Option<NodeKind> {
        match &handle.data {
            NodeData::Document => Some(NodeKind::Document),
            NodeData::Doctype { .. } => Some(NodeKind::Doctype),
            NodeData::Text { contents } => Some(NodeKind::Text(contents.borrow().to_string())),
            NodeData::Comment { contents } => Some(NodeKind::Comment(contents.to_string())),
            NodeData::Element { name, attrs, .. } => Some(NodeKind::Element {
                name: name.local.to_string(),
                attrs: attrs
                    .borrow()
                    .iter()
                    .map(|attr| (attr.name.local.to_string(), attr.value.to_string()))
                    .collect(),
            }),
            NodeData::ProcessingInstruction { .. } => None,
        }
    }

    fn body_children_or_root_children(&self) -> Vec<NodeId> {
        if let Some(body) = self
            .live_node_ids()
            .find(|&id| matches!(&self.node(id).kind, NodeKind::Element { name, .. } if name.eq_ignore_ascii_case("body")))
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
            if !source.node(child).removed {
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
            .filter(|&id| !self.node(id).removed)
    }

    fn mark_removed(&mut self, node: NodeId) {
        let children = self.node(node).children.clone();
        self.node_mut(node).removed = true;
        self.node_mut(node).parent = None;
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

        let mut ancestor = self.node(node).parent;
        for selector in parts[..parts.len() - 1].iter().rev() {
            let mut matched = None;
            while let Some(candidate) = ancestor {
                if self.matches_simple(candidate, selector) {
                    matched = Some(candidate);
                    ancestor = self.node(candidate).parent;
                    break;
                }
                ancestor = self.node(candidate).parent;
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
        if let Some(tag) = &selector.tag {
            if !name.eq_ignore_ascii_case(tag) {
                return false;
            }
        }
        if let Some(id) = &selector.id {
            if !attrs
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("id") && value == id)
            {
                return false;
            }
        }
        selector.classes.iter().all(|class| {
            attrs.iter().any(|(name, value)| {
                name.eq_ignore_ascii_case("class")
                    && value.split_whitespace().any(|candidate| candidate == class)
            })
        })
    }

    fn text_content_into(&self, node: NodeId, out: &mut String) {
        if self.node(node).removed {
            return;
        }
        if let NodeKind::Text(text) = &self.node(node).kind {
            out.push_str(text);
        }
        for child in self.node(node).children.iter().copied() {
            self.text_content_into(child, out);
        }
    }

    fn serialize_node(&self, node: NodeId, out: &mut String) {
        if self.node(node).removed {
            return;
        }
        match &self.node(node).kind {
            NodeKind::Document => {
                for child in self.node(node).children.iter().copied() {
                    self.serialize_node(child, out);
                }
            }
            NodeKind::Doctype => out.push_str("<!DOCTYPE html>"),
            NodeKind::Text(text) => out.push_str(&escape_text(text)),
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
                    out.push_str(&escape_attr(value));
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
        }
    }
}

#[derive(Debug)]
struct SimpleSelector {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
}

impl SimpleSelector {
    fn parse(input: &str) -> Option<Self> {
        if input.is_empty() || input.contains(['>', '+', '~', '[', ':', ',']) {
            return None;
        }

        let mut tag = None;
        let mut id = None;
        let mut classes = Vec::new();
        let mut buf = String::new();
        let mut mode = SelectorMode::Tag;

        let flush = |mode: SelectorMode,
                     buf: &mut String,
                     tag: &mut Option<String>,
                     id: &mut Option<String>,
                     classes: &mut Vec<String>| {
            if buf.is_empty() {
                return;
            }
            match mode {
                SelectorMode::Tag => *tag = Some(std::mem::take(buf)),
                SelectorMode::Id => *id = Some(std::mem::take(buf)),
                SelectorMode::Class => classes.push(std::mem::take(buf)),
            }
        };

        for ch in input.chars() {
            match ch {
                '#' => {
                    flush(mode, &mut buf, &mut tag, &mut id, &mut classes);
                    mode = SelectorMode::Id;
                }
                '.' => {
                    flush(mode, &mut buf, &mut tag, &mut id, &mut classes);
                    mode = SelectorMode::Class;
                }
                '*' if matches!(mode, SelectorMode::Tag) && buf.is_empty() => {}
                _ => buf.push(ch),
            }
        }
        flush(mode, &mut buf, &mut tag, &mut id, &mut classes);

        if tag.is_none() && id.is_none() && classes.is_empty() {
            return None;
        }
        Some(Self { tag, id, classes })
    }
}

#[derive(Debug, Clone, Copy)]
enum SelectorMode {
    Tag,
    Id,
    Class,
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

fn escape_text(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_attr(input: &str) -> String {
    escape_text(input).replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::Document;

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
