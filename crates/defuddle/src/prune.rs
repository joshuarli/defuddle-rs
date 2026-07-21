use crate::dom::Document;

pub struct Registry {
    fns: Vec<fn(&mut Document)>,
}

impl Registry {
    pub fn new() -> Self {
        Self { fns: Vec::new() }
    }

    pub fn register(&mut self, f: fn(&mut Document)) {
        self.fns.push(f);
    }

    pub fn run(&self, doc: &mut Document) {
        for f in &self.fns {
            f(doc);
        }
    }
}

pub fn build_registry() -> Registry {
    let mut r = Registry::new();
    crate::extractor::substack::register_prune(&mut r);
    r
}
