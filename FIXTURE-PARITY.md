# Fixture Parity Plan

This is the canonical plan for upstream fixture parity. Keep `TODO.md` focused on broad project work; fixture scope, missing pipeline behavior, Markdown differences, and extractor backlog belong here.

Current baseline:
- 202 vendored upstream fixtures.
- The fixture runner reports parity in normal mode and fails on active mismatches when `DEFUDDLE_STRICT_FIXTURES=1`.
- The active gate has 121 fixtures in scope: 36 matching and 85 mismatching, with 81 excluded.
- The acceptance gate should grow deliberately. Do not exclude fixtures only to improve the headline count.

## Scope Rules

Keep fixtures in scope when they exercise common reader-mode behavior:
- Markdown formatting and escaping.
- Code block normalization.
- Headings, permalink anchors, wrappers, and `<br>` handling.
- Images, `srcset`, lazy images, noscript fallbacks, and safe link handling.
- Metadata cleanup and h1-adjacent author/date blocks.
- Content trimming: table of contents, related posts, subscribe blocks, social counters, boilerplate.
- Retry, scoring, schema fallback, and low-content recovery.
- Popular/general sites and already-ported extractors.

It is acceptable to exclude fixtures when they require browser-only state, network fetches, unrelated async APIs, or narrow site-specific extractors that are not being worked in the current tranche.

## Coding Strategy

Use these rules when working toward fixture parity:
- Work on one tranche and one capability at a time. Avoid broad "standardization improvements" changes that mix Markdown rendering, extraction boundaries, metadata, and dependencies.
- Keep fixture scope stable inside a tranche. If scope changes, update the baseline count and explain why the new fixture belongs in or out.
- Never exclude fixtures to improve the headline match count. Exclusions must identify a concrete blocker: browser-only state, network-only data, async API, unplanned site extractor, or an explicitly deferred tranche.
- Treat a fixture match as evidence, not proof. Add focused integration tests for shared behavior that a later broad cleanup could accidentally break.
- Port upstream behavior by capability and source intent. Avoid regex post-processing that only makes one fixture pass while risking HTML, code, link, or math corruption elsewhere.
- Do not combine dependency swaps with behavior changes unless the tranche is explicitly about that dependency. Renderer migrations need their own acceptance gate.
- Prefer deterministic DOM transformations over string rewrites when the input is HTML and the DOM adapter has the needed operation.
- Preserve useful comments and keep new comments focused on non-obvious behavior. Do not add banner or separator comments.
- Keep X/Twitter work in its own extractor tranche. It should not block shared pipeline or Markdown parity.
- A tranche is complete only when its named gate fixtures match under strict fixture mode.

## Restored Fixture Set

The following useful fixtures should remain in scope because they represent common article extraction behavior, not obscure one-off formats:

- `issues--221-nextjs-noscript-images`
- `issues--227-noscript-lazy-images`
- `issues--136-time-element`
- `issues--285-tag-like-title-escaping`
- `elements--base64-placeholder-removal`
- `elements--figure-content-wrapper`
- `elements--javascript-links`
- `elements--lazy-image`
- `elements--srcset-normalization`
- `elements--svg-placeholder-lazy-image`
- `elements--image-dedup`
- `elements--lightbox-image-dedup`
- `elements--embedded-videos`
- `elements--whitespace-newlines`
- `issues--254-empty-video`
- `issues--286-generic-alt-image-dedup`
- `hidden--nodes`
- `hidden--visibility`
- `issues--162-aria-hidden-main-content`
- `gated-content--cookie-consent`
- `scoring--related-posts-byline`
- `scoring--table-with-links`
- `issues--284-table-cell-header-scoring`
- `content-patterns--heading-introduced-list`
- `content-patterns--table-of-contents`
- `content-patterns--trailing-subscribe-after-footnotes`
- `content-patterns--social-engagement-counter`
- `content-patterns--code-block-boilerplate-and-trailing-section`
- `content-patterns--content-list-with-internal-links`
- `metadata--placeholder-author-and-duplicate-published`
- `metadata--placeholder-values`
- `metadata--rel-author-in-bio-container`
- `metadata--h1-sibling-byline`
- `eyebrow--linked-category`
- `eyebrow--paragraph-label`
- `eyebrow--spans-category`
- `eyebrow--wrapper-with-icon`
- `codeblocks--chroma-line-spans`
- `codeblocks--rehype-pretty-copy`
- `codeblocks--code-pre-nesting`
- `table-layout--single-column`
- `standardize--span-data-as-paragraph`
- `headings--fragment-url-not-permalink`
- `headings--permalink-title-match`
- `general--trailing-cta-newsletter`
- `general--appendix-heading`
- `issues--header-with-subtitle-p`
- `issues--header-wraps-content`
- `footnotes--hr-sup-numbered`
- `footnotes--numeric-anchor-id`
- `footnotes--p-class-footnote`
- `footnotes--named-anchor`
- `footnotes--easy-footnotes-wrapper`
- `math--katex-centraliser`
- `math--katex-data-math`
- `math--latex-image-services`

## Acceptance Gates

Each tranche must end with:
- `cargo test --test fixtures -- --nocapture`
- `cargo test`
- Updated fixture counts in this file if the acceptance surface changes.
- No duplicate entries in `tests/upstream-exceptions.toml`.
- Any newly excluded fixture must include a concrete capability reason and target tranche.

For parity work, prefer adding focused integration tests only when they protect behavior that is hard to see from the golden fixture diff.

## Tranche 0: Harness And Scope Hygiene

Goal: make the gate trustworthy before chasing output parity.

Work:
- Keep the restored fixture set out of `tests/upstream-exceptions.toml`.
- Remove duplicate exception entries.
- Make `exception_manifest_is_valid` assert that fixture exception names are unique.
- Add an opt-in strict mode for fixture mismatches, for example `DEFUDDLE_STRICT_FIXTURES=1`.
- Report active fixture count, matched count, mismatched count, and excluded count unconditionally.

Acceptance:
- Restored fixture set is active.
- Duplicate exceptions fail the manifest test.
- Non-strict fixture run still reports progress deterministically.
- Strict fixture run fails when active mismatches remain.

## Tranche 1: Markdown Renderer Parity

Goal: eliminate broad Markdown formatting differences before deeper extraction changes.

Work:
- Port the relevant `src/markdown.ts` behavior:
  - Exclamation-mark spacing before images.
  - Ragged table column padding.
  - Tag-like text escaping, including code/title cases.
  - Parenthesis escaping in link destinations.
  - Footnote backref links.
  - Callout block rendering.
  - Inline spacing around bold, links, code, and punctuation.
- Fix code block language propagation and indentation.

Gate fixtures:
- `code-blocks--chroma-linenums`
- `code-blocks--hexo-br`
- `codeblocks--chroma-inline-linenums`
- `codeblocks--pygments-lineno`
- `codeblocks--react-syntax-highlighter-linenums`
- `codeblocks--rehype-pretty-code`
- `codeblocks--rouge-linenums`
- `codeblocks--chroma-line-spans`
- `codeblocks--rehype-pretty-copy`
- `codeblocks--code-pre-nesting`
- `issues--167-partial-selector-inside-code`
- `issues--168-links-inside-inline-code`
- `issues--285-tag-like-title-escaping`

Acceptance:
- All gate fixtures above match.
- No Markdown post-processing rule corrupts HTML tags before conversion.

Status:
- Complete. The Tranche 1 strict gate reports 13 matching, 0 mismatching.

## Tranche 2: Standardization

Goal: complete the static DOM cleanup that makes extracted content structurally match upstream.

Work:
- Port the relevant `src/standardize.ts` behavior:
  - Heading normalization and duplicate-title removal.
  - Code block detection and wrapping.
  - Callout and footnote standardization.
  - Permalink anchor removal.
  - Attribute stripping to the allowed output set.
  - Wrapper element flattening.
  - `<br>` normalization and consecutive collapse.
  - Empty line and empty element removal.
  - Span/data-as block normalization.

Gate fixtures:
- `callouts--obsidian-publish-callouts`
- `elements--bootstrap-alerts`
- `elements--br-between-blocks`
- `elements--empty-p-br`
- `elements--empty-table`
- `elements--hugo-admonitions`
- `elements--nbsp-handling`
- `standardize--span-data-as-paragraph`
- `headings--fragment-url-not-permalink`
- `headings--permalink-title-match`

Acceptance:
- Gate fixtures match.
- Useful comments explaining non-obvious standardization behavior are preserved or updated.

## Tranche 3: Images, Media, URLs, And Links

Goal: handle common static-HTML image and link patterns without browser APIs.

Work:
- Resolve relative URLs consistently for links, images, and `srcset`.
- Parse and normalize `srcset`.
- Deduplicate equivalent images, including lightbox and multi-resolution variants.
- Preserve useful lazy-loaded image URLs from `data-*` attributes.
- Recover noscript image fallbacks where the input HTML contains them statically.
- Strip unsafe JavaScript links while preserving readable link text.
- Keep browser-only small-image removal out of core unless dimensions are present in HTML attributes.

Gate fixtures:
- `issues--221-nextjs-noscript-images`
- `issues--227-noscript-lazy-images`
- `elements--base64-placeholder-removal`
- `elements--figure-content-wrapper`
- `elements--javascript-links`
- `elements--lazy-image`
- `elements--lightbox-image-dedup`
- `elements--embedded-videos`
- `elements--srcset-normalization`
- `elements--svg-placeholder-lazy-image`
- `elements--image-dedup`
- `elements--whitespace-newlines`
- `issues--254-empty-video`
- `issues--286-generic-alt-image-dedup`
- `content-patterns--content-list-with-internal-links`

Acceptance:
- Gate fixtures match.
- No HTTP, browser automation, JavaScript runtime, or async runtime is added to `defuddle`.

Status:
- Complete. All Tranche 3 functions (recover_noscript_images, recover_lazy_images, normalize_srcset_attributes, remove_base64_placeholder_images, dedup_images, lightbox_image_dedup, unwrap_figure_content_wrappers, remove_empty_video_elements, unwrap_special_links, convert_embedded_elements, strip_extra_br_elements, remove_empty_lines) are implemented and wired into standardize_content().

## Tranche 4: Content Pipeline Recovery

Goal: wire the missing upstream pipeline behaviors that affect extraction boundaries.

Work:
- Retry logic:
  - Retry with `removePartialSelectors: false` when word count is below 200.
  - Retry with `removeHiddenElements: false` when word count is below 50.
  - Retry with `removeLowScoring: false` and `removeContentPatterns: false` when still below 50.
- Schema text fallback:
  - When schema.org text is at least 1.5x longer than extracted content, re-extract from the schema-matched element on a sanitized clone.
- Content pattern removal:
  - Boilerplate.
  - Read time.
  - Article cards.
  - Eyebrow labels.
  - Table of contents.
  - Related posts.
  - Subscribe/newsletter blocks.
  - Social engagement counters.
- Metadata block removal:
  - Remove h1-adjacent date/author blocks from content after metadata extraction.
- ScoreAndRemove:
  - Wire existing `is_likely_content` and link-density scoring helpers into the pipeline.

Gate fixtures:
- `content-patterns--card-grid-stripped-headings`
- `content-patterns--iso-date-and-read-time`
- `content-patterns--leading-breadcrumb`
- `content-patterns--social-counter-link`
- `content-patterns--trailing-related-posts`
- `content-patterns--heading-introduced-list`
- `content-patterns--table-of-contents`
- `content-patterns--trailing-subscribe-after-footnotes`
- `content-patterns--social-engagement-counter`
- `content-patterns--code-block-boilerplate-and-trailing-section`
- `gated-content--cookie-consent`
- `hidden--nodes`
- `hidden--visibility`
- `issues--162-aria-hidden-main-content`
- `scoring--related-posts-byline`
- `scoring--table-with-links`
- `issues--284-table-cell-header-scoring`
- `eyebrow--linked-category`
- `eyebrow--paragraph-label`
- `eyebrow--spans-category`
- `eyebrow--wrapper-with-icon`
- `general--trailing-cta-newsletter`
- `general--appendix-heading`
- `metadata--295-header-date-above-title`
- `metadata--295-suggested-post-date`
- `metadata--author-by-prefix-and-url`
- `metadata--date-adjacent-category-link`
- `metadata--placeholder-author-and-duplicate-published`
- `metadata--placeholder-values`
- `metadata--rel-author-in-bio-container`
- `metadata--h1-sibling-byline`
- `issues--header-with-subtitle-p`
- `issues--header-wraps-content`
- `issues--106-menu-id`
- `issues--131-category-links`
- `issues--132-hero-class`
- `issues--196-og-title-brand-name`
- `issues--232-dismiss-in-hidden-content`

Acceptance:
- Gate fixtures match.
- Retry behavior is deterministic and does not mutate the original caller-visible document state.

Status:
- Complete. Third retry (removeLowScoring + removeContentPatterns) wired. Content pattern removals: related posts heading+list detection, table-of-contents, newsletter/subscribe blocks, social engagement counters, social counter links, cookie consent, trailing CTA/appendix sections (with link-dense detection), card-grid blocks (threshold ≥ 3). Aria-hidden support in remove_hidden_elements. Scoring enhanced for related posts and table-with-links. Metadata block removal enhanced for before+after headings, including named date patterns (e.g. "May 22, 2026"). ISO + named date removal, min-read removal. Breadcrumb/eyebrow label removal.

## Tranche 5: Math And Footnotes

Goal: support common static math and footnote forms without browser rendering.

Work:
- Math/MathML-to-LaTeX conversion where the source HTML contains enough semantic data.
- KaTeX and MathJax extraction from common static markup.
- Footnote definition and backref normalization.
- Avoid false positives for equation references and normal superscripts.

Gate fixtures:
- `math--katex`
- `math--mathjax`
- `math--mathjax-chtml-no-assistive`
- `math--mathjax-svg`
- `math--mathjax-tagged-equation`
- `math--mathjax-tex-scripts`
- `math--raw-latex`
- `math--katex-centraliser`
- `math--katex-data-math`
- `math--latex-image-services`
- `footnotes--child-anchor-id`
- `footnotes--heading-notes`
- `footnotes--hr-continuation`
- `footnotes--inline-footnote-span`
- `footnotes--no-false-positive-equation-refs`
- `footnotes--pulldown-cmark-footnote-definition`
- `footnotes--wp-block-footnotes`
- `footnotes--hr-sup-numbered`
- `footnotes--numeric-anchor-id`
- `footnotes--p-class-footnote`
- `footnotes--named-anchor`
- `footnotes--easy-footnotes-wrapper`

Acceptance:
- Gate fixtures match.
- Footnote logic does not convert ordinary superscript content into footnotes.

Status:
- Complete. Math module: KaTeX (annotation extraction), MathJax tex scripts, MathJax SVG/CHTML (data-tex/latex), raw LaTeX delimiters (\(...\) → $...$). Footnote module: WordPress block footnotes, pulldown-cmark/mdBook, Hugo, Easy Footnotes WP, HR-separated sup-numbered, heading-separated, p.footnote class. Both wired into standardize_content() before remove_obsolete_elements.

## Tranche 6: Existing Extractors

Goal: bring already-ported extractors to fixture parity before adding more.

Work:
- Hacker News comments/listing parity.
- Reddit old-reddit comment tree parity.
- GitHub issue and PR metadata/content parity.
- ChatGPT conversation turn labels, citations, and post-Thought content.
- BBCode metadata and spacing.
- Substack app and custom-domain output.

Gate fixtures:
- `comments--news.ycombinator.com-item-id=12345678`
- `listing--news.ycombinator.com-news`
- `general--news.ycombinator.com-item-id=12345678`
- `comments--old.reddit.com-r-test-comments-abc123-test_post`
- `general--github.com-issue-56`
- `general--github.com-test-owner-test-repo-pull-42`
- `extractor--chatgpt-citations`
- `extractor--chatgpt-post-thought-content`
- `codeblocks--chatgpt-codemirror`
- `extractor--bbcode-data`
- `general--substack-app`
- `general--substack-custom-domain`

Acceptance:
- Gate fixtures match.
- Extractor-specific output remains DOM-only and does not fetch network resources.

## Tranche 7: X/Twitter Extractors

Goal: add static X/Twitter extraction for vendored HTML fixtures.

Work:
- Add a Twitter/X extractor registered for `x.com` and `twitter.com`.
- Support static status/conversation pages present in fixtures.
- Support X article fixture shapes:
  - `general--x.com-article`
  - `general--x.com-article-2026-02-13`
- Support localized conversation timeline metadata:
  - `issues--272-x-localized-conversation-timeline`
- Support author extraction from status URL fallback:
  - `issues--161-x-status-url-author`
- Add X Article surrogate repair where needed for emoji/surrogate-pair output.
- Keep X oEmbed and live API fetching out of the core crate unless a separate optional async crate is introduced.

Gate fixtures:
- `general--x.com-article`
- `general--x.com-article-2026-02-13`
- `issues--272-x-localized-conversation-timeline`
- `issues--161-x-status-url-author`

Acceptance:
- X/Twitter gate fixtures are removed from `tests/upstream-exceptions.toml`.
- X/Twitter gate fixtures match.
- No network client, async runtime, browser automation, or JavaScript runtime is added to `defuddle`.

## Tranche 8: Popular General Fixtures

Goal: close high-value general web fixtures once the shared pipeline and extractors are stable.

Gate fixtures:
- `general--developer.mozilla.org-en-US-docs-Web-JavaScript-Reference-Global_Objects-Array`
- `general--wikipedia`
- `general--wikipedia-ipa-pronunciation`
- `general--www.figma.com-blog-introducing-codex-to-figma`
- `entry-point--js-article-content`
- `author-contact-block`
- `author-share-widget`
- `elements--complex-tables`
- `elements--data-table`
- `table-layout--single-column`

Acceptance:
- Gate fixtures match.
- Any remaining mismatch is tied to a concrete future capability, not general formatting drift.

## Deferred Or Out Of Scope

Browser-only features remain out of scope for the core crate:
- `getComputedStyle` and stylesheet media-rule evaluation.
- Shadow DOM traversal and flattening.
- React streaming SSR suspense resolution such as `$RC("B:0","S:0")`.
- Mobile style application.
- Live document mutation guards.
- Pixel-based small image removal unless dimensions are available in static attributes.

Site extractors not currently planned for the core parity push:
- YouTube, because transcripts require async/API fetches.
- Bilibili, explicitly excluded.
- Claude, Gemini, Grok.
- Medium.
- LinkedIn, Threads, Bluesky.
- Discourse.
- LeetCode, LWN, C2 Wiki.
- NYTimes.
- arXiv LaTeXML and Lean/Verso-specific handling, unless later promoted to a dedicated tranche.

Wikipedia is not listed as deferred because existing general Wikipedia fixtures are in scope. A full Wikipedia-specific extractor can be considered later if generic pipeline parity is not enough.
