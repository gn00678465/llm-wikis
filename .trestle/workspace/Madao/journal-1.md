## 2026-08-07T05:19:54.600Z — pre-0.1.0 CLI refinements

四項 CLI 優化:config list/validate 子命令、query spinner(stderr、僅互動終端)、全子命令人類可讀錯誤改 stderr、Claude --append-system-prompt + --setting-sources project 與 Codex -c developer_instructions=(不加 Skill,實測會回歸)。真實 claude/codex 端對端驗證通過;operator guide 新增 §2.7a(SKILL.md allowed-tools 要求)與 README Development 區段。PR #5(draft)。後續:使用者實測回報 TOML 反斜線模板誤導與 query 缺 -- 的錯誤訊息問題,轉入新 task 修正。

Commits: ddc7999 78638bf 1c8c9f8 b09198d 4026ea8

## 2026-08-07T13:31:41.370Z — first-run config and query UX fixes

六項首次上手指引修正(語意零變更):TOML 反斜線錯誤附三種合法寫法、init 模板/範例改單引號路徑、query 缺 --/--wiki 附用法提示、ENTRYPOINT_UNVERIFIED 附 doctor --live 解法、help 文字清理(內部規劃參照移出 --help 並以測試 pin 住)、doctor 多 agent 嘗試附一次一組提示。本地安裝驗證通過(含 PATH 補登)。歸檔使用 --waive-gates-verdict(機器 verdict 被壞 cargo shim false-block,人工核實證據在 evidence/;runtime trace 經使用者授權輪替至 trace.jsonl.pre-0.1.0.bak)。

Commits: 93bc301 6310d21 0c1ba4f d968a89

## 2026-08-08T05:56:08.866Z — pre-0.1.0 CLI 強化：skills 目錄、markdown 終端渲染、init 防覆寫與互動設定

query 人類模式新增 termimad TTY 渲染（--plain 逃生口，pipe/--json byte-for-byte 不變）；config init 新增 TTY 自動精靈與覆寫確認（--yes/--force，非 TTY 對 agent 安全不變）；新增 skills/llm-wikis-usage 跨工具 agent 技能目錄與 README 安裝說明；兩份規格文件同步修訂。三輪 evaluate：R1 Block（AC 證據缺、G12 表格解析、trace-audit 繼承、人工 gate 待驗）→R2 Block（AC 引用黏連 token 誤判）→R3 僅剩 G4 flaky，經使用者授權 waive 歸檔。八個 TTY 情境由使用者於真實終端逐一驗證。

Commits: 7300381, ae335d9, ff04822, 62722b3

## 2026-08-10T22:43:10.893Z — provider 設定支援 Claude/Codex model 與 reasoning effort

issue #7：[providers.<agent>] 新增 optional model/effort，未設定時 argv 逐位元不變（兩個原有 exact_argv 測試 expected 向量一行未動）。Claude 映射 --model/--effort；Codex 因無獨立 effort flag 且以 --ignore-user-config 啟動，映射為 --model 與 -c model_reasoning_effort="…"，並以真的 spawn .cmd shim 的測試證明含雙引號 argv 逐字送達。只套用於 query 與 doctor --live，version/auth probe 結構上不沾。model/effort 納入 compatibility fingerprint、PROVIDER_CONTRACT_VERSION 升為 2（使用者選定：改 model/effort 需重跑 doctor --live）。規格 §6.1/§15.1、operator 指南、README、config.example.toml、init 樣板同步。gates round 1 verdict pass（30 gates 全過，含未略過的 G4 全量測試）；期間曾因主機行程列舉延遲導致兩個 deadline-race 測試失敗，以 base commit 實測比對確認為環境問題並蒸餾成 AGENTS.md 新規則。另有 Sonnet subagent 獨立驗證九項驗收條件全數 CONFIRMED、無 findings。

Commits: 38138b8 261e3ac b2dab17 27303d1

