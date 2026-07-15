# Fixture Parity

This is the working doc for closing the upstream fixture parity gap. Keep it focused on the current gate, the remaining shared capabilities, and the order of work. Broader project tasks belong elsewhere.

## Current Baseline

Measured with `cargo test --test fixtures -- --nocapture`:

- 202 vendored upstream fixtures.
- 114 active fixtures.
- 88 excluded fixtures in `tests/upstream-exceptions.toml`.
- 59 active fixtures match.
- 55 active fixtures mismatch.

This is a moving baseline. Update it only from the full fixture command, not
from a filtered run. The current count reflects the shared footnote, DOM
cleanup, URL, and Markdown work in the working tree.

The fixture runner is non-blocking by default and prints the current count. Use strict mode for a real parity gate:

```bash
DEFUDDLE_STRICT_FIXTURES=1 cargo test --test fixtures -- --nocapture
```

Use a comma-separated filter when working a narrow set:

```bash
DEFUDDLE_FIXTURE_FILTER=fixture-one,fixture-two cargo test --test fixtures -- --nocapture
```

The full test suite also has one known pre-existing failure unrelated to fixture parity:
`markdown_conversion::wraps_link_destinations_with_spaces_in_angle_brackets`. Until that is fixed,
run the full suite for visibility and use this passing integration command as the regression gate:

```bash
cargo test --test integration -- --skip markdown_conversion::wraps_link_destinations_with_spaces_in_angle_brackets
```

## Scope Rules

Keep fixtures active when they exercise common static reader-mode behavior:

- Markdown formatting, escaping, tables, code blocks, callouts, and footnotes.
- Standard DOM cleanup: headings, wrappers, attributes, empty elements, layout tables, `<br>` normalization, and permalink anchors.
- Images, safe links, lazy/static fallback images, `srcset`, and URL resolution.
- Content boundary behavior: scoring, retries, hidden content, content-pattern removal, schema fallback, and metadata block removal.
- Popular/general sites and extractors already present in this crate.

Math fixtures are excluded from the core parity gate. Math extraction and rendered math fidelity are deferred.

Exclude fixtures only for concrete out-of-scope requirements:

- Browser-only state such as computed styles, live DOM mutation, shadow DOM, or mobile CSS.
- Network/API-only data or transcript fetches.
- Async-only extractors that do not belong in the core crate.
- Niche site-specific extractors that are not part of the current parity push.
- Intentional Rust API differences.

Do not exclude fixtures just to improve the headline count.

## Acceptance Rules

For each parity tranche:

- Keep fixture scope stable unless the change intentionally promotes or excludes a named fixture.
- Start with a representative filter while developing, then run the full fixture gate before considering the tranche complete.
- Run full `cargo test` before calling a tranche done; the known link-spacing failure must remain the only unrelated failure.
- Update the baseline counts when the active surface changes or a tranche materially reduces mismatches.
- Add focused integration tests when the behavior is shared and easy to regress outside the golden diff.
- Do not add HTTP, browser automation, JavaScript runtime, async runtime, or broad dependencies to `defuddle`.
- Classify each mismatch before changing code.

The useful mismatch categories are:

- Metadata preamble.
- Content HTML.
- Markdown-only.
- Extractor selection or extractor output.
- URL/image normalization.
- Fixture or exception bookkeeping.

The harness currently reports only one aggregate count. Use its narrow filter and diff output to assign a category, and record the category in the tranche notes or issue. Measure progress by category as well as by total matched fixtures.

## Faster Parity Method

Do not treat each fixture as an independent bug. A fixture mismatch is evidence
of a missing shared rule, and several fixtures commonly fail for the same rule.
The preferred loop is:

1. Run the full gate and copy the mismatch names into capability buckets.
2. Select one or two representative fixtures per bucket and compare their
   computed output with the upstream TypeScript implementation.
3. Port the upstream rule at the appropriate layer (`pipeline.rs`,
   `standardize.rs`, `markdown.rs`, metadata, scoring, or an extractor).
4. Run every representative fixture for that bucket, plus the full gate, to
   detect regressions before moving on.
5. Record the bucket result and remaining exceptions here; do not add a
   fixture-specific rewrite unless the upstream rule itself is fixture-specific.

The order should be driven by fan-out, not by fixture filename order:

| Priority | Shared bucket | Typical symptoms | Primary source |
| --- | --- | --- | --- |
| 1 | Markdown normalization | spacing, escaping, tables, callouts, footnotes | `src/markdown.ts`, `markdown.rs` |
| 2 | Content boundaries and cleanup | extra cards, CTAs, hidden nodes, headings, bylines | `src/standardize.ts`, `src/parse.ts`, pipeline/scoring |
| 3 | URL and media normalization | relative links, lazy images, `srcset`, placeholders | `src/standardize.ts`, `src/utils.ts` |
| 4 | Metadata and schema fallback | date/author/site discrepancies | `src/metadata.ts`, `src/schema.ts` |
| 5 | Existing extractors | site-specific content or formatting | corresponding upstream extractor |

Port complete upstream rule groups where practical. Avoid accumulating regex
patches in the Markdown postprocessor: DOM normalization should happen before
conversion, and Markdown postprocessing should contain only renderer-specific
normalization. A change that improves one fixture but increases another bucket
should be treated as a regression until the shared cause is understood.

The current active mismatches are concentrated in these buckets:

- Markdown formatting: callouts, blockquote spacing, code formatting, inline
  code, tables, and footnote edge cases.
- Content boundaries: related cards, table of contents, trailing newsletter or
  subscribe sections, hidden content, and byline/metadata blocks.
- URLs and media: relative links, lazy/noscript images, `srcset`, placeholders,
  embedded videos, and duplicate images.
- Metadata/schema: date-adjacent blocks, placeholder values, and author/site
  fallback behavior.
- Existing extractors: ChatGPT citations, GitHub, Reddit, Hacker News,
  Substack, Wikipedia, and static X article cases.

The next pass should therefore port shared Markdown and standardization rules
from upstream in larger groups, then rerun the full gate. Footnote and math
work must remain separate: footnotes are in scope for readable Markdown, while
math remains deferred.

## Work Order

### 1. Shared Markdown Rules

Port the upstream Markdown rule group as one tranche. Use representative
fixtures to validate the group, rather than fixing individual output strings.

Focus:

- Tag-like text escaping.
- Parentheses and spaces in link destinations.
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

### 2. Standardization And Content Boundaries

Port the upstream standardization and content-boundary rules that remove or
reshape content before Markdown conversion. This is the highest-leverage pass
after shared Markdown behavior.

Focus:

- Heading cleanup and duplicate title removal.
- Wrapper flattening without losing semantic blocks.
- Empty element, empty line, and `<br>` normalization.
- Layout table unwrapping without damaging data tables.
- Permalink anchor removal.
- Hidden content and `aria-hidden` handling.
- Retry behavior for low word counts when its choices differ from upstream.
- Schema text fallback when its selection differs from upstream.
- Removal of TOCs, related posts, newsletters, cards, social counters,
  breadcrumbs, and metadata/date blocks.

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

### 3. Code Blocks

Code block behavior is part of the standardization/Markdown pass, but is kept
as a separate validation bucket because its whitespace rules are easy to break.
Fix DOM normalization before Markdown post-processing wherever possible.

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

### 4. URLs And Images

Keep URL and image normalization separate from media, math, and footnote work because it has a distinct parity surface and broad downstream impact.

Focus:

- Relative URL resolution for links, images, `srcset`, posters, and lazy-image attributes.
- Best-candidate `srcset` selection and normalization.
- Lazy image and noscript fallback recovery.
- Placeholder/base64 image removal.
- Lightbox and duplicate image removal.

Representative fixtures:

- `issues--221-nextjs-noscript-images`
- `issues--227-noscript-lazy-images`
- `elements--base64-placeholder-removal`
- `elements--figure-content-wrapper`
- `elements--javascript-links`
- `elements--lazy-image`
- `elements--lightbox-image-dedup`
- `elements--srcset-normalization`

### 5. Embedded Media

Focus only on static, DOM-representable media behavior. Network-backed transcript or embed enrichment remains deferred.

Representative fixtures:

- `elements--embedded-videos`

### 6. Footnotes

Footnotes remain in scope when they affect readable Markdown output.

Focus:

- Footnote definition normalization without converting ordinary superscripts.

Representative fixtures:

- `footnotes--child-anchor-id`
- `footnotes--heading-notes`
- `footnotes--pulldown-cmark-footnote-definition`
- `footnotes--wp-block-footnotes`

### 7. Extractors Already In Core

After shared behavior stabilizes, address extractor-specific mismatches. These extractors are implemented in the core crate; the remaining work is parity in boundaries, metadata, and output formatting.

Focus:

- ChatGPT turn labels, hidden Thought blocks, CodeMirror extraction, citation footnotes, and Markdown spacing.
- GitHub issue/PR content boundaries, author/date metadata, comment extraction, and page chrome removal.
- Reddit old-reddit duplicate nested comments, blockquote spacing, and URL normalization.
- Hacker News listing/comment parity if mismatches remain after shared Markdown fixes.
- Substack app/custom-domain output if active mismatches remain.
- X/Twitter static article and conversation extraction: metadata, title/image preservation, and localized/status URL cases without network calls.

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

## Deferred

These should not block the current core parity push:

- Fetching YouTube/Bilibili transcripts or other network/API data.
- Browser-only computed style and stylesheet media-rule evaluation.
- Shadow DOM flattening.
- React streaming SSR suspense execution.
- Mobile style application.
- Pixel-based small-image removal unless static dimensions are present.
- Math extraction and rendered math HTML/MathML/KaTeX/MathJax fidelity.
- New site extractors for Claude, Gemini, Grok, Medium, LinkedIn, Threads, Bluesky, Discourse, LeetCode, LWN, C2 Wiki, NYTimes, arXiv LaTeXML, Lean/Verso, or other niche formats.
