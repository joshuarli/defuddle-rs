# Defuddle for Rust

Defuddle extracts readable article content and metadata from static HTML. The Rust library accepts HTML supplied by the caller and returns cleaned HTML, optional Markdown, and metadata. It does not fetch pages or execute JavaScript.

The workspace contains the `defuddle` library, the `defuddle` command-line program, and an opt-in `defuddle-wasm` adapter.

## Availability

`defuddle`, `defuddle-cli`, and `defuddle-wasm` are version `0.0.0`. Vendored `htmd` is `0.5.6` and is not published from this repository. Install the CLI from a checkout with:

```sh
cargo install --path crates/defuddle-cli
```

## Rust library

Add the library to your application's `Cargo.toml`:

```toml
defuddle = { git = "https://github.com/joshuarli/defuddle-rs" }
```

Parse HTML and request Markdown alongside the cleaned HTML:

```rust
use defuddle::{DefuddleOptions, parse_html};

let html = r#"<html><head><title>Example</title></head>
<body><article><h1>Example</h1><p>Readable text.</p></article></body></html>"#;
let result = parse_html(
    html,
    DefuddleOptions {
        url: Some("https://example.com/article".into()),
        separate_markdown: true,
        ..Default::default()
    },
)?;

println!("{}", result.content);
if let Some(markdown) = result.content_markdown.as_deref() {
    println!("{markdown}");
}
```

The optional URL supplies the page address for metadata and relative-link resolution; Defuddle does not request it. Results also include title, author, description, publication date, site, domain, language, image, favicon, meta tags, and schema data.

## Command line

The `defuddle` program reads an HTML file or standard input. If the source argument is omitted, it reads standard input when input is piped. A SingleFile ZIP archive is also accepted.

```sh
defuddle --markdown --url https://example.com/article article.html > article.md
cat article.html | defuddle --json --url https://example.com/article
```

By default, the command writes cleaned HTML to standard output. `--markdown` writes Markdown, `--json` writes the result and metadata as JSON, and `--output FILE` selects a destination file. `--frontmatter` writes metadata fields; combine it with `--markdown` to append the article body. Values are inserted without YAML quoting, so metadata containing newlines or YAML syntax can make the block invalid.

| Option | Purpose |
| --- | --- |
| `-m`, `--markdown`, `--md` | Output Markdown instead of HTML. |
| `-j`, `--json` | Output the serialized result, including content and metadata. |
| `-f`, `--frontmatter` | Output metadata frontmatter; add `--markdown` to include the article body. |
| `-p`, `--property NAME` | Output one value: `title`, `author`, `description`, `domain`, `favicon`, `image`, `language`, `published`, `site`, or `content`. |
| `-o`, `--output FILE` | Write output to a file. |
| `--url URL` | Set the document URL for metadata and relative-link resolution. It does not fetch the URL. |
| `--extractor NAME` | Force a site adapter: `hackernews`, `substack`, `reddit`, `github`, `gmail`, `chatgpt`, `mastodon`, or `twitter`. |
| `--debug` | Include the content selector in `--json` output. Set `DEFUDDLE_PROFILE=1` for phase timings. |
| `-h`, `--help` | Show command help. |

The built-in site adapters recognize Hacker News, Substack, Reddit, GitHub, Gmail, ChatGPT, Mastodon, and Twitter/X pages. Adapters work from the supplied HTML and do not fetch additional content.

## WebAssembly

The `defuddle-wasm` crate exports `parse_page(page_html, page_url, extractor_name)`, which returns a JSON string, and `list_extractors()`, which returns the supported adapter names separated by newlines. The JSON includes cleaned HTML, Markdown, and selected metadata. This adapter always requests Markdown and removes iframes.

## Help and license

Report problems at [the GitHub repository](https://github.com/joshuarli/defuddle-rs). The `defuddle` crates declare the MIT license. This tree has no top-level `LICENSE` file. Vendored `htmd` is Apache-2.0; see [its license](crates/htmd/LICENSE).
