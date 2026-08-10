# Implementation plan — provider model 與 reasoning effort

順序：設定層 → argv 映射 → 請求傳遞 → fingerprint → 文件 → 全量回歸。
每一步都先寫失敗測試再實作，並在每步結束跑一次 Verification Plan。

## Step 0 — baseline

跑一次 Verification Plan 確認 branch 起點是綠的。失敗即為既有狀況，停下回報，
不歸因本任務（例外：AGENTS.md 記錄的兩個 deadline-race flaky 測試）。

**Rollback point**: 尚未修改任何檔案。

## Step 1 — 設定欄位與驗證

Design 參照：§1。

1. `tests/config_contract.rs`：新增 `model_and_effort_round_trip`（設定被讀入）與
   `model_and_effort_validation_rejects_bad_values`（空字串、含 `\n`、含空白、
   超長、`--flag` 形狀、effort 非法字元各一組，全部 `CONFIG_INVALID`）。先紅。
2. `src/config.rs`：`ProviderConfig` 加 `model`/`effort`；`Config::validate` 改用
   `validate_provider_table` helper；新增 `validate_model`、`validate_effort`。
3. `tests/doctor.rs`（兩處）與 `tests/query_service.rs`（一處）的 `ProviderConfig`
   struct literal 補上新欄位 `None`。

**Rollback point**: `git checkout -- src/config.rs tests/`。

## Step 2 — Claude argv

Design 參照：§3.1。

1. `tests/claude_adapter.rs`：新增 `exact_argv_with_model_and_effort`；保留原
   `exact_argv` 不動（未設定時逐位元不變的證據）。新增
   `probe_argv_never_carries_model_or_effort`。
2. `src/providers/claude.rs`：`build_argv` 加兩個 `Option<&str>` 參數並在尾端附加。
3. 更新 `invoke` 內的呼叫端。

**Rollback point**: `git checkout -- src/providers/claude.rs tests/claude_adapter.rs`。

## Step 3 — Codex argv

Design 參照：§3.2。

1. `tests/codex_adapter.rs`：新增 `exact_argv_with_model_and_effort` 與
   `probe_argv_never_carries_model_or_effort`；原 `exact_argv`/`flag_order` 不動。
2. `src/providers/codex.rs`：`build_argv` 加參數，`--model` 與
   `-c model_reasoning_effort="<e>"` 置於 exec 之後、既有 `-c` override 之側。
3. `tests/process_supervisor.rs`：新增
   `batch_shim_preserves_quoted_config_override_argument` —— 真的 spawn 一個
   `.cmd` shim，斷言 `model_reasoning_effort="high"` 逐字送達（Windows only）。

**Rollback point**: `git checkout -- src/providers/codex.rs tests/`。

## Step 4 — 請求傳遞

Design 參照：§2。

1. `ProviderRequest` 加 `model`/`effort` 欄位。
2. `src/query.rs` 的建構點自 `config.providers.table_for(agent)` 填值。
3. 兩個 adapter 的 `invoke` 把值傳進 `build_argv`。
4. 修掉因欄位新增而編譯失敗的測試建構點。

**Rollback point**: `git checkout -- src/providers/ src/query.rs`。

## Step 5 — fingerprint

Design 參照：§5。

1. `tests/probes.rs`：新增 `fingerprint_changes_when_model_or_effort_changes`。
2. `src/query.rs`：`CompatibilityFingerprintInput` 加兩個 optional 欄位；
   `PROVIDER_CONTRACT_VERSION` 改 `"2"`。
3. `src/doctor.rs:574` 與 `src/query.rs:665` 兩個建構點填值。
4. 修 `tests/doctor.rs` 兩個 fingerprint 建構點。

**Rollback point**: `git checkout -- src/query.rs src/doctor.rs tests/`。

## Step 6 — 文件

Design 參照：§6。逐檔更新後跑 `cargo test --test config_init --test cli_contract
--test spec_drift`（INIT_TEMPLATE 有測試逐字比對）。

**Rollback point**: `git checkout -- docs/ README.md config.example.toml src/config.rs`。

## Step 7 — 全量回歸

跑完整 Verification Plan，把輸出寫進 `evidence/round-1.md` 與
`evidence/gates-round-1.json`。
