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
| B9 | 額度／餘額查詢（訂閱配額、Coding Plan 週期、帳戶餘額） | ✅（`quota_query_all`、訂閱費、價格週期）＋ **每來源可設定的用量查詢**（樣板／自訂、卡片顯示） | `usage_query/`、§0.9.22 |
| B10 | **自訂用量查詢腳本**（其他上游自己寫腳本查） | ⚠️ 有自訂查詢，但是**宣告式抽取規格**而不是 JavaScript（引擎相依與可測試性的取捨，見 §0.9.22） | `usage_query/extract.rs` |
| B11 | 自訂價格／從 models.dev 匯入 | ✅ | `Pricing.tsx`、`catalog_fetch`、`price_extract` |

### C. 擴充面板

| # | CC Switch 的功能 | 我們的現況 | 證據 |
|---|---|---|---|
| C1 | **MCP 管理**（統一面板、範本、同步到各工具、匯入既有設定、Deep Link） | ✅ 面板／樣板／逐工具同步／匯入／整欄切換都做了（**只同步可接管的三個工具**；Deep Link 屬 P4） | `mcp/`、§0.9.23 |
| C2 | **Prompts 管理**（Markdown 編輯器、寫進 CLAUDE.md／AGENTS.md、原內容回填） | ✅ 逐工具預設集／單一啟用／**智慧回填**／首次匯入／編輯器（預覽是簡易版，未引入 Markdown 引擎） | `prompt/`、§0.9.24 |
| C3 | **Skills 管理**（搜 skills.sh／GitHub／ZIP 安裝、更新、symlink 同步） | ⚠️ GitHub 儲存庫安裝／tarball 下載／內容雜湊更新／symlink-複製同步／備份還原都做了；**不做** skills.sh 搜尋與 ZIP 安裝（見 §0.9.25） | `skills/`、§0.9.25 |
| C4 | **Session Manager**（瀏覽／搜尋各工具對話、複製續聊指令、刪除） | ✅ 兩欄式瀏覽／搜尋／依工具篩選／續聊指令複製／單筆與批次刪除；**OpenCode 只讀不刪**（別人的資料庫）；批次以外的 TOC 未做 | `sessions/`、§0.9.26 |
| C5 | Workspace／Memory 編輯（OpenClaw／Hermes） | ❌ | 無 |

### D. 系統與平台

| # | CC Switch 的功能 | 我們的現況 | 證據 |
|---|---|---|---|
| D1 | 雲端同步（WebDAV／S3） | ⚠️ **WebDAV 完成**（測試連線／上傳／下載／自動同步／遠端快照／下載前安全備份）；**S3 未做**（要自簽 SigV4 且沒有可驗證端點） | `cloudsync/`、§0.9.31 |
| D2 | **Deep Link**（`ccswitch://` 一鍵匯入來源／MCP／prompt／skill 庫） | ✅ `tokengateway://`（參數照抄）：解析／預覽／確認後才寫入／HKCU 自動註冊／已有實例時轉交；`usage*` 參數刻意忽略 | `deeplink/`、§0.9.27 |
| D3 | CLI 工具版本管理（看版本、安裝、升級、全部升級、重複安裝診斷、WSL） | ⚠️ 有版本查詢／更新（`tool_versions`、`tool_latest`、`tool_update`）；無 WSL、無一鍵全部升級 | `commands/tools*` |
| D4 | 內建小工具（跳過首次確認、隱藏 AI 署名、VS Code 擴充跟隨切換） | ⚠️ 部分（Claude 分層設定），其餘 ❌ | `providerform/ClaudeTier.tsx` |
| D5 | 主題（深／淺／系統） | ✅ System／Light／Dark；淺色是完整調色（表面色＋系統色＋光暈），跟隨系統即時切換；777 處硬編色改用 token | `index.css`、`lib/appearance.ts`、§0.9.28 |
| D6 | App 開機自啟 | ✅ HKCU 的 Run 機碼（免管理員）；狀態以登錄檔為準（不另外存一份） | `autostart.rs`、§0.9.28 |
| D7 | 自動更新 | ❌ 需要更新伺服器與簽章金鑰（Tauri updater），目前沒有發佈管道 | — |
| D8 | 原子寫入／自動備份 | ✅ | `fsutil.rs`（原子寫入）、`tools/backup.rs`（baseline 備份） |
| D9 | i18n（zh／zh-TW／en／ja） | ⚠️ 機制完成：**繁中／英文**、切換即時生效、資料庫為真相源；導覽＋**設定頁 100% 翻譯**（字典 135 個 key），其餘頁面未翻，全站覆蓋率約 15%（腳本量測） | `lib/i18n*`、§0.9.32 |
| D10 | 系統匣常駐 | ✅（基本） | `lib.rs` 托盤 |
| D11 | 全域快捷鍵／`--silent` 等啟動參數 | ❌ | 無 |
| D12 | 備份管理面板（自動備份間隔／保留數、立即備份／還原／改名／刪除） | ✅ 間隔（停用／6／12／24／48／168 小時，預設 24）與保留份數（3～50，預設 10）都照 cc-switch；立即備份／還原（還原前自動安全備份）／改名／刪除；用 SQLite 線上備份 API | `dbbackup.rs`、§0.9.30 |
| D13 | 診斷日誌開關＋等級（error／warn／info／debug／trace） | ⚠️ 有追蹤與診斷中心，但沒有可調的日誌等級 | `trace.rs`、診斷頁 |
| D14 | 關閉視窗時縮到系統匣（預設開） | ✅ 預設縮到系統匣（與 cc-switch 相同）；可切「直接結束」；攔 `CloseRequested` ＋ 托盤常駐 | `window.rs`、§0.9.29 |
| D15 | Lightweight Mode（托盤專用、視窗銷毀、需要時重建） | ✅ 托盤「輕量模式」銷毀視窗、行程與網關照常；托盤「開啟主視窗」或 Deep Link 喚醒重建 | `window.rs`、§0.9.29 |
| D16 | 各工具設定目錄可自訂（含 WSL 路徑） | ❌ 路徑寫死主目錄 | `tools/`、`prompt/sync.rs` 等 |

## 2. 分期計畫

每一期都要：三個閘門全綠、檔案 ≤400 行、真實 UI ＋ 真實 CLI 實機驗證、
`docs/TESTING.md` 補 §0.9.x／§9.x、一個 commit。

### P1 — 來源管理與切換體驗（最高價值，先把「換來源」這件事做到跟 CC Switch 一樣順）

進度（2026-10-01）：**P1 全部完成**（P1.1–P1.3 見 §0.9.20、P1.4–P1.6 見 §0.9.21）。
P1.6 的「用滑鼠點托盤圖示」這一步無法在這台機器自動化，已記錄原因與人工檢查法。

| 項目 | 內容 | 驗收 | 狀態 |
|---|---|---|---|
| P1.1 來源預設集 | 內建常見上游的 base_url＋協議，選了就填好 | 新增來源對話框可選預設，選完欄位自動填妥；有測試覆蓋 | ✅ **38 個**（非 90+，全部實測過） |
| P1.2 複製來源 | 一鍵複製（含模型與定價），名稱自動加序號 | 複製後兩筆獨立、可各自編輯 | ✅ |
| P1.3 匯入／匯出來源 | JSON 匯出（預設不含金鑰）、匯入 skip 語意 | 匯出→匯入可還原且零殘骸；有測試 | ✅ |
| P1.4 每工具故障轉移佇列 UI | 在來源頁可看到候選順序 | 畫面顯示順序＝`resolve_model_providers` 的實際結果 | ✅（以模型為中心；順序仍由來源清單決定） |
| P1.5 連線檢查 | 來源詳情面板的「檢查連線」：只測位址可達性 | 對真實上游回報可達；對壞位址回報失敗；有測試 | ✅（CC Switch 放在卡片上，我們放詳情面板） |
| P1.6 系統匣切換來源 | 托盤列出每個可接管工具的可用來源，點了就切換 | 切換後設定檔確實改變 | ⚠️ 功能完成、內容與契約有測試；**滑鼠點擊本身無法自動化驗證**（見 §0.9.21） |

### P2 — 用量與額度的最後一哩

進度（2026-10-01）：**P2.1／P2.2 完成**（見 `docs/TESTING.md` §0.9.22）。

| 項目 | 內容 | 狀態 |
|---|---|---|
| P2.1 自訂用量查詢 | 每來源可設定的查詢（樣板＋自訂規格、佔位符、逾時、自動間隔、測試） | ✅（自訂那格是宣告式規格，非 JS —— 見 §0.9.22） |
| P2.2 配額顯示對齊 | 來源卡底部顯示百分比／餘額／重置、三段式顏色、多方案展開、重新查詢 | ✅ |
| P2.3 內建樣板數量 | cc-switch 有官方訂閱／Token Plan／第三方餘額三類共十餘個；我們目前 6 個 | ⬜（要增加得先確認各家的回應形狀） |

### P3 — 擴充面板（MCP／Prompts／Skills／Sessions）

四個新子系統，各自都是獨立的大工程（MCP 面板＋同步、Markdown prompt 庫＋回填、
skills.sh／GitHub 安裝＋symlink、對話瀏覽＋續聊指令）。

進度（2026-10-01）：**P3 全部完成**（C1 MCP／C2 Prompts／C3 Skills／C4 Sessions）。

| 項目 | 內容 | 狀態 |
|---|---|---|
| P3.1 MCP 管理 | 統一面板、5 個樣板、逐工具同步（Claude／Codex／OpenCode）、匯入既有設定、整欄切換 | ✅ |
| P3.2 Prompts 管理 | 逐工具預設集、單一啟用、智慧回填、首次匯入、編輯器 | ✅ |
| P3.3 Skills 管理 | GitHub 儲存庫掃描、tarball 安裝、內容雜湊更新、symlink／複製同步、解除安裝備份與還原 | ✅（skills.sh 與 ZIP 除外，見 §0.9.25） |
| P3.4 Session Manager | 瀏覽／搜尋各工具對話、複製續聊指令、單筆與批次刪除 | ✅（OpenCode 只讀；TOC 未做） |

### P4 — 平台面

i18n（zh-TW／en）、淺色主題、App 開機自啟、自動更新、雲端同步（WebDAV／S3）、
Deep Link（`tokengateway://`）。

| 項目 | 內容 | 狀態 |
|---|---|---|
| P4.1 Deep Link | `tokengateway://v1/import?…` 一鍵匯入來源／提示詞／MCP／技能；HKCU 自動註冊；已有實例時轉交 | ✅ |
| P4.2 i18n | zh-TW／en 雙語 | ⚠️ 機制＋設定頁完成（覆蓋率 15%）§0.9.32 |
| P4.3 淺色主題 | System／Light／Dark（跟隨系統即時切換） | ✅ §0.9.28 |
| P4.4 App 開機自啟 | Windows 用 HKCU Run 機碼（免管理員） | ✅ §0.9.28 |
| P4.8 資料庫備份管理 | 排程（間隔／保留份數）、立即備份、還原（先安全備份）、改名、刪除 | ✅ §0.9.30 |
| P4.9 自動更新 | 檢查／下載新版本 | ⬜（需要更新伺服器與簽章金鑰 —— 目前沒有發佈管道） |
| P4.10 雲端同步 | WebDAV（測試連線／上傳／下載／自動同步）；S3 未做 | ⚠️ §0.9.31 |
| P4.9 自動更新 | 檢查更新（manifest 網址可設定）＋下載連結 | ⚠️ 檢查完成、自動安裝不做 §0.9.33 |
| P4.11 收尾 | i18n（導覽＋設定頁 100%，其餘 15%）、日誌等級、自訂工具目錄 | ⚠️ §0.9.32；日誌等級與自訂目錄未做 |

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

