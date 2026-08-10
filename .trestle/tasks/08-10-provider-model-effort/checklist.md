# Checklist — provider model 與 reasoning effort

## Hard Gates

| id | check | type |
|---|---|---|
| G1 | `cargo build --all-targets --all-features` | gate |
| G2 | `cargo fmt --all --check` | gate |
| G3 | `cargo clippy --all-targets --all-features -- -D warnings` | gate |
| G4 | `cargo test --all-targets --all-features -- --test-threads=1` | gate |
| G5 | `cargo test --test config_contract -- --test-threads=1` | gate |
| G6 | `cargo test --test claude_adapter -- --test-threads=1` | gate |
| G7 | `cargo test --test codex_adapter -- --test-threads=1` | gate |
| G8 | `cargo test --test probes -- --test-threads=1` | gate |
| G9 | `cargo test --test spec_drift -- --test-threads=1` | gate |
| G10 | `rg -F "pub model: Option<String>" src/config.rs` | gate |
| G11 | `rg -F "pub effort: Option<String>" src/config.rs` | gate |
| G12 | `rg -F "model_reasoning_effort" src/providers/codex.rs` | gate |
| G13 | `rg -n -- "--effort" src/providers/claude.rs` | gate |
| G14 | `rg -F "PROVIDER_CONTRACT_VERSION: &str = \"2\"" src/query.rs` | gate |
| G15 | `rg -F "model_declaration" src/query.rs src/doctor.rs` | gate |
| G16 | `rg -F "effort" config.example.toml` | gate |
| G17 | `rg -F "effort" README.md` | gate |
| G18 | `rg -F "effort" docs/llm-wikis.md` | gate |
| G19 | `rg -F "effort" docs/2026-07-28-llm-wikis-external-query-design.md` | gate |
| G20 | 未設定 model/effort 時的 Claude/Codex argv 與本任務前逐位元相同 —— evaluator 直接比對兩個未帶新參數的 exact_argv 測試相對於 base commit 的 diff，該兩個測試的 expected 向量必須完全沒有變動 | gate |
| G21 | version probe 與 auth probe 的 argv 不含 model/effort —— evaluator 確認 probe 呼叫端仍自行列出 args、未改走 build_argv | gate |
