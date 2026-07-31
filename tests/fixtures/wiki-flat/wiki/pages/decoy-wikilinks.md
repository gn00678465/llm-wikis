# Example Wikilink Syntax (synthetic fixture page)

This page demonstrates the wikilink grammar for schema authors. Every slug
below is deliberately dangling: it resolves to nothing anywhere in this
fixture tree, the same way a real schema file's syntax examples do.

- Bare form: [[nonexistent-page]]
- Labelled form: [[also-missing|Missing Page]]
- Markdown form: [[still-not-real](../pages/still-not-real.md)]

Also ignored, not rejected, per spec §11.1: [[raw/source-notes.txt]],
[[assets/diagram.svg]], a bare https://example.invalid/ URL, an ordinary
[Markdown link](https://example.invalid/), and dangling prose like [[.
