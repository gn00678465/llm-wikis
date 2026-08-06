# Product

## What this project is

`llm-wikis` is a small Rust CLI that lets an AI coding agent (Claude Code or
Codex CLI) answer a question from a pre-built, curated knowledge base — a
"wiki" — without the caller needing to `cd` into that wiki's project, copy
files, or hand-craft a prompt. Wikis are registered once in a TOML config;
after that `llm-wikis query` runs from anywhere. Every invocation is
read-only: a full-content snapshot check rejects any run that would have
mutated a wiki page. (Source: README.md, Cargo.toml `description`,
docs/llm-wikis.md.)

Current version: 0.1.0-beta.1 (Source: Cargo.toml, git tag history).

## Current phase goals

(Source: developer, init interview 2026-08-06.)

Ship the first stable release, v0.1.0, gated on two things in order:

1. Fix and optimize the known configuration-related issues first — the
   config surface has problems the developer wants corrected before any
   stable release.
2. Then verify the remaining platforms: live end-to-end provider queries on
   Linux/WSL and macOS (currently verified on Windows only — README.md
   "Verification status").

Only after both is v0.1.0 (first non-prerelease tag) published.

## Constraints

- License: MIT (Source: Cargo.toml, LICENSE).
- Toolchain: Rust edition 2024, `rust-version = "1.97"` (Source: Cargo.toml,
  rust-toolchain.toml).
- Supported platforms: Windows x64 (`x86_64-pc-windows-msvc`), Linux/WSL
  x86-64 (`x86_64-unknown-linux-musl`), macOS Apple Silicon; binaries are
  unsigned (Source: README.md, docs/llm-wikis.md §1.2).
- Hard safety guarantee: the wrapper never modifies the knowledge base it
  queries — enforced by a content snapshot check that fails the run on any
  mutation (Source: README.md, src/snapshot.rs, tests/mutation_snapshot.rs).
- Runtime resource caps are config-enforced: `timeout_seconds`,
  `max_question_bytes`, `max_stdout_bytes`, `max_stderr_bytes`; zero-value
  byte caps are rejected as CONFIG_INVALID (Source: README.md config example,
  commit cfea608).
- CI gates: `cargo fmt --check`, `cargo clippy -D warnings`,
  `cargo test --all-targets --all-features -- --test-threads=1`, plus
  installer verification scripts (Source: .github/workflows/ci.yml).

## Open questions

(none)
