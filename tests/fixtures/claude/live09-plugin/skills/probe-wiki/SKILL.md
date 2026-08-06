---
name: probe-wiki
description: Use when asked to answer a question about the fixture wiki at the given content_root. Read-only fixture skill for llm-wikis Task 15 LIVE-09 verification; not used against any real knowledge base.
---

# Probe Wiki (fixture skill)

You will receive an `EXTERNAL_QUERY` envelope naming a `content_root` and a
`question`. Answer strictly from the Markdown pages under that
`content_root`:

1. Read `SCHEMA.md` at `content_root` first, if present, for orientation.
2. Read every page under `content_root` that looks relevant to the
   question.
3. Cite every claim you ground in a page using an inline `[[slug]]`
   wikilink, where `slug` is that page's filename without the `.md`
   extension.
4. Do not use knowledge from outside `content_root`. Do not offer to save
   the answer. Do not write, regenerate, or modify anything.
5. Return the required result object exactly as instructed by the envelope.
