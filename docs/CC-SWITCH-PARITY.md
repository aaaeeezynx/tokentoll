# 與 CC Switch 的功能對齊計畫（Parity Plan）

> 使用者裁示（2026-09-30）：「Claude Code 那個沒關係，只要工具功能/體驗完全對齊
> CC Switch 即可。」這份文件是**先盤點、再實作**的那個盤點。
>
> 參考對象：[farion1231/cc-switch](https://github.com/farion1231/cc-switch)
> README（v3.20.4）與[使用者手冊目錄](https://github.com/farion1231/cc-switch/blob/main/docs/user-manual/en/README.md)。
> 本 App 的判定一律以**程式碼為證**（指令清單、元件清單、`Cargo.toml` 外掛清單），
> 不用猜的。

## 0. 兩邊的定位差異（先講清楚，才不會亂抄）

| | CC Switch | 本 App（Token Gateway） |
|---|---|---|
| 主要身分 | 多工具的**設定切換器**（順便附帶本機路由與用量） | **本機網關 ＋ 用量帳務**（順便接管工具設定） |
| 受管工具 | **10 個**：Claude Code、Claude Desktop、Codex、Gemini CLI、Grok Build、OpenCode、OpenClaw、Hermes、Pi、MiniMax Code | **7 個**（用量統計）：Claude Code、Codex、OpenCode、Hermes Agent、DeepSeek Harness、Cursor、Antigravity；其中 **3 個可接管路由**（Claude Code／Codex／OpenCode） |
| 路由埠 | 15721 | 15722 |
| 強項 | 90+ 來源預設集、MCP／Skills／Prompts／Sessions 面板、雲端同步、Deep Link、i18n | 金鑰與配額、8 個用量鏡頭、價格計算機、診斷中心（追蹤／剝離記憶／協議記憶／來源健康）、DSH 歷史回填 |

**這個差距是雙向的**：下面第 3 節列出「我們有而 CC Switch 沒有」的東西，
對齊不代表要把它砍掉。

## 1. 對齊矩陣

狀態：✅ 已有｜⚠️ 部分｜❌ 缺

### A. 來源管理與切換

| # | CC Switch 的功能 | 我們的現況 | 證據 |
|---|---|---|---|
| A1 | 來源 CRUD（新增／編輯／刪除／啟用停用） | ✅ | `providers.rs`、`components/providers/*` |
| A2 | **90+ 來源預設集**（選預設＋填 Key 就能用） | ⚠️ **38 個**（全部實測過上游位址；不含模型清單，一律現場抓） | `presets.rs`、§0.9.20 |
| A3 | 一鍵切換（每個工具各自的來源清單） | ✅ | `SwitchDialog.tsx`、`switch_apply`／`switch_off` |
| A4 | **系統匣快速切換** | ❌ | 系統匣只有「顯示主視窗／退出」（`lib.rs`） |
| A5 | 拖曳排序 | ✅ | `provider_reorder`（含排列合法性校驗） |
| A6 | **複製來源** | ✅ | `providers_io.rs::duplicate_provider`、§0.9.20 |
| A7 | **匯入／匯出來源** | ✅ | `providers_io.rs`（預設不含金鑰、匯入是 skip 語意、後端寫檔）、§0.9.20 |
| A8 | 內建「官方登入」來源，可切回官方 | ✅ | `to_native`（原生來源化）＋官方憑證提示 |
| A9 | **通用來源**（一份設定同步到多個工具） | ❌ | 來源是綁 `app_type` 的 |
| A10 | **專案（Projects）**：把整組設定存成專案一鍵切換 | ❌ | 無 |
| A11 | 編輯時顯示「切換後設定檔的長相」 | ✅ | `switch_plan` 預覽（`SwitchPlan`） |
| A12 | OAuth 多帳號中心（Copilot／ChatGPT／xAI） | ❌ | 無（CC Switch 自己也標 Beta 並提醒可能違反上游條款） |
| A13 | Claude Desktop 第三方來源／模型對映 | ❌ | 我們不管理 Claude Desktop |

### B. 本機路由與高可用

| # | CC Switch 的功能 | 我們的現況 | 證據 |
|---|---|---|---|
| B1 | 路由總開關 | ✅ | 網關啟停（`proxy_start`／`proxy_stop`） |
| B2 | 每工具路由開關 | ✅ | 接管／還原（`switch_apply`／`switch_off`） |
| B3 | API 格式轉換（Anthropic／Chat／Responses／Gemini） | ⚠️ 三種可互相翻譯；**Gemini 只做原生直通**（沒有 Gemini↔其他 的翻譯器） | `proxy/matrix.rs`（`E_GEMINI_IN_ONLY`／`E_GEMINI_OUT_ONLY`） |
| B4 | **每工具的故障轉移佇列（可排序、可看）** | ⚠️ 有自動換手＋候選佇列，但順序是**全域**來源順序，沒有每工具佇列 UI | `proxy/failover.rs`、`resolve_model_providers` |
| B5 | 斷路器 ＋ 來源健康監控 | ✅ | `proxy/health.rs`、診斷頁「來源健康狀態」 |
| B6 | Rectifier（修上游吃不下的請求：thinking signature、圖片不支援回退） | ⚠️ 有「上游拒收欄位就剝離並記住」的那一半；**沒有** thinking signature／圖片回退 | `provider_stripped_fields`、`proxy/retry.rs` |
| B7 | **連線檢查**（來源卡上按一下測位址可達性） | ❌ | 只有 `codex_doctor`（診斷 Codex 設定，不是來源） |
| B8 | 用量統計（含「沒開路由也能從本機 session log 匯入」） | ✅ 我們更強（8 個鏡頭、CSV 匯出、歷史回填） | `usage/*`、`history/*` |
| B9 | 額度／餘額查詢（訂閱配額、Coding Plan 週期、帳戶餘額） | ✅（`quota_query_all`、訂閱費、價格週期） | `usage/quota*`、`pp_*` |
| B10 | **自訂用量查詢腳本**（其他上游自己寫腳本查） | ❌ | 無 |
| B11 | 自訂價格／從 models.dev 匯入 | ✅ | `Pricing.tsx`、`catalog_fetch`、`price_extract` |

### C. 擴充面板

| # | CC Switch 的功能 | 我們的現況 | 證據 |
|---|---|---|---|
| C1 | **MCP 管理**（統一面板、範本、同步到各工具、匯入既有設定、Deep Link） | ❌ | 全庫只在「保留未知段」測試裡出現 `[mcp_servers]` |
| C2 | **Prompts 管理**（Markdown 編輯器、寫進 CLAUDE.md／AGENTS.md、原內容回填） | ❌ | 無 |
| C3 | **Skills 管理**（搜 skills.sh／GitHub／ZIP 安裝、更新、symlink 同步） | ❌ | 無 |
| C4 | **Session Manager**（瀏覽／搜尋各工具對話、複製續聊指令、刪除） | ❌ | 我們的 `history` 是**用量**回填，不是對話瀏覽 |
| C5 | Workspace／Memory 編輯（OpenClaw／Hermes） | ❌ | 無 |

### D. 系統與平台

| # | CC Switch 的功能 | 我們的現況 | 證據 |
|---|---|---|---|
| D1 | 雲端同步（WebDAV／S3） | ❌ | 無 |
| D2 | **Deep Link**（`ccswitch://` 一鍵匯入來源／MCP／prompt／skill 庫） | ❌ | 無 |
| D3 | CLI 工具版本管理（看版本、安裝、升級、全部升級、重複安裝診斷、WSL） | ⚠️ 有版本查詢／更新（`tool_versions`、`tool_latest`、`tool_update`）；無 WSL、無一鍵全部升級 | `commands/tools*` |
| D4 | 內建小工具（跳過首次確認、隱藏 AI 署名、VS Code 擴充跟隨切換） | ⚠️ 部分（Claude 分層設定），其餘 ❌ | `providerform/ClaudeTier.tsx` |
| D5 | 主題（深／淺／系統） | ⚠️ 只有強調色（多組 accent），**固定深色** | `lib/theme.ts`、`Settings.tsx` |
| D6 | App 開機自啟 | ❌ 只有**網關**自啟（`auto_start_proxy`） | `Settings.tsx` |
| D7 | 自動更新 | ❌ | `Cargo.toml` 只有 `tauri-plugin-opener` |
| D8 | 原子寫入／自動備份 | ✅ | `fsutil.rs`（原子寫入）、`tools/backup.rs`（baseline 備份） |
| D9 | i18n（zh／zh-TW／en／ja） | ❌ 只有 zh-TW | 無任何 i18n 檔 |
| D10 | 系統匣常駐 | ✅（基本） | `lib.rs` 托盤 |
| D11 | 全域快捷鍵／`--silent` 等啟動參數 | ❌ | 無 |

## 2. 分期計畫

每一期都要：三個閘門全綠、檔案 ≤400 行、真實 UI ＋ 真實 CLI 實機驗證、
`docs/TESTING.md` 補 §0.9.x／§9.x、一個 commit。

### P1 — 來源管理與切換體驗（最高價值，先把「換來源」這件事做到跟 CC Switch 一樣順）

進度（2026-10-01）：**P1.1／P1.2／P1.3 完成**（見 `docs/TESTING.md` §0.9.20），
P1.4／P1.5／P1.6 待做。

| 項目 | 內容 | 驗收 | 狀態 |
|---|---|---|---|
| P1.1 來源預設集 | 內建常見上游的 base_url＋協議，選了就填好 | 新增來源對話框可選預設，選完欄位自動填妥；有測試覆蓋 | ✅ **38 個**（非 90+，全部實測過） |
| P1.2 複製來源 | 一鍵複製（含模型與定價），名稱自動加序號 | 複製後兩筆獨立、可各自編輯 | ✅ |
| P1.3 匯入／匯出來源 | JSON 匯出（預設不含金鑰）、匯入 skip 語意 | 匯出→匯入可還原且零殘骸；有測試 | ✅ |
| P1.4 每工具故障轉移佇列 UI | 在來源頁可看到「這個工具目前的候選順序」，並可調整（至少顯示） | 畫面顯示順序＝`resolve_model_providers` 的實際結果 | ⬜ |
| P1.5 連線檢查 | 來源卡上「檢查連線」：測位址可達性（**不送真實模型請求**，與 CC Switch 語意一致） | 對真實上游回報可達；對壞位址回報失敗；有測試 | ⬜ |
| P1.6 系統匣切換來源 | 托盤選單列出每個工具目前的來源，可直接切換 | 托盤切換後設定檔確實改變 | ⬜ |

### P2 — 用量與額度的最後一哩

| 項目 | 內容 |
|---|---|
| P2.1 自訂用量查詢腳本 | 讓使用者為上游寫「查餘額／配額」的腳本（CC Switch 的 "custom usage script"） |
| P2.2 配額顯示對齊 | 來源卡直接顯示配額／餘額狀態（我們已有資料，缺「在來源卡上顯示」） |

### P3 — 擴充面板（MCP／Prompts／Skills／Sessions）

四個新子系統，各自都是獨立的大工程（MCP 面板＋同步、Markdown prompt 庫＋回填、
skills.sh／GitHub 安裝＋symlink、對話瀏覽＋續聊指令）。**這一期最花時間**，
但它是 CC Switch 使用者最有感的部分。

### P4 — 平台面

i18n（zh-TW／en）、淺色主題、App 開機自啟、自動更新、雲端同步（WebDAV／S3）、
Deep Link（`tokengateway://`）。

## 3. 我們有而 CC Switch 沒有的（對齊時不得退化）

- **金鑰管理**：配額、模型白名單、應用白名單、輪替、只顯示前後綴。
- **用量 8 個鏡頭** ＋ CSV 匯出 ＋ 月度／時段分析。
- **價格計算機**（多情境試算、從上游抓價）。
- **診斷中心**：`proxy_trace` 追蹤、上游拒收欄位記憶、協議記憶、來源健康（斷路器）。
- **DSH（DeepSeek Harness）歷史回填**：把本機 session 的 token 用量補進帳。
- **用量頁「本機工具」篩選保留全部選項**（cursor／antigravity／hermes 等）—— 硬性要求。

## 4. 明確不做（附理由）

| 項目 | 理由 |
|---|---|
| Anthropic→Responses 翻譯器 | 使用者 2026-09-30 明示「Claude Code 那個沒關係」 |
| Claude Desktop／Gemini CLI／Grok Build／OpenClaw／Pi／MiniMax Code 的受管支援 | 我們不是設定切換器；先要求「現有 7 個工具的體驗對齊」，新增受管工具另案（使用者 2026-09-30 裁示：先不動） |

## 5. 使用者審查結論（2026-09-30）

| 問題 | 裁示 |
|---|---|
| 要跑到哪一期 | **P1 → P2 → P3 → P4 全部做**，每期完成後回報 |
| 受管工具清單要不要也對齊（10 個工具） | **先不動**：維持 7 個工具做用量統計、3 個（Claude Code／Codex／OpenCode）可接管 |

因此第 4 節「明確不做」維持不變，新增受管工具一事另案處理。

