# DEFR-1: Ignored extraction options

Status: draft

## Summary

`DefuddleOptions.markdown`, `remove_images`, and `language` are public and unused. The CLI `--lang` flag only stores `language`. Markdown output follows `separate_markdown`. Image stripping follows `remove_small_images`.

## Motivation

A caller that sets these fields gets the same result as leaving them at the default, with no error. The README and `--help` describe `--lang` as a preferred BCP 47 language. `result.language` still comes from `metadata::get_language`.

## Design

Remove `markdown` and `remove_images` from `DefuddleOptions`. No in-repo caller reads them. The CLI `--markdown` flag stays, and it continues to set `separate_markdown`.

For `language`, pick one of the choices below and delete the other behavior. Do not keep a parsed flag that the pipeline drops.

## Alternatives considered

Wire `markdown: true` to mean `separate_markdown` or to replace `content` with Markdown. That preserves a field nobody in this repo sets, and it collides with `separate_markdown`. Removing it is smaller.

Treat `remove_images: true` as "drop every img". That is a new behavior, not a repair of the current one.

## Open questions

- What should `--lang` do? Override `result.language`, choose among alternate page bodies, or go away? Recommendation: remove the flag, the field, the help line, and the README row until a caller has a selection rule. Do not invent one in the port.

## Decision

Removed `DefuddleOptions.markdown`, `remove_images`, and `language`. Removed `--lang` and its help and README lines; the flag is rejected. Markdown output stays `separate_markdown`. Image removal stays `remove_small_images`.
