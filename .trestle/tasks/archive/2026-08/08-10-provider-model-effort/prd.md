# provider 設定支援 Claude/Codex model 與 reasoning effort

來源：GitHub issue #7。`[providers.claude]` / `[providers.codex]` 目前只能設
`executable`，query 用哪個 model 與 reasoning effort 完全由 provider CLI 預設值
決定。本任務讓 operator 可明確指定，且未設定時行為與現版本逐位元相同。

## Findings

- F1 `ProviderConfig`（`src/config.rs:233-236`）目前僅有 `executable`，且帶
  `#[serde(deny_unknown_fields)]`；`Config::validate`（`src/config.rs:392-403`）
  以 struct pattern `ProviderConfig { executable: Some(exe) }` 解構，新增欄位會
  讓該 pattern 無法編譯，必須改寫。
- F2 `validate_executable`（`src/config.rs:499-539`）是既有的值驗證範本：空值、
  控制字元、空白、字元集各自回不同訊息的 `CONFIG_INVALID`。新欄位驗證沿用同一
  風格與錯誤碼，不新增 error code（`tests/spec_drift.rs` 鎖定 27 碼封閉集合）。
- F3 Claude argv 由 `build_argv`（`src/providers/claude.rs:109-154`）建構，固定
  部分後接 optional `--plugin-dir`；exact-argv 測試在 `tests/claude_adapter.rs`
  的 `exact_argv`。Codex argv 由 `build_argv`（`src/providers/codex.rs:53-81`）
  建構，`-c` override 已有兩個（`mcp_servers={}`、`developer_instructions=...`）；
  exact-argv 測試在 `tests/codex_adapter.rs` 的 `exact_argv`。
- F4 version/auth probe 走 `probe_request`（`src/providers/claude.rs:250`、
  `src/providers/codex.rs:247`），args 由呼叫端逐一列出，與 `build_argv` 無關 ——
  只要不動 probe 呼叫端，model/effort 天然不會出現在 `--version` 與 auth probe。
- F5 `doctor --live` 重用 `QueryService::query(.., QueryMode::Verification)`
  （`src/doctor.rs:9`），因此 model/effort 只要進入 `ProviderRequest` → `build_argv`
  就同時覆蓋 query 與 live doctor；靜態 doctor 不啟動 query，天然不受影響。
- F6 `list` 與靜態 doctor 都不建構 argv；靜態 doctor 的 `executable` check 只做
  `resolve_executable` 與 version/auth probe（`src/doctor.rs:497-520`）。
- F7 probe 快取的 `compatibility_fingerprint` 輸入是
  `CompatibilityFingerprintInput`（`src/query.rs:314-332`），建構於
  `src/doctor.rs:574` 與 `src/query.rs:665`；規格 §15.1（design 文件 line 988）
  說該 fingerprint 涵蓋「selected wiki、its provider table、query_prompt、
  the provider executable declaration、provider safety/contract version」。
  model/effort 就住在 `[providers.<agent>]` 這張 provider 宣告表裡。
- F8 `ProviderConfig` 目前在三個測試以 struct literal 直接建構
  （`tests/doctor.rs:151`、`tests/doctor.rs:292`、`tests/query_service.rs:143`），
  新增欄位需同步補齊。
- F9 Codex `-c` 的 value 會先被 TOML 解析、失敗才 fallback 成 raw literal
  （`src/providers/codex.rs:37-52` doc comment）。`model_reasoning_effort="high"`
  這種「argv value 內含雙引號」的形狀，在 Windows `.cmd` shim 路徑是本任務唯一
  沒有既有測試覆蓋的新形狀（既有 metacharacter 測試涵蓋 `& | ^ %VAR%` 但不含 `"`）。
- F10 `AGENTS.md` 記錄：`src/cli.rs` 的 `///` 是 `--help` 文字。本任務不新增任何
  CLI flag（設定只走 TOML），因此不觸及該約束，也不動 JSON envelope。

## Decisions

- D1 設定形狀：`[providers.<agent>]` 新增 optional `model` 與 `effort` 兩個字串
  欄位（issue #7 建議格式）。兩者皆未設定時，argv 與現版本逐位元相同。
- D2 `model` 驗證規則：非空、UTF-8 位元組長度 ≤ 128、無控制字元、無空白字元、
  不以 `-` 開頭。字元集不再收斂（issue 明列「允許 provider alias 與完整 model
  ID」，收斂字元集會讓未來的新 ID 形狀被反序列化擋掉）。
- D3 `effort` 驗證規則：非空、長度 ≤ 32、字元限 `[A-Za-z0-9_-]`、首字元須為
  英數（issue 要求的 token 集之嚴格子集，理由同 D2 的不以 `-` 開頭）。
  effort 是否被該 model 支援不由本 CLI 判斷，provider 回報的錯誤維持一般
  provider failure。
- D4 Claude 映射：`--model <model>`、`--effort <effort>`，各自為獨立 argv value，
  接在既有固定 argv 與 optional `--plugin-dir` 之後。
- D5 Codex 映射：`--model <model>` 與 `-c model_reasoning_effort="<effort>"`，
  兩者都放在 `exec` 之後、與既有 `-c` override 相鄰。`-c` 是 `--config` 的短旗標
  （同一個旗標），採 `-c` 是為了與檔案裡既有的兩個 override 一致；issue 驗收條件
  寫的 `--config model_reasoning_effort=...` 指的是這個 override 本身。effort 值
  依 Codex 官方設定參考帶雙引號，讓它以 TOML 字串而非 raw literal fallback 解析。
- D6 套用範圍：只有 `query` 與 `doctor --live`（同一條 `QueryService::query` 路徑）。
  `--version` probe、auth status probe、靜態 doctor、`list` 都不帶（F4/F5/F6）。
- D7 fingerprint（使用者 2026-08-10 選定）：model/effort 納入
  `compatibility_fingerprint`，改 model 或 effort 會讓既有 live-doctor probe 失效、
  query 需重跑 `doctor --live`——與改 `query_prompt` 同等對待。同時把
  `PROVIDER_CONTRACT_VERSION` 由 `"1"` 升到 `"2"`，因為 `build_argv` 的 wire shape
  確實變了（該常數的 doc comment 就是這麼規定的）。
- D8 文件同步：`config.example.toml`、`INIT_TEMPLATE`（註解形式的可選欄位說明，
  不新增互動精靈問題）、README、operator guide `docs/llm-wikis.md`、design 規格
  `docs/2026-07-28-llm-wikis-external-query-design.md` §6.1 與 §15.1 一併更新。
- D9 不在範圍：CLI flag 形式的 model/effort 覆寫、per-wiki model 設定、對 effort
  值做 provider/model 相容性判斷、任何 JSON envelope 形狀變更。

## Acceptance Criteria

- [x] AC1: `ProviderConfig` 支援 optional `model` 與 `effort`；未設定時
  Claude 與 Codex 的 argv 與本任務前完全相同。
  (evidence: src/config.rs ProviderConfig, tests/claude_adapter.rs test exact_argv,
  tests/codex_adapter.rs test exact_argv)
- [x] AC2: Claude exact-argv 測試涵蓋 `--model` 與 `--effort`，且兩者各為獨立
  argv value。
  (evidence: tests/claude_adapter.rs test exact_argv_with_model_and_effort)
- [x] AC3: Codex exact-argv 測試涵蓋 `--model` 與 model_reasoning_effort override。
  (evidence: tests/codex_adapter.rs test exact_argv_with_model_and_effort)
- [x] AC4: version probe 與 auth status probe 的 argv 不含 model 或 effort。
  (evidence: tests/claude_adapter.rs test probe_argv_never_carries_model_or_effort,
  tests/codex_adapter.rs test probe_argv_never_carries_model_or_effort)
- [x] AC5: 空值、控制字元、空白、超長、非法字元的 model/effort 在 config load
  階段即失敗於 CONFIG_INVALID。
  (evidence: tests/config_contract.rs test model_and_effort_validation_rejects_bad_values)
- [x] AC6: 問題內容與 model 值仍以分離 argv/stdin 傳遞，不經 shell；含雙引號的
  Codex override value 在 Windows batch shim 路徑逐字送達。
  (evidence: tests/process_supervisor.rs test batch_shim_preserves_quoted_config_override_argument)
- [x] AC7: model/effort 進入 compatibility fingerprint：改 model 或 effort 會讓
  既有 probe 記錄失效。
  (evidence: tests/probes.rs test fingerprint_changes_when_model_or_effort_changes)
- [x] AC8: `config init` 樣板、config.example.toml、README 與 operator guide 都
  說明 model/effort 設定；JSON output contract 不受影響。
  (evidence: config.example.toml, README.md, docs/llm-wikis.md,
  docs/2026-07-28-llm-wikis-external-query-design.md, tests/config_init.rs)

## Verification Plan

```
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features -- --test-threads=1
```

已知例外（非本任務造成）：`tests/process_supervisor.rs` 兩個 deadline-race 測試
在高延遲環境偶發失敗，見 AGENTS.md。
