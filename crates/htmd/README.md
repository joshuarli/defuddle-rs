# htmd

Vendored copy of [htmd](https://github.com/letmutex/htmd) 0.5.6. `defuddle` uses it to convert HTML to Markdown. The crate is a path dependency of this workspace. It is not the published `htmd` package. This repository does not ship the upstream CLI or benchmarks.

The license is Apache-2.0. See [LICENSE](LICENSE).

`crates/defuddle/src/markdown.rs` builds the `HtmlToMarkdown` converter this workspace uses.
