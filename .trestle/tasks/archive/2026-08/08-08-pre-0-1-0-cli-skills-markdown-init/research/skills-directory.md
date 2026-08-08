# 研究：在 llm-wikis repo root 加入 `skills/` 目錄

任務：為 llm-wikis（一個讓 AI coding agent 查詢預建 wiki 知識庫的 Rust CLI）設計一組「教
AI agent 如何使用這個 CLI 本身」的 skills，並決定佈局、內容範圍與發佈方式。

---

## 1. Claude Code Agent Skills 規格

來源：[Extend Claude with skills — code.claude.com](https://code.claude.com/docs/en/skills)（官方文件，2026 版本）

### 1.1 檔案格式

- 每個 skill 是一個目錄，`SKILL.md` 是必要進入點；其餘檔案可選。
- `SKILL.md` = YAML frontmatter（`---` 包住）+ Markdown 指令內容。
- Claude Code 遵循一個跨工具的開放標準 **Agent Skills**（`agentskills.io`），並在此
  標準之上擴充自己的欄位（invocation control、subagent 執行、動態 context 注入）。

> "Claude Code skills follow the Agent Skills open standard, which works across
> multiple AI tools."

### 1.2 Frontmatter 欄位（Claude Code 擴充版，全部 optional，只有 `description`
被標為 recommended）

| 欄位 | 用途 |
|---|---|
| `name` | 顯示名稱，預設用目錄名 |
| `description` | **建議必填** — Claude 用它判斷何時載入這個 skill；被截斷在 1,536 字元 |
| `when_to_use` | 補充觸發條件/範例 prompt，計入同一個 1,536 字元上限 |
| `allowed-tools` | 該 skill 被呼叫的那一輪，預先核准哪些工具（空白/逗號分隔字串或 YAML list）|
| `disallowed-tools` | 該輪禁止使用哪些工具 |
| `disable-model-invocation` | `true` = 只能被使用者手動 `/name` 呼叫，Claude 不會自動觸發 |
| `user-invocable` | `false` = 只有 Claude 能觸發，不出現在 `/` 選單 |
| `context: fork` | 以獨立 subagent context 執行這個 skill |
| `argument-hint` / `arguments` | 給 `$ARGUMENTS` / `$name` 參數替換用 |
| `license` / `compatibility` / `metadata` | Agent Skills 標準的欄位，Claude Code 接受但不作用 |

只給 **跨工具相容的最小集合**（Agent Skills spec 六個欄位）：
`name, description, license, compatibility, metadata, allowed-tools` —
超出這六個欄位，在 claude.ai skill 上傳 / Skills API / `package_skill.py` 打包路徑
會直接報錯拒絕，例如官方文件給出的錯誤訊息：

> "Unexpected key(s) in SKILL.md frontmatter: argument-hint. Allowed properties
> are: allowed-tools, compatibility, description, license, metadata, name"

這一點對 llm-wikis 的 skill 有直接影響：**如果想要同一份 `SKILL.md` 同時被 Claude
Code 原生讀取、又符合 Agent Skills 標準可攜性，frontmatter 應該只用這六個欄位之
內的子集**（llm-wikis 場景大概只需要 `name` + `description`，必要時加
`allowed-tools`）。

### 1.3 目錄結構慣例

官方範例：

```text
my-skill/
├── SKILL.md           # Main instructions (required)
├── template.md        # Template for Claude to fill in
├── examples/
│   └── sample.md      # Example output showing expected format
└── scripts/
    └── validate.sh    # Script Claude can execute
```

另一段落給出更貼近「文件型」skill 的慣例：`scripts/`（可執行）、`references/`
（詳細文件，需要時才載入）、`assets/`（範本/二進位檔）。**`SKILL.md` 建議控制在
500 行以內，細節搬到 `references/*.md`，並在 `SKILL.md` 內用連結指向它們**（"Keep
`SKILL.md` under 500 lines. Move detailed reference material to separate files."）。

### 1.4 存放位置與誰能用

| 位置 | 路徑 | 適用範圍 |
|---|---|---|
| Personal | `~/.claude/skills/<name>/SKILL.md` | 使用者所有專案 |
| Project | `.claude/skills/<name>/SKILL.md` | 此專案 |
| Plugin | `<plugin>/skills/<name>/SKILL.md` | plugin 啟用處，`plugin-name:skill-name` 命名空間 |
| Enterprise | managed settings | 組織全體 |

**分享方式（官方列出的三種）**：

> "Project skills: Commit `.claude/skills/` to version control. Plugins:
> Create a `skills/` directory in your plugin. Managed: Deploy
> organization-wide through managed settings."

### 1.5 llm-wikis 既有文件中的相關約定（本 repo 自己的規格）

`docs/llm-wikis.md:249-256` 已經定義了「wiki 自己的 query skill」要放在哪：

```
.claude/skills/wiki-query/SKILL.md   (Claude, entrypoint = "/wiki-query")
.agents/skills/wiki-query/SKILL.md   (Codex,  entrypoint = "$wiki-query")
```

且 `docs/llm-wikis.md:362-387`（§2.7a）明確要求 Claude 這類 wiki skill 的
frontmatter 必須宣告 `allowed-tools: Read, Grep, Glob`，否則 `--permission-mode
dontAsk` 下所有讀取工具會被拒絕、答案退化成「無法存取 wiki 頁面」。

**注意區分**：這是「**wiki 自己**要提供給 llm-wikis 呼叫的 query skill」，跟本任務
要做的「**教 agent 怎麼用 llm-wikis 這個 CLI**」的 skill 是兩件不同的事——本任務的
skill 不會被 `llm-wikis query` 當作 entrypoint 呼叫，而是給操作 llm-wikis 的
agent（人類 driver 旁邊的 Claude Code / Codex session）自己讀，教它怎麼下
`llm-wikis config init / list / doctor / query` 指令、怎麼讀 `--json` 輸出、怎麼
處理 error code。

---

## 2. Codex CLI 的對應機制

來源：[Build skills — learn.chatgpt.com/docs/build-skills](https://learn.chatgpt.com/docs/build-skills.md)
（`developers.openai.com/codex/skills.md` 308 redirect 到此，同一份官方文件）

### 2.1 格式與必要欄位

> "The `SKILL.md` file must include `name` and `description`."

最小 frontmatter 與 Claude Code 相容子集完全一致——`name` + `description`。

### 2.2 探索路徑（由專案往上到系統層）

| Scope | 路徑 | 用途 |
|---|---|---|
| REPO | `$CWD/.agents/skills` | 目前目錄下的專案專屬 workflow |
| REPO | `$REPO_ROOT/.agents/skills` | 整個 repo 共用 |
| USER | `$HOME/.agents/skills` | 使用者個人、跨 repo |
| ADMIN/SYSTEM | `/etc/codex/skills` | 機器層級預設 |

這與 llm-wikis 自己文件裡 Codex wiki skill 路徑 `.agents/skills/wiki-query/SKILL.md`
（`docs/llm-wikis.md:255`）完全對得上——是同一套慣例。

### 2.3 目錄內容慣例

`SKILL.md`（必要）+ `scripts/`、`references/`、`assets/`（可選）+
`agents/openai.yaml`（可選，UI metadata，Claude Code 側沒有這個檔案）。

### 2.4 觸發方式

- Implicit：prompt 符合 `description` 時自動觸發（可關閉）。
- Explicit：在 Codex CLI 或 IDE extension 打 `$skill-name` 明確引用
  （對照 llm-wikis 文件裡 Codex 的 entrypoint 語法 `$wiki-query`，`docs/llm-wikis.md:254`，
  也是同一套 `$name` 慣例，非本專案自創）。

### 2.5 AGENTS.md 與 skills 的關係

官方文件本身沒有把兩者放在一起比較，但另一份第三方整理（未經官方驗證，列為佐證
而非權威來源）指出 AGENTS.md 是「一直載入的專案級指示」，skills 是「依需求載入
的程序性知識」——這與 Claude Code 文件裡對 CLAUDE.md vs skill 的說法（"Unlike
CLAUDE.md content, a skill's body loads only when it's used"）方向一致，可視為
交叉驗證，但沒有找到 OpenAI 官方對這兩者關係的明文比較（**Not Found**：Codex
官方文件未直接說明 AGENTS.md 與 skills 的優先序/合併規則）。

---

## 3. 先例：CLI/工具 repo 如何在自己 repo 內附帶 skills

### 3.1 本機可見的三種真實先例

**(a) 純 `skills/` 目錄 + 泛用安裝 CLI（`npx skills add`）**

本機 `D:\Skills\skills`（`gn00678465/skills` repo，同一位使用者自己維護的另一個
repo）就是這個模式：

```
skills/
├── decision-tree-helper/{SKILL.md, evals/, references/}
├── git-commands/{SKILL.md, evals/, references/}
├── powershell-skill/{SKILL.md, assets/, evals/, references/, scripts/}
└── wsl-skill/{SKILL.md, evals/, references/}
```

`README.md` 只給一行安裝指令：

```
npx skills add https://github.com/gn00678465/skills.git
```

`skills.sh` 這個第三方 CLI 生態（[skills.sh/agent/codex](https://www.skills.sh/agent/codex)）
明確支援 Codex：

> "Run the command below from your project root, then start a new Codex
> session... `npx skills add <owner>/<repo>` ... installs SKILL.md files
> into your repository so Codex can reference them across sessions."

沒有找到該 CLI 要求來源 repo 具備任何特殊 manifest 的證據——純粹掃描
`skills/*/SKILL.md` 這種慣例目錄結構（**Not Found**：`skills.sh` 內部實作細節/
是否 symlink 或 copy，官方頁面沒說清楚，只查到「安裝 SKILL.md 檔案到你的
repo」這句話）。這代表**只要 repo 有一個 `skills/<name>/SKILL.md` 慣例目錄，
不需要額外設定就能被這套第三方安裝器消費**——對 Claude Code 也有等價文件
（`alirezarezvani/claude-skills` 的 INSTALLATION.md，symlink 或 copy 到
`~/.claude/skills/` 或 `.claude/skills/`）。

**(b) Claude Code plugin marketplace（`.claude-plugin/marketplace.json` + `plugins/<name>/skills/`）**

本機 `D:\Skills\harness-dev\harness-tools`（同一使用者維護的另一 repo，且正是
本專案 `.trestle/` 工作流程背後的 trestle plugin 來源）：

```
harness-tools/
├── .claude-plugin/marketplace.json
└── plugins/trestle/
    ├── .claude-plugin/plugin.json
    ├── skills/
    │   ├── trestle-init/SKILL.md
    │   ├── trestle-plan/SKILL.md
    │   ├── ...（每個 slash-command 對應一個 skill 目錄）
    │   └── references/interview-rules.md   ← 共用參考文件放在 skills/ 底下、不隸屬單一 skill
    ├── agents/ hooks/ commands/ scripts/ templates/
```

`marketplace.json`（`D:\Skills\harness-dev\harness-tools\.claude-plugin\marketplace.json:1-15`）：

```json
{
  "name": "trestle",
  "owner": { "name": "Madao" },
  "description": "Trestle engineering toolkit — writer/evaluator separation, handoff artifacts, observability patterns.",
  "plugins": [
    { "name": "trestle", "source": "./plugins/trestle",
      "description": "...", "version": "0.1.0" }
  ]
}
```

`trestle-init/SKILL.md` frontmatter 範例（`D:\Skills\harness-dev\harness-tools\plugins\trestle\skills\trestle-init\SKILL.md:1-4`）：

```yaml
---
name: trestle-init
description: First-time setup of .trestle/ for a project, or refreshing the AGENTS.md pointer block on re-run. Use when the user asks to adopt trestle / start using this workflow in a repo that doesn't have .trestle/ yet.
---
```

這是官方文件第 1.4 節「Add a `.claude-plugin/plugin.json` to a skill folder and
it loads as a plugin」的實際落地版本，且是**這個開發者自己已經在用、本專案
`.trestle/` 之所以能運作的機制**——強先例，但**只涵蓋 Claude Code**，Codex 沒有
對應的 plugin/marketplace 概念（見第 2 節；且 `docs/llm-wikis.md:352-360` 也明講
「Codex installed plugins are out of scope for 0.1.0」——這是 llm-wikis 自己查詢
機制的限制，不是本研究新發現，但同一套「Codex 不支援 plugin 命名空間」的事實在
這裡也成立）。

**(c) llm-wikis 自己文件裡已規定的「wiki skill」佈局**——見第 1.5 節，是本 repo
現有慣例而非外部先例，但代表使用者已經接受「同一個功能，Claude 用
`.claude/skills/`、Codex 用 `.agents/skills/`」的雙路徑模式。

### 3.2 三種模式的取捨（先給表，第 4 節再細講成本）

| 模式 | Claude Code | Codex | 需要的 repo 額外檔案 |
|---|---|---|---|
| (a) 純目錄 + `npx skills add` | 支援（symlink/copy 到 `.claude/skills/`）| 支援（複製到 `.agents/skills/`）| 無，只要 `skills/<name>/SKILL.md` |
| (b) Claude plugin marketplace | 支援（`/plugin install`）| **不支援**（無 plugin 概念）| `.claude-plugin/marketplace.json` + `plugins/<name>/.claude-plugin/plugin.json` |
| (c) 手動 copy（無安裝器） | 支援 | 支援 | 無，純文件說明 |

---

## 4. llm-wikis 的 skill 內容應涵蓋什麼

依據 `docs/llm-wikis.md` 與 `README.md` 通讀後，一個「教 agent 怎麼用 llm-wikis」
的 skill 至少要覆蓋：

1. **指令總覽**（`docs/llm-wikis.md:428-438`，§3.1）：`config init/list/validate`、
   `list`、`doctor [--wiki][--agent][--live]`、`query --wiki <id> [--agent] -- <question>`，
   以及 `--config <absolute-path>` 是 operator/testing 專用旗標而非 caller-facing
   （§2.2，`docs/llm-wikis.md:165-184`）。
2. **問題輸入方式**：`-- <question>` positional 或 stdin 二選一，兩者都給是
   `ARGUMENT_INVALID`（`docs/llm-wikis.md:461-466`，§3.2）。
3. **`--json` envelope 與 exit class 對照表**（`docs/llm-wikis.md:476-501`，§3.3）：
   `schema_version: "1.0"`、成功/失敗都吐同一份文件、7 級 exit code（0/2/3/4/5/6/7/70）
   與各自意義。這是 agent 要能程式化解析結果的關鍵。
4. **`doctor` 診斷流程**：static checks（`config, roots, wiki_structure, entrypoint,
   executable, auth, read_scope, live_contract, mutation`，`docs/llm-wikis.md:694`）
   全部離線；`--live` 會消耗 model quota、且必須同時給 `--wiki` 與 `--agent`
   （`docs/llm-wikis.md:702-704`）。
5. **`ENTRYPOINT_UNVERIFIED` / 活體 probe 機制**（§2.9，`docs/llm-wikis.md:403-424`）：
   fingerprint 變了（`query_prompt`、skill 目錄、provider 版本、roots）就要重跑
   `doctor --live`。
6. **錯誤碼查表**（§3.8，`docs/llm-wikis.md:711-730`）：整張表列了 20+ 個 error
   code 該怎麼排查，是最適合放進 `references/errors.md` 的內容（太長，不該塞進
   `SKILL.md` 主體，符合官方「500 行以內、細節移到 references」的建議，見 1.3）。
7. **安全/唯讀契約**（§3.4/§3.10）——對「使用 llm-wikis 的 agent」而言，重點是
   **這個工具本身唯讀、不會寫回 wiki**，以及 `CLAUDE_READ_SCOPE_BROAD` /
   `CODEX_READ_SCOPE_BROAD` 這兩個警告代表什麼（§3.6），細節（17 層防護機制的
   完整清單）對「怎麼用」這個目的來說是雜訊，不建議放進主 skill，若要放也該放
   `references/security.md`。
8. **設定/註冊 wiki**（§2.4，`docs/llm-wikis.md:220-274`）：`config.toml` 最小
   範例、`query_prompt` 限制（一行、≤500 bytes，§2.5）、`project_skill` vs
   `local_plugin` 兩種 load 模式（§2.7）。

### 4.1 建議拆幾個 skill

依官方「reference content vs task content」的分類（`code.claude.com/docs/en/skills`
第 199-235 行區段）：

- **一個總覽 skill**（例如 `llm-wikis-usage`）：`description` 對準「使用者要用
  llm-wikis 查某個 wiki / 診斷某個 wiki 連不上」這類觸發語句，內容是「怎麼下
  `query`/`doctor`/`list` 指令、怎麼讀 `--json` 輸出的頂層欄位、遇到失敗先看
  哪個表」，把完整錯誤碼表和安全模型細節搬到 `references/`。這符合上面第 6、7
  點「太長不該塞主體」的判斷。
- 不建議一開始就拆多個 skill（例如「query skill」「doctor skill」「config
  skill」各自獨立）——llm-wikis 指令集本身很小（4 個子命令），拆太細會讓
  `description` 觸發條件彼此重疊、agent 難以判斷該載入哪一個，且違反
  ladder 第 1 條（YAGNI）。**先做一個總覽 skill + 1-2 個 `references/*.md`**，
  之後如果實測發現某個子領域（例如錯誤碼排查）常常被單獨問到、值得獨立觸發，
  再拆不遲。

---

## 5. 安裝/發佈方式選項比較

| 選項 | 做法 | Claude Code 相容 | Codex 相容 | 成本 |
|---|---|---|---|---|
| **純目錄，使用者手動 copy** | repo root 放 `skills/llm-wikis-usage/SKILL.md`，README 寫一行 `cp -r skills/llm-wikis-usage ~/.claude/skills/` 或 `.agents/skills/` | 是 | 是 | 最低——只需要寫對目錄結構 + README 一段話，無額外檔案、無需維護安裝腳本 |
| **install script 順帶安裝**（把 skill 複製/symlink 進 `~/.claude/skills/`、`~/.agents/skills/`）| 修改現有 `install.sh`/`install.ps1`，安裝完 binary 後再複製 skill 目錄 | 是 | 是 | 中——install.sh/ps1 已存在（`docs/llm-wikis.md:19-53`），但要新增「skill 是否已存在、要不要覆蓋」的邏輯，且要決定裝到 personal（`~/.claude/skills`）還是不裝、讓使用者自己選 project-level；此外 llm-wikis 的 install 腳本目前只管理二進位檔＋PATH（`docs/llm-wikis.md:117-137`），混入「修改使用者 `~/.claude` 設定」是新的副作用面，需要卸載邏輯配套（目前連 binary 都「no uninstaller」，§1.6） |
| **Claude Code plugin**（`.claude-plugin/marketplace.json` + `plugins/<name>/skills/`）| 依第 3.1(b) 節的 trestle 範例佈局 | 是（`/plugin install` 或 marketplace add）| **否**——Codex 沒有 plugin/marketplace 概念（第 2 節），需要另外維護一份 `.agents/skills/` 給 Codex 使用者，等於兩套佈局 | 最高——多一層 `.claude-plugin/plugin.json`/`marketplace.json` schema 要維護，且天生只服務一半的目標使用者（llm-wikis 明確是「Claude Code **或** Codex CLI」雙 provider 工具，`README.md:3-4`），若只做 plugin 會讓 Codex 使用者完全拿不到這個 skill |
| **第三方安裝器**（`npx skills add https://github.com/.../llm-wikis.git`，同第 3.1(a) 先例）| 只要 repo 有 `skills/<name>/SKILL.md` 慣例目錄即可被消費，無需額外 manifest | 是 | 是（`skills.sh` 官方頁面明確列出 Codex 支援，見第 3.1(a)）| 低——複用純目錄佈局，只是多告訴使用者一種「不想手動 copy 就用這個」的選項；缺點是引入一個 llm-wikis 專案不維護的外部依賴（第三方 CLI 的可用性/版本風險） |

**取捨結論方向（供 planner 參考，非決策）**：純目錄佈局同時滿足 Claude Code 與
Codex，成本最低，也符合 ladder 第 1-2 條（YAGNI + 複用已有 install.sh/README
慣例而非新增 marketplace 機制）。Claude plugin marketplace 先例雖然存在且品質好
（本專案自己在用），但它結構性地排除 Codex 使用者，與 llm-wikis 「雙 provider」
的產品定位（`README.md:3`）衝突，除非決定同時維護 plugin 版與純目錄版兩份。

---

## Load-bearing claims

1. **Claude Code SKILL.md 只有 `description` 被建議必填，其餘欄位全選填；
   跨工具可攜的欄位子集僅有 `name, description, license, compatibility, metadata,
   allowed-tools` 六個，其他欄位在 claude.ai/Skills API 打包路徑會被拒絕。**
   — [code.claude.com/docs/en/skills](https://code.claude.com/docs/en/skills)
   §Frontmatter reference 與 §Using skill frontmatter outside Claude Code
   （抓取全文第 237-296 行區段，含官方錯誤訊息原文引用）。
2. **Codex CLI 的 skill 探索路徑固定為 `$CWD/.agents/skills` →
   `$REPO_ROOT/.agents/skills` → `$HOME/.agents/skills` → `/etc/codex/skills`，
   frontmatter 必要欄位僅 `name`/`description`**，且此路徑與 llm-wikis 自己
   `docs/llm-wikis.md:255`（`.agents/skills/wiki-query/SKILL.md`）的既有慣例一致。
   — [learn.chatgpt.com/docs/build-skills.md](https://learn.chatgpt.com/docs/build-skills.md)
   （`developers.openai.com/codex/skills.md` 官方 308 導向的同一份文件）。
3. **本機已有兩個真實先例可直接參照佈局**：(a) 純 `skills/<name>/SKILL.md` +
   `npx skills add` 一行安裝（`D:\Skills\skills\README.md:1-9`，且
   [skills.sh/agent/codex](https://www.skills.sh/agent/codex) 證實同一套機制支援
   Codex），(b) Claude plugin marketplace（`D:\Skills\harness-dev\harness-tools\.claude-plugin\marketplace.json:1-15`
   + `D:\Skills\harness-dev\harness-tools\plugins\trestle\skills\trestle-init\SKILL.md:1-4`），
   後者是本專案 `.trestle/` 工作流程實際依賴的機制，但只服務 Claude Code、不服務
   Codex，與 llm-wikis 雙 provider 定位（`README.md:3`）有結構性衝突。
