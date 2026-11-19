use crate::dom::Document;

pub struct Registry {
    fns: Vec<fn(&mut Document) -> bool>,
}

impl Registry {
    pub fn new() -> Self {
        Self { fns: Vec::new() }
    }

    pub fn register(&mut self, f: fn(&mut Document) -> bool) {
        self.fns.push(f);
    }

    pub fn run(&self, doc: &mut Document) -> bool {
        let mut changed = false;
        for f in &self.fns {
            changed |= f(doc);
        }
        changed
    }
}

pub fn build_registry() -> Registry {
    let mut r = Registry::new();
    crate::extractors::substack::register_prune(&mut r);
    r
}
