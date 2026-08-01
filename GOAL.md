Optimize defuddle-rs for a roughly 20% end-to-end performance improvement across the representative benchmark suite.

The current Markdown postprocessing optimization is already committed. Treat the current HEAD and PERF.md as the authoritative baseline, but capture fresh 100-sample release benchmarks before making changes.

Primary targets:

- At least ~20% lower median wall time on each major Markdown workload:
  - substack_body_markdown
  - substack_raw_markdown
  - wikipedia_markdown
  - github_issue_markdown
  - hackernews_comments_markdown
- At least ~20% lower median wall time on the representative generic HTML workloads:
  - substack_body_html
  - substack_raw_html
  - wikipedia_html
- At least ~15–20% reduction in cumulative allocation bytes on the workloads improved by each change.
- Reduce peak live allocation where practical; do not trade a meaningful wall-time win for a large memory regression.
- No representative workload may regress by more than 5% without explicit evidence that the tradeoff is necessary.
- Preserve exact fixture output and all existing public behavior.

Investigate ambitious infrastructural changes, including:

1. Selector/query execution planning:
   - Batch or plan static selectors across the generic pipeline.
   - Consider scope-aware traversal, candidate buckets, compiled predicates, and mutation-safe read/mutate phases.
   - Do not add a general-purpose selector index unless measurements show that invalidation complexity is justified.

2. One-pass generic pipeline feature collection:
   - Collect subtree text, link density, descendant counts, content flags, hidden/media indicators, and scoring features in shared traversals.
   - Avoid merely adding a cache or feature vector if it does not eliminate enough work.

3. Serializer and buffer reuse:
   - Add output-buffer APIs only where they eliminate real temporary allocations.
   - Target repeated subtree serialization, Markdown preprocessing, footnote assembly, sanitizer output, and final extraction.
   - Measure cumulative allocations as well as wall time.

4. Retry/reparse redesign:
   - Measure retry frequency and cost first.
   - Evaluate pristine DOM cloning, copy-on-write payloads, mutation journals, rollback, or staged pruning.
   - Do not retain a clone/journal design that adds cost to ordinary pages without a clear retry-path win.

5. DOM ownership and storage:
   - Evaluate string interning, compact text/attribute storage, shared immutable payloads, copy-on-write mutation, node payload separation, and reuse of removed-node capacity.
   - Prefer measured, incremental changes over a speculative rewrite.
   - Pay particular attention to clone cost, attribute mutation, serialization, and memory layout.

Workflow requirements:

- Make one focused commit per retained optimization.
- Before each change, capture the relevant baseline benchmarks.
- After each change, run the same benchmark command and compare medians, allocation counts, cumulative bytes, peak live allocation, and variance.
- Use at least 100 Divan samples for final comparisons; repeat noisy comparisons when necessary.
- Treat changes within normal run-to-run noise as neutral.
- Keep experiments isolated so each commit is independently bisectable.
- Preserve unrelated user changes.
- Do not run pre-commit hooks.
- Do not push to any remote.
- Run focused tests after each behavior-changing experiment.
- Run the complete fixture harness before retaining a behavior-affecting change.
- Run `cargo test --workspace`, `cargo fmt --all -- --check`, and `git diff --check` before completion.

Discard policy:

- Discard an approach if it is neutral, slower, allocation-neutral, or too complex for the measured gain.
- Do not keep abstractions merely because they are architecturally elegant.
- For every discarded approach, add a concise entry to PERF.md describing:
  - what was attempted;
  - which workloads were measured;
  - before/after medians;
  - allocation results;
  - why it was discarded.
- Keep the worktree clean except for intentional committed changes.

Completion criteria:

- The retained commits collectively achieve approximately 20% lower median wall time across the representative HTML and Markdown benchmark groups, or provide strong measured progress toward that target with all remaining approaches exhausted.
- No major workload regresses by more than 5%.
- Fixture parity remains exact.
- PERF.md contains current benchmark results, retained optimization entries, and discarded-experiment evidence.
- The final response lists every retained commit, every discarded approach, benchmark deltas, test commands, and any remaining gap to the 20% target.
