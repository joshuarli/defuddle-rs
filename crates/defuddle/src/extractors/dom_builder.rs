use crate::dom::{Document, NodeId};
use crate::pipeline::ExtractedContent;

pub(crate) struct Builder {
    pub(crate) document: Document,
    pub(crate) root: NodeId,
}

impl Builder {
    pub(crate) fn new() -> Self {
        let document = Document::empty();
        let root = document.root();
        Self { document, root }
    }

    pub(crate) fn element(&mut self, parent: NodeId, name: &str, attrs: &[(&str, &str)]) -> NodeId {
        self.document.create_element(parent, name, attrs)
    }

    pub(crate) fn text(&mut self, parent: NodeId, text: &str) {
        self.document.append_text(parent, text);
    }

    pub(crate) fn clone_children(
        &mut self,
        source: &Document,
        source_node: NodeId,
        parent: NodeId,
    ) {
        for child in source.children(source_node).iter().copied() {
            if !source.is_removed(child) {
                self.document.clone_node_from(source, child, parent);
            }
        }
    }

    pub(crate) fn finish(self, root: NodeId) -> ExtractedContent {
        ExtractedContent {
            document: self.document,
            root,
        }
    }
}
