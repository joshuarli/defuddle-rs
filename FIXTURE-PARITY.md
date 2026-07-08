# Fixture Parity

This is the working doc for closing the upstream fixture parity gap. Keep it focused on the current gate, the remaining shared capabilities, and the order of work. Broader project tasks belong elsewhere.

## Current Baseline

Measured with `cargo test --test fixtures -- --nocapture`:

- 202 vendored upstream fixtures.
- 124 active fixtures.
- 78 excluded fixtures in `tests/upstream-exceptions.toml`.
- 30 active fixtures match.
- 94 active fixtures mismatch.

The fixture runner is non-blocking by default and prints the current count. Use strict mode for a real parity gate:

```bash
DEFUDDLE_STRICT_FIXTURES=1 cargo test --test fixtures -- --nocapture
```

Use a comma-separated filter when working a narrow set:

```bash
DEFUDDLE_FIXTURE_FILTER=fixture-one,fixture-two cargo test --test fixtures -- --nocapture
```

## Scope Rules

Keep fixtures active when they exercise common static reader-mode behavior:

- Markdown formatting, escaping, tables, code blocks, callouts, math, and footnotes.
- Standard DOM cleanup: headings, wrappers, attributes, empty elements, layout tables, `<br>` normalization, and permalink anchors.
- Images, media, safe links, lazy/static fallback images, `srcset`, and URL resolution.
- Content boundary behavior: scoring, retries, hidden content, content-pattern removal, schema fallback, and metadata block removal.
- Popular/general sites and extractors already present in this crate.

Exclude fixtures only for concrete out-of-scope requirements:

- Browser-only state such as computed styles, live DOM mutation, shadow DOM, or mobile CSS.
- Network/API-only data or transcript fetches.
- Async-only extractors that do not belong in the core crate.
- Niche site-specific extractors that are not part of the current parity push.
- Intentional Rust API differences.

Do not exclude fixtures just to improve the headline count.

## Acceptance Rules

For each parity change:

- Keep fixture scope stable unless the change intentionally promotes or excludes a named fixture.
- Run the narrow fixture filter first, then `cargo test --test fixtures -- --nocapture`.
- Run full `cargo test` before calling a tranche done.
- Update the baseline counts in this file when the active surface changes or a tranche materially reduces mismatches.
- Add focused integration tests when the behavior is shared and easy to regress outside the golden diff.
- Do not add HTTP, browser automation, JavaScript runtime, async runtime, or broad dependencies to `defuddle`.

## Work Order

### 1. Extractors Already In Core

These mismatches are high-signal because extractor output bypasses much of the shared pipeline.

Focus:

- ChatGPT turn labels, hidden Thought blocks, CodeMirror extraction, citation footnotes, and Markdown spacing.
- GitHub issue/PR content boundaries, author/date metadata, comment extraction, and page chrome removal.
- Reddit old-reddit duplicate nested comments, blockquote spacing, and URL normalization.
- Hacker News listing/comment parity if mismatches remain after shared Markdown fixes.
- Substack app/custom-domain output if active mismatches remain.
- X/Twitter static article and conversation extraction already has a core extractor; finish metadata, title/image preservation, and localized/status URL cases without network calls.

Representative fixtures:

- `extractor--chatgpt-citations`
- `extractor--chatgpt-post-thought-content`
- `codeblocks--chatgpt-codemirror`
- `general--github.com-issue-56`
- `general--github.com-test-owner-test-repo-pull-42`
- `comments--old.reddit.com-r-test-comments-abc123-test_post`
- `comments--news.ycombinator.com-item-id=12345678`
- `listing--news.ycombinator.com-news`
- `general--substack-app`
- `general--substack-custom-domain`
- `general--x.com-article`
- `general--x.com-article-2026-02-13`
- `issues--161-x-status-url-author`
- `issues--272-x-localized-conversation-timeline`

### 2. Code Block Normalization

Code block output still drives obvious Markdown mismatches. Fix DOM normalization before Markdown post-processing whenever possible.

Focus:

- Syntax-highlighter line wrappers and line-number gutters.
- Copy-button/control removal.
- CodeMirror block text preservation.
- Language propagation from container classes and data attributes.
- `<br>` handling inside code.
- Avoid stripping or merging inline code inside normal prose.

Representative fixtures:

- `code-blocks--chroma-linenums`
- `code-blocks--hexo-br`
- `codeblocks--chroma-inline-linenums`
- `codeblocks--chroma-line-spans`
- `codeblocks--pygments-lineno`
- `codeblocks--react-syntax-highlighter-linenums`
- `codeblocks--rehype-pretty-code`
- `codeblocks--rehype-pretty-copy`
- `codeblocks--rouge-linenums`
- `issues--167-partial-selector-inside-code`
- `issues--168-links-inside-inline-code`

### 3. Shared Markdown Rules

After the DOM shape is correct, close renderer drift with targeted Markdown rules.

Focus:

- Tag-like text escaping.
- Parentheses in link destinations.
- Inline spacing around links, bold, code, punctuation, and images.
- Ragged table padding.
- Footnote reference/backref formatting.
- Callout rendering.
- Stable blank-line behavior outside fenced code.

Representative fixtures:

- `issues--285-tag-like-title-escaping`
- `elements--complex-tables`
- `elements--data-table`
- `callouts--obsidian-publish-callouts`
- `elements--bootstrap-alerts`
- `elements--hugo-admonitions`
- Footnote fixtures that remain active after extractor/code-block work.

### 4. Standardization And Content Boundaries

These should be capability changes in `standardize.rs`, `pipeline.rs`, or scoring, not one-fixture string rewrites.

Focus:

- Heading cleanup and duplicate title removal.
- Wrapper flattening without losing semantic blocks.
- Empty element, empty line, and `<br>` normalization.
- Layout table unwrapping without damaging data tables.
- Permalink anchor removal.
- Hidden content and `aria-hidden` handling.
- Retry behavior for low word counts.
- Schema text fallback when schema content is materially longer.
- Content-pattern removal for TOCs, related posts, newsletters, cards, social counters, breadcrumbs, and metadata/date blocks.

Representative fixtures:

- `standardize--span-data-as-paragraph`
- `headings--fragment-url-not-permalink`
- `headings--permalink-title-match`
- `elements--br-between-blocks`
- `elements--empty-p-br`
- `elements--empty-table`
- `table-layout--single-column`
- `content-patterns--table-of-contents`
- `content-patterns--trailing-related-posts`
- `content-patterns--trailing-subscribe-after-footnotes`
- `content-patterns--card-grid-stripped-headings`
- `hidden--nodes`
- `hidden--visibility`
- `issues--162-aria-hidden-main-content`
- `scoring--related-posts-byline`
- `scoring--table-with-links`

### 5. Images, Media, URLs, Math, And Footnotes

Much of this is implemented, but fixture parity still depends on exact ordering, attribute cleanup, and Markdown output.

Focus:

- Relative URL resolution for links, images, `srcset`, posters, and lazy-image attributes.
- Best-candidate `srcset` selection and normalization.
- Lazy image and noscript fallback recovery.
- Placeholder/base64 image removal.
- Lightbox and duplicate image removal.
- Embedded media conversion.
- Static KaTeX/MathJax/raw-LaTeX extraction.
- Footnote definition normalization without converting ordinary superscripts.

Representative fixtures:

- `issues--221-nextjs-noscript-images`
- `issues--227-noscript-lazy-images`
- `elements--base64-placeholder-removal`
- `elements--figure-content-wrapper`
- `elements--javascript-links`
- `elements--lazy-image`
- `elements--lightbox-image-dedup`
- `elements--embedded-videos`
- `elements--srcset-normalization`
- `math--katex`
- `math--mathjax`
- `math--mathjax-svg`
- `math--mathjax-tex-scripts`
- `footnotes--child-anchor-id`
- `footnotes--heading-notes`
- `footnotes--pulldown-cmark-footnote-definition`
- `footnotes--wp-block-footnotes`

## Deferred

These should not block the current core parity push:

- Fetching YouTube/Bilibili transcripts or other network/API data.
- Browser-only computed style and stylesheet media-rule evaluation.
- Shadow DOM flattening.
- React streaming SSR suspense execution.
- Mobile style application.
- Pixel-based small-image removal unless static dimensions are present.
- New site extractors for Claude, Gemini, Grok, Medium, LinkedIn, Threads, Bluesky, Discourse, LeetCode, LWN, C2 Wiki, NYTimes, arXiv LaTeXML, Lean/Verso, or other niche formats.
