# Design — provider model 與 reasoning effort

Branch: `feat/7-provider-model-effort`（自 `feat/pre-0.1.0-cli-refinements` 開出）。

## 0. 必須存活的跨切面不變式

- 未設定 model/effort 時，Claude 與 Codex 的 argv 逐位元不變。這是本任務唯一的
  回歸風險來源，由兩個既有的 `exact_argv` 測試（不帶新參數的版本）保護。
- 不新增 `ErrorCode`、doctor check name、wrapper warning code —— `tests/spec_drift.rs`
  機械化核對這三個封閉集合。model/effort 的驗證錯誤一律沿用 `CONFIG_INVALID`。
- 不新增 CLI flag，`--help` 文字不變（AGENTS.md 的 `///` vs `//` 約束不觸及）。
- JSON envelope 形狀不變；`ProviderConfig` 只被設定檔讀取，不出現在任何輸出 envelope。
- argv 永遠不經 shell 組字串：model 與 effort 各自是獨立 `OsString`，effort 只
  以 `model_reasoning_effort="<value>"` 這一個 value 出現。

## 1. 設定層（src/config.rs）

```rust
pub struct ProviderConfig {
    pub executable: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
}
```

`Config::validate` 目前用 struct pattern 解構（`ProviderConfig { executable: Some(exe) }`），
新增欄位後改成綁定整個 table 再逐欄驗證，Claude/Codex 共用一個 helper：

```rust
fn validate_provider_table(table: &ProviderConfig) -> Result<(), AppError>
```

驗證規則（prd D2/D3）：

| 欄位 | 規則 | 失敗訊息主旨 |
|---|---|---|
| model | 非空 | provider model must not be empty |
| model | ≤ 128 UTF-8 bytes | provider model is too long |
| model | 無控制字元 | provider model must not contain control characters |
| model | 無空白 | provider model must be one model name, not arguments |
| model | 不以 `-` 開頭 | provider model must not start with '-' |
| effort | 非空、≤ 32 bytes | provider effort ... |
| effort | 僅 `[A-Za-z0-9_-]`、首字英數 | provider effort ... |

長度上限以 UTF-8 位元組計（`value.len()`），不是字元數 —— issue 明寫「UTF-8 byte
長度上限」。

## 2. Provider 請求傳遞（src/providers/mod.rs、src/query.rs）

`ProviderRequest` 新增 `model: Option<String>` 與 `effort: Option<String>`。
`QueryService::query` 自 `config.providers.table_for(agent)` 取值填入
（`src/query.rs:746` 的建構點）；`table_for` 已存在（`src/config.rs:222-228`）。

## 3. Argv 映射

### 3.1 Claude（src/providers/claude.rs）

`build_argv` 簽章加兩個 `Option<&str>`，附加在既有固定 argv 與 optional
`--plugin-dir` 之後：

```
... --setting-sources project [--plugin-dir <dir>] [--model <m>] [--effort <e>]
```

放最後而非插入中段，是為了讓既有 `exact_argv`/`flag_order` 斷言的相對位置全部
不動 —— 新旗標只可能出現在尾端。

### 3.2 Codex（src/providers/codex.rs）

`--model` 與 effort override 都在 `exec` 之後，緊接既有兩個 `-c` override：

```
... -c mcp_servers={} -c developer_instructions=... [--model <m>]
    [-c model_reasoning_effort="<e>"] --disable browser_use ...
```

effort value 帶雙引號是刻意的（prd D5）：`-c` 的 value 先被 TOML 解析，
`"high"` 解析成 TOML 字串；不帶引號則走 raw-literal fallback。兩者對 Codex 等價，
但帶引號與官方設定參考一致，且 effort 的字元集（`[A-Za-z0-9_-]`）保證不需跳脫。

雙引號在 argv 裡對 Windows `.cmd` shim 是新形狀，用一個真的 spawn 的
process_supervisor 測試證明它逐字送達，而不是只靠 argv 單元測試（prd F9/AC6）。

## 4. 探測面（不變）

`version` 與 `auth_status` 走 `probe_request` 並自行列出 args，本任務不改那些
呼叫端 —— 因此 model/effort 結構上不可能出現在探測 argv。以測試把這件事釘住，
避免日後有人「順手」把 probe 也改成走 `build_argv`。

## 5. Probe 快取 fingerprint（src/query.rs、src/doctor.rs）

`CompatibilityFingerprintInput` 新增 `model_declaration: Option<&str>` 與
`effort_declaration: Option<&str>`，兩個建構點（`src/doctor.rs:574`、
`src/query.rs:665`）同步填入。`PROVIDER_CONTRACT_VERSION` 由 `"1"` 升到 `"2"`。

兩者都做的理由：新欄位負責「日後改 model/effort 要重新驗證」，版本號負責
「這次 argv wire shape 變了，所有舊 probe 一次失效」——後者正是該常數 doc
comment 規定的動作。實務上第一次升級兩者都會讓舊記錄失效，這是預期行為：
operator 升級後第一次 query 會被要求重跑 `doctor --live`。

## 6. 文件

| 檔案 | 變更 |
|---|---|
| `config.example.toml` | `[providers.*]` 加 model/effort 範例（註解說明 optional） |
| `src/config.rs` INIT_TEMPLATE | 加註解形式的 optional 欄位說明，不改互動精靈 |
| `README.md` | provider 設定段落補 model/effort |
| `docs/llm-wikis.md` | provider 設定段落補 model/effort 與已知值清單 |
| design 規格 §6.1 | provider 宣告允許 model/effort 的一句 |
| design 規格 §15.1 | fingerprint 涵蓋 provider 宣告的 model/effort |

已知值只寫成「目前已知」的敘述性清單，不寫成規範性封閉集合 —— issue 明確要求
未來新增值不必改反序列化 schema。
