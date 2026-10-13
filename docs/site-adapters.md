# Site adapters

The core extracts supported sites through synchronous, DOM-only adapters. The registry checks the page URL, then the adapter checks for its expected document shape. An explicitly requested `--extractor NAME` skips the URL gate but still requires matching markup.

| Adapter | Automatic URL gate | Supported content | Rust module |
| --- | --- | --- | --- |
| `hackernews` | `news.ycombinator.com` | Stories, listings, and nested comments | [Hacker News](../crates/defuddle/src/extractors/hackernews.rs) |
| `reddit` | `reddit.com` | Post body and old-Reddit comments | [Reddit](../crates/defuddle/src/extractors/reddit.rs) |
| `github` | `github.com` | Issues and pull requests | [GitHub](../crates/defuddle/src/extractors/github.rs) |
| `gmail` | `mail.google.com` | Rendered `.adn.ads` message rows and quoted-history cleanup | [Gmail](../crates/defuddle/src/extractors/gmail.rs) |
| `chatgpt` | `chatgpt.com` | Conversation turns and citations | [ChatGPT](../crates/defuddle/src/extractors/chatgpt.rs) |
| `substack` | `substack.com` | Notes and article content | [Substack](../crates/defuddle/src/extractors/substack.rs) |
| `mastodon` | `mastodon.` | Posts, media, and replies | [Mastodon](../crates/defuddle/src/extractors/mastodon.rs) |
| `twitter` | `x.com` or `twitter.com` | Articles and conversations | [Twitter/X](../crates/defuddle/src/extractors/twitter.rs) |

These are URL substring gates, not a complete list of supported page shapes. For example, Substack's adapter can match supported markup on a custom domain when requested explicitly. The adapter names are returned by `adapter_names()` and exposed by the CLI and WASM adapter.

Adapter output bypasses generic page scoring and boilerplate removal, then passes through the shared sanitization, URL resolution, extractor-output standardization, and optional Markdown conversion stages. Keep site-specific behavior in its adapter and shared behavior in the generic pipeline or its dedicated standardization, footnote, and embed modules.

## Add an adapter

1. Add a module under `crates/defuddle/src/extractors/` and implement `SiteAdapter::matches()` and `SiteAdapter::extract()`.
2. Register its factory and name in `crates/defuddle/src/extractors/mod.rs`.
3. Return DOM-backed output. Build generated markup with the DOM builder or escape page-derived text.
4. Add focused integration coverage, unsafe-attribute coverage, and a representative fixture where available.
