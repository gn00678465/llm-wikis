# Distill — 08-12-query-leaf-inline-termimad

## Routed changes

| target | change | reason |
|---|---|---|
| AGENTS.md `## Working rules` | 擴充既有的 gates.ts 格式陷阱條目，補兩個本輪實際踩到的新實例：(1) checklist 的純敘述 gate 列只要含反引號 token（本例是 `--json`），literal command extractor 就會把它當指令執行並 exit 1 —— 敘述列必須完全不帶反引號，不是「不要寫成指令」而已；(2) `## Deferral Checks` 區段一旦存在就必須是 `\| id \| check \| approved-by \|` 表格，寫「無 deferral」的敘述會被判為 unparseable 而 block，沒有 deferral 就整段刪掉。 | 兩者各讓 round 1 被 block 一次，且都與既有條目同屬 gates.ts 解析器家族，合併進同一條而非另起近似條目。 |
| AGENTS.md `## Working rules` | 新條目：prd.md 的 `## Expected Files` 是 gates.ts scope gate 的白名單，任何實作中才發現要動的檔案（本例是三個測試 binary：list、query_service、viewer）都必須補進去，否則 verdict 直接 block；同時 AC 的 `- [ ]` 必須在 gate 前改成 `- [x]`，否則每條 AC 都回報 "not checked"。 | 規劃期不可能完全預知會動到哪些測試檔，這是每個任務都會遇到的收尾動作，寫下來讓下次一次到位而不是被 block 後才補。 |
| AGENTS.md `## Working rules` | 新條目：任何「預設值會去接觸主機環境」的設定（本例 `[viewer].backend` 預設 leaf 會去 PATH 找並 spawn 真的二進位），在測試 fixture 的 `Config` literal 裡必須顯式關掉，不能沿用 `Default`。否則測試結果取決於執行機器上裝了什麼，在我的機器綠、在 CI 變 warn。 | 這是本任務唯一一個「若沒注意就會寫出環境相依測試」的陷阱，且未來任何新增的外部工具整合都會重演。 |

## Nothing to record

- reviewed: prd.md Findings F1-F11 與 Decisions D1-D12、design.md 全部五節、
  checklist.md G1-G25、research/leaf-inline.md 的 F1-F12 與七項 open risks、
  evidence/gates-round-1.json 兩次執行的差異。
- why: 其餘候選都已經有永久歸宿或屬任務範圍。leaf 的行為事實（SPEC 語法、
  不讀 NO_COLOR、`--inline` 的版本下限、config/theme 無旁路）已寫進
  `docs/llm-wikis.md` 與 research 檔，屬產品文件與一手證據，重抄成 agent 工作
  規則只會製造第二份會漂移的副本。capture-then-commit 的理由寫在
  `src/viewer.rs` 的模組 doc comment，就在改動它的人一定會讀到的地方。
  主機行程列舉延遲造成的 deadline-race flakiness 上個任務已建立規則，本輪
  再次出現（verify-3 先 exit 101、重跑通過）並被該規則正確涵蓋，不需要加強。
  沒有新的可機器檢查規則，因此不提 evolve.ts。

## ARCHITECTURE.md / PRODUCT.md

ARCHITECTURE.md 值得記一筆：終端渲染從「in-process crate（termimad）」改為
「外部二進位（leaf）」，是本專案第一個為了純展示目的而 spawn 外部程序的決定，
且與 08-08 研究當時的結論相反（該研究明確反對此路線，理由是把展示需求變成安裝
需求）。使用者 2026-08-12 在知悉該結論的情況下選定，決策與其反對理由都應留在
架構決策紀錄裡，否則未來有人只讀到 08-08 的結論會以為這是誤植。
