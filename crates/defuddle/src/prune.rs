use crate::dom::Document;

pub(crate) struct Registry {
    fns: Vec<fn(&mut Document) -> bool>,
}

impl Registry {
    pub(crate) fn new() -> Self {
        Self { fns: Vec::new() }
    }

    pub(crate) fn register(&mut self, f: fn(&mut Document) -> bool) {
        self.fns.push(f);
    }

    pub(crate) fn run(&self, doc: &mut Document) -> bool {
        let mut changed = false;
        for f in &self.fns {
            changed |= f(doc);
        }
        changed
    }
}

pub(crate) fn build_registry() -> Registry {
    let mut r = Registry::new();
    crate::extractors::substack::register_prune(&mut r);
    r
}
