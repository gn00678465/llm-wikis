# Synthetic Fixture Schema (wiki-flat)

This file exists only so `tests/wiki_preflight.rs` and `tests/citations.rs` can
exercise the `wiki_structure` check against a real `SCHEMA.md`. It documents
no real toolchain and describes no real knowledge base.

## Wikilink grammar (informational example, not parsed by this fixture's tests)

- `[[slug]]` — bare
- `[[slug|label]]` — labelled
- `[[slug](path/to/file.md)]` — markdown form
