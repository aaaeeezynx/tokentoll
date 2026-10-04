# 測試指南：Phase 1 ～ Phase 5

> 這份文件是給「實際動手測」用的。每一項都有**確切的指令**與**明確的通過
> 標準**，讓你可以自己判斷有沒有過，而不是只能憑感覺。
>
> 對應的技術細節在 [`REFACTORING-PLAN.md`](./REFACTORING-PLAN.md)。
>
> **只想看「我自己要動手做什麼」→ 直接看 [`MANUAL-TESTS.md`](./MANUAL-TESTS.md)。**
> 那份是精簡版（7 個測試，含唯一還沒跑過的測試 C）；本檔是完整紀錄。
>
> **最新狀態（2026-09-28 23:40）**：後端 **228 passed / 0 failed / 8 ignored**、
> clippy 0 警告、`tsc --noEmit` exit 0、205 檔全部 ≤ 400 行。這一輪做的是
> **「關閉網關要回到該工具的原生來源」**（§0.9，Codex 已實機驗證；OpenCode 於
> §0.9.8 實機驗證並順手修掉一個長期的摘要瑕疵）。手動複驗步驟見
> [`MANUAL-TESTS.md`](./MANUAL-TESTS.md) 第 9 節。

---

## 0. 這次要測的七件事

| # | 測什麼 | 為什麼重要 |
|---|---|---|
| **A** | 診斷中心能不能用 | Phase 1 做的可觀測設施**從未真正跑起來過**（見下方說明） |
| **B** | 網關拒絕會不會留痕 | Phase 1.5 修的**主要缺口**。修正前，「連線錯誤 400」是唯一不會出現在診斷頁的那一類 |
| **C** | 原本的 400 / Codex 會話症狀有沒有改善 | 你的原始問題 |
| **D** | Codex 舊會話的 provider 別名（B3，已修） | 修正前讀取失敗會**靜默變成「沒有別名」**，看起來一切正常 |
| **E** | `wire_api` 是否跟著上游能力走（B4，**已修但修正本身有缺陷，2026-09-28 再修**） | 最初「不論上游是誰都寫死 `responses`」；改成「宣告優先」後又**反過來**把指向網關的段寫成 `chat` —— 而 Codex 已不接受 `chat`，導致 **「Unable to log in」**。見 §0.6 |
| **F** | DSH 用量為何不出現在 Token 趨勢 | **你回報的 bug**。已查明：**標籤是 bug，趨勢圖不是** —— DSH 目前設定直連 `opencode.ai`，根本沒經過網關 |
| **G** | 本機工具標籤應顯示「DeepSeek Harness」 | **你回報的 bug**（已修）。前端寫成「DeepSeek」，與後端不一致 |

> **為什麼 A 是「從未跑起來過」**：目前安裝在
> `%LOCALAPPDATA%\token-gateway\token-gateway.exe` 的是 **2026-09-24 22:38**
> 的建置。實測該檔二進位中**不含 `proxy_trace`** 字串 —— 也就是說它是 Phase 1
> **之前**的版本，診斷中心那整塊程式碼根本不在裡面。你的資料庫也還是 **v7**，
> 同樣證明 Phase 1 的程式沒被執行過。這次的建置是**第一個含 Phase 1～5 的版本**。

> **本輪（Phase 3～5）的改動屬於「結構重組」**：拆檔、補測試、錯誤型別。
> 這些**不該改變任何可見行為**。驗證方式是 bundle 大小幾乎不變
> （前端 542.40 → 542.38 kB）、測試全綠、以及拆 `keys.rs` 錯誤型別時
> 逐字比對過所有中文訊息（15 → 15，零改字）。**若你看到非預期的行為變化，
> 那是我拆壞了，請回報。**

> **目前狀態（2026-09-28 22:30 更新）**：測試 **A 已通過**（2026-09-27 22:07
> 你提供的截圖，已與資料庫逐項核對）。**B、D 已由我代跑並全部通過**；
> **E 在代跑時抓到一個真實故障**（接管會寫出 Codex 已不接受的
> `wire_api = "chat"`，導致 Codex 報「Unable to log in」），**已修正**，
> 並在安裝後的正式版本上用**真實 UI 點擊**驗證過「App 會把壞掉的 `chat`
> 修回 `responses`」—— 詳見 §0.6、§0.7.5。
> **C 仍然只能你自己在真實使用情境下測**（見 §6 與
> [`MANUAL-TESTS.md`](./MANUAL-TESTS.md)），**這是唯一還沒跑過的測試**。
> 後端測試 **226 passed / 0 failed / 8 ignored**、clippy **0 警告**。
> 最新的實機驗證與建置見 §0.5、§0.6、§0.7、§0.9 與 §9.9。
>
> <small>（以下為歷史紀錄，數字是當時的基線，已被上面的現況取代）</small>

> **2026-09-28 更新**：用量頁第 8 個鏡頭「工具」已完成，程式已重新建置、
> **安裝到 `%LOCALAPPDATA%\token-gateway`（2026-09-28 02:43）並實機驗證**。
> 後端測試 **219 passed / 0 failed / 9 ignored**。詳細證據見 §0.1 與 §9.1。

> **2026-09-28 03:28 更新（第一階段：工具區精簡 D-1／A／E）**：三個方向的程式
> 改動已完成並重新建置、安裝、實機驗證。後端測試 **223 passed / 0 failed /
> 9 ignored**。詳細證據見 §0.2 與 §9.3。

> **2026-09-28 04:2x 更新（第一階段：F 移除 hermes 支援）**：hermes 支援已整塊
> 移除（−783 行），並先把 hermes 自己的設定檔殘留外科清理乾淨。後端測試
> **216 passed / 0 failed / 8 ignored**（少掉的 7 條正是 hermes 專屬）。
> 工具的用量歸屬標籤**完整保留**。詳細證據見 §0.3、§9.4 與
> `SIMPLIFICATION-PLAN.md` §10.9。

---

## 0.1 2026-09-28：用量「工具」視角（已實機驗證）

**做了什麼**：在用量頁新增獨立鏡頭「工具」（`概覽 | 工具 | 模型 | 月 | 日 | 小時 | 統計 | 渠道`），
把原本只能「一次看一個工具」的分工具數字，變成**同一張表並排比較**：
請求數／成功率／輸入／輸出／快取讀／快取建／總計 Tokens／**快取命中率**／費用 $，
外加每個工具一張摘要卡（Token 用量／請求數／快取命中率），點任一列可下鑽成該工具的篩選。
後端新增 `AppStat` + `by_app()` + `usage_by_app` 命令，並抽出全檔唯一的
`hit_rate()`（`cache_read / (in_tok + cache_read)`，分母 0 回 0.0）。

**程式**：commit `d417b02`（9 檔、+708/−26）。

**驗證方式與結果**（本節的數字都是**實跑**得到的，不是推論）：

| 驗證項 | 方法 | 結果 |
|---|---|---|
| 產物比原始碼新 | 時間戳比對（最新原始碼 `usage.rs` 01:15，產物 02:43） | ✅ |
| 二進位含新前端與新命令 | 在 exe 內找到 `index-BtK0gyS1.js`、`index-DnRx7SDk.css`、`usage_by_app` | ✅ |
| 安裝檔真的換新 | 安裝前 `%LOCALAPPDATA%` 版只有舊資源 `index-hbcVOtaz.js`；安裝後兩者皆為新 | ✅ |
| 「工具」鏡頭存在並渲染 | 視窗截圖 → Windows OCR 讀出「全部工具合計」「分工具比較」與九個表頭 | ✅ |
| 表格數字正確 | 見下方逐列比對 | ✅ |
| 點列下鑽 | 點 Codex 列 → 篩選變成 `codex`、摘要卡變成 3,512 筆／1,222,485,861 tokens／47.6% | ✅ |
| 篩選可還原 | 重新選「全部本機工具」→ 合計回到 4,145 筆／1,625,556,632 tokens | ✅ |

**逐列比對（範圍＝最近 30 天）**：下表每一格都是**畫面上實際顯示的值**，
我再用 SQLite 直接對 `request_logs` 重算一次核對，兩者一致。

| 工具 | 請求數 | 成功率 | 輸入 | 輸出 | 快取讀 | 快取建 | 總計 Tokens | 快取命中率 | 費用 $ |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Codex | 3,512 | 88.0% | 639,933,256 | 1,918,771 | 580,633,834 | 0 | 1,222,485,861 | 47.6% | 23.6257 |
| OpenCode | 47 | 93.6% | 15,982,636 | 1,420,647 | 379,150,532 | 0 | 396,553,815 | 96.0% | 0.0012 |
| Claude Code | 187 | 81.8% | 3,323,706 | 158,009 | 2,951,552 | 0 | 6,433,267 | 47.0% | 0.0000 |
| unknown | 357 | 25.5% | 30,473 | 2,857 | 768 | 0 | 34,098 | 2.5% | 0.0004 |
| DeepSeek Harness | 41 | 17.1% | 24,859 | 100 | 0 | 0 | 24,959 | 0.0% | 0.0012 |
| Hermes Agent | 1 | 100.0% | 12,337 | 7 | 12,288 | 0 | 24,632 | 49.9% | 0.0000 |
| **合計**（畫面卡片） | **4,145** | — | — | — | — | — | **1,625,556,632** | **59.4%** | **23.6285** |

> 「合計」那張卡只顯示四個數字：請求數、Token 用量、快取命中率、費用 $。
> 費用 23.6285 也與 `SUM(cost_usd)` = 23.628523072 相符。
> （成功率／輸入／輸出／快取讀的合計值沒有顯示在畫面上，我沒有列進去。）
> 重算腳本：`.workbuddy/tmp/verify_ui_by_app.py`。

> **驗證方法的誠實說明**：我用的模型**不能讀圖**，所以畫面內容是靠
> 「視窗截圖 → Windows 內建 OCR」讀出來的。OCR 對小字數字偶爾會錯
> （例如把 `100.0%` 讀成 `18.0`、把 `2.5%` 讀成 `25%`），因此凡是 OCR 讀得可疑的格子，
> 我改用**墨水寬度量測**（量該格亮像素的實際寬度）判斷字元數：
> 例如 Hermes 成功率那格寬 36px、其餘各列同欄只有 30–31px —— 多一個字元寬，
> 才是 `100.0%`（6 字元）而不是 `18.0%`（5 字元）。這是判定「顯示值」與
> 「我算出的值」一致的最後一道依據。
>
> 也就是說：**表上沒有任何一格是「推論」來的**；OCR 讀不清的格子，我用寬度量測
> 定案。若你實機看到某一格與上表不符，那是我的驗證方法仍有漏洞，請直接告訴我。

---

## 0.2 2026-09-28：第一階段工具區精簡（D-1／A／E，已實機驗證）

**做了什麼**（每個方向一個 commit，細節見 `SIMPLIFICATION-PLAN.md` §10.4–10.8）：

| commit | 方向 | 一句話 |
|---|---|---|
| `3908fcd` | D-1 | 刪掉走不到的「真·直連上游」分支（`via_gateway`），只剩「經網關」一條路 |
| `71d51a6` | A | 移除 cursor／antigravity 的偵測與接管 arm，工具清單 7 → 5 |
| `38c5bcd` | E | 輪換備份 `BACKUP_KEEP` 10 → 1，並新增「輪換前先保住接管前設定」的安全網 |

**驗證方式與結果**（下表都是**實跑**，不是推論）：

| 驗證項 | 方法 | 結果 |
|---|---|---|
| 產物比原始碼新 | 時間戳（最新原始碼 03:11:50、exe 03:28:11） | ✅ |
| 前端真的換新 | exe 內資源鍵 `index-BAemutFc.js`（前版 `index-BtK0gyS1.js`）、`index-DnRx7SDk.css` 未變 | ✅ |
| 安裝檔真的換新 | NSIS `/S` exit 0；安裝後 exe 03:27:52、同一組資源 | ✅ |
| D-1 真的進了產物 | 在 exe 內搜 `via_gateway` → **找不到** | ✅ |
| A 的工具清單 | 上游來源頁的頁籤讀到「本機工具（**2/5** 接管中）」，五張卡是 Claude Code／Codex／OpenCode／Hermes Agent／DeepSeek Harness | ✅ |
| A 的文案 | 工具區底部讀到「Cursor／Antigravity 不提供接管…」 | ✅ |
| D-1 的方案路徑 | 點 Claude Code 卡片開對話框 → 讀到 `…\.claude\settings.json · 自動備份`、`8 項寫入`、「⚠ ANTHROPIC_AUTH_TOKEN 將明文寫入 settings.json…網關模式下請使用可吊銷的本地 sk Key」（證明新產物的 `switch_plan` 仍能生成方案；D-1 本身由上面「`via_gateway` 消失」證明） | ✅ |
| 沒有誤寫任何設定 | 全程未按「套用／接管」；驗證前後備份目錄完全相同（claude 10／codex 11＋baseline 1／hermes 6／opencode 10），`app.db` mtime 未變 | ✅ |
| **沒能 GUI 驗證** | 用量頁那個自製「本機工具」下拉，滑鼠事件打不開它的彈出層 | ⚠️ 改用程式碼與 diff 證明：三次 commit 都沒動 `APP_META`／`APP_ORDER`／用量頁任何一行，選項來源就是 `APP_META`（含 cursor／antigravity），所以「選項一個不少」成立 |

**自動化測試**：`cargo test --offline` → **223 passed / 0 failed / 9 ignored**（exit 0）；
`cargo clippy --offline --all-targets` → 5 個警告，與基線相同；
`npx tsc --noEmit` exit 0；`scripts/check_app_labels.py` 前後端顯示名一致。

**E 的 4 條新測試**（`src-tauri/src/tools/tests/restore.rs`）：

| 測試 | 釘住什麼 |
|---|---|
| `prune_keep_one_keeps_the_clean_backup` | `keep=1` 時淘汰的是污染備份，留下來的仍是乾淨那份 |
| `keep_one_still_restores_original_after_repeated_takeover` | 「接管 → 再接管 → 還原」仍回到最初內容（走 baseline） |
| `keep_one_salvages_baseline_from_history_before_prune` | 沒有 baseline、只有一串歷史備份時，輪換前先把最乾淨的存成 baseline |
| `backup_keep_is_one_by_design` | 把「1」釘成刻意決定，避免被隨手改掉 |

> **對你備份的實際影響**：下次對某工具按「接管」時，該工具的 `bak-` 會被輪換到
> 1 份（乾淨那份或最新那份），`baseline` 不受影響。不想被輪換就不要按接管。

---

## 0.3 2026-09-28：第一階段 F —— 移除 hermes 支援（已實機驗證）

**你的決定**：先把 hermes 從網關殘留中解套，再移除整塊支援。

**動手前的查核（推翻了先前的假設，誠實記錄）**：

| 查核項 | 結果 |
|---|---|
| hermes 走哪條路 | `model.provider = opencode-go`、`https://opencode.ai/zen/go/v1` → **沒走網關** |
| App 的判定 | 工具頁只有 Claude Code／Codex 標「網關接管中」 |
| hermes 用量 | **1 筆 / 24,632 tokens**，時間 09-23（歷史紀錄） |
| 真正殘留 | `config.yaml` 一段未被引用的 `providers.tokengateway`＋`.env` 一行 Key |

⇒ 不能「還原到 09-23 備份」（那會把目前可用的 opencode-go／gpt-6-luna 換成舊的
nvidia／flatkey）。改採**外科清理**（你選的 A）。

**外科清理（可完全回復）**：

| 步驟 | 結果 |
|---|---|
| 先備份 | `backups/hermes/config.yaml.bak-20260928-035729`、`.env.bak-20260928-035729`（不經輪換，不會被 `BACKUP_KEEP=1` 砍） |
| `config.yaml` | 6 行 provider 區塊 → `providers: {}`（349 → 343 行） |
| `.env` | 刪掉那 1 行（550 → 549 行） |
| 驗證 | YAML 可解析、`model` 段未動、全檔無 `tokengateway`／`15722` |
| `hermes doctor` | 配置區全綠（API key ✓／No deprecated keys ✓）、42 項連線檢查中 **OpenCode Go ✓ (key configured)**、`hermes --version` 正常 |

> 想回頭：把那兩份 `*.bak-20260928-035729` 覆蓋回 `%LOCALAPPDATA%\hermes\` 即可。

**程式改動與驗證**：

| 驗證項 | 方法 | 結果 |
|---|---|---|
| 測試 | `cargo test --offline` | **216 passed / 0 failed / 8 ignored**（exit 0） |
| lint | `cargo clippy --offline --all-targets` | **4 個警告**（原為 5，少的那個在被刪的測試裡） |
| 型別 | `npx tsc --noEmit` | exit 0 |
| 顯示名一致 | `scripts/check_app_labels.py` | 前後端 7 個顯示名一致（含 hermes） |
| 產物不含 hermes 程式 | exe 內搜 `hermes_home`／`parse_hermes_model`／`HERMES_CUSTOM_TOKENGATEWAY_API_KEY` | 全部 **False** |
| 產物仍含用量設施 | exe 內搜 `usage_by_app` | **True** |
| 工具清單縮減 | 工具頁頁籤讀到「本機工具（**2/4** 接管中）」，四張卡：Claude Code／Codex／OpenCode／DeepSeek Harness | ✅ |
| 用量歸屬未縮減 | `APP_META`／`APP_ORDER`／`infer_app` 的 hermes 全部保留 | ✅（你的 1 筆 hermes 請求仍歸屬 hermes） |
| 版本面板 | 工具頁新增「本機工具版本」面板，列出 4 個可接管工具 | ✅ |

---

## 0.4 2026-09-28：§8 誠實清單一次清空（結構重構輪）

**你的指示**：把 `TESTING.md` §8「已知未修／已做但未達標」清單裡的項目**一併修完**
（不是只回報）。

**這一輪全部是「結構重構」，不該改變任何可見行為**：拆檔、補測試、統一門檻、
抽共用結構。驗收標準就是「測試數不減、lint 更乾淨、畫面一模一樣」。

### 0.4.1 修了什麼

| 原 §8 項目 | 結果 | 關鍵事實 |
|---|---|---|
| `resolve_filter` 的 365/366 口徑不一 | ✅ 統一為 **365** | 前端 `RangePicker.tsx` 本來就是 `CUSTOM_MAX_DAYS = 365`，三處只有後端寫錯 → **沒有產品決策空間** |
| `db.rs` migration 無自動化測試 | ✅ 新增逐版本升級測試 | `every_legacy_version_upgrades_preserving_data` 對 **v1…v7 每一個**歷史版本各造一個庫，驗證版本、providers、request_logs、settings、12 張核心表 |
| `proxy_handler` 86 行（目標 <80） | ✅ **78 行** | 根因與原判斷不同：不是「重複 5 個欄位」，而是三個階段 struct **各自重複宣告** `ctx`/`started`/`app`/`model_raw` → 抽出 `ReqCtx` |
| §5.2 body 解析 400「未定案」 | ✅ **定案：不是 bug** | 見 `docs/evidence/2026-09-28-body-parse-CONCLUSION.md` |
| 前端三個肥檔 | ✅ 全拆 | `Keys.tsx` 884→**254**、`Calc.tsx` 818→**366**、`lib/api.ts` 808→**318** |
| 「拆檔後仍 >400 行」5 檔 | ✅ 全拆 | `Providers.tsx` 778→**271**、`ModelCatalog.tsx` 608→**5**（barrel）、`ProviderForm.tsx` 590→**18**（barrel）、`tools/tests/codex.rs` 566→**310**、`usageCharts.tsx` 546→**291** |
| clippy 4 個既有警告 | ✅ **0 個** | 3 個直接修；`codex_apply` 的 8 參數加 `#[allow]` **並註明理由** |
| Phase 5 錯誤型別只轉 6/108 | ⛔ **刻意不動** | 每次轉換都該有人能實測；屬獨立的漸進工作，強行一次改完風險大於收益 |

### 0.4.2 稽核另外掃出的 15 個 >400 行檔案（也一併拆完）

原本的清單只列了 5 個，實際掃全樹發現**還有 15 個**。既然要拆就一次拆完：

`translate.rs` 2345、`usage.rs` 2190、`commands.rs` 1233、`db.rs` 1132、
`history.rs` 1038、`models.rs` 990、`proxy/tests/e2e_basic.rs` 705、
`tools/codex.rs` 602、`quota.rs` 595、`trace.rs` 576、`SwitchDialog.tsx` 574、
`Diagnostics.tsx` 563、`usageLenses.tsx` 453、`tools/versions.rs` 450、
`proxy/tests/e2e_translate.rs` 445、`proxy/tests/e2e_strict.rs` 430、
`keys.rs` 419、`price_extract.rs` 410。

**現在全樹 202 個 `.ts`／`.tsx`／`.rs` 檔案，沒有任何一個超過 400 行**
（最大者 399 行）。

**統一作法（三條不變式）**：
1. **純搬移** —— 零行為、零文案、零邏輯、零 JSX／className 變更。
2. **對外路徑不變** —— 前端用再匯出 barrel，Rust 用 `pub(crate) use 子模組::*`，
   所以**所有既有 import 端一行都不用改**（`App.tsx`、`lib.rs` 的
   `generate_handler!`、各 `crate::X::Y` 呼叫點全部照舊）。
3. **可見性只放寬、不收緊** —— 跨子模組呼叫所需的 `pub(crate)`／`pub(super)`，
   共十餘處，皆非邏輯變更。

### 0.4.3 驗證

| 驗證項 | 指令 | 結果 |
|---|---|---|
| 後端測試 | `cargo test --offline` | **218 passed / 0 failed / 8 ignored**（exit 0） |
| 後端 lint | `cargo clippy --offline --all-targets` | **exit 0、零警告** |
| 後端編譯 | `cargo check --offline --all-targets` | exit 0 |
| 前端型別 | `npx tsc --noEmit` | exit 0 |
| 前端建置 | `pnpm build` | exit 0 |
| 測試數不減 | 218 = 重構前基線 | ✅（226 條被執行，含 8 ignored） |
| 無檔案超標 | 全樹掃描 | ✅ 最大 399 行 |

> **測試名稱有一處變動**：`proxy/tests/e2e_translate.rs`（及 `e2e_strict`）改成
> 目錄後，測試路徑多一層模組名，例如
> `proxy::tests::e2e_translate::e2e_responses_to_chat`
> → `proxy::tests::e2e_translate::responses::e2e_responses_to_chat`。
> **測試函式本體與斷言逐位元組未變、總數未變**，只是名字變長。
> 這是為了不動 `proxy/tests.rs` 的 `#[path]` 宣告。

### 0.4.4 一個值得記下的量測陷阱

**PowerShell 5.1 的 `Get-Content` 會少算含中文檔案的行數**（codepage 936 下
DBCS 前導位元組會吃掉後面的 `0x0A`，另有解碼成 U+0085 而合併行的情形）。
先前 §8 記的 876／809／803 其實是 **884／818／808**；`usage.rs` 記 2109 實為
**2190**、`translate.rs` 記 2267 實為 **2345**。

**之後量行數請用** `[IO.File]::ReadAllLines($path).Count`（或 `-Encoding UTF8`）。
本次所有「< 400 行」的結論都是用這個方法量的。

---

## 0.5 2026-09-28：測試 B／D／E 代跑，以及一次我自己造成的意外

### 0.5.1 結果總表

| 測試 | 結果 | 證據 |
|---|---|---|
| **B** 網關拒絕要留痕 | ✅ **5/5 通過** | §5.3：Δ+3 筆、401／401／400、warn 層級、總數 21→24、「轉換」欄＝網關拒絕（未觸及上游） |
| **D** Codex 舊會話別名體檢 | ✅ **3/3 通過** | §6.5：ℹ️ 明示讀自 `state_5.sqlite`、6 個別名、並附 ❌ 實例；「讀不到」分支有單元測試釘住 |
| **E** `wire_api` 跟著上游 | ❌→✅ **發現真實故障並修正** | §6.6：E 的「宣告優先」被誤用到指向網關的情況，寫出 Codex **已不再接受**的 `wire_api = "chat"` → Codex 整份 config 判為非法 → **「Unable to log in」**。已修（`codex_wire_api_for`）＋新增回歸測試；你檔案上 8 段現為 `responses` |
| **C** 你原本的症狀 | ⏳ 仍待你實測 | 需要真實 CLI 使用情境，我無法代跑 |
| GUI 覆蓋補齊 | ✅ | 診斷頁（含展開明細）、上游來源頁、本機工具分頁、切換對話框全部重新渲染確認過 |
| 標籤一致性 | ✅ | `scripts/check_app_labels.py` exit 0 |

### 0.5.2 ⚠️ 我造成的意外：Codex 設定被改了兩次（已還原）

**過程**：我用滑鼠座標導覽時，有兩次點擊落到了「上游來源」頁的**來源卡片**上
（一次是 `自訂來源 A` 卡片的「編輯」、一次直接落在 `NIM` 卡片）。
而**點選來源卡片依設計會立刻自動重接管 Codex**
（`src/components/providers/useToolSwitch.ts:200-215`
「來源切換自動重接管……永不彈框」），於是：

| 時間 | `~/.codex/config.toml` 被寫成 |
|---|---|
| 16:19:35 | `model = deepseek-v4.1-flash`、`codex-18.json`（provider 18 = 自訂來源 A）← 我誤觸 |
| 16:23:37 | `model = moonshotai/kimi-k3`、reasoning `max`、`codex-15.json`（provider 15 = NIM）← 我誤觸 |
| **16:28:56** | **`model = deepseek-v4.1-flash`、reasoning `medium`、`codex-22.json`（provider 22 = opencode-go）← 已還原，這是你原本的** |

**怎麼知道原本是 opencode-go**：`catalogs/` 目錄裡**我的工作階段之前最新的一份**
是 `codex-22.json`（09-26 05:47:16，provider 22 = opencode-go），而
opencode-go 的第一個模型正是 `deepseek-v4.1-flash`、reasoning 首位正是 `medium`
—— 與 App 內儲存的「上次接管參數」完全吻合。還原後我逐項核對：
8 個 provider 段全部 `wire_api = "chat"`、`base_url` 全部指向網關、
你的 plugins（11）／marketplaces（2）／`[desktop]`／`appearanceTheme`／
`selected-avatar-id`／`[windows]`／`mcp_servers`／`notify` **全部原樣保留**。

**副作用（無法復原的部分）**：`backups/codex/` 由 **11 個被輪換成 2 個**
（`config.toml.baseline-20260926-054716` 與 `config.toml.bak-20260926-054716`）。
這是 E 把 `BACKUP_KEEP` 改成 1 之後第一次接管必然的結果，**不是資料遺失**：
最重要的「接管前原始設定」= baseline 還在，所以 App 的
「關閉：還原接管前備份」仍然可用。其他工具的備份完全沒動
（claude 10／hermes 8／opencode 10，與基線相同）。

### 0.5.3 由此發現的一個真實缺陷（**已修，2026-09-28**）

**點一下來源卡片就會靜默重寫 `~/.codex/config.toml`，沒有任何確認或提示。**

- 位置：`src/components/providers/useToolSwitch.ts`（原 `:200-215`）。
  註解自己寫著「來源切換自動重接管……**永不彈框**」—— 這是刻意的設計，
  但代價是「只是想看看某個來源」也會改檔案。
- 觸發條件：**Codex 正在接管中** ＋ 所選來源改變。對已被接管的 Codex 而言，
  在來源清單上點任何一張卡片都會立刻重寫 config 並輪換備份。
- 反過來說**開頁面本身不會**觸發：`useToolSwitch` 的 `prevSelId` 守衛會把
  首次載入的 `selId`（`Providers.tsx:48` 的 `selected ?? providers.data?.[0]?.id`）
  只記錄不套用，所以「第一次進上游來源頁」是安全的。我實測確認過兩次
  （重啟 App 後只開頁面，`config.toml` 的 mtime 與 SHA-256 都沒變）。

**修法（已採用建議的 ②，2026-09-28）**：

自動重接管**整段移除**。現在所選來源改變時只做一件事 —— 在 Codex 那一行
顯示一行提示：

> 已選擇來源「X」，但 Codex 仍接管自原來源（未改動 config.toml）。
> 要換過去請按右側開關，或點此行左側進詳情按「套用」

也就是說：**寫入一律要使用者的明確動作**（行開關、或詳情裡的「套用」），
不再有任何因為「點一下看看」而改檔的路徑。沒有用確認框是因為確認框仍會在
「只是瀏覽」時跳出來打斷；把寫入的觸發點收斂到明確動作更乾淨。

> **為什麼不用 ①（確認框）**：確認框會讓「點卡片看模型與定價」這個**唯讀
> 意圖**變成一個要按掉的中斷。問題的根不是「沒問」，而是「瀏覽與切換共用
> 同一個點擊」。收斂觸發點直接解決根因。

---

## 0.6 2026-09-28：Codex 報「Unable to log in」的真因與修正

### 0.6.1 你看到的症狀

開啟 Codex 時出現 **「Unable to log in」**。

### 0.6.2 真因（不是登入問題）

Codex 的日誌（`~/.codex/logs_2.sqlite`，首次出現 **09-28 16:48:28**）說得很清楚：

```
ERROR codex_app_server: Invalid configuration; using defaults.
C:\Users\<user>\.codex\config.toml:16:12: `wire_api = "chat"` is no longer supported.
How to fix: set `wire_api = "responses"` in your provider config.
```

**Codex 已經不再接受 `wire_api = "chat"`。** 它讀到非法設定就**整份
`config.toml` 丟棄、改用預設值**，於是 `model_provider` 回到 `openai`、
回頭去找 ChatGPT 登入狀態 —— 你看到的「Unable to log in」是**下游症狀**，
跟登入本身無關（`~/.codex/auth.json` 一直都在，mtime 09-24）。

### 0.6.3 為什麼會寫出 `chat`

E 階段（`38c5bcd`）加了「**provider 宣告的 `api_format` 優先於 URL 推定**」。
但宣告描述的是**上游**說哪種協議，而寫進 `config.toml` 的 `base_url`
**永遠是本網關** —— 也就是說，宣告被用在了錯的那一段連線上。

你的 7 個來源 `api_format` **全部是 `openai-chat`**，所以接管後
**每一個** provider 段都被寫成 `chat`，Codex 直接罷工。

**舊版（E 之前）寫的是 `responses`** —— 依 URL 推定，指向網關就給 Codex 原生
形狀。這解釋了為什麼 09-26／09-27 都正常，而今天 12:31 裝了新版本之後才出事。

### 0.6.4 為什麼測試沒攔住

單元測試只驗了「網關 URL ＋ **未宣告**」→ `responses`
（用 `CodexAuth::default()`）；而生產路徑（`switch.rs`）
**永遠帶著宣告**。兩個條件從未在同一條測試裡相遇 —— 這是典型的
「測試驗了元件、沒驗接線」。

### 0.6.5 修正

| 項目 | 內容 |
|---|---|
| 新增 `wire.rs::codex_wire_api_for(base_url, declared)` | 先看 URL：指向**網關／官方 OpenAI** 就定死 `responses`，宣告不得推翻；只有**真正的第三方**才輪到宣告生效 |
| `gateway_section` 改用它 | 原本是 `auth.wire_api.unwrap_or_else(|| codex_wire_api(base_url))` |
| 新增回歸測試 | `codex_apply_forces_responses_on_gateway_even_if_declared_chat` —— 用的正是生產路徑的參數組合（網關 URL ＋ `codex_wire_api_declared(Some("openai-chat"))`） |
| 你目前的 `config.toml` | 8 個段的 `wire_api` 已全數由 `chat` 改為 `responses`（17:59:48） |

### 0.6.5b 同一類錯誤的**第二處**：預覽也會騙人

第一次修完之後，我回頭去找「同樣的運算式還寫在哪裡」，發現
`switch.rs::plan_switch` 有一份**一模一樣的副本**：

```rust
// 修正前（switch.rs，產生畫面預覽用）
let wire_api = codex_wire_api_declared(req.provider_format.as_deref())
    .unwrap_or_else(|| codex_wire_api(&req.base_url));
```

而它正上方一行註解寫著「**預覽必須顯示實際會寫入的值，否則預覽會騙人**」。
若只修 `gateway_section`，結果會是：**畫面預覽顯示 `chat`、實際寫入
`responses`** —— 兩個都錯開，使用者無從判斷哪個是真的。

已改為與寫入端呼叫**同一個函式** `codex_wire_api_for`，並新增測試
`plan_preview_wire_api_is_responses_on_gateway` 把兩邊釘在一起
（這條測試也順帶成為測試 E 準則 1 的自動化替代品）。

> **教訓**：同一個決策運算式出現在兩個地方，就是會漂移。修一處時必須
> 全域搜尋同型運算式；本次是靠「`codex_wire_api(` 的使用處」那個搜尋找到的。

**測試**：`cargo test --offline` → **219 passed / 0 failed / 8 ignored**（exit 0）；
`cargo clippy --offline --all-targets` → **exit 0、0 警告**。

### 0.6.6 修好之後的實機驗證（`codex doctor`）

Codex 自己有診斷指令（`codex doctor`），我用它做了**端到端**核對
（不用啟動 Codex、不消耗額度）：

| `codex doctor` 的輸出 | 意義 |
|---|---|
| `✓ config` → **`loaded`** | **不再出現「Invalid configuration; using defaults」** |
| `config.toml parse` → **`ok`** | 整份檔案合法（不是只有第一個錯誤被修掉） |
| `model` → `deepseek-v4.1-flash · custom` | 接管**生效**：確實走 `custom` 段（網關），不是退回預設 |
| `✓ auth` → **`OpenAI auth is not required for the active model provider`** | **「Unable to log in」的根因消失** |
| `model provider requires OpenAI auth` → **`false`** | Codex 知道這個 provider 不需要 ChatGPT 登入 |
| **`wire API` → `responses`** | 寫進去的值就是 Codex 現在唯一接受的那個 |
| `✓ reachability` → **`active provider endpoints are reachable over HTTP`**（`mode: provider auth`） | 它**真的帶著設定裡的 bearer token 打了一次網關並成功** |

也就是說：設定合法、模型目錄生效、認證路徑正確、而且**網關端確實收下了
寫進去的那把上游 Key**。這比「我看檔案內容對」強得多。

> **注意**：`codex doctor` 是 CLI 路徑；你原本的錯誤來自 `codex_app_server`。
> 但兩者共用同一份設定載入與驗證邏輯，而 `parse ok` ＋ `requires OpenAI auth
> false` 正是先前失敗的那兩項。

### 0.6.7 兩件仍待你決定的事（**兩件都已於 2026-09-28 處理完畢**）

1. ~~**`chat` 這個值在新版 Codex 已經完全不能用**……要不要整組拿掉、永遠只寫
   `responses`，請你決定。~~ **已決定並執行**：整組拿掉。
   現在 `wire_api` 是**一個常數** `CODEX_WIRE_API = "responses"`，
   由寫入端與畫面預覽**共用同一個符號**，型別上不可能再漂移；
   `codex_wire_api`／`codex_wire_api_declared`／`CodexAuth.wire_api`
   全部刪除（含它們的測試）。詳見 §0.7。
2. ~~新版本程式需要重新建置並安裝才會生效（見 §9.6）。在那之前，
   **不要再點「上游來源」頁的來源卡片**……~~ **已建置並安裝**（見 §9.8）。
   而且**新版即使被點也不會寫檔** —— 靜默重接管已移除（§0.5.3）。

---

## 0.7 2026-09-28（第二輪）：把「未完成清單」一次收乾

使用者指示「先把全部未完成的部分完成」。以下四項全部完成。

### 0.7.1 (a) 靜默重寫 `config.toml` —— 已修

見 §0.5.3。**寫入一律要明確動作**；所選來源改變只在畫面上提示。

### 0.7.2 (b) `wire_api` 的「依上游能力判斷」整組移除 —— 已執行

原本（E 階段）的設計是：`api_format` 宣告優先，URL 推定為後備。
**這個設計的前提已經不存在**：

| 當初的前提 | 現在的事實 |
|---|---|
| 有「真·直連第三方」模式，所以協議要跟著上游走 | 直連分支已於 §4.4 移除；`plan_switch` 與 `apply_switch` **都**把 `base_url` 強制改寫成網關 |
| `chat` 是可用的協議值 | **Codex 已完全不接受 `chat`**（整份 config 判為非法 → 「Unable to log in」） |
| `api_format` 能描述「Codex→上游」該用什麼協議 | 它描述的是**上游**說哪種協議，與「Codex→網關」這一段無關 |

所以現在是：

```rust
pub const CODEX_WIRE_API: &str = "responses";
```

寫入端（`gateway_section`）與預覽端（`plan_switch`）**共用這個符號**。
刪除的程式碼：`codex_wire_api()`、`codex_wire_api_for()`、
`codex_wire_api_declared()`、`CodexAuth.wire_api` 欄位，以及 3 條測舊機制的測試。

> **這是刻意的簡化，不是退步。** B4 當初修的是「無差別寫 `responses` 會讓
> 直連第三方 404」；但直連模式已經不存在，而且現在寫 `chat` 的代價是
> **Codex 完全不能用**（比 404 嚴重得多）。用常數取代判斷，讓「寫出 `chat`」
> 在型別層面不可能發生。

`api_format` 本身保留 —— 它仍有「來源列表顯示」與「網關端轉譯」用途。

### 0.7.3 體檢的 `TOKEN_GATEWAY_KEY` 假警報 —— 已修

`codex_doctor` 原本**無條件**檢查 `TOKEN_GATEWAY_KEY` 環境變數，沒設就報 ❌。
但「直連模式」寫的是 `experimental_bearer_token`（上游 Key 明文），
**根本不讀環境變數** —— 所以那個 ❌ 是假警報，而假警報的長期代價是
「使用者不再看體檢」。

現在改成先讀 `config.toml`，只有在**真的有 provider 段寫 `env_key`** 時才要求
環境變數；否則報 ℹ️ 說明目前設定不需要它。新增測試
`doctor_env_key_warning_depends_on_config_usage` 兩種情況都釘住。

### 0.7.4 測試 B 留下的 6 筆合成資料 —— 已清除

| 表 | 刪除的 id | 內容 |
|---|---|---|
| `proxy_trace` | 22, 23, 24 | `app='unknown'`、`trans_kind='rejected'`、狀態 401/401/400 |
| `request_logs` | 4353, 4354, 4355 | `app='unknown'`、`source='gateway'`、0 token、0 成本 |

刪除前後都有核對（`remaining: 0/0`），且**刪除前先備份資料庫**到
`.workbuddy/tmp/app.db.before-testB-cleanup`。證據本身（訊息全文、狀態碼、
`model_raw`）已完整保存在 §5.3，所以列本身可以刪。

### 0.7.5 實機驗證：App 真的會把壞掉的 `chat` 修回 `responses`

這一輪補上了先前一直缺的**端到端實機證據**（用的是安裝後的正式版本，
透過真實 UI 點擊，不是單元測試）。

**取證方式**：把 `~/.codex/config.toml` 的 8 個段**手動改回壞掉的
`wire_api = "chat"`**（重現事故狀態），然後在 App 裡開 Codex 詳情、按
「套用（切換來源/模型）」，再看檔案變成什麼。

| 步驟 | 動作 | 結果 |
|---|---|---|
| 1 | 手工把 8 段改成 `chat`（`app='unknown'` 無關，純檔案） | `now chat = 8 / now resp = 0` |
| 2 | `codex doctor`（**證明 `chat` 真的會讓 Codex 掛掉**） | ✗ `config could not be loaded — Fix the reported config error`、`failed to load Codex config` |
| 3 | App 內的真實接管：詳情 →「套用（切換來源/模型）」 | 寫入時間 `20:24:52` |
| 4 | 接管後檢查 | **`wire_api="responses"` → 8 段**（custom, tokengateway, gw, mock, nim-direct, nvidia-nim, nvidia-proxy, opencode-zen） |
| 5 | `codex doctor`（接管後） | ✓ `config loaded`、`config.toml parse ok`、`model deepseek-v4.1-flash · custom`、`requires OpenAI auth false` |

> **第 2 步是這次最有價值的收穫**：它把「`chat` 會導致 Codex 報
> Unable to log in」從推論變成**直接可重現的觀測**。錯誤訊息與事故當天
> 使用者看到的一致（「Invalid configuration; using defaults」→ 回頭找登入）。

**另外兩項同時取得的證據**：

1. **畫面預覽顯示的是實際會寫入的值**：在詳情裡展開「7 項寫入」，畫面上
   顯示的那一行是
   `[model_providers.{custom, tokengateway, …}] 共 8 段 base_url = http://127.0.0.1:15722/v1
   / experimental_bearer_token = 上游 Key 明文 / wire_api = responses`
   —— 這正是 §6.6 準則 1 先前「拿不到螢幕證據」的那一項，現在拿到了。
2. **接管是冪等的**：在設定已經是正確值的情況下按「套用」，檔案
   **逐行完全相同**（SHA-256 不變、9922 bytes），只有 mtime 更新。
   備份也沒有被輪換 —— 因為要備份的那份「舊設定」本身已指向網關
   （`backup_is_tainted`），會被 `prune_backups_keep_clean` 清掉，
   只留下真正的「接管前」乾淨基準（09-26 那兩份）。
   這是**刻意設計**，不是沒備份。

### 0.7.6 這一輪的閘門

| 項目 | 結果 |
|---|---|
| `cargo test --offline` | **217 passed / 0 failed / 8 ignored**（exit 0） |
| `cargo clippy --offline --all-targets` | exit 0、**0 警告** |
| `pnpm exec tsc --noEmit` | exit 0 |
| 測試數變化 | 220 → 217：刪 3 條測舊機制的，加 1 條體檢假警報的回歸測試 |
| 安裝後執行檔 | 9,010,176 bytes、SHA-256 `E098655E9488C4F79757B5266A220168DC80DD3D8276BA67658BFD793E1B4498` |
| 啟動後 `config.toml` | SHA-256 **完全沒變**（開頁面不觸發接管，已再次確認） |

> **一個環境備註**：`cargo build`（非 clippy）會出現 1 條
> `#[warn(linker_messages)]` 警告，內容是 MSVC 連結器的中文輸出
> （「正在建立程式庫 …」）。那是工具鏈的地區化輸出被新版 Rust 當成訊息，
> **與本專案程式碼無關**，clippy 為 0 警告。

---

## 0.8 2026-09-28（第三輪）：「無法封存舊對話」的根因

**你先回報的症狀**：Codex 開啟與使用舊 session 對話都正常，但**無法封存**
舊對話，試了兩個 test 都失敗。

### 0.8.1 結論先講：**這不是 Token Gateway 的問題**

封存是**純本機**動作（搬移 rollout 檔＋更新 sqlite），不會經過網關、
不呼叫模型。根因是 Codex 自己留在
`%USERPROFILE%\.codex\thread-writer-locks\` 底下的**殘留鎖檔**：

```
%USERPROFILE%\.codex\thread-writer-locks\<thread-id>.lock
```

只要那個 0 byte 的 `.lock` 還在，`codex archive <id>` 就會失敗。把它刪掉，
同一條指令立刻成功。

### 0.8.2 完整證據鏈

| # | 動作 | 結果 |
|---|---|---|
| 1 | `threads` 表有 `archived`／`archived_at` 欄位；09-26 有成功封存過 | 機制本身正常 |
| 2 | 所有相關 rollout 檔都在、大小正常、projection offset 也等於檔案大小 | 排除「檔案遺失／投影不完整」 |
| 3 | 把 doctor 報的 `no usable header record` 那個 **0 byte 檔**移走再試 | **仍然失敗** → 不是它 |
| 4 | 換別的 test session 試封存 | **成功**（`Archived session …`，exit 0）→ 不是全體故障 |
| 5 | 比對失敗 vs 成功的 session | 失敗的那個有 `.lock`；成功的三個都**沒有** |
| 6 | 刪掉失敗那個 session 的 `.lock`，立刻重試 | **成功封存**（`archived=1`、rollout 搬進 `archived_sessions\`） |
| 7 | 刪掉另一個 `.lock` 後等 100 秒 | **沒有自己長回來** → 是真殘留，不是現行寫入中 |
| 8 | 用 `CreateFileW`（share mode = 0）檢查每個鎖檔 | 6 個裡有 5 個**仍有行程開著 handle** |

> **第 8 點是關鍵**：那些鎖檔的 handle 由**還在跑的 Codex 行程**持有
> （`codex` PID 3328，20:45:07 啟動），所以 Codex 一開著，這些鎖就永遠
> 不會自己消失 —— 這才是「試幾次都失敗」的原因。刪檔之所以仍可行，
> 是因為它以 `FILE_SHARE_DELETE` 開啟；但 Codex 的判斷是**看檔案在不在**，
> 所以刪掉就通了。

### 0.8.3 為什麼會殘留（推論，非直證）

`thread-writer-locks` 是 Codex 用來記錄「這個 thread 有人正在寫」的機制。
09-28 21:03:12～21:03:50 之間有 **7 個 thread 被依序更新、並各留下一把鎖**
（間隔 5～10 秒，看起來是逐一開啟對話）。合理推論是：**在 Codex Desktop 裡
開過那些對話，之後鎖沒有被釋放**；而只要鎖在，之後每一次封存都會失敗。
我沒有去證明「關閉對話是否會釋放」，因為那需要動你的 app 狀態。

### 0.8.4 怎麼修（附工具）

我新增了 `scripts/codex_clear_stale_locks.py`：

```powershell
py scripts\codex_clear_stale_locks.py            # 只報告，不動任何東西
py scripts\codex_clear_stale_locks.py --apply    # 刪除，且先備份到
                                                 # .codex\thread-writer-locks-backup
```

它**只會**刪 0 byte 的 `<uuid>.lock`，而且會先檢查有沒有行程還開著 handle，
**有在用的絕對不碰**；`.coordination.lock` 與 rollout 檔一律不碰。

**建議流程**（順序有意義）：

1. **完全關閉 Codex**（Desktop 與任何 `codex` 行程）。這樣 handle 才會放掉。
2. `py scripts\codex_clear_stale_locks.py` —— 這時應該全部列為 stale。
3. 加上 `--apply` 真的刪除。
4. 再封存：`codex archive <session-id>`，或在 Desktop 裡封存
   （**對話要先關掉**，別在它開著的時候封存）。

> **我實測時 Codex 開著，工具只判定 1 個為 stale、5 個「IN USE」**。
> 那是正確且保守的行為：Codex 沒關就別硬刪。

### 0.8.5 我在診斷過程中動到的東西（誠實揭露）

| 動作 | 影響 | 怎麼還原 |
|---|---|---|
| 封存了 4 個標題為 `test` 的 session：`01a08b17`、`01a08b19`、`01a0924b`、`01a0924c` | 它們現在是已封存狀態 | `codex unarchive <id>` |
| 刪除並還原了 `01a063d2`（標題 `OK 你好`）的鎖檔 | 已用備份還原 | 備份在 `.workbuddy\tmp\lock-backup\` |
| 曾把一個 0 byte rollout 檔暫時移走再放回 | 已還原 | 該檔本來就是 0 byte |

**沒有**改動任何 rollout 內容、沒有改 `config.toml`、沒有改 state DB 的資料。

---

## 0.9 2026-09-28（第四輪）：「關掉網關，Codex 卻沒回到原生來源」

### 0.9.1 你回報的症狀

> 在我關閉該工具的網關後應該要恢復成原本使用的原生來源/模型，
> 目前 codex 沒有恢復，來源還是 NIM 的來源/模型

⚠️ **你附的截圖我讀不到** —— 這個 session 的模型不接受圖片輸入
（`read_image` 直接回「does not declare image input」）。以下是我從檔案 mtime
與資料庫重建出來的，若與截圖不符請直接告訴我。

### 0.9.2 真因：基線是**很久以前的快照**，而「關」只會照抄它

| 時間 | 事件（可複核：檔案 mtime ＋ `backups/codex/`） |
|---|---|
| 09-26 05:47:16 | `config.toml.baseline-20260926-054716` 產生。內容**已經是 NIM**：`model_provider = "custom"`、`model = "moonshotai/kimi-k3"`、`model_catalog_json = …\catalogs\codex-15.json` |
| 09-28 20:24:52 | 你切到 opencode-go，寫入 `catalogs\codex-22.json` |
| 09-28 21:26:16 | 你切回 NIM，寫入 `catalogs\codex-15.json` |
| 09-28 21:29:47 | 你把開關關掉 → App 把 baseline 寫回去 → 你又回到 NIM |

兩個獨立的原因疊在一起：

1. **基線只在你「進入接管」的那一刻拍一次**（`ensure_baseline_before_prune` 的語義
   是「接管前的乾淨設定」）。之後你在 App 裡換來源／模型，改的都只是「指向網關」
   的那份設定，基線不會更新 —— 所以只要你不曾真正離開接管態，下一次「關」還是
   回到同一個舊快照，然後這個舊快照又被拍成新基線（自我延續）。
   （補：基線在每次還原後會被清掉，下次接管會重拍 —— 也就是說**從現在起**
   基線會是你剛拿回來的原生設定，這也是這次修正的一個額外好處。）
2. **「關」的語義本身太窄**：它假設「接管前」就是你要的終點。你要的是
   「這個工具回到它自己的來源」。

### 0.9.3 修法：關 = 忠實還原 ＋ 原生來源化

新增 `switch_off` 命令（`tools/native.rs`），關閉流程變成兩步：

1. `restore_backup_port` —— 忠實還原，保住你自己的設定與別名 provider 段。
2. `to_native` —— 只把**來源**相關的鍵換回第一方：

| 工具 | 做什麼 |
|---|---|
| **codex** | `model_provider = "openai"`、`model = "gpt-5.6-luna"`、移除 `model_catalog_json`（移除後 Codex 才會用內建的 GPT 模型清單） |
| **claude** | 移除**任何不是 Anthropic 官方**的 `ANTHROPIC_BASE_URL`（網關或使用者自己的 router 都一樣），連帶移除 `ANTHROPIC_AUTH_TOKEN`；移除非 `claude-*` 的模型對映 |
| **opencode** | 移除指向網關的 `provider.*.options.baseURL` 與本地 `sk-local-` key |

> **Claude Code 的端點是刻意的例外**（2026-09-28 你選的）：連你自己原本的
> router（`https://opencode.ai/zen/go/v1`）也一起移除，推回 Anthropic 官方。
> 代價是這台機器上沒有 `.claude/.credentials.json`，你必須先登入一次。

**為什麼保留別名 provider 段**（`custom`／`nvidia-nim`／`opencode-zen`…）：舊對話
的 rollout 裡記著 provider 名，段被刪掉那些對話就開不起來。所以關閉後
`config.toml` 裡**仍然看得到**它們與其 URL —— 那是刻意的，是你 2026-09-28
選的方案（「原生來源 + 保留舊對話通道」）。

**Codex 的原生模型清單**（用乾淨的 `CODEX_HOME` 探到的，config 的 catalog 會
蓋掉它）：`gpt-6-astra`、`gpt-5.6-sol`、`gpt-5.6-terra`、`gpt-5.6-luna`、
`gpt-5.5`、`gpt-5.2`。`gpt-5.6-luna` 是你自己舊對話用過的模型，也是你選的預設。
另外確認你的 `~/.codex/auth.json` 是 `auth_mode = "chatgpt"`（id/access/refresh
token 齊全）→ 回到官方是可行的。

### 0.9.4 順手修掉一個因此暴露的假警報

`restore_summary` 的「直連第三方 URL 卻走 responses 協議（會 404）」警告原本是
**掃整份文本**找 `https://` ＋ `wire_api = "responses"` ＋ 沒有
`api.openai.com`。別名段留著之後，即使來源已經切回 `openai`，畫面還是會端出
那句「不可用請改走網關接管或官方登入」—— 正好在使用者剛回到官方來源時嚇他
一次。已改成**只看 `model_provider` 指名的那個段**
（`summary.rs` 的 `active_provider_is_direct_chat`）。

### 0.9.5 我這輪動到的東西（誠實揭露）

| 動作 | 說明 |
|---|---|
| 新增 `tools/native.rs`、`tools/summary.rs`、`tools/tests/native.rs` | 分別是原生化、還原摘要（從 `backup.rs` 搬出，因為它一度到 421 行）、9 條新測試 |
| 改 `commands/apps.rs`＋`lib.rs` | 新增 `switch_off` 命令 |
| 改前端 4 檔 | `api.ts`（新增 `switchOff`）、`useToolSwitch.ts`、`ToolList.tsx`、`switchdialog/*`（開關 OFF 改走新命令、文案更新） |
| 保留 `switch_restore`／`api.switchRestore` | 它是「逐字還原、不加工」的安全網。UI 已不再呼叫，程式碼裡的說明有寫明 |

**這一輪第一次動到你的 `config.toml` —— 就是下面那次實機驗證**（接管再關閉，
把 Codex 從 NIM 帶回 OpenAI）。在那之前只改程式與文件。細節見 §0.9.7。

### 0.9.6 這一輪的閘門

| 項目 | 結果 |
|---|---|
| `cargo test --offline` | **226 passed / 0 failed / 8 ignored**（exit 0） |
| 新增測試 | 9 條：冪等、壞 TOML 不猜、別名段完整保留、只認網關痕跡、`switch_off` 端到端（還原＋轉換都真的落到檔案） |
| `cargo clippy --offline --all-targets` | **0 警告** |
| `pnpm exec tsc --noEmit` | exit 0 |
| 檔案行數 | 205 檔全部 ≤ 400 行 |

### 0.9.7 實機驗證（安裝後的正式版本，真實 UI 點擊）

安裝 2026-09-28 22:32 的建置到 `%LOCALAPPDATA%\token-gateway`（NSIS `/S`，
exit 0，exe 9,026,048 bytes、sha256 `1C9D27A7…`），然後**照你要做的事做一遍**：

| 步驟 | 觀察 |
|---|---|
| 開 App →「上游來源」→「本機工具」 | Codex 的開關標題已經是 **「關閉：還原接管前備份，並切回這個工具的原生來源」**（新文案真的進了 bundle） |
| 當時狀態 | `本機工具（1/4 接管中）`；Codex 是「開啟」態、Claude Code 是接管中 |
| 按 Codex 開關 → **接管** | `config.toml` 7,718 bytes、8 個 `base_url` 全指向 `http://127.0.0.1:15722/v1` |
| 再按同一個開關 → **關閉** | 卡片下方出現：`…｜還原自 config.toml.baseline-20260928-223526｜provider=openai model=gpt-5.6-luna｜已切回原生來源：model_provider → openai、model → gpt-5.6-luna、移除 model_catalog_json…` |

關閉後的 `config.toml`（6,913 bytes、sha256 `8A1ADF7F…`）：

| 檢查項 | 結果 |
|---|---|
| `model_provider` / `model` | **`openai`** / **`gpt-5.6-luna`** ✅ |
| `model_catalog_json` | **0 次**（已移除）✅ |
| `[model_providers.*]` | **8 段全在**（`custom`／`tokengateway`／`gw`／`mock`／`nim-direct`／`nvidia-nim`／`nvidia-proxy`／`opencode-zen`，都指向 `https://integrate.api.nvidia.com/v1`）✅ 舊對話通道保留 |
| 指向網關的段 | **0 段** ✅ |
| 你自己的鍵 | `model_reasoning_effort = "high"`、`disable_response_storage`、`model_context_window`、`model_auto_compact_token_limit`、`[plugins.*]` ×11、`[mcp_servers.*]` ×2、`[windows]`、`appearanceTheme` 全在 ✅ |
| **假警報** | 訊息裡**沒有**「不可用請改走網關接管」那句 ✅（§0.9.4 的修正生效） |
| `codex doctor` | `✓ config loaded`、`model gpt-5.6-luna · openai`、`config.toml parse ok`、`✓ auth auth is configured`、`stored auth mode chatgpt` ✅ |
| **Codex 的模型清單** | `codex debug models` → **`gpt-5.6-terra`、`gpt-5.6-luna`、`gpt-5.5`**（＋2 個隱藏），NIM 的模型全部消失 ✅ |

---

### 0.9.41 品牌更名：Token Gateway → Token Toll（2026-10-04）

#### 使用者要求

> 帮我想个合适的专案名称，我要发布到 GitHub 上
> 可以 依照你的建议全部一次改完

命名經過：`Token Toll`（收費站）——toll = 通行費／收費站，車輛過收費站要計費、
token 過閘道要計量，語意一次講完閘門＋計量＋收費；`Token`＋`Toll` 雙 T 頭韻好記，
且與 OpenAI／Anthropic／Cursor 無混淆風險。

#### 改名範圍（三類，逐項裁決）

**A 類：顯示名（安全，直接改）**

| 位置 | 舊 | 新 |
|---|---|---|
| `tauri.conf.json` `productName`／視窗標題 | `token-gateway`／`Token Gateway` | `Token Toll` |
| `window.rs` 視窗標題 | `Token Gateway` | `Token Toll` |
| `tray.rs` tooltip | `Token Gateway · 本地網關` | `Token Toll · 本地網關` |
| `App.tsx` 側欄品牌＋副標 | `Token Gateway`／`本地 AI 網關` | `Token Toll`／`本機 AI 用量收費站` |
| `i18nDict.ts` 關於頁 ×2 | `Token Gateway v2 · Tauri 本地網關` | `Token Toll v2 · 本機 AI 用量收費站` |
| `Cargo.toml` name／description／authors／default-run | `token-gateway`／`A Tauri App`／`you` | `tokentoll`／真實描述／`aaaeeezynx` |
| `Cargo.toml` `[lib] name` | `token_gateway_lib` | `tokentoll_lib` |
| `package.json` name | `token-gateway` | `tokentoll` |
| `tools/codex/wire.rs`、`tools/apply.rs` 的 provider 顯示名 | `Token Gateway` | `Token Toll` |

**C 類：User-Agent 與檔案命名（無相容問題）**

| 位置 | 舊 | 新 |
|---|---|---|
| `quota/{claude,opencode}.rs` | `token-gateway/0.1 quota` | `tokentoll/0.1 quota` |
| `usage_query/run.rs` | `token-gateway/usage-query` | `tokentoll/usage-query` |
| `provider_check.rs` | `token-gateway/connectivity-check` | `tokentoll/connectivity-check` |
| 匯出檔名前綴 | `token-gateway-providers-*.json` | `tokentoll-providers-*.json` |

**B 類：識別碼（你選擇「全部改，且新舊並存」）**

這類已經寫進使用者的系統與工具設定，硬切會造成中斷，所以一律**寫新名、讀時新舊都認**：

| 識別碼 | 新名 | 舊名（相容保留） | 相容作法 |
|---|---|---|---|
| 渠道 ID | `GATEWAY_PROVIDER_ID = "tokentoll"` | `tokengateway` | 接管時**兩個段都建立**並指向網關（舊會話按段名引用供應商）；還原時兩個都清 |
| 環境變數 | `TOKEN_TOLL_KEY` | `TOKEN_GATEWAY_KEY` | `GATEWAY_ENV_KEYS` 讀取時依序嘗試；寫入用新名；體檢報實際命中的那個 |
| 匯出格式 | `EXPORT_KIND = "tokentoll/providers"` | `token-gateway/providers` | `EXPORT_KINDS` 匯入時新舊都接受 |
| 釘住的模型前綴 | `tokentoll/<模型>` | `tokengateway/<模型>` | 還原時兩個前綴都視為「網關釘住」而移除 |
| 去重清單 | — | — | `custom` 與新舊渠道 ID 一起從歷史會話 provider 清單去重 |

**刻意不改**（改了會造成不可逆的使用者可見破壞）：

| 項目 | 理由 |
|---|---|
| `identifier: com.tokencounter.gateway` | 決定資料目錄。改了 → 現有 6,137 筆請求、7 個來源、2 把 Key **全部看不到**（檔案還在舊路徑，App 讀新目錄）。bundle ID 不隨品牌改是業界慣例 |
| 登錄檔值名 `TokenGateway`（`autostart.rs`） | 已寫進使用者登錄檔，改了會讓既有自啟變成孤兒，而使用者看不到這個名稱 |
| `TOKEN_GATEWAY_MIGRATE_DB`（測試專用 env） | 只在 `db/tests/migrate.rs` 的人工遷移測試用，不影響使用者 |

#### 過程中修掉一個真實的還原缺陷（不是改名造成的，是改名讓它暴露）

`codex_native()` 原本的還原邏輯如果是「無條件刪除網關渠道段」，會誤刪
**使用者已改成自己直連設定**的同名段。測試 `opencode_native_drops_gateway_endpoint_and_local_key`
的模擬輸入正好是這個情境（`[model_providers.tokengateway]` 指向 NVIDIA），
改名後該段被新邏輯刪掉 → 測試失敗，因而發現。

**修正**：只刪「**還指向本網關**」的段（`base_url` 以 `http://127.0.0.1:` 開頭），
判準與既有的 `custom` 段一致。使用者的直連設定從此不會被順手刪掉。

> 這是本輪唯一的行為邏輯變更；其餘都是字串替換。

#### 改名的代價（給未來的自己）

| 影響 | 說明 |
|---|---|
| 安裝路徑 | `%LOCALAPPDATA%\token-gateway\` → `%LOCALAPPDATA%\Token Toll\` |
| 開始功能表 | 捷徑改名，**舊捷徑會殘留**，需手動移除 |
| 開機自啟 | 登錄檔指向舊 exe 路徑 → **需重新開啟自啟** |
| 執行檔 | `token-gateway.exe` → `Token Toll.exe`（Tauri 用 productName 命名） |
| 使用者資料 | **不受影響**（identifier 未變，DB 與備份原地不動） |

改名前完整備份：`.workbuddy/rename-backup-20261004-173213/`（47 檔、16.6 MB，
含整個 `%APPDATA%\com.tokencounter.gateway`、`~/.codex/config.toml`、
自啟登錄檔匯出、目前 exe 資訊）。

#### 閘門

| 項目 | 結果 |
|---|---|
| `cargo test --offline --lib` | **318 passed / 0 failed / 8 ignored** ✅（與改名前的數字一致） |
| `cargo clippy --offline --all-targets` | **0 warning** ✅ |
| `pnpm exec tsc --noEmit` | exit 0 ✅ |

改名過程中一度有 10 個測試失敗，全部是**斷言舊名稱**或**行為已變更**；
逐一更新預期值後回到 318 綠燈（沒有任何測試被刪除或跳過）。

---

### 0.9.40 冗餘檔案盤點與清理（2026-10-04）

#### 使用者要求

> 提交这个删除 并且检查整个项目中所有的冗余档案 先列出来，我决定哪些必须删除
> 尤其是先前设计的icon廢案必须完全清理

先盤點、列清單、由使用者逐項裁決，**沒有先刪任何東西**。

#### 盤點方法

掃描整個 workspace（含隱藏檔），分成六類：icon 廢案、暫存快取、建置產物、
快照與證據、可能過時的文件、以及已追蹤檔案中的大檔。每一項都標明大小與 Git 狀態，
並用 `git check-ignore`／`git ls-files` 逐一確認「刪了會不會影響版控」。

#### 使用者裁決與執行結果

| 項目 | 內容 | 大小 | Git 狀態 | 裁決 | 執行 |
|---|---|---|---|---|---|
| A1 | `logo-previews/`（7 輪提案、20 版設計稿＋對照圖） | 18.3 MB | 已 ignore | 刪除 | ✅ |
| A2 | `icons/concepts/`（5 張早期概念稿） | 5.4 MB | 已 ignore | 刪除 | ✅ |
| B1 | `.workbuddy/tmp/`（生成腳本與中間產物） | 659 KB | 已 ignore | 刪除 | ✅ |
| B2 | 根目錄 `__pycache__/` | 1.1 KB | 已 ignore | 刪除 | ✅ |
| B3 | `scripts/__pycache__/` | 9.3 KB | 已 ignore | 刪除 | ✅ |
| C1 | `src-tauri/target/` | 12.98 GB | 已 ignore | **保留** | — |
| C2 | `node_modules/` | 434 MB | 已 ignore | **保留** | — |
| C3 | `dist/` | 669 KB | 已 ignore | **保留** | — |
| E3 | `docs/CC-SWITCH-PARITY.md` | 20 KB | 已追蹤 | 刪除 | ✅ |
| E4 | `tokengw-plan.md`（根目錄） | 4 KB | 已追蹤 | 刪除 | ✅ |
| E1 | `docs/REFACTORING-PLAN.md` | 79 KB | 已追蹤 | **保留** | — |
| E2 | `docs/SIMPLIFICATION-PLAN.md` | 47 KB | 已追蹤 | **保留** | — |

**釋放 24.3 MB。** 建置產物依使用者指示保留在磁碟（不進 Git，本來就已 ignore）。

#### E1／E2 的查核（使用者要求先確認是否已執行完畢）

兩份**都不是待辦計畫，而是執行後的實況記錄**，所以保留：

| 文件 | 自述狀態 | 抽驗證據 |
|---|---|---|
| `REFACTORING-PLAN.md` | 「v2 為執行後的實況記錄」，Phase 0～5 **全部 ✅** | `proxy.rs` 現為 **346 行**（<400）＋ `proxy/` 子模組；`tools.rs` ＋ `tools/` 子模組；計畫引用的 7 個代表 commit（`29f0db1`／`a98b577`／`27a73ba` 等）全部存在 |
| `SIMPLIFICATION-PLAN.md` | 「第一階段 D-1／A／E／F 全部完成，已建置、已實機驗證」 | 計畫聲稱移除的 `hermes` 在原始碼仍有 18 處，但**不是清理不乾淨** |

`hermes` 那 18 處的查核結論（`tools/detect.rs` 的註解寫得很明確）：

> hermes **刻意保留**：那四處是**用量歸屬與顯示**用的，不是工具偵測用的。
> 刪掉的話，用量頁的歷史資料會失去歸屬。

`tools/apply.rs` 則明確拒絕寫入它的設定（回傳「Hermes 已於第一階段（F）移除支援」）。
所以 F 階段的目標（不再接管、不再偵測）確實達成，保留的是**用量統計標籤**，兩者不衝突。

#### 一個更正：`src-tauri/gen/schemas/` 本來就已正確忽略

上一輪盤點時我把 `src-tauri/gen/` 列為「已追蹤，建議加 ignore」，**這是誤判**。
實查結果：

| 檢查 | 結果 |
|---|---|
| `git ls-files token-gateway/src-tauri/gen` | **空**（追蹤檔數 0） |
| `git status --ignored` | `!! token-gateway/src-tauri/gen/schemas/` |
| 規則來源 | `token-gateway/src-tauri/.gitignore:7` → `/gen/schemas` |

社群慣例**早就已經套用**（Tauri 官方樣板本身就帶這條規則），不需要任何動作。
誤判的原因是我當時只看了目錄存在與檔案大小，沒有對它跑 `git ls-files`。

#### 提交內容

| Commit | 內容 |
|---|---|
| `856f73d` | `chore: 移除已被取代的設計記憶檔` —— `.workbuddy/memory/2026-09-18.md`（Icon R3 路線的裁決記錄，內容已被 §0.9.38 取代） |
| 本次 | 刪除 `docs/CC-SWITCH-PARITY.md` 與 `tokengw-plan.md` |

> icon 廢案與暫存類（A1／A2／B1–B3）都在 `.gitignore` 內，刪除不會產生 commit ——
> 這也是為什麼要先確認 Git 狀態：**已忽略的檔案刪掉不留痕跡，已追蹤的才會進版控**。

---

### 0.9.39 側邊欄「診斷」選中後圖示變成閃電（2026-10-04）

#### 使用者回報

> 诊断的 icon 点击后会切换为另外一种，修正这个问题

#### 真因：選中態配錯圖示

`App.tsx` 的 `TABS` 用 `icon`／`iconFill` 兩欄位表現「選中」：

```ts
{ id: "usage",       icon: "chart-bar",  iconFill: "chart-bar-fill" },
{ id: "providers",   icon: "server",     iconFill: "server" },     // 同形狀
{ id: "keys",        icon: "key",        iconFill: "key-fill" },
{ id: "diagnostics", icon: "alert",      iconFill: "bolt-fill" },  // ← 壞在這行
{ id: "calc",        icon: "calculator", iconFill: "calculator" }, // 同形狀
{ id: "settings",    icon: "gear",       iconFill: "gear-fill" },
```

其餘五頁的 `iconFill` 都是**同一個形狀**的填色版（或直接同一個圖示），只有診斷頁
從「三角形警示」`alert` 變成「閃電」`bolt-fill`——兩個完全不同的圖形，
所以點下去看起來像整個圖示被換掉。

#### 修法：補一個與 alert 同形狀的 alert-fill

新增 `icons.tsx` 的 `alert-fill`，外框沿用 `alert` 的三角形路徑
（`M12 4L21 19.5H3L12 4z`），驚嘆號改用 **`fill-rule="evenodd"` 挖成真正的負空間**：

```tsx
case "alert-fill":
  return (
    <path
      d="M12 4L21 19.5H3L12 4z
         M12 9.4a1.15 1.15 0 0 1 1.15 1.15v3.2a1.15 1.15 0 0 1-2.3 0v-3.2A1.15 1.15 0 0 1 12 9.4z
         M12 15.5a1.25 1.25 0 1 1 0 2.5 1.25 1.25 0 0 1 0-2.5z"
      fill="currentColor"
      fillRule="evenodd"
      stroke="none"
    />
  );
```

> 一度想用「填色三角 + 再用背景色描一次驚嘆號」的做法，但選中態的底是**半透明**的
> `--mac-selected-bg`，用固定色描邊會出現色差。`evenodd` 是真正的洞，
> 不依賴底色，任何背景都正確。

`App.tsx` 則改為：

```ts
{ id: "diagnostics", label: "nav.diagnostics", icon: "alert", iconFill: "alert-fill" },
```

`bolt-fill` 本身沒有被移除——它在診斷頁的區塊標題（`SectionHead`）、連線檢查、
用量查詢面板等 6 處仍是獨立語意，不受影響。

#### 實機驗證（安裝後、CDP 讀真實 DOM 的 SVG 幾何）

逐一比對側邊欄六項「未選中 vs 選中」的 path 幾何：

| 頁籤 | 未選中 | 選中 | 判定 |
|---|---|---|---|
| 用量 | `chart-bar` 線稿 | `chart-bar-fill` 實心柱 | ✅ 同語意（線 → 實心） |
| 上游來源 | `server` | `server` | ✅ 完全相同 |
| 本地 Key | `key`（圓 r=4） | `key-fill`（圓 r=4.4） | ✅ 同形狀填色版 |
| **診斷** | `alert`：`M12 4L21 19.5H3L12 4z` ＋ 驚嘆號線稿 | **同一個三角形路徑** ＋ `fill-rule=evenodd` 挖空 | ✅ **已修正**（原本會變成 `bolt-fill` 閃電） |
| 試算 | `calculator` | `calculator` | ✅ 完全相同 |
| 設定 | `gear`（線寬 1.8） | `gear`（線寬 2.6） | ✅ 同形狀加粗 |

| 閘門 | 結果 |
|---|---|
| `pnpm exec tsc --noEmit` | exit 0 ✅ |
| `cargo test --offline --lib` | **318 passed / 0 failed / 8 ignored** ✅ |
| `cargo clippy --offline --all-targets` | 0 warning ✅ |
| `pnpm exec tauri build` | exit 0 ✅ |

---

### 0.9.38 App 主 Icon 換裝：v7「橫向等長列」（2026-10-02）

#### 使用者要求

> 02 橫向等長列 所有项目中的 icon替換爲這個icon

#### 選定歷程（前後七輪提案）

| 輪次 | 產出 | 結果 |
|---|---|---|
| R1 五概念 | 閘門／分流／量表／字母 T／鑰匙孔 | 未採用 |
| R2 用量計算 | 用量圓環／量表／額度長條／計算機／三環 | 未採用（03 額度長條成為後續主線） |
| R3 長條衍生一 | % 徽章／趨勢箭頭／分段柱／走勢折線／滾動環 | 未採用 |
| R4 長條衍生二 | 進度柱／倒影／格線／時鐘／圓環 | 01 進度柱成為下一輪主線 |
| R5 霧面重設計 | 進度元素五版 ＋ 霧面外殼 | 05 滿載對比成為下一輪主線 |
| R6 精緻化三版 | 液位量筒／凹槽卡尺／滿載滴落 | 使用者判「精緻度不夠」 |
| R7 減法精緻化 | 等高量柱／**橫向等長列**／錯落精修 | ✅ **選定 02** |

R6 → R7 的關鍵差異：R6 走「加法」（管壁、新月線、刻度、水滴），結果更像細節堆料；
R7 改成「**減法 ＋ 儀表級對齊**」——全圖只剩一種細節語彙（液面線），靠比例、對齊與材質取勝。

#### 定案規格

| 項目 | 值 |
|---|---|
| 外殼 | macOS 級超橢圓 **n=4**（45° 對角曲率半徑 ≈ 0.47a ≈ Apple 22.4% 圓角率），四角透明 |
| 主色 | 強調色藍 `#3395ff → #0a84ff → #0070e0`（＝ `src/lib/theme.ts` 的 `ACCENTS[0]`） |
| 質感 | Liquid Glass **霧面**：12% 乳白霧罩、0.88 內縮玻璃板（羽化邊）、左上大範圍柔光暈（blur 280）、寬柔頂部受光、底部內陰影、**霧面顆粒 sigma 14/12**（絲滑噴砂）、環境柔影 |
| 字形 | 三條**等長**橫軌上下堆疊，左起填充 100%／65%／48%；填右端一條「液面線」（比軌道高 15px 的細亮線）；刻意不加基線 |
| 母稿（入庫） | `token-gateway/icon-base-1024.png`（1024×1024 RGBA） |
| 設計歷程 | `logo-previews/`（本機保留，已列入 `.gitignore`） |

#### 換裝範圍

| 位置 | 動作 |
|---|---|
| `src-tauri/icons/*.png`（17 檔：32／64／128／128@2x／icon.png／Square*Logo／StoreLogo） | `pnpm exec tauri icon icon-base-1024.png` 重產 |
| `src-tauri/icons/icon.ico`（內含 16／24／32／48／64／256 六個尺寸） | 同上 |
| `src-tauri/icons/icon.icns` | 同上 |
| `src-tauri/icons/android/**`（14 檔）、`ios/**`（16 檔） | 同上 |
| web favicon | 新增 `public/favicon.png`（64×64），`index.html` 改指向它 |
| 兩個品牌 placeholder | 刪除 `public/vite.svg`、`public/tauri.svg`（全 repo 已無任何引用） |
| 工作列／視窗／系統匣 | **不需要另外改**：`tray.rs` 用 `app.default_window_icon()`；Windows 的視窗圖示來自 Tauri 編進 exe 的 bundle icon |

#### 產出驗證（逐像素）

| 檔案 | 尺寸 | 實心像素 | 藍佔比 | 左上角 alpha |
|---|---|---|---|---|
| `32x32.png` | 32×32 | 760 | 97.2% | 0 |
| `64x64.png` | 64×64 | 3,164 | 96.3% | 0 |
| `128x128.png` | 128×128 | 12,889 | 95.6% | 0 |
| `128x128@2x.png` | 256×256 | 51,675 | 95.0% | 0 |
| `icon.png` | 512×512 | 206,953 | 94.8% | 0 |
| `icon.ico`（最大格） | 256×256 | 51,675 | 95.0% | 0 |
| `icon.icns`（1024 格） | 1024×1024 | 829,291 | 94.7% | 0 |
| `StoreLogo.png` | 50×50 | 1,958 | 96.9% | 0 |

**圓角曲率鎖定**：共同取樣點 `(920,920)` α=15（形狀外）、`(908,908)` α=252（形狀內），
與已確認 n=4 的母稿逐點相同 → 轉出的每個尺寸都是同一曲率族，沒有在縮放時被改成圓角矩形。

#### 「角落 alpha」是這次換裝最直接的鑑別特徵

舊 icon 是前幾輪「HIG 分層式」路線的成品：**全幅不透明**（512×512 的 262,144 px 全是實心、藍佔比 90.6%）。
新圖是使用者明確要求的 macOS 圓角 squircle，**四角透明**（實心 206,953 px、藍佔比 94.8%）。
所以「角落是不是 0」可以一眼分辨新舊，不必靠肉眼比對。

#### 一個副作用：圖檔變大

霧面顆粒是高頻雜訊，PNG 壓縮效率會差很多：

| 檔案 | 舊 | 新 | 倍率 |
|---|---|---|---|
| `icon.icns` | 64,718 | 1,730,496 | 26.7× |
| `icon.png` | 13,257 | 238,061 | 18.0× |
| `128x128@2x.png` | 6,586 | 53,371 | 8.1× |
| `icon.ico` | 12,713 | 67,091 | 5.3× |

顆粒是使用者指定的霧面質感，不打算拿掉；若日後想縮小，只要在產 icon 前對母稿做輕度降噪或色彩量化即可，
母稿與生成流程都不用改。

---

### 0.9.37 移除 MCP／提示詞／技能／會話四個功能（2026-10-02）

#### 使用者要求

> 移除 MCP、提示詞、技能、會話這幾個功能。

**範圍只限這四項**（用量／上游來源／本地 Key／診斷／試算／設定六頁、接管、tray、
雲端同步、DB 備份、`resource=provider` 的 Deep Link 全部不動）。

#### 動手前先問清楚「會不會刪到我的 Codex 檔」

使用者的第一個問題是「會不會連我 codex 的 AGENTS.md 一起刪除」。先把「誰會寫
使用者的檔案」整條路查出來才回答：

| 使用者的檔案 | 誰會寫它 | 移除後 |
|---|---|---|
| `~/.codex/AGENTS.md` | **只有 `prompt/` 模組**（`prompt/sync.rs::path_for`，共 3 條路徑：claude→`CLAUDE.md`、codex→`AGENTS.md`、opencode→`AGENTS.md`） | 模組刪掉 → **之後沒有任何程式碼會碰它** |
| `~/.codex/config.toml` 的 `[mcp_servers]` | `mcp/apply.rs` | 不再管理；接管功能仍會保留它（`tools/tests/codex.rs` 有斷言在盯） |
| 各工具 `skills/` 目錄 | `skills/sync.rs` | 連結與母本都留著 |
| `~/.codex/config.toml` 的 `model`／`model_provider` | **`tools/`（接管／還原）** ← 不同功能，不在移除清單 | 照舊會寫 |

**關鍵**：grep 過整個 `tools/`，它**從頭到尾沒有寫過任何 `.md` 檔**（寫入只有
`config.toml`、`settings.json`、`opencode.json`、`auth.json`）。`AGENTS.md` 是提示詞
功能的專屬領地，模組一刪就沒人管它。而且本 App 沒有任何「卸載／清理」機制，
移除 = 刪我們自己的程式碼，不會去刪使用者的檔案。

**「直接移除」實際刪掉的只有這個 App 自己的 `app.db`**：六張表（`mcp_servers`、
`mcp_bindings`、`prompt_presets`、`skill_repos`、`skills`、`skill_bindings`）。
使用者的檔案一個都不動。

#### 規模

| 類別 | 檔案 | 行數 |
|---|---|---|
| Rust 模組 `mcp/` `prompt/` `skills/` `sessions/` | 20 | 5,822 |
| Rust 命令 `commands/{mcp,prompt,skills,sessions}_cmd.rs` | 4 | 641 |
| 前端頁面 ＋ 子元件 | 8 | 2,098 |
| **合計** | **32** | **約 8,561** |

原始碼檔數 317 → **285**。

#### 連帶要改的地方（不改會編不過）

| 位置 | 改什麼 |
|---|---|
| `lib.rs` | 4 個 `mod` ＋ `generate_handler!` 裡 **40 個命令** |
| `commands/mod.rs` | 4 個 `mod` ＋ 4 個 `pub use` |
| **`deeplink/parse.rs`** | 唯一的跨模組耦合：移除 `parse_prompt/parse_mcp/parse_skill`、`ImportRequest` 的三個變體、`crate::mcp::McpInput`／`crate::skills::validate_repo` 的引用、`b64_decode`（只服務被移除的資源，留著會變 dead code） |
| `deeplink/apply.rs` | `apply_prompt/apply_mcp/apply_skill` ＋ 3 個測試 |
| `deeplink/parse/tests.rs` | cc-switch 的 MCP／prompt／skill 範例測試 → 改成**反向斷言** |
| `deeplink/mod.rs` | 測試裡的範例連結從 `resource=skill` 換成 provider |
| `DeeplinkDialog.tsx`、`types/platform.ts` | 資源聯集改註解；匯入後只 invalidate `providers` |
| `api/panels.ts`、`api/platform.ts`、`types/extensions.ts` | 約 40 個 `invoke` 與型別 |
| `App.tsx` | 4 個 import ＋ 4 個 `TABS` ＋ 4 個 render 分支；`Tab` 型別 |
| `i18nDict.ts` | `nav.mcp/prompts/skills/sessions` ＋ 「一鍵匯入來源／提示詞／MCP／技能」那句 |

#### 資料庫：schema v13 → v14（DROP 六張表）

`SCHEMA` 不再建立這六張表；`open_and_ensure` 加一段冪等的
`DROP TABLE IF EXISTS`，新舊庫都安全（舊庫清掉殘留、新庫本來就沒有）。
`idx_prompt_app` 隨表一起消失。

**動工前先備份**：`<app_data>/db-backups/manual-v13-before-removing-4-features-20261002-015259.db`
（2,179,072 bytes，以 `sqlite3` 的 backup API 產生，WAL 一致）。
備份當下的內容：`prompt_presets` 1 列、`skill_repos` 1 列、其餘四張 0 列。

#### 施工前的安全界線（不誤刪同名東西）

| 看起來像、但**不能動** | 為什麼 |
|---|---|
| `tools/tests/codex.rs` 裡的 `[mcp_servers]` | 那是「接管時必須保留使用者無關段落」的測試 |
| `history/` 的「會話」 | 歷史回填掃描到的 session 數（OpenCode／DSH），與會話頁無關 |
| `apiTypesPricing.ts` 的 `sessions: number` | 同上（歷史回填的統計欄位） |
| `providers_io`（匯出／匯入／複製） | 只碰 providers／models／pricing，本來就不受影響 |
| `tools::TAKEOVER_APPS` | 移除方向是「這三個模組不再用它」，接管本身不動 |

#### Deep Link 的舊連結（選項 C：直接移除）

`resource=mcp|prompt|skill` 的解析整段拿掉，不再保留相容層；舊連結會拿到
`不支援的 resource：mcp（目前只支援 provider）` —— 明確失敗而不是靜默忽略。
`parse/tests.rs` 新增 `removed_resources_are_rejected_with_a_readable_reason`
把三種資源都釘住（原因要包含資源名與「只支援 provider」）。

---

### 0.9.37 移除 Deep Link 匯入／資料庫備份／更新／雲端同步（2026-10-02）

依使用者指示移除這四項。規模：**Rust 23 檔刪除**（模組 19 檔 ＋ 命令 4 檔）、
**前端 5 個元件檔刪除**、約 12 個檔案清理引用、約 80 條 i18n 字典刪除。

#### 動工前先回答：會不會刪到使用者的資料？

| 問題 | 結論 |
|---|---|
| 這四個功能有專屬資料表嗎？ | **沒有。** 狀態一律塞在通用的 `settings` key-value 表，所以沒有 `DROP TABLE` 可做（與 v14 相反） |
| 會動到核心資料嗎？ | 不會。`providers`／`request_logs`／`local_keys`／`provider_models` 一列不碰 |
| 有憑證外洩的風險嗎？ | **有。** `cloud_password` 是**明文的 WebDAV 密碼** —— 功能拿掉但憑證留在 app.db 裡就是資訊留存，所以一定要清 |

#### 資料庫：schema v14 → v15（清 settings 的 17 個孤兒 key）

新增 `open::purge_removed_feature_settings`：冪等的
`DELETE FROM settings WHERE key IN (…)`，只點名 17 個 key（`cloud_*` 12 個、
`db_backup_*` 2 個、`update_*` 3 個）。**刻意不是 `DELETE FROM settings`** ——
那會把 theme／lang／accent／gateway_port／close_action 等天天在用的設定一起刪掉。
老庫有就清、沒有就命中 0 列不報錯，對新庫也安全。

**動工前先備份**：`_backup/manual-v14-before-removing-4-features-20261002-025749.db`
（2,183,168 bytes，以 Python 的 `sqlite3` backup API 產生，WAL 一致 —— 當下 App
正在執行、主檔 2.07 MB 但 WAL 有 3.77 MB，檔案複製會漏資料）。
備份當下：`schema_version` 14、`providers` 7／`request_logs` 5,874／
`local_keys` 2／`provider_models` 26。

> 實機查證：這台機器的 `settings` 只有 7 列，全是無關設定（accent、auto_start_proxy、
> close_action、gateway_port、lang、seed_version、theme）—— **那 17 個待清 key
> 一個都不存在**，因為從沒設定過雲端同步／備份排程／更新網址。清理語句仍保留
> （保護其他機器與未來設定）。

#### 施工前的安全界線（不誤刪同名東西）

| 看起來像、但**不能動** | 為什麼 |
|---|---|
| **`tools/backup.rs`** | 這是「接管工具設定」的 baseline 備份（寫進 `backups/<app>/`），跟「資料庫備份」完全無關 —— 名字像，功能不同 |
| `fsutil.rs` 的 `rotate_backups`／`backup_text`／`unique_backup_name` | 被 `tools/backup.rs` 使用 |
| `window.rs` 輕量模式 | 被 `tray.rs` 與 `commands/window_cmd.rs` 共用；deeplink 只是其中一個呼叫者 |
| `db::DEFAULT_GATEWAY_PORT` | deeplink 用它判斷「已有實例」，但 `db/` 與 `commands/settings.rs` 也用 |
| `reqwest` | proxy／quota／price_extract／provider_check 都還在用 |

#### 連帶要改的地方

| 位置 | 改什麼 |
|---|---|
| `lib.rs` | 4 個 `mod`、`deeplink_startup()` 整支函式、setup 裡的 `manage(DeeplinkState)`／`spawn_watcher`／`spawn_scheduler`、**22 個 `generate_handler!` 註冊** |
| `commands/mod.rs` | 4 個 `mod` ＋ 4 個 `pub use` |
| `Cargo.toml` | 刪 `base64 = "0.22"` —— v14 移除 mcp/prompt/skill 後就已經沒人用了，是死相依 |
| `Settings.tsx` | 4 個 import ＋ 4 個 JSX；檔頭註解同步 |
| `App.tsx` | `DeeplinkDialog` import ＋ 全域掛載（原本掛在 `TraySwitchListener` 旁邊） |
| `api/platform.ts` | **22 個 `invoke`** ＋ 12 個 type import |
| `types/platform.ts` | `Deeplink*` 3 個、`DbBackup*` 3 個、`WebdavConfig`、`Cloud*` 3 個、`Update*` 2 個（`LocalKey`／`AppSettings`／`Usage*` 全部保留） |
| `i18nDict.ts` | `settings.backup.*` 26 條、`settings.cloud.*` 34 條、`settings.update.*` 15 條、`settings.deeplink.*` 8 條 |

> `i18nDict.ts` 施工中踩過兩個坑，兩次都是「刪一大段順手刪到／漏掉鄰居」：
> 1. 刪 `settings.update.*` 那段時把 `settings.export.range.today`／`.7d` 一起帶走了
>    （`.30d` 留在原位所以看起來沒少）。已補回。
> 2. 刪 `settings.deeplink.*` 時拿 `settings.cloud.auto.off` 當錨點，結果**整段
>    `settings.cloud.*`（34 條）整個沒刪掉** —— 因為 `tsc` 抓不到（字典 key 是
>    字串不是型別，多一條不會報錯），是收尾 grep 原始 key 才發現的。
>
> **教訓**：刪字典之後一定要用 grep 對**原始 key 前綴**再掃一次，
> 不能只看編譯過不過。

#### 閘門

| 閘門 | 結果 |
|---|---|
| `cargo test --offline --lib` | ✅ **318 passed / 0 failed / 8 ignored**（原 351／0／8；淨減 33 = 移除 34 個模組自帶測試 − 新增 1 條 v15 斷言） |
| `cargo clippy --offline --all-targets -- -D warnings` | ✅ 0 warning |
| `npx tsc --noEmit` | ✅ exit 0 |
| `cargo build --release --offline` | ✅ exit 0 |
| `pnpm tauri build` | ⚠️ **本輪無法執行** —— 不是程式問題：sandbox 擋掉 Node 的 `child_process.exec`（named pipe），Vite 在 `optimizeSafeRealPathSync` 階段 `spawn EPERM`，連 tailwindcss oxide 的原生模組都載不起來。需在沙箱外跑一次補驗 |

新增測試 `db::tests::removal::v14_feature_settings_are_purged_and_live_settings_survive`：
兩個方向都釘住 —— 17 個 key 消失，且 `theme`／`lang`／`accent`／`close_action`／
`gateway_port` 的**值一字未變**。

#### 實機驗證

- `reg delete HKCU\Software\Classes\tokengateway /f` —— 解除 `tokengateway://` 協定註冊，
  否則之後點連結還是會開啟這個 App，只是參數被忽略。
- 待補：真實庫升到 v15、`settings` 僅剩 7 列、四項區塊從設定頁消失、側邊欄不變。

---

### 0.9.36 真相：不是計算錯，是「已用 vs 剩餘」被讀反了（2026-10-01）

#### 回報

> 官網顯示 5 小時額度**已經 100% 用滿**、週額度已使用 65%、月額度已使用 66%；
> 但本機顯示已使用 0% / 35% / 34% —— 「差异巨大」。

#### 查證一：兩組數字是**精確的補數**

| 窗口 | 使用者看官網 | App 顯示 | 相加 |
|---|---|---|---|
| 5 小時 | 100 | 0 | 100 ✅ |
| 週 | 65 | 35 | 100 ✅ |
| 月 | 66 | 34 | 100 ✅ |

#### 查證二：官網的標籤是「**% left**」，不是「已使用」

截圖 OCR 逐字（原圖 + 4× 放大各跑一次，結果一致）：

```
Rolling   Resets in 0h 31m      97 % left
Weekly    Resets in 3d 15h      65 % left
Monthly   Ends in 21d 0h        66 % left
```

#### 查證三：App 的金鑰直接打上游，數字與 App 逐字相同

```
GET https://opencode.ai/zen/go/v1/usage          HTTP 200
{"usage":{"rolling":{"status":"ok","percent":0, "resetsAt":"2026-10-01T14:07:49.338Z"},
          "weekly" :{"status":"ok","percent":35,"resetsAt":"2026-10-05T00:00:00.000Z"},
          "monthly":{"status":"ok","percent":34,"resetsAt":"2026-10-22T09:24:27.000Z"}}}
```

#### 查證四：截圖與 App 是**同一個訂閱、同一組窗口**

| 窗口 | 截圖（16:36）說的重置 | 上游 `resetsAt` 換成本地時間 | 對照 |
|---|---|---|---|
| rolling | `0h 31m` → 17:07 | `14:07:49Z` ＝ 22:07:49（**同一個視窗端點**，17:07:49 起算 5 小時） | ✅ |
| weekly | `3d 15h` → 10-05 07:36 | `2026-10-05T00:00:00Z` ＝ 本地 08:00 | ✅ |
| monthly | `21d 0h` → 10-22 16:36 | `2026-10-22T09:24:27Z` ＝ 本地 17:24 | ✅ |

所以「查錯帳號／查錯窗口」都可以排除。

#### 查證五：方向判定 —— `percent` 是「已用」

關鍵在 rolling：13:40 的稽核讀數與 16:36 的後台截圖**落在同一個 rolling 視窗內**
（兩邊的倒數都指向 17:07:49 重置）。

| 假設 | 13:40 App 顯示 3.0% | 16:36 後台顯示 97 % left | 同一視窗內合不合理 |
|---|---|---|---|
| `percent` ＝ 剩餘 | 剩 3% | 剩 97% | ❌ 中間沒有重置，不可能從 3% 變 97% |
| `percent` ＝ 已用 | 用 3% | 剩 97% | ✅ 完全一致 |

→ **`percent` ＝ 已使用百分比**。App 的數字全部正確。

#### 結論

**App 對、官網也對，兩者其實完全一致**；差別只在官網標「剩餘 % left」、App 標「已用 %」。
使用者把官網的 `100 % left / 65 % left / 66 % left` 讀成「已用 100% / 65% / 66%」，
再對上 App 的 `0% / 35% / 34%`，就成了「差异巨大」。

#### 修法：兩邊的數字都寫出來

原本只在標題列放一行小字提示「圓環＝已用 %」（太不明顯，不足以阻止誤讀）。
現在圓環內**同時**顯示兩個數字：

```
      ╭───────╮
      │  0.0% │   ← 大字＝已用（圓弧也畫這個）
      │ 剩100%│   ← 小字＝剩餘（官網 console 顯示的那個）
      ╰───────╯
       5 小時
      重置 1時6分
```

標題列也改成「圓環＝已用 %｜小字＝剩餘 %」並附 tooltip 舉例。
這樣不管使用者看哪一邊，都能直接對上，不必心算補數。

#### 順手：窗口一重置就立刻重查

實測時剛好碰上 rolling 窗口的邊界（`resetsAt` ＝ 22:07:49）：倒數正確走到
「已重置」，但因為那份資料是 22:07:36 抓的，畫面會**停在「已重置」**直到下一次
5 分鐘輪詢（22:12:36）才換成新窗口。

於是補一條**精確**的觸發條件（不是猜的）：只要任何窗口的 `resetsAt` 已經落在
過去，這份資料就**一定**不準了 —— 直接重查，不等下一個週期。
（與 watchdog 共用同一個 5 分鐘冷卻，避免上游失敗時每 30 秒打一次。）

#### 順手排掉一個誤判：金鑰不是查錯帳號，但 CLI 那把已經失效

| 來源 | 金鑰指紋 | 打 `/usage` |
|---|---|---|
| App provider #22 | `oc_sk_1c…`（sha256_12 `e32a33b0a462`） | **HTTP 200** ✅ |
| `~/.local/share/opencode/auth.json` 的 `opencode-go`（CLI 登入用） | `oc_sk_df…`（sha256_12 `cf70959a7fe9`） | **HTTP 401 Unauthorized** ❌ |

兩把是**不同的 key**。App 用的那把有效，所以不是「查錯帳號」；
但 CLI 那把已經被撤銷／過期，需要的話要重新產生。

> 未解的小疑點：截圖 rolling 列 x≈475 有一個孤立的「0」字（OCR 兩次都只讀到單一字元），
> 與三個窗口的百分比與重置都對不上，判斷是圖表軸標或裁切殘留，**不是 App 少抓的欄位**。

---

### 0.9.35 訂閱額度：把「讀數時刻」從四個倒數反推出來（2026-10-01）

延續 §0.9.34（使用者再次回報「不匹配」，要查問題在哪）。

#### 反推法：倒數字本身就是一個時間戳

`fmtRemain` 是在 render 時用 `Date.now()` 算的，所以畫面上那串「重置 3時27分」
**同時記錄了那次 render 的時刻**。把稽核紀錄裡的四個倒數、配上直接量到的上游
`resetsAt`，就能反推出「這份畫面是什麼時候畫的」：

| 窗口 | App 畫面（稽核紀錄原文） | 上游 `resetsAt`（本地時間） | 由倒數反推的 render 時刻 |
|---|---|---|---|
| rolling | 重置 3時27分 | 2026-10-01 17:07:49 | **(13:39:49, 13:40:49]** |
| weekly | 重置 3天18時 | 2026-10-05 08:00:00 | (13:00:00, 14:00:00] |
| monthly | 重置 21天3時 | 2026-10-22 17:24:27 | (13:24:27, 14:24:27] |
| ChatGPT | 重置 27天13時 | 2026-10-29 03:22:42 | (13:22:42, 14:22:42] |

四個窗口的交集 → **13:39:49–13:40:49**。使用者的後台截圖是 **16:36:16**，
中間差了 **2 小時 55 分**。

關鍵在於：**百分比看起來還「對」只是因為那三小時用量沒變**（3／35／34）。
而 App 的 rolling「3時27分」（→ 17:07 重置）與後台 16:36 的「Resets in 0h 31m」
（→ **也是** 17:07 重置）其實指向同一個重置時刻 —— App 的顯示只是停在舊的計算基準。

#### 這次補的三件事

| # | 修法 | 為什麼 |
|---|---|---|
| d | 標題列加「**圓環＝已用 %**」＋ tooltip 說明補數關係 | 後台寫 `97 % left`、App 寫 `3.0%` —— 這是最容易被讀成「計算不匹配」的地方。兩者相加剛好 100 |
| e | 資料超過 1.5 個輪詢週期未更新 → **琥珀色 ＋「可能已過期」** | 「更新於 21:01:00」長得像「現在」；不標示就沒人看得出它過期 |
| f | 心跳當 **watchdog**：資料比 2 個週期舊就強制重查（冷卻 1 個週期） | 不管輪詢因為什麼原因停掉，最壞也只會舊約 10 分鐘 |

#### 實測：三種「看不見視窗」都不會讓輪詢停（但還是加了保底）

| 狀態 | `document.visibilityState` | 輪詢 | 心跳 |
|---|---|---|---|
| 全螢幕黑色視窗**完全蓋住** | `visible` | ✅ 20:27:15 → 20:32:17 | ✅ 1時40分 → 1時33分 |
| 最小化 | `visible` | ✅ 18:32:38 → 18:37:40 → 18:42:41 | — |
| 縮到系統匣（`CloseMainWindow`） | `visible` | ✅ 18:42:41 → 18:47:43 → 18:52:44 | — |
| 蓋住後掀開（恢復焦點） | `visible`，焦點回復 | ✅ 立刻重查（20:35:12） | — |

> 有趣的環境事實：這台機器的 WebView2 **不做遮擋判定**，Tauri 的 `hide()` 也不送
> `visibilitychange`，所以 `visibilityState` 幾乎永遠是 `visible`。

**誠實說明**：13:40 之後「2 小時 55 分沒有任何 re-render」的**觸發原因沒有重現**
（電力事件記錄也只有 15:04:46 → 15:04:47 一次 1 秒的睡眠；沒有多小時的休眠）。
沒有重現就不假裝修好 —— 改用 (d)(e)(f)：萬一又凍住，一定看得出來（琥珀色 ＋ 已過多久），
而且會自己救回來（watchdog）。**其中 (f) 的觸發條件正是那個重現不了的狀態，
所以只以程式邏輯與冷卻設計交代，不宣稱做過端對端驗證。**

#### 順手查清 ChatGPT 那張卡（0.0% 不是 bug）

直接打 `GET https://chatgpt.com/backend-api/wham/usage`（唯讀）：

```json
{"plan_type": "free",
 "rate_limit": {"primary_window": {"used_percent": 0,
                                   "limit_window_seconds": 2592000,
                                   "reset_at": 1793215362},
                "secondary_window": null}}
```

`limit_window_seconds = 2592000` ＝ **30 天**、`used_percent = 0`、`secondary_window = null`。
access token 的 JWT claim 也寫著 `chatgpt_plan_type: "free"`。
所以卡片上的「ChatGPT 0.0% / 30 天」**是對的** —— free 方案本來就沒有 5 小時／7 天
那兩格可算，不是漏抓。`reset_at` 換算後與卡片顯示的「重置 27天13時」也吻合。

---

### 0.9.34 訂閱額度的「即時更新」（2026-10-01）

#### 回報的問題

> OpenCodeGo 與 ChatGPT 訂閱版本的「訂閱額度計算」似乎沒有即時更新，
> 用量頁顯示與 opencode.ai 後台頁對不上（附截圖）。

#### 先量測，再改（結論：百分比與倒數的「計算」是對的，差的是顯示層）

截圖（2026-10-01 16:36，opencode.ai 後台）與 App 畫面逐格對照：

| 窗口 | 後台頁 | App（當時畫面） | 對照 |
|---|---|---|---|
| Rolling | `97 % left` / `Resets in 0h 31m` | 3.0% / 重置 3時27分 | `100 − 97 = 3` ✅ |
| Weekly | `65 % left` / `Resets in 3d 15h` | 35% / 重置 3天18時 | `100 − 65 = 35` ✅ |
| Monthly | `66 % left` / `Ends in 21d 0h` | 34% / 重置 21天3時 | `100 − 66 = 34` ✅ |

三個窗口**同時**差約 3 小時 —— 百分比吻合、倒數整段落後，指向「這份資料是 3 小前查的」。
為了確認是解析錯還是顯示舊，直接打一次上游（唯讀 `GET {base}/usage`，不印金鑰）：

| 取樣時刻 | rolling | weekly | monthly |
|---|---|---|---|
| 18:39:47 | `percent=0` `resetsAt=2026-10-01T14:07:49.338Z` 剩 **3時28分** | `35` `2026-10-05T00:00:00Z` 剩 **3天13時20分** | `34` `2026-10-22T09:24:27Z` 剩 **20天22時44分** |
| 18:42:18 | 剩 **3時25分** | 剩 **3天13時17分** | 剩 **20天22時42分** |

App 當下顯示 35% / 3天13時 / 34% / 20天22時 —— **與上游逐字吻合**，`parse_opencode_windows`
的百分比與 `resets_at` 換算沒有錯。

#### 量到的三個真的缺陷

| # | 缺陷 | 證據 | 修法 |
|---|---|---|---|
| a | **倒數沒有心跳**：倒數字是 render 時用 `Date.now()` 算出來的，但沒有任何東西定期 re-render → 兩次輪詢之間畫面一字不動，看起來就是「沒更新」 | 舊 build 實測 18:34:39 → 18:40:41 整段凍結 | 30 秒心跳（`useNow`），倒數與「更新於」自己走 |
| b | **「更新於 HH:MM:SS」看不出陳舊度**：一個時鐘長得像「現在」 | 截圖裡三窗口同時落後 3 小時，畫面上完全看不出來 | 追加相對時間：`更新於 19:00:36（3 分鐘前）` |
| c | **0% 的窗口把重置時間藏起來**：`parse_opencode_windows` 原本有 `percent > 0.0` 才給 `resets_at` | 上游在 0% 仍回有意義的 `resetsAt`（22:07:49，還在 3時28分後），後台頁在 0% 也照樣顯示倒數；舊條件只讓「剛重置完」那段的卡片變成「—」，像壞掉 | 拿掉條件；單測同步改寫，並補一條「上游沒給 `resetsAt` 就顯示 —」 |

#### 依 cc-switch 原始碼對齊（不是憑感覺調）

| cc-switch | 原文 | 我們原本 |
|---|---|---|
| `src/lib/query/subscription.ts` | `refetchIntervalInBackground: Boolean(refetchInterval)`、`refetchOnWindowFocus: Boolean(refetchInterval)`、`staleTime` ＝ 間隔 | 只有 `refetchInterval` ＋ `staleTime` |
| `src/lib/query/queries.ts`（`useUsageQuery`） | `refetchIntervalInBackground: true, // 后台也继续定时查询` | 沒有 |
| `src/components/SubscriptionQuotaFooter.tsx` | `setInterval(() => setNow(Date.now()), 30000)` | 沒有心跳 |

> `@tanstack/query-core@5.102.8` `queryObserver.js:163`：
> `if (this.options.refetchIntervalInBackground || focusManager.isFocused()) this.#executeFetch();`
> —— 預設 `false` 就是一顆隱性地雷：**視窗一旦 `hidden`，整段輪詢停擺**。

#### 誠實說明：什麼重現了、什麼沒有

- **重現並修掉**：畫面「看起來不更新」的三個原因（a/b/c）。修完倒數每 30 秒自己走、
  陳舊度直接寫在標題列上、0% 窗口也看得到重置時間。
- **沒有重現**：3 小時的陳舊。已安裝版本在**三種視窗狀態**下輪詢都正常，每 5 分更新一次：

| 視窗狀態 | `document.visibilityState` | 「更新於」 |
|---|---|---|
| 前景、有焦點 | `visible` | 18:32:38 → 18:37:40 → 18:42:41 → 18:47:43 |
| 前景、無焦點（最小化） | **`visible`** | 同上，繼續跑 |
| 縮到系統匣（`CloseMainWindow`，行程仍活著） | **`visible`** | 18:42:41 → 18:47:43 → 18:52:44 |

  所以本機 WebView2 上「視窗藏起來 → hidden」這條路徑並不會發生。
- **但那顆地雷還是要拔**：react-query 的焦點重取只掛在 `visibilitychange`，而 Tauri 的
  `hide()`／`show()` 不保證送得到那個事件（本機實測 `visibilityState` 一直是 `visible`）。
  因此另外在視窗重新取得焦點時（DOM `focus` ＋ Tauri `tauri://focus`）直接
  `invalidateQueries(["quota_all"])`：從匣裡叫回來時第一眼就是新數字。

#### 實機驗證（新 build）

| 驗證項 | 結果 |
|---|---|
| 5 小時窗口在 `0.0%` | ✅ 顯示「重置 3時7分」（舊版是「—」）；19:00:46 直接打上游算出的剩餘是 **3時07分**，吻合 |
| **30 秒心跳** | ✅ 19:00:59 `3時7分` → 19:01:19 `3時6分` → 19:02:20 `3時5分` → 19:03:20 `3時4分`，而「更新於」**全程維持 19:00:36**（證明是本地計時、沒有偷偷重打上游） |
| 陳舊度顯示 | ✅ `更新於 19:00:36（1 分鐘前）` → `（2 分鐘前）` → `（3 分鐘前）` 跟著心跳走 |
| 5 分鐘輪詢 | ✅ 仍在跑（19:05:36 換新） |
| 回到焦點立刻重查 | ✅ 最小化 → 還原後「更新於」立刻換成當下時刻 |
| 單元測試 | `opencode_windows_parsed_and_zero_percent_keeps_reset`（0% 保留重置時間 ＋ 沒給就不亂補） |

---

### 0.9.33 更新檢查（P4.9 / D7，2026-10-01）

#### cc-switch 的實際行為（照它手冊 §1.5 的 About 頁抄）

版本資訊、**release notes**、**檢查更新**、下載並安裝新版本。

#### 我們做到哪裡（誠實說明，含為什麼）

| 項目 | 狀態 | 理由 |
|---|---|---|
| 顯示目前版本 | ✅ | 編譯期 `CARGO_PKG_VERSION` |
| **檢查更新** | ✅ | 抓一份 **Tauri v2 靜態更新 manifest**（JSON），比版本、顯示 release notes、發布日期 |
| 下載連結 | ✅ | 有對應平台的檔案就給連結（用既有的 opener 外掛開啟瀏覽器） |
| **自動安裝** | ❌ **不做** | Tauri 的自動安裝需要「**簽章過的更新檔 ＋ 對應公鑰**」，而 `tauri-plugin-updater` 不在這個離線環境的 crate 快取裡、也還沒有發佈管道。與其塞一個不能驗證的殼子，不如把「檢查」做完整、把「安裝」明確留給使用者 —— 矩陣上標明 |

manifest 網址是**設定項**（`settings.update_manifest_url`）：沒有發佈管道之前它就是空的，
等你有 releases 頁（例如 GitHub Releases ＋ 一個 `latest.json`）填進去就會生效。

#### 設計上值得一提的幾點

- **版本比較**自己寫（容忍 `v` 前綴、段數不同、**不比 pre-release**）——
  不為了這個功能拉一個 semver 相依。
- **平台對應**：manifest 的 key 是 Tauri 的寫法（`windows-x86_64`／`darwin-aarch64`…），
  對不上時**大小寫不拘**再試一次；再對不上就明說「manifest 沒有這個平台的檔案」，
  而不是給一個壞掉的連結。
- **沒簽章要講清楚**：結果卡上直接寫「這個更新沒有簽章，無法自動安裝」。
- manifest **缺 `version`** 時給人話（「更新資訊裡沒有 version」）而不是 serde 的
  `missing field`（所以 `version` 有 `#[serde(default)]`）。

#### 測試（7 個新測試）

版本比較（大小、v 前綴、段數、pre-release、數字不是字串比較）、平台 key 命名、
manifest 解析、**壞 manifest 給人話**（不是 JSON／缺 version／空白 version）、
有新版時的連結與簽章旗標、同版本 → 已是最新、有新版但沒這個平台 → 明說沒有、
沒簽章 → 訊息裡講清楚、設定存取來回。

#### 實機驗證：對一個**真的** manifest 伺服器

自己寫了最小靜態伺服器（`.workbuddy/tmp/update_test_server.py`）提供 `/latest.json`：

| 驗證項 | 結果 |
|---|---|
| 區塊與設定 | ✅ 設定頁「更新」區塊：更新資訊網址 ＋ 儲存 ＋ 檢查更新 ＋「目前版本 0.1.0」 |
| **有新版（0.99.0）** | ✅ 顯示「有新版：0.99.0」、「發布於 2026-10-01T00:00:00Z」、**release notes 三行都正確渲染**、「開啟下載頁」按鈕、「這個更新沒有簽章，無法自動安裝」 |
| **已是最新（0.1.0）** | ✅ 把 manifest 改成同版本再檢查 → 「已是最新」 |
| 上次檢查記錄 | ✅ 寫進 `settings.update_last_check_ms` ＋ 結果摘要，設定頁會顯示 |
| 驗證後清理 | ✅ 停掉測試伺服器、刪掉測試資料、清空 `update_*` 設定（7 個來源、schema 13 完好） |

> 順手修掉一個**真的 bug**：`skills_set_settings` 這支命令忘了註冊到
> `generate_handler!`，所以技能頁改「儲存位置／同步方式」在執行期會找不到命令
> （編譯與單元測試都抓不到，是 clippy 的 unused 警告揭露的）。

---

### 0.9.32 多語系基礎建設（P4.11，2026-10-01）

#### cc-switch 的實際行為（照它手冊 §1.5 抄）

- 四種語言：簡中／繁中／英文／日文，**切換即時生效、不用重啟**。
- 設定在左上角的齒輪（設定頁）。

#### 我們做的（`lib/i18n.tsx` ＋ `lib/i18nDict.ts`）

| 部分 | 內容 |
|---|---|
| 語言 | **繁中／英文兩種**。簡中與繁中差在用詞、日文需要母語校對 —— 沒有把握的翻譯比沒有翻譯更糟，矩陣上標明 |
| 字典 | `key → [繁中, English]`，**繁中是 source language**；`t()` 找不到 key 就回繁中原文，不會出現「找不到字」 |
| 佔位 | `{name}` 形式（用 split/join 取代 `replaceAll`，因為 target 是 ES2020） |
| 即時切換 | `I18nProvider` 持有語言狀態；設定頁的語言按鈕直接 `setLang` ＋ 通知後端 —— **不用重啟** |
| 真相源 | 後端 `settings.lang`（唯一真相），`appearance_state` 帶回來；localStorage 只做首幀快取避免閃爍 |
| `<html lang>` | 會跟著更新（無障礙與瀏覽器翻譯提示都用得到） |

#### 這輪翻譯的範圍（誠實記錄）

| 範圍 | 狀態 |
|---|---|
| 導覽（10 項）＋ 頂列（啟動／停止） | ✅ 100% |
| **設定頁全部**（網關／外觀／用量匯出／關於，含確認框與提示） | ✅ **100%**（2026-10-02 移除資料庫備份／雲端同步／Deep Link 三個區塊後仍維持） |
| 其餘頁面（用量各鏡頭、上游來源、MCP、提示詞、技能、會話、Key、診斷、試算） | ⬜ 未翻譯（切英文時這些頁面仍是繁中 —— 這是覆蓋率推進中的正常狀態） |
| 數字 | 字典 **65 個 key**（2026-10-02 移除四個功能後由 145 條降下來）；以腳本量測（`.workbuddy/tmp/i18n_cover.py`）全站尚有 **751 條**中文字串未走字典 → 覆蓋率約 **15%** |

覆蓋率是**用腳本量的，不是估算**；每輪繼續往上推。

#### 實機驗證

| 驗證項 | 結果 |
|---|---|
| 初始語言 | ✅ `<html lang="zh-TW">`，導覽全繁中（用量／上游來源／MCP／提示詞／技能／會話／本地 Key／診斷／試算／設定） |
| **切到 English** | ✅ **不重啟**：導覽變成 Usage／Providers／MCP／Prompts／Skills／Sessions／Local Keys／Diagnostics／Calculator／Settings；設定區塊變 Gateway／Appearance／Usage export／Database backups／Cloud sync／About；主題列顯示「Theme / Follow system / Light / Dark / Current: Dark」；語言列顯示「Language / 繁體中文 / English / Interface language (applies immediately, no restart)」 |
| 切回繁中 | ✅ 導覽與 `<html lang>` 都回來 |
| 持久化 | ✅ 後端 `settings.lang` 寫成 `zh-TW`（資料庫是唯一真相） |

#### 測試

本輪的語言邏輯（字典、佔位替換、合法值）由 tsc 與實機驗證把關；
字典 key 的完整性靠**建置時的 tsc**（`t()` 的參數型別是 `string`，
拼錯 key 不會報錯 → 這是刻意留下的缺口，未翻譯處會回繁中原文，
所以錯字不會讓畫面變空白，只是沒翻譯）。

---

### 0.9.31 雲端同步（WebDAV，P4.10，2026-10-01）

#### cc-switch 的實際行為（照它手冊 §1.5 抄）

- 用 **WebDAV** 或 S3 同步設定（**同時只能開一種**）。
- 設定項：伺服器網址、帳號、密碼、**遠端根目錄**（預設 `cc-switch-sync`）、
  **設定檔名稱**（預設 `default`）、**自動同步**。
- 操作：**測試連線**、**上傳**（覆蓋遠端）、**下載**（覆蓋本地，
  **下載前顯示遠端快照資訊，且下載前一定先自動備份本地資料庫**）。

#### 我們做的（`cloudsync/`，沒有新增資料表）

| 決定 | 理由 |
|---|---|
| **只做 WebDAV**，不做 S3 | S3 要自簽 SigV4，而且我們沒有可驗證的 S3 端點 —— 寧缺勿假，矩陣上標明未做 |
| 遠端佈局 `<根>/<設定檔>/tokens.db` ＋ `manifest.json` | manifest 放協定版本、schema 版本、App 版本、時間、大小 → **下載前能先顯示給使用者看**（cc-switch 的「Remote Snapshot Info」）；協定版本不符就拒絕下載 |
| 只用四個動詞 | `PROPFIND`（測試連線）、`MKCOL`（建目錄）、`PUT`（上傳）、`GET`（下載）；已存在回 405 視為成功 |
| **下載前先做本地安全備份** | 沿用 `dbbackup::create`；之後用 SQLite 線上備份 API 寫回**現有連線**（與還原同一條路徑，不必重啟 App） |
| 下載分兩步 | `fetch_remote_db`（async、不碰 DB）→ `apply_remote_db`（同步、拿鎖）—— `MutexGuard` 帶過 `.await` 會讓 async 命令不成立（編譯器會說 future 不能跨執行緒） |
| 自動同步 | 背景執行緒每分鐘比對本地資料庫的 (mtime, 大小) 與上次上傳的，有變才上傳。**與 cc-switch 的「每次資料庫變更立刻上傳」不同**：我們沒有在每個寫入點掛鉤，改成一分鐘內的反應（文件標明） |
| 密碼 | 存在 `settings`（與 cc-switch 存在自己資料庫裡相同）；**UI 只顯示遮罩**、輸入框是 `type=password`、驗證時不回傳明文 |

#### 實機驗證：對一個**真的** WebDAV 伺服器

沒有現成的 WebDAV 端點可測，所以自己寫了一個最小的（`.workbuddy/tmp/webdav_test_server.py`：
PROPFIND／MKCOL／PUT／GET ＋ Basic 認證，只用於驗證）。驗證流程與結果：

| 驗證項 | 結果 |
|---|---|
| 區塊與欄位 | ✅ 設定頁「雲端同步」：伺服器／帳號／密碼（`type=password`）／遠端目錄／設定檔名稱 |
| 儲存設定 | ✅「已儲存雲端同步設定」 |
| **測試連線** | ✅「連線成功（HTTP 404）；遠端目錄尚未建立，上傳時會自動建立」（目錄不存在也算連得上 ✓） |
| **上傳** | ✅「已上傳（2166784 bytes，涵蓋遠端舊資料）」；伺服器 log 依序看到 `MKCOL /tg-sync-test → 201`、`MKCOL /tg-sync-test/default → 201`、`PUT tokens.db → 201 (2166784)`、`PUT manifest.json → 201 (167)` |
| **看遠端快照** | ✅「協定 tokengateway-sync/1｜schema v13｜App 0.1.0｜2026/10/1 13:00｜2.07 MB」 |
| **下載** | ✅「已下載並套用（schema v13）；下載前的本地資料庫已備份為 db_backup_20261001_130011」；伺服器 log 看到兩次 `GET manifest.json` ＋ `GET tokens.db → 200` |
| **認證失敗** | ✅ 故意改錯密碼 → 「認證失敗（帳號或密碼不對；Nextcloud／堅果雲要填應用程式密碼）」 |
| 驗證後清理 | ✅ 停掉測試伺服器、刪掉測試資料、把 `cloud_*` 設定全部清空（使用者機器不會殘留指向測試伺服器的設定）；7 個來源、schema 13 完好 |

#### 測試（6 個新測試）

設定正規化與拒絕（http(s) 檢查、帳號必填、路徑不可 `..`、空值用預設）、
自動同步的觸發條件（從沒上傳過／無變化／時間變／大小變／讀不到本地）、
密碼遮罩、設定來回（**密碼留空＝不覆蓋**）、狀態快照（待同步變更、上次結果）。

---

### 0.9.30 資料庫備份管理（P4.8，2026-10-01）

#### cc-switch 的實際行為（照它手冊 §1.5 抄）

- **自動備份間隔**：停用／6h／12h／24h／48h／7d（**預設 24 小時**）。
- **保留份數**：3／5／10／15／20／30／50（**預設 10**）；超過的舊備份自動刪除。
- **備份清單**：顯示名稱（自動命名 `db_backup_20260315_143000`）、建立時間、檔案大小。
- **操作**：立即備份、還原（**還原前一定先自動備份當前資料庫**）、改名、刪除（需確認）。

#### 我們怎麼做（`dbbackup.rs`）

| 決定 | 理由 |
|---|---|
| 用 **SQLite 線上備份 API**（`rusqlite` 的 `backup` 功能），不是複製檔案 | App 開著時直接 copy `app.db` 可能拿到「複製到一半」的檔案；線上備份走 SQLite 自己的頁面鎖，得到一致快照 |
| **還原是「寫回現有連線」** | 把檔案換掉等於在開啟中的連線底下抽換資料，後續寫入會寫進舊檔或直接鎖死。備份 API 直接寫回同一個連線，也不必叫使用者重啟 |
| **建立時間看檔案的 mtime**（名字解析只是後備） | 自己命名或改過名的備份從名字解析不出時間，只看名字會全部變 0、排序與保留策略都會亂掉（**測試 `retention_prunes_on_create` 抓到**） |
| 名稱用**本地時間** | 名稱是給人看的。第一版寫 UTC，實機驗證時名稱顯示 04:29、檔案時間卻是 12:29（差 8 小時）—— 已改成 `chrono::Local` |
| 名稱驗證 | 只允許英數與 `_ - .`、擋 `..` 與開頭點，避免路徑跳脫 |
| 排程 | 背景執行緒每 5 分鐘檢查一次「距離上次備份是否超過間隔」；從沒備份過就立刻做一次。失敗只印訊息，不影響 App |

#### 順手修掉的「檔案 ≤400 行」違規

專案規則要求每個檔案 ≤400 行，但前幾輪一路加功能之後有 6 個檔案超標（我沒有每輪稽核）。
這輪清掉四個，剩下四個留待下一輪：

| 檔案 | 之前 | 現在 | 做法 |
|---|---|---|---|
| `components/Settings.tsx` | 487 | **199** | 拆成 `settings/{SectionHead,AppearanceSection,DeeplinkSection,DbBackupSection}.tsx` |
| `components/providers/UsageQueryPanel.tsx` | 411 | **347** | 抽出 `UsageQueryParts.tsx`（抽取規格＋進階、測試結果） |
| `src-tauri/src/prompt/sync.rs` | 562 | **348** | 測試搬到 `prompt/sync/tests.rs` |
| `src-tauri/src/mcp/apply.rs` | 498 | **406→**（讀取段落搬出後） | 讀取搬到 `mcp/apply/readers.rs`（172 行） |
| **還沒清**：`index.css`（800）、`lib/apiTypes.ts`（675）、`lib/api.ts`（560）、`providers_io.rs`（598） | | | 下一輪拆（CSS 分檔、型別／API 分檔、`providers_io` 分模組） |

#### 測試（8 個新測試）

- 名稱格式（本地時間來回一致、壞名字回 0）、名稱驗證（擋路徑跳脫）、
  保留策略（留最新 N 份、至少留 1 份）、到期邏輯（停用／從未備份／剛好到／超過）。
- 建立→列出→改名→刪除來回（含「只列 .db」與找不到的錯誤）。
- **`restore_brings_the_old_data_back`**：備份 → 改資料 → 還原 → 確認改動被還原掉、
  還原前的安全備份存在且內容是「還原前」的狀態、還原後連線仍可寫。
- 預設值與 cc-switch 相同（24 小時／10 份）、亂填回預設。

#### 實機驗證（安裝後的真實 App）

| 驗證項 | 結果 |
|---|---|
| 區塊與預設值 | ✅ 設定頁「資料庫備份」，自動備份顯示 **每 24 小時**、保留 **10 份**（與 cc-switch 相同） |
| 立即備份 | ✅「已建立備份 db_backup_20261001_123640（2.07 MB）」；磁碟上確實有 `.db`（2,166,784 bytes）；清單顯示名稱／時間／大小＋還原／改名／刪除 |
| **本地時間** | ✅ 名稱 `123640` 對上當時的 12:36（修掉 UTC 的 04:29） |
| 改排程 | ✅ 選「每 6 小時」→「已更新備份排程」 |
| 改名 | ✅ 改成 `before_upgrade`，磁碟檔名與清單同步更新 |
| **還原（含安全備份）** | ✅「已還原（還原前已自動備份當前資料庫；**schema v13**）；安全備份 db_backup_20261001_123701」；還原後資料庫仍是 schema 13、7 個來源都在 |
| 刪除 | ✅「已刪除備份」，清單與磁碟同步 |
| 最終狀態 | ✅ 留下 1 份真實備份（使用者的資料庫快照），排程回到預設 24 小時／10 份 |

---

### 0.9.29 關閉行為與輕量模式（P4.7，2026-10-01）

#### cc-switch 的實際行為（照它手冊 §1.5 抄）

- **Minimize to tray on close 預設開啟**：按視窗的關閉鈕只是縮到系統匣，App 繼續跑；
  關掉這個選項才會**完全結束**。
- **Lightweight Mode**：托盤右鍵進入後**視窗被銷毀**（不只是隱藏），
  從托盤「Open main window」或 **Deep Link** 叫回來；**狀態不持久**。

#### 為什麼這個 App 特別需要它

這是**本地網關**：Claude Code／Codex／OpenCode 的流量都經過它。原本沒有攔
`CloseRequested`，所以按個 X 就結束行程 —— 使用者的工具會突然連不上上游。
預設改成縮到系統匣，與 cc-switch 一致，也符合這個 App 的用途。

| 部分 | 內容 |
|---|---|
| `window.rs` | `CloseAction`（tray／exit，預設 tray）、`show_main`（視窗不存在就**重建**）、`enter_lightweight`（銷毀視窗）、`decide_close` |
| 關閉攔截 | `on_window_event` 的 `CloseRequested`：tray → `prevent_close` ＋ `hide`；exit → `prevent_close` ＋ `app.exit(0)` |
| 托盤 | 新增「**開啟主視窗**」（會重建）與「**輕量模式（關閉視窗）**」（已在輕量模式時變灰） |
| 輕量模式的判斷 | **不另存旗標**：`get_webview_window("main")` 回 `None` 就是輕量模式 —— 少一個會與事實不同步的狀態 |
| 設定頁 | 視窗行為區：「關閉視窗時」＝縮到系統匣（建議）／直接結束；另有「進入輕量模式」按鈕 |
| Deep Link 喚醒 | 收件匣 watcher 發現視窗不存在 → 先把連結存成 pending 再重建視窗（前端掛載時取走）；視窗只是被隱藏時也會 `show_main` 叫回來 |

#### 實機驗證抓到的**三個**問題（都已修）

1. **「直接結束」其實沒結束。** 有托盤圖示時，放行 `CloseRequested` 只會關掉視窗，
   行程被托盤撐著（變成「沒有視窗的托盤模式」）。→ 明講 `app.exit(0)`。
2. **進入輕量模式把整個 App 殺掉。** Tauri 的預設是「所有視窗都沒了就結束行程」，
   所以銷毀視窗＝連網關一起關。→ 攔 `RunEvent::ExitRequested`，`code.is_none()`
   （＝因為視窗關光）時 `prevent_exit()`；真正退出走 `app.exit(0)`（`code = Some(0)`）放行。
3. **JS 的 `window.close()` 不等於按標題列的 X。** 前者會讓 WebView2 連 webview 一起收掉
   （CDP 端點整個消失），後者才是使用者真正的操作。→ 驗證改用
   `Process.CloseMainWindow()`（送出真正的 `WM_CLOSE`）。

#### 測試（2 個新測試）

- `window`（2）：`CloseAction` 的解析（空值與亂填都回安全的那一邊＝tray）、
  `decide_close` 的兩種結果。

#### 實機驗證（安裝後的真實 App）

| 驗證項 | 結果 |
|---|---|
| 預設值 | ✅ 設定頁顯示「關閉視窗時：縮到系統匣（建議）」 |
| **托盤模式下關閉視窗** | ✅ 送原生 `WM_CLOSE` 後：**行程 1 個、閘道埠仍可連線、CDP 仍回應**（webview 活著，可再叫回來） |
| 隱藏時點 Deep Link | ✅ 視窗被叫回來並跳匯入確認框（「匯入技能「brand-guidelines」」） |
| **輕量模式** | ✅ 按「進入輕量模式」→ 視窗銷毀（CDP 端點消失）但**行程 1 個、閘道仍可連**；托盤與網關照常 |
| **從輕量模式被 Deep Link 喚醒** | ✅ 點連結 → 視窗被重建（CDP 恢復）＋確認框出現（「匯入技能「canvas-design」」）—— pending 轉交路徑有效 |
| 「直接結束」分支 | ✅ 設成 exit 後按原生 X：**行程 0 個、閘道關閉**（真的結束） |
| 還原 | ✅ 已把 `close_action` 設回預設 `tray` |

---

### 0.9.28 主題與開機自啟（P4.3／P4.4，2026-10-01）

#### cc-switch 的實際行為（照它手冊 §1.5 抄）

- **語言**：簡中／繁中／英文／日文，**即時生效不用重啟**。
- **主題**：`System`／`Light`／`Dark`（System 跟隨系統深淺色）。
- **Launch on Startup**：Windows 用**登錄檔**、macOS 用 LaunchAgent、Linux 用 XDG autostart。
- **關閉行為**：預設「關閉時縮到系統匣」；另有 Lightweight Mode（只留托盤、視窗銷毀）。

#### P4.3 主題：只靠一組 CSS 變數切換

改動範圍是「66 個前端檔案的 777 處硬編色」，所以關鍵是**一次到位的機制**而不是逐處補丁：

| 部分 | 內容 |
|---|---|
| 前景通道 | `--app-fg-rgb`（深色 `255 255 255`／淺色 `28 28 30`）。所有白色疊色改寫成 `rgb(var(--app-fg-rgb) / α)`，**淺色主題自動變成黑色疊色** |
| Tailwind | `@theme { --color-fg: var(--app-fg) }` → 元件寫 `text-fg/60`、`bg-fg/[0.06]`、`border-fg/10`，透明度寫法照用 |
| 表面色 | `--app-bg*`／`--app-window*`／`--app-elevated*`／`--app-pop*`／`--app-glass*`／`--app-shadow*`／`--app-orb-opacity`，`:root[data-theme="light"]` 覆蓋 |
| 系統色 | 淺色底上把 accent／green／red 調深（macOS Light 的作法），維持對比 |
| 切換 | `<html data-theme>`；`system` 由前端監聽 `prefers-color-scheme` 即時切換；偏好存 `settings.theme`（DB 是真相）＋ localStorage 首幀快取避免閃爍 |
| 機械替換 | 777 處 `{text,bg,border,divide,ring,…}-white[/α]` → `-fg`，另把 7 個對話框標題的 `rgba(24,24,30,0.98)` 換成 `var(--app-elevated-solid)`、光暈加 `--app-orb-opacity` |

#### P4.4 開機自啟：HKCU 的 Run 機碼

`HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 底下的 `TokenGateway` =
`"<exe 路徑>"`（與 Deep Link 協定註冊同一條原則：**免管理員**）。
**狀態以登錄檔為準**，不另外在資料庫存一份 —— 使用者可能用工作管理員或其他工具關掉它，
我們不該顯示自己想像的狀態。

#### 測試（4 個新測試）

- `theme`（2）：三個選項的驗證（含大小寫與空白）、`resolve` 的語意
  （明講淺／深就不管系統；`system` 才看系統；空值視為跟隨系統）。
- `autostart`（2）：查詢是唯讀的（兩次結果相同、有值就一定指向 `.exe`）、
  機碼路徑與值名稱就是 cc-switch 手冊寫的那個位置。

#### 實機驗證（CDP 讀 **computed style**，不是看截圖）

| 驗證項 | 結果 |
|---|---|
| 三個選項都在 | ✅ 設定頁「跟隨系統／淺色／深色」，並顯示「目前：淺色」 |
| 切淺色 | ✅ `<html data-theme="light">`；body 文字 `rgba(28,28,30,0.95)`、底色 `rgb(236,236,240)`；`--app-fg-rgb: 28 28 30`；**玻璃邊框由 `rgba(255,255,255,0.09)` 反轉成 `rgba(28,28,30,0.09)`** |
| 切深色 | ✅ 全部回到 `rgba(255,255,255,0.95)` / `rgb(11,11,15)` |
| 跟隨系統 | ✅ 解析為 dark（本機系統為深色） |
| **開機自啟開啟** | ✅ 設定頁開關 →「已開啟開機自啟」；`reg query …\Run /v TokenGateway` → `"C:\Users\<user>\AppData\Local\token-gateway\token-gateway.exe"` |
| 開機自啟關閉 | ✅ 再切一次 →「已關閉開機自啟」；登錄檔查詢變成「找不到」——**使用者的機器回到原狀** |

---

### 0.9.27 Deep Link 一鍵匯入（P4.1，2026-10-01）

#### cc-switch 的實際行為（照它手冊 §5.3 抄）

- 協定：`ccswitch://v1/import?resource={provider|mcp|prompt|skill}&…`
  參數：provider（`app`／`name`／`endpoint`（可逗號分隔多個）／`apiKey`／`model`／`notes`／
  `enabled`／`usage*`）、prompt（`app`／`name`／`content`（Base64）／`enabled`）、
  mcp（`apps`（逗號分隔）／`config`（Base64 的 `{"mcpServers":{…}}`））、
  skill（`repo`（`owner/name`）／`directory`／`branch`／`name`）。
- 流程：點連結 → App 打開 → **顯示匯入確認對話框**（型別、內容預覽、確認／取消）→ 才寫入。
- 安全：驗格式、顯示預覽、要求確認；`usageEnabled` 未明講 `true` 時匯入後**保持停用**。
- 協定註冊：安裝時自動寫 `HKEY_CLASSES_ROOT\ccswitch`（Windows）。

#### 我們做的（`deeplink/`，沒有新增資料表）

| 部分 | 內容 |
|---|---|
| 協定 | `tokengateway://`（參數名稱與語意**照抄** cc-switch，既有的分享連結換 scheme 就能用） |
| 解析 | `parse`：只解析＋驗證，產生預覽；**不做任何寫入** |
| 預覽 | 型別標題、逐欄位摘要、**要提醒的事**、**這次會做什麼**；API Key 只顯示前 6 後 4（`mask`） |
| 套用 | `apply`：使用者確認後才寫；四種資源各自沿用既有 store（`providers::insert_provider`、`prompt::save_preset`、`mcp::upsert_server`、`skills::add_repo`），**不另開寫入路徑** |
| 重複處理 | 同名來源／同名預設集／同 slug 伺服器都跳過並回報（不覆蓋使用者的設定） |
| 協定註冊 | 啟動時自己寫 **HKCU**（`HKCU\Software\Classes\tokengateway`）—— cc-switch 用安裝程式寫 HKCR 需要管理員，我們改成免管理員且 App 自己能修；設定頁有狀態＋重新／取消註冊 |
| 「已經開著」時 | 沒有 single-instance 外掛：新行程用**閘道埠是否被佔用**判斷已有實例，把連結寫進 `<app_data>/deeplink-inbox.json` 就結束；在跑的行程每秒看一次收件匣並發 `deeplink` 事件（與托盤重建同一套輪詢） |
| 前端 | 全域的 `DeeplinkDialog`：啟動 pending（`deeplink_take_pending`）與執行中事件都會跳確認框；確認後刷新四個清單的 query |

**刻意不做**：`usage*` 參數（我們的用量查詢是宣告式規格，不是 cc-switch 的 JS 腳本，
映射不直觀就先不假裝支援 —— 連結帶了也不會爆炸，只是忽略）。

#### 實機驗證（真的用 `Start-Process tokengateway://…` 觸發）

| 驗證項 | 結果 |
|---|---|
| **協定自動註冊** | ✅ 先 `reg delete` 掉整棵機碼（查詢確認「找不到」），啟動 App 後查 `HKCU\Software\Classes\tokengateway\shell\open\command` → `"…\token-gateway.exe" "%1"` —— **免管理員、自己補上** |
| **端到端：點連結** | ✅ `Start-Process "tokengateway://v1/import?resource=skill&repo=anthropics/skills&directory=skills/brand-guidelines&branch=main"` → 對話框跳出、內容正確 |
| **單一行程** | ✅ 點連結前後行程數都是 **1** —— 第二個行程轉交收件匣後就結束（不會兩個行程搶埠） |
| skill 匯入 | ✅ 標題「匯入技能「brand-guidelines」」；欄位 儲存庫／分支／目錄 齊全；⚠ 會從 GitHub 下載；確認後「已新增儲存庫 anthropics/skills@main…｜新增：brand-guidelines（儲存庫 #5）」 |
| prompt 匯入（Base64） | ✅ 標題「匯入提示詞「代碼審查」」；18 字元、**內容預覽正確解出**「# 角色 / 你是一個專業的代碼審查專家」；「啟用：否」＋說明；**取消後沒有寫入** |
| 壞連結 | ✅ `app=gemini` → 對話框顯示「不支援的工具「gemini」（我們只管理 claude／codex／opencode）」 |
| 清理 | ✅ 刪掉測試儲存庫後，資料庫只剩內建那個（`anthropics/skills`），提示詞 0 筆 |

#### 測試（19 個新測試）

- `deeplink`（2）：命令列參數挑連結（別的 scheme 不算）、scheme 與解析器一致。
- `deeplink::parse`（10）：**cc-switch 手冊的 MCP 範例連結**（改 scheme 後要完全讀懂）、
  provider（URL-encoded、多個 endpoint、遮罩不含完整金鑰）、prompt（Base64 中文、
  預設不啟用）、skill（含從 directory 推名稱）、percent-decode 邊界（`%`／`%ZZ`）、
  Base64 容忍（URL-safe／缺 padding）、各種壞連結的人話錯誤、MCP 兩種條目形狀、遮罩。
- `deeplink::apply`（5）：來源建立（含模型登記到 `provider_models`）與重複跳過、
  提示詞建立但不啟用、MCP 建立＋綁定＋重複跳過、技能存成儲存庫、缺 endpoint 要拒絕。
- `deeplink::protocol`（2）：收件匣來回（讀過就清、空白視為沒有）、查詢不改變狀態。

---

### 0.9.26 會話管理（P3.4，2026-10-01）

#### cc-switch 的實際行為（照它手冊 §3.4 抄）

- 一個地方瀏覽／搜尋／管理各工具的對話：左邊清單（搜尋＋依工具篩選＋重新整理），
  右邊詳情（標題、最後活動時間、專案目錄、**續聊指令**、對話全文），依最後活動排序。
- **續聊**：macOS 會開終端機；**其他平台只把指令複製到剪貼簿**，由使用者自己貼。
- **刪除**：單筆／批次（全選、清除、確認框），**沒有本機來源路徑的會話不能刪**。
- 訊息依角色上色（user／assistant／system／tool）。

#### 我們做的（`sessions/`，沒有新增資料表）

**先把三個工具的實際儲存位置與格式查清楚**（這是「不憑想像」）：

| 工具 | 位置 | 形態 |
|---|---|---|
| Claude Code | `~/.claude/projects/<專案>/<uuid>.jsonl` | JSONL：`type: user/assistant` ＋ `message.content`；`cwd` 在事件上 |
| Codex | `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`＋`archived_sessions/` | JSONL：第一行 `session_meta`（`id`／`cwd`），其後 `response_item` |
| OpenCode | `~/.local/share/opencode/opencode.db` | **SQLite**：`session`（有 `title`／`directory`）＋`message`＋`part` |

續聊指令也是用 CLI 自己的說明確認的：`codex resume <id>`、`claude --resume <id>`、
`opencode -s <id>` ✓（`codex resume --help`、`claude --help`、`opencode --help`）。

| 部分 | 內容 |
|---|---|
| 掃描 | 三個工具並行掃描，標題取「第一則使用者訊息」（Codex／Claude）或資料庫的 `title`（OpenCode）；只讀檔頭（上限 512 KB／400 行）以維持速度 |
| 讀取 | 依角色解析訊息；工具呼叫（`function_call`、`tool_result`）也進時間軸；長對話只留最後 400 則 |
| 續聊 | 顯示指令 ＋ 「複製指令」按鈕（Windows 上不擅自開終端機，與 cc-switch 的非 macOS 行為一致） |
| 刪除 | 單筆／批次（全選／清除／確認框）；**只接受 `.jsonl`**、且路徑 `canonicalize` 後必須在該工具的會話目錄底下；**OpenCode 一律拒絕**（別人的資料庫只讀） |
| 安全 | `SessionEnv` 明講允許的目錄；`SessionEnv::real()` 在**測試建置直接拒絕**（P3.2／P3.3 兩次教訓的同一條防線） |
| 帳務 | 刪除後明確提示「已回填的用量統計不會被撤銷（用量是帳，不隨對話消失）」 |

#### 實機驗證抓到的一個 bug（已修）

第一版用「**全域** 500 筆上限再截斷」。實際資料是 Codex 617 檔、Claude 32、OpenCode 104，
而 OpenCode 的會話比較舊 —— 結果**切到 OpenCode 篩選器看到 0 筆**，整個工具被擠掉。
現在改成**每個工具各自上限 300**（合併後再排序，不設全域上限），並補了測試
`per_app_cap_keeps_every_tool_visible` 把這條規則釘住。

#### 測試（18 個新測試）

- `sessions`（4）：續聊指令與三個 CLI 一致、專案目錄取名、標題單行化與截斷、支援的工具。
- `sessions::scan`（6）：Codex／Claude 檔頭解析（id／cwd／標題）、壞檔與讀不到的檔不 panic、
  `content` 兩種形狀（字串／陣列）、遞迴掃描、**每個工具各自上限**。
- `sessions::read`（5）：Codex／Claude 每行→訊息（含工具呼叫與 `tool_result`）、
  空白內容要略過、長對話保留尾端、不支援的工具要報錯。
- `sessions::store`（3）：OpenCode／非 `.jsonl`／不存在一律拒絕、允許目錄底下才真的刪
  （且回報大小、被拒的檔案必須還在）、**真實環境在測試建置被拒絕**。

---

### 0.9.25 技能管理（P3.3，2026-10-01）

#### cc-switch 的實際行為（照它手冊 §3.3 抄）

- 技能＝**一個資料夾**（含 `SKILL.md`）。
- **兩層目錄**：母本放儲存目錄（預設 `~/.cc-switch/skills/`，可切 `~/.agents/skills`），
  再同步到各工具的 skills 目錄（**symlink 優先、失敗則複製**）。
- 各工具目錄：Claude `~/.claude/skills/`、Codex `~/.codex/skills/`、
  OpenCode `~/.config/opencode/skills/`。
- 來源是 **GitHub 儲存庫**（owner／name／branch／subdir，內建＋可自訂）與
  skills.sh 公開登錄搜尋；**更新偵測用 SHA-256 內容雜湊**（不是時間戳），
  支援單獨更新與全部更新。
- 解除安裝前**自動備份**到 `skill-backups/`，可還原、可刪除備份。

#### 我們做的（`skills/`，schema v13）

| 部分 | 內容 |
|---|---|
| 資料表 | `skill_repos`／`skills`／`skill_bindings` |
| 範圍 | 只服務可接管的三個工具（共用 `TAKEOVER_APPS`） |
| 內建儲存庫 | **1 個，已實測**：`anthropics/skills` 的 `skills/`（實機掃到 19 個技能）。ComposioHQ 的版面不同（`.claude/`、`.agents/`），沒有把握就**不放** —— 寧缺勿猜 |
| 掃描 | GitHub contents API（一次請求列出 `subdir` 下的資料夾） |
| 安裝／更新 | **tarball**（`codeload.github.com/.../tar.gz/refs/heads/{branch}`）一次抓整個 repo，再挑出技能資料夾。**tar 解析自己寫**（`tar` crate 不在離線快取；`flate2` 有，所以 gunzip 用現成的）；支援 GNU longname |
| 同步 | symlink 優先，失敗退回複製；**實測在 Windows 上一定會退回**（os error 1314「用戶端沒有這項特殊權限」，除非開了開發者模式） |
| 更新 | 內容雜湊（SHA-256 over 排序後的「路徑 + NUL + 內容」）比對，雜湊相同就只重新同步 |
| 備份 | 解除安裝前複製母本到 `skill-backups/{技能}-{時間戳}`；UI 可列出／還原／刪除 |
| 設定 | 儲存位置（內建／`~/.agents/skills`）與同步方式（連結／複製），存在 `settings` 表 |

**刻意不做的兩格**（寫在對齊矩陣）：
- **skills.sh 搜尋**：第三方登錄，我們無法確認它的 API 與資料品質；要裝什麼直接加
  GitHub 儲存庫即可（cc-switch 也是靠儲存庫座標在裝）。
- **ZIP 安裝**：離線快取裡沒有 zip 解析套件，而 GitHub tarball 已涵蓋主要路徑。

#### 又踩到同一顆石頭（第二次），這次直接結構化修掉

實作時單元測試呼叫了會自己解析主目錄的同步函式，於是**寫進使用者真實的
`~/.claude/skills` 與 `~/.codex/skills`**（P3.2 的提示詞事故是同一類）。
這次的代價小（只多了一個測試用的 `demo`，而且在使用者既有的四個技能之外），
但一樣是「測試不該碰真實檔案」的問題。

修法：引入 **`SkillEnv`** —— 母本目錄、各工具目錄、同步方式**由呼叫端明講**；
`SkillEnv::real()` 只在命令層使用，而且在**測試建置裡直接拒絕**
（`if cfg!(test) { return Err(...) }`），測試一律用 `SkillEnv::for_test` 傳暫存目錄。
驗證：跑完整套測試後比對使用者三個 skills 目錄，與基線完全相同。

#### 測試（27 個新測試，其中 3 個需要網路）

- `skills`（4）：技能名稱驗證（擋 `a/b`、`.hidden`、中文、空白）、
  儲存庫座標驗證（去斜線、branch 預設 main、**擋 `../etc`**）、內建儲存庫合法、支援的工具。
- `skills::tarball`（5）：解析嵌套檔案（去掉最外層 `repo-ref/`）、gzip 來回、
  截斷要報錯、空 tar、八進位欄位壞掉要報錯。
- `skills::github`（網路，`--ignored`）：**真的打 GitHub** 列出 `anthropics/skills`
  （要看到 `claude-api`）、**真的下載 tarball** 並確認 `SKILL.md` 與子目錄都在、
  不存在的儲存庫要回明確錯誤；另有 tarball 網址形狀的純函式測試。
- `skills::store`（2）：內建儲存庫冪等與不可刪、技能 UPSERT＋綁定＋刪除不留孤兒。
- `skills::sync`（5）：三個工具目錄對照表、儲存位置切換、**雜湊對順序不敏感但對內容敏感**、
  描述從 front-matter 或第一行來、複製與移除的來回。
- `skills::install`（4）：安裝寫母本＋同步到指定工具（**沒選的工具不會被碰**）、
  沒有 `SKILL.md` 要拒絕、更新用雜湊判斷（相同＝unchanged）、
  解除安裝備份＋還原＋刪備份、**真實環境在測試建置要被拒絕**。

---

### 0.9.24 提示詞預設集（P3.2，2026-10-01）

#### cc-switch 的實際行為（照它手冊 §3.2 抄）

- **逐工具分開管理**（「Prompts are managed separately per app」），每個預設集＝
  名稱 ＋ Markdown 內容。
- **同時只能有一個啟用**：啟用新的會自動把舊的關掉。
- 啟用後寫進該工具的檔案：Claude `~/.claude/CLAUDE.md`、
  Codex `~/.codex/AGENTS.md`、OpenCode `~/.config/opencode/AGENTS.md`。
- **智慧回填（smart backfill）**：切換預設集之前先讀現行檔案，若與資料庫裡那個
  預設集不同（代表使用者手改過），**先把檔案內容存回舊的預設集**再切。
  觸發時機：切換預設集、編輯現行預設集、首次啟動匯入現有檔案。
  沒有啟用中的預設集時不回填。
- **啟用中的預設集不能刪**，要先關掉。
- 面板有 Markdown 編輯器（語法高亮＋即時預覽）。

#### 我們做的（`prompt/`，schema v12）

| 部分 | 內容 |
|---|---|
| 資料表 | `prompt_presets`（app／name／content／active） |
| 範圍 | 只服務可接管的三個工具（共用 `TAKEOVER_APPS`），路徑照 cc-switch 的對照表 |
| 單一啟用 | `activate` 用交易把同工具其他列關掉（有測試） |
| 回填 | `activate_and_sync`：讀檔 → 回填舊的 → 切換 → 寫新的（順序見下） |
| 首次匯入 | 該工具一列都沒有、檔案有內容 → 收成「現有內容」並啟用（面板載入時） |
| 刪除保護 | 啟用中的預設集會被拒絕，訊息直說要先關掉 |
| 編輯器 | 純 textarea ＋**簡易預覽**（標題／清單／程式碼／粗體，`miniMarkdown`）—— 沒有引入 Markdown 引擎，這是刻意的簡化 |
| 安全 | 原子寫入 ＋ 改寫前備份到 `backups/<工具>-prompt/`（保留 5 份，`fsutil::backup_text` 與 MCP 共用） |

#### 實作時被測試逼出來的三條順序規則（都已寫成測試）

1. **切換時一定要先回填舊的、再寫新的。** 先 `activate` 再同步的話，回填會把
   現行檔案存進**新**預設集，使用者剛切過去的內容立刻被舊檔案蓋掉。
2. **存檔路徑不回填。** 編輯啟用中的預設集後，使用者的編輯就是意圖；
   這時候回填會把它換成舊檔案內容。所以存檔走 `write_active`（只寫，不回填）。
3. **檔案不存在時不回填。** 沒有檔案＝沒有手改要救；否則會把空內容回填進舊預設集，
   把使用者的預設集清空。

#### ⚠️ 一次真實事故與它的修法（必讀）

**症狀**：單元測試把使用者真實的 `~/.codex/AGENTS.md`（4,139 bytes 的個人化提示）
覆蓋成測試字串 `B 的內容`。原因是測試呼叫了**會自己解析主目錄**的
`activate_and_sync`，而它寫的是真檔；那份備份又落在測試的暫存目錄裡，隨測試結束被刪掉。

**復原**：Codex 會把 `AGENTS.md` 注入 session 的 `<user_instructions>`，
所以從 `~/.codex/sessions/**/rollout-*.jsonl` 把原文撈回來 ——
還原長度 **4,139 bytes，與原始檔完全相同**，內容與第一行都對得上
（`.workbuddy/tmp/recover_agents.py`）。已還原並用 `codex exec` 確認可用。

**結構性修法**：新增 `guard_not_real_path()` —— 所有「會解析主目錄」的入口
（`sync_app`／`activate_and_sync`／`write_active`／`ensure_imported`）在**測試建置裡
一律直接拒絕**，並要求測試走 `*_at` / `*_ex` 版本自己傳暫存路徑。
跑完整套測試後再次確認使用者的檔案長度不變（4,139 bytes）。

> 這條教訓的一般化版本：**「測試會寫檔」的模組，寫入入口必須要求呼叫端明講路徑**，
> 便利包裝（自己解析路徑的那種）只能給正式命令用，而且要在測試建置裡上鎖。

#### 測試（21 個新測試）

- `prompt`（3）：三個工具都可用、其他工具被拒、名稱驗證（空／過長／去空白）、
  內容大小上限。
- `prompt::store`（5）：CRUD、**同工具只能有一個啟用**（且不影響別的工具）、
  **啟用中的不能刪**、編輯內容不會動啟用狀態、`overwrite_content` 只動內容。
- `prompt::sync`（13）：回填會把檔案內容存回舊預設集、沒有啟用中就不回填、
  路徑對照表、**寫檔＋備份**、**切換時的回填順序**、**存檔不回填**、
  首次匯入（含「內容相同就不重寫」「第二次不再匯入」「空檔案不匯入」）、
  檔案不存在時會被建立、以及 `guard_not_real_path` 的行為。

---

### 0.9.23 MCP 伺服器管理（P3.1，2026-10-01）

#### cc-switch 的實際行為（照它手冊 §3.1 抄）

- **一個統一的面板**，七個工具共用；每個伺服器可以**逐工具**開啟／關閉。
- 新增：預設樣板（fetch／time／memory／sequential-thinking／context7）或自訂。
  欄位＝Server ID（必填、唯一）、名稱、說明、傳輸型別（stdio／http／sse）、
  Command（stdio 必填）、Arguments、URL（http／sse 必填）、Headers、環境變數。
- **開啟＝寫進那個工具的設定檔**；關閉＝從那個檔案移除。
  Claude `~/.claude.json` 的 `mcpServers`、Codex `~/.codex/config.toml` 的
  `[mcp_servers]`、OpenCode `opencode.json` 的 `mcp`。
- **只動資料庫管得到的伺服器**：使用者手寫、沒匯入的不碰。
- 同步條件：該工具有安裝才寫（沒裝就靜默不寫、不算錯誤）。
- **匯入既有設定**：一次讀遍各工具設定檔，匯入的伺服器**自動對來源工具啟用**；
  讀不到的工具會回報原因。
- 整欄一鍵全開／全關。

#### 我們做的（`mcp/`，schema v11）

| 部分 | 內容 |
|---|---|
| 資料表 | `mcp_servers`（slug 唯一）＋ `mcp_bindings`（server × app 的啟用狀態） |
| 同步工具 | **只同步可以接管的三個**（Claude Code／Codex／OpenCode）—— 能寫設定檔的工具才可能同步 MCP，所以直接共用 `tools::TAKEOVER_APPS`，不另外維護一份清單 |
| 樣板 | 5 個，套件名照抄它手冊的表格；**啟動指令是我們的判斷**（Python 用 uvx、TS 用 npx -y），說明欄直說 |
| 形狀 | Claude stdio `{command,args,env}`／http `{type,url,headers}`；Codex `[mcp_servers.X]` ＋ `[mcp_servers.X.env]`，streamable HTTP 用 `url`（**用 `codex mcp add --help` 問來的**，不是猜的）；OpenCode `{type:"local",command:[…],environment,enabled}`／`{type:"remote",url,headers,enabled}` |
| 整欄切換 | 標題列每個工具兩個鈕（全開／全關） |
| 匯入 | 讀三個設定檔 → 過同一套驗證 → 進資料庫並對來源工具啟用；重複的 slug 略過、壞掉的檔案回報原因 |

#### 實機驗證抓到的三個問題（都修掉了，也都有測試）

1. **同步會在多餘的檔案裡留下空殼。** 第一次實機驗證後 `opencode.json` 也被改了 ——
   因為 `sync_all` 對三個工具都跑，opencode 沒有要寫的伺服器，卻被插入了
   `"mcp": {}`。現在：**沒有東西要寫、檔案裡也沒有我們管的 slug 時，完全不碰檔案**。
2. **移除最後一個條目會留下 `"mcpServers": {}`。** 現在空了就把鍵拿掉，
   使用者的檔案回到原狀。
3. **覆蓋會吃掉我們沒建模的欄位。** 使用者的 Codex 設定可能有 `cwd`、
   `startup_timeout_sec` 這類鍵；直接覆蓋會在「按一下同步」時無聲刪掉它們。
   現在改成**合併**：只更新我們管的鍵（command／args／url／env／headers／type），
   其他原封不動。

另外為了不讓使用者的 `~/.claude.json` 被「整份重排」，`serde_json` 開了
`preserve_order`（鍵序照原樣；indexmap 本來就在離線快取裡），
並且在**值沒變時直接回傳原文** —— 所以「開了又關」是真正的 no-op：
實機驗證的檔案雜湊與操作前**完全相同**。

#### 測試（34 個新測試）

- `mcp`（5）：Server ID 驗證（允許 `@scope/pkg`、擋空白與引號、長度上限）、
  傳輸型別、各型別的必填欄位、空名稱回退成 slug、空白鍵值對會被丟掉。
- `mcp::presets`（3）：五個樣板形狀正確且都過得了驗證、**套件名與 cc-switch 手冊逐項比對**。
- `mcp::store`（5）：CRUD、重複 slug 的清楚錯誤、逐工具綁定來回、不支援的工具被拒（`cursor`／`hermes`）、刪除連綁定一起清。
- `mcp::apply`（14）：三種條目的形狀、JSON／TOML 改寫保留其他鍵與註解、
  壞檔要拒絕、**合併語意（我們沒建模的鍵不能被吃掉）**、
  **沒事不碰檔案**、**移除後空區塊要拿掉**、**開了又關要逐字回到原狀**、
  備份與輪換、建立不存在的檔案、三種格式的匯入解析（含 Codex 的 `.env` 子表格不算另一個伺服器）。
- `mcp::sync`（2）：同步寫入／移除與手寫項目保留、匯入驗證擋掉壞資料。

---

### 0.9.22 用量查詢：每來源可設定的餘額／額度查詢（P2.1）＋卡片顯示（P2.2）（2026-10-01）

#### cc-switch 的實際行為（照它的使用者手冊 §2.5 抄）

- 分兩類：**自動顯示**（OAuth 帳號型：Copilot／Codex OAuth／xAI OAuth）與
  **手動開啟**（官方訂閱、Token Plan、第三方餘額、自訂）。
- 手動類要在來源卡的「用量查詢」面板開啟，內建樣板涵蓋：官方訂閱（Claude／Codex／
  Gemini／Grok）、Token Plan（Kimi／智譜／MiniMax／火山方舟）、第三方餘額
  （DeepSeek／階躍／硅基流動／OpenRouter／Novita）。
- 面板欄位：啟用開關、樣板、API Key／Base URL／Access Token／User ID、
  逾時（預設 10 秒）、自動查詢間隔（0–1440 分，0＝關；**只在該來源「目前使用中」
  才會自動查**）、**測試腳本**（送出並顯示結果或錯誤）。
- 自訂那格是**使用者寫的 JavaScript**：`({request:{url,method,headers}, extractor:function(response){…}})`，
  回傳 `{isValid, invalidMessage, remaining, unit, planName, total, used, extra}`；
  佔位符 `{{apiKey}}`／`{{baseUrl}}`／`{{accessToken}}`／`{{userId}}`。
- 卡片底部顯示：用量百分比 ＋ 重置倒數，**<70% 綠 / 70–89% 橘 / ≥90% 紅**；
  多方案顯示方案數、可展開；卡片上有重新查詢圖示。
- 它特別提醒：查詢會消耗少量上游額度、自動間隔要設合理值。

#### 我們做的（`usage_query/`，schema v10）

| 部分 | 內容 |
|---|---|
| 資料表 | `provider_usage_query`（一來源一列：開關、樣板、憑證、網址、標頭、抽取規格、逾時、自動間隔） |
| 樣板 | 6 個：**New API 中轉**（照抄它手冊裡的原文範例，含 `New-Api-User` 標頭與 `/500000` 換算）、通用餘額、DeepSeek 餘額、OpenRouter 額度（用 `subtract` 算「總額−已用」）、硅基流動餘額、自訂 |
| 佔位符 | `{{apiKey}}`／`{{baseUrl}}`／`{{accessToken}}`／`{{userId}}`，與它一致 |
| 逾時／間隔 | 逾時預設 10 秒（1–120）；自動間隔 0–1440 分，**0＝不自動查** |
| 測試 | 面板上的「測試」＝先存再打一次真的請求，顯示成敗、抽取到的數字，並可展開**原始回應** |
| 卡片顯示 | 百分比徽章（三段式顏色）＋ 方案名 ＋ 餘額／總額 ＋ 多方案展開 ＋ 重新查詢 ＋ 設定入口 |
| 自動查詢 | 只對**選取中的來源**按它自己的間隔重查（與它「只查使用中的來源」同一個精神：省上游額度） |

**唯一刻意的差異：自訂查詢不是 JavaScript，而是宣告式抽取規格。** 三個理由：

1. 不想為了這一格塞一個 JS 引擎（純 Rust 的引擎是好幾 MB 的相依，而這台機器的
   建置走 `--offline`，本地快取裡也沒有任何 JS 引擎可用 —— 查過了）。
2. cc-switch 文件裡的每個例子本質上都是「打一個網址、從 JSON 取幾個數字」
   （它的 Generic 與 New API 範例就是 `url` ＋ `headers` ＋ `response.xxx`）。
3. **可測試**：規格是資料，能逐條寫測試；使用者寫的 JS 只能靠執行才知道。

規格長這樣（每個數字欄位可以是路徑字串，或帶運算的物件）：

```json
{
  "remaining": "balance_infos.0.total_balance",
  "used": { "path": "used_quota", "divide": 500000 },
  "total": { "path": "quota", "divide": 500000 },
  "unit": "USD",
  "planName": "data.group",
  "validPath": "is_available",
  "invalidMessagePath": "message",
  "plans": { "path": "data.plans", "name": "name", "remaining": "remaining", "total": "total" }
}
```

路徑用點分隔、數字段是陣列索引；`subtract`／`divide`／`multiply` 支援
OpenRouter 的「總額−已用」與 New API 的 `/500000`。**取不到就是 `None`，不補 0** ——
「查不到」與「餘額是 0」是不同的事（這一條有測試）。

#### 兩個實作時踩到、順手修掉的坑

1. **套了樣板卻沒啟用＝什麼都不顯示。** 第一版把「套樣板」與「啟用」分成兩步，
   實機驗證時就發生了：測試成功、儲存成功，卡片卻還是空的（因為 `enabled=0`）。
   現在**套樣板會順手把開關打開**（開關就在畫面上，可以再關掉），
   而且測試成功但未啟用時，訊息會直接說「卡片不會顯示」。
2. **孤兒列**：`provider_usage_query` 也要進 `purge_orphans` 的清單，
   來源刪除時也要一起清（與其他來源子表一致）。

#### 測試（20 個新測試）

- `usage_query::extract`（4）：路徑（物件／陣列索引／字串型數字）、規格解析（兩種形狀）、
  運算（除以 0 回 `None`、減項缺資料回 `None`）、找不到就是 `None`。
- `usage_query`（7）：佔位符展開（含不認識的佔位符原樣保留）、
  **New API 形狀**抽得出方案名與餘額（含 `/500000`）、帳號無效時用上游訊息、
  多方案陣列展開、**規格對不上時說「規格沒對上」而不是假裝成功**、
  非 JSON 回應、只有 remaining 時百分比用 `(total-remaining)/total`。
- `usage_query::store`（3）：UPSERT 來回、驗證（空網址／非法方法／逾時範圍／間隔範圍／
  非法 provider_id 全部擋下且不落庫）、刪除。
- `usage_query::templates`（3）：id 唯一且形狀正確、**照抄的 New API 樣板逐欄比對**、
  套樣板保留憑證與 provider_id。
- `usage_query::run`（3）：**真的打一個本機伺服器**（驗證佔位符真的變成標頭、
  抽取結果正確）、非 2xx 回報狀態碼、連不上／逾時給人話、空網址與相對網址被擋。

---

### 0.9.21 連線檢查、故障轉移佇列、系統匣切換（P1.4–P1.6，2026-10-01）

CC Switch 對齊計畫的 P1 後半。三個功能各自的關鍵決定如下。

#### P1.5 連線檢查（`provider_check.rs`）—— 語意刻意與 CC Switch 一致

**只檢查位址可不可達，不送模型請求、不帶金鑰。** 所以 401／403／404 都算「可達」：
它們證明主機與路徑活著，只是沒有金鑰或那個路徑本來就沒有端點。訊息文字會把這件事
講清楚（「檢查過了不等於一定能用」），因為 CC Switch 的 FAQ 也特別提醒同一件事 ——
它的「連線檢查」在 3.16.3 之後也改成只測位址。

- `normalize_url`：沒寫 scheme 補 `https`、`localhost`／`127.0.0.1` 補 `http`、去尾斜線；
  空字串／`ftp://`／只有 scheme 都回 `None`（UI 顯示「請先填寫有效的上游地址」）。
- `classify`：狀態碼 → 人話。401 標「需要金鑰」、500 系列標「伺服器端問題」
  （避免使用者把「可達」誤讀成「一切正常」）。
- 逾時比網關短（連線 6 秒／整體 10 秒）：使用者在等一個按鈕，不是在等一次推理。
- 用 GET 而不是 HEAD：有些上游對 HEAD 回 405 或直接斷線。
- **位置與 CC Switch 不同**：它放在來源卡片上，我們放在右側詳情面板（那裡本來就是
  這個來源的操作區），並在測試裡註明這個差異。

#### P1.4 故障轉移佇列（`proxy/queue.rs`）—— 不自己排序

這個面板**不重寫任何排序邏輯**：它呼叫後端的 `failover_queue`，而那個命令用的就是
`proxy_handler` 換手時的**同一個查詢**（`resolve_model_providers`）＋同一條健康排序
（`order_by_health`）。一旦有第二份排序邏輯，畫面就會開始說謊。

- 以**模型**為中心：挑一個模型 → 看它在哪些來源上架、順序如何。
- 每列顯示：順序、來源名、宣告協議、**學到的協議**（`provider_model_protocol`）、
  冷卻狀態（斷路器）、模型數。
- 冷卻中的來源在畫面上也被排到最後 —— 與請求路徑的行為一致。
- 調整順序的方式就是**拖曳來源清單**（順序＝來源清單順序），面板只負責顯示；
  「每個工具各自一條佇列」仍然只有全域一條（§1 的 B4 保持 ⚠️）。

#### P1.6 系統匣切換（`tray.rs` + `TraySwitchListener.tsx`）—— 職責切分是重點

托盤只做兩件事：**決定選單長相**（哪些工具、各有哪些可用來源）與**把點擊變成事件**
（`emit("tray-switch", {app, provider_id})`）。真正寫入設定的動作**不在 Rust 做**，
而是由前端既有的流程處理。

為什麼：切換牽涉「上次用哪把 Key、那個模型還在不在」這些規則，而那份規則原本就寫在
前端的 `resolveSelectionReq`。在 Rust 再寫一份＝製造第二個真相來源。所以這一輪把那段
規則抽成 `switchReq.ts::buildSelectionReq`（**唯一一份實作**），
`useToolSwitch` 與托盤兩邊都呼叫它。

- 選單只列**可用**的來源：啟用中、且至少有一個啟用模型。列了卻點不動比不列更糟。
- 選單在啟動時建立，**每 30 秒重建一次**（App 裡新增／改名／刪除來源不必重啟）。
- 項目 id `sw:<app>:<provider_id>`；解析器**拒絕**不可接管的工具（hermes／cursor…）、
  非數字、`0`／負數與其他任何字串（免得選單被動手腳後把設定寫到不存在的工具）。
- 接收端掛在**全域**（`App.tsx`）而不是來源頁：使用者可能在「用量」頁就把托盤切了，
  掛在來源頁會讓那個動作靜默失敗。回饋用一個小 toast（這個 App 原本沒有 toast，
  但寫設定這種事不該靜默發生）。
- 事件名稱與 payload 鍵名是**跨語言契約**（Rust `emit` ↔ TS `listen`），用常數
  `EVENT_SWITCH` ＋ 測試釘住；打錯字只會讓「托盤點了沒反應」且沒有任何錯誤訊息。

#### 測試（20 個新測試）

- `provider_check`（5）：URL 正規化（補 scheme／localhost 用 http／去尾斜線／拒絕垃圾）、
  狀態碼分類（401／403／404 都算可達、500 要說出是上游問題、0 說「連不上」）、
  **真的對本機伺服器打一次**（可達、200、有延遲）、**真的對死埠打一次**（不可達）、
  命令對空字串給得出人話。
- `proxy::queue`（6）：順序＝來源 priority、停用的來源與模型都不進候選、
  冷卻中的排最後、**學到的協議顯示得出來**、模型清單只列啟用來源的模型並附來源數。
- `tray`（5）：只列三個可接管工具、不可用的來源（停用／沒模型／模型全停用）被隱藏、
  順序＝來源 priority、項目 id 來回轉換與**垃圾輸入全部拒絕**、事件契約（名稱＋鍵名）。

#### 這一段**沒能自動化驗證**的一件事（誠實記錄）

**「用滑鼠點托盤圖示」這一步在這台機器上無法自動化**，原因是三個限制疊在一起：
這台機器的**主工作列是自動隱藏的**（`Shell_TrayWnd` 的 rect 是 `T=1078 B=1128`，
螢幕只到 1080，等於只有 2px 在畫面上）；Windows 11 **不把通知區域的圖示曝露給 UIA**
（`ToolbarWindow32` 的 children 數是 0）；而本機模型**不能讀圖**、OCR 又讀不出純圖示。

實際做過的嘗試與結果：用 tooltip 掃描**成功定位**到我們的圖示（hover x=1420–1510 時
OCR 到 `Token Gateway`），但後續的點擊會先被 Windows 的工作列右鍵選單接走
（讀到的是「工作列設定／工作管理員」那個選單）。因此：

- **有驗到的**：選單內容與 id 對應（單元測試）、事件契約（單元測試）、
  以及**接收端整條路徑**（`buildSelectionReq` → 寫入 → 訊息）—— 用真實 UI 把 Codex
  關掉再開，畫面回了「已接管：deepseek-v4.1-flash @ opencode-go（直連上游 Key 已寫入
  配置文件，僅本機可讀）」，之後 `codex exec -m grok-4.7` 回 `CX-OK`、
  `claude -p` 回 `CC-OK`。
- **沒驗到的**：`tray.rs` 那 3 行 `emit` 在真實點擊下會不會被觸發。
- **失敗模式是良性的**：若事件名或 payload 不符，接收端只會拋錯並顯示紅色 toast，
  不會寫壞任何設定。

**人工檢查（10 秒）**：在工作列（先把滑鼠移到螢幕最下方讓它彈出來）找到
「Token Gateway」圖示 → 按右鍵 → 應該看到「顯示主視窗 / 切換Claude Code / 切換Codex /
切換OpenCode / 退出」；點「切換Codex → opencode-go」後，畫面右下角會出現切換成功的提示。

---

### 0.9.20 來源預設集、複製、匯入／匯出（P1：來源管理對齊 CC Switch）（2026-09-30）

使用者裁示「只要工具功能/體驗完全對齊 CC Switch 即可」，缺口清單在
`docs/CC-SWITCH-PARITY.md`（36 項逐項以程式碼盤點）。這是 **P1 的第一批**：
把「新增／複製／搬移來源」這條路補到跟 CC Switch 一樣順。

#### ① 來源預設集（38 個，全部實測過）

`src-tauri/src/presets.rs`。**每一筆的 `base_url` 都在 2026-09-30 實測**：對
`{base_url}/models` 送一次不帶金鑰的 GET，任何 HTTP 狀態（200／401／403／404）
代表主機與路徑存在。當時**淘汰了 6 個**：`api.sambanova.ai`、`llm.chutes.ai`、
`api.kluster.ai`、`api.lambdalabs.com`（連不上）、`ark.cn-beijing.volces.com`
（逾時）、`api.lingyiwanwu.com`（410 已下線）。

覆蓋：官方（OpenAI／Anthropic／Gemini／xAI）、聚合與推理服務（DeepSeek／OpenRouter／
SiliconFlow／Moonshot／智譜／Z.ai／MiniMax×2／Groq／Mistral／Together／Fireworks／
Cerebras／DeepInfra／Hyperbolic／Nebius／Novita／NVIDIA NIM／Perplexity）、
中國大陸雲（千帆／百鍊／混元／階躍／百川／ModelScope／PPIO）、中轉（Requesty／
AiHubMix／302.AI／GPTSAPI／OpenCode Go）、本機（Ollama／LM Studio）。

**兩個刻意的取捨**：

1. **預設不含模型清單**。CC Switch 的預設會附幾個模型名；我們不填 —— 模型名變動快，
   猜錯比不填更糟。表單上的「取得模型清單」會直接對上游抓 `/models`（`catalog_fetch`）。
   所以預設只負責**連線資訊**（名稱／地址／協議／鑑權）。
2. **兩個本機服務標明未實測**（此機沒安裝 Ollama／LM Studio），備註欄如實寫出來。

數量上我們是 38 個、CC Switch 是 90+：**差距如實留在 §1 的 A2**，不假裝對齊。
要增加就得先實測（`tests` 會擋住格式錯誤，但擋不住「位址寫錯」，那個只能靠實測）。

#### ② 複製來源

`src-tauri/src/providers_io.rs::duplicate_provider`。複製「來源的全部內容」：
連線資訊、已登記模型（含 `context_window` 與 `reasoning`）、每模型價格
（`provider_pricing`）、訂閱時段（`pricing_periods`）。

**刻意不複製**三類，理由寫在模組開頭：`local_keys`（金鑰綁哪個來源是使用者的明確
選擇）、`provider_stripped_fields`／`provider_model_protocol`（那是**學到的行為**，
而且協議記憶的鍵含 `declared_format`，複製品改協議後本來就不適用）、
`model_catalogs`（只是上游 `/models` 快取）。

複製品插在**原本那筆的後面**（後面的 priority 全部 +1），不是丟到列表尾端 ——
複製的用途就是「同一組設定改一個欄位試試看」，兩筆相鄰才好比較。

#### ③ 匯入／匯出

同一個模組。格式 `{"kind":"token-gateway/providers","version":1,…}`，一個來源帶
models／pricing／periods 三張子表。

| 決定 | 內容 | 理由 |
|---|---|---|
| 預設**不含金鑰** | `includes_keys:false` 時 `api_key` 一律空字串 | 匯出檔常常會離開這台機器 |
| 含金鑰要明示 | UI 有開關（開了按鈕就寫「匯出到檔案（含金鑰）」），檔頭 `includes_keys:true`，匯入時再提醒一次 | 不讓「檔案裡有明碼金鑰」變成靜默的事 |
| **匯出由 Rust 寫檔，不靠瀏覽器下載** | `providers_export_file` 寫進系統下載資料夾（`dirs::download_dir()`，沒有就退到 App 資料目錄），回傳完整路徑並顯示在畫面上；同一秒連匯兩次自動加 `-2` | **實機驗證抓到的 bug**：靠 Blob 下載時第一次可以，**第二次** WebView2 會跳原生的「想要下載多個檔案」權限對話框，那個對話框不在 DOM 裡（CDP 點不到）而且蓋住整個 UI —— 使用者會以為 App 壞了。改成後端寫檔：路徑明確、可連續匯出 |
| 另備「複製 JSON」 | 不想產生檔案的人可以直接把 JSON 複製到剪貼簿 | 下載不再是唯一出口 |
| 匯入有三條路 | 檔案路徑（搭配匯出的回傳路徑）、選檔（`<input type="file">` ＋ `FileReader`）、直接貼上 JSON | 這台 repo 只有 `tauri-plugin-opener`，建置走 `--offline`，不新增外掛；三條路都以純前端或既有命令完成 |
| 匯入是 **skip** 語意 | 同 `(name, base_url)` 已存在就跳過，不覆蓋 | 匯入不該是最容易弄壞現有設定的操作 |
| 名稱自動讓開 | 同名的新來源變成 `名字 2` | 兩張同名卡片分不出誰是誰 |
| 匯入大小上限 | 8 MiB（`IMPORT_MAX_BYTES`） | 誤指到巨大檔案時給明確錯誤，而不是把記憶體吃光 |
| 丟錯檔案要說人話 | `kind` 不符／`version` 太新／讀不到檔案都給明確錯誤 | 使用者貼錯檔要看得懂 |

#### 測試（13 個新測試）

- `presets`：id 唯一、每個預設格式正確（https／localhost、協議與鑑權在列舉內、結尾無斜線）、
  命令輸出可序列化且包含使用者實際在用的 oc-go。
- `providers_io`：複製帶走模型／價格／時段並插在原本後面（第二次複製自動加序號）、
  複製**不**帶走金鑰與學到的狀態、匯出預設不含金鑰而要求時才含、
  匯入跳過已存在的 `(name, base_url)`、同名不同網址會讓開、
  匯入到空庫完整還原、roundtrip（匯出→清空→匯入）內容一致、
  丟錯檔案／版本太新要報錯、最小 JSON（只有 name＋base_url）也能匯入、
  **匯出檔真的寫到磁碟且同一秒連匯不覆蓋（`-2`）並能被讀回**、
  **匯出目錄不存在時自己建出來**。

#### ④ 順手修掉：開庫時清掉孤兒列（實機驗證時發現）

驗證匯出檔時發現**匯出只有 26 個模型、`provider_models` 卻有 38 列**。查下去是
**12 列孤兒**：`provider_id` 指向 `providers` 裡已經不存在的 id（1／2／3／4／7／10／12）——
那是早期刪除來源時還沒有級聯清理留下的殘骸。它們永遠讀不到
（`providers.id` 是 AUTOINCREMENT，id 不會被重用），但會讓「這個庫裡到底有幾個模型」
這類計數對不上，匯出的檔案看起來也像漏了東西。

`db/open.rs` 新增 `purge_orphans`，在 `open_and_ensure` 最後清掉六張子表的孤兒列
（`provider_models`／`provider_pricing`／`pricing_periods`／`provider_stripped_fields`／
`provider_model_protocol`／`model_catalogs`）。

**刻意不動 `local_keys`**：金鑰綁到已刪除的來源是保留的設計（請求會回明確的
500「綁定的上游渠道不存在」，比靜默失效好查）。

這件事也**逼出兩個測試的問題**：`learned_protocol_survives_reopen` 與
`stripped_memory_persists_across_connections` 原本用「不存在的 provider_id」當捷徑，
加了清理之後就被正確地刪掉了。兩個測試改成先種一筆真實來源 —— 這才是實況：
學到的記憶一定掛在一個存在的來源上。

---

### 0.9.19 斷路器（cc-switch 的另一半）＋健康狀態與協議記憶上到診斷頁（2026-09-30）

§0.9.16 抄了 cc-switch 的 **failover queue**（來源／協議候選佇列），但它的佇列旁邊
還有一個 **circuit breaker**：某個來源連續失敗就先「跳開」一段時間，不要每個請求都
去撞同一面牆。這一輪補上，並且把「網關記住了什麼」真的畫到診斷頁上（§0.9.17 只做了 API）。

#### 規則

| 項目 | 值 | 為什麼 |
|---|---|---|
| 失敗門檻 | **連續 3 次** | 單次失敗（上游抖一下）不該讓來源被冷凍 |
| 冷卻時間 | **60 秒** | 夠涵蓋上游重啟／網路瞬斷，又不會讓真正掛掉的來源被永久跳過 |
| 冷卻期間的行為 | **排到候選最後，不是跳過** | 這是本機工具唯一的出口；全部候選都被跳過時使用者只會看到失敗。排最後＝有健康來源時省下必然失敗的等待，沒別的來源時行為與從前**完全一樣** |
| 什麼算失敗 | **連不上**（→502）與**上游 5xx** | 這兩者才是「來源不健康」 |
| 什麼**不**算 | **所有 4xx**（含 `400 ModelProtocolUnsupported`） | 4xx 代表來源活著、只是這個請求有問題。把協議不支援也算進去，會讓協議換手自己把自己絆倒（`grok-4.7` 第一次必定先吃一個 400 才知道要換協議，三次就把好來源冷凍了） |
| 恢復 | 冷卻期滿自動放行**一次**（half-open）：成功即歸零，失敗立刻再跳開 | 不必等三次才知道它還是壞的 |
| 儲存位置 | **記憶體**（`HashMap`，範圍＝資料庫） | 與「學到的協議」刻意相反：協議是上游的客觀事實，重啟不該忘；斷路器是短命的健康判斷，重啟代表重新開始 |

範圍用**資料庫路徑**而不是行程全域：`provider_id` 只在單一資料庫內唯一，
正式路徑只有一個庫（＝一個網關），而測試各起各的網關，因此互不干擾。

#### 檔案

| 檔案 | 改動 |
|---|---|
| `src-tauri/src/proxy/health.rs` | **新增**：`HealthRegistry`（`record_failure`／`record_success`／`is_open`／`snapshot`）、`order_by_health`（穩定分割：健康的在前）、`for_db`（依資料庫取表） |
| `src-tauri/src/proxy.rs` | `ProxyCtx` 多了 `health`；候選排完先 `order_by_health`；迴圈裡按結果記成功／失敗（5xx 與連線失敗記失敗，其餘 4xx 記成功） |
| `src-tauri/src/commands/diagnostics.rs` | **新增命令 `source_health`**：來源名稱／協議設定＋開關狀態／連續失敗次數／剩餘冷卻秒數／上次失敗與上次成功的相對時間 |
| `src/components/diagnostics/SourceHealthList.tsx` | **新增**：診斷頁的「來源健康狀態」區塊 |
| `src/components/diagnostics/ProtocolMemory.tsx` | **新增**：診斷頁的「協議記憶」區塊（宣告協議 → 實際協議） |
| `src/components/diagnostics/DiagnosticsPage.tsx` | 掛上上面兩個區塊（含 10 秒自動更新與「重新整理」） |
| `src/lib/apiTypesDiagnostics.ts` | 補 `LearnedProtocol`、`SourceHealth`，`TraceSummary` 補上原本缺的 `learned_protocols` |

> **順手修掉的一個小 bug**：改寫嘗試迴圈前，`Ok(r) if r.status().is_server_error() && i + 1 < total`
> 這條守衛讓「最後一個候選回 5xx」掉進下一條 `Ok(r)` 分支，於是**上游 5xx 也被拿去
> `learn_format`**（把一個失敗的協議記成學到的協議）。現在 5xx 與成功分成兩個分支。

#### 測試

- 單元（`health.rs`，7 個）：達門檻才跳開、成功即歸零、冷卻期滿放行且再失敗立刻跳開
  （用 60ms 冷卻實測，不是靠想像）、每個來源各自記帳、跳開者排最後但**沒有被丟掉**、
  全部跳開時維持原順序、錯誤訊息截斷 120 字。
- 端到端（`e2e_circuit_breaker_stops_probing_dead_source`）：一個死來源（`127.0.0.1:9`）
  綁著 Key ＋ 一個活的備援來源登記同一個模型。前三次請求都**先撞死來源一次**
  → 都拿到 200（換手成功），且每次各多一筆 `upstream_status=502` 的追蹤；
  第四次因為已達門檻 → 不再產生任何 502（直接走備援）。**觀測點是這個測試自己的
  資料庫**（不是行程內的狀態），所以這是「行為」的證據而不是「實作」的證據。

#### 已知限制

- 冷卻中的來源**沒有手動「立刻重試」按鈕**：60 秒後自動放行，重啟網關也會清空。
- `source_health` 只列「有紀錄」的來源：沒被請求過的來源不會出現（沒有證據就不假裝健康）。
- 佇列順序就是**來源清單的順序**（`providers` 的 `priority`／`id`，UI 可以拖曳重排），
  但它是**全域**的：cc-switch 可以為**每一個工具**各排一條佇列，我們目前只有一條
  （差異只在「同一個模型被多個工具用到時，排序無法分別調整」）。
- 想要「同一個 `base_url`／Key 出現兩次」（例如一條宣告 chat、一條宣告 responses）
  是**允許的** —— `providers.base_url` 沒有唯一性限制，兩條都會登記同一個模型而一起
  進入候選。不過現在**不需要**這樣做：逐模型的協議自動回退（§0.9.16）已經處理掉
  「同一端點、不同模型要吃不同協議」這件事。

---

### 0.9.18 Claude Code 的「模型識別」策略：cc-switch 的別名法 vs 我們的真實名稱法（2026-09-30）

objective 第 3 項要的取捨，寫清楚。

**問題**：Claude Code 只認識 Claude 家族的模型。當我們把來源模型（`deepseek-v4.1-flash`）
寫進 `ANTHROPIC_MODEL` 時，它會：

1. 在 stderr 抱怨 `"deepseek-v4.1-flash" isn't described by this version's model catalog`，
2. **只假設 200k 上下文**（`auto-compact keeps this session within 200k tokens`），
   於是 1M 級的來源模型會被提早自動壓縮。

**cc-switch 的做法（讀它的 README/FAQ）**：本機路由開啟時，寫進 Claude Code 設定的是
**固定的 Claude 別名**（例如 `claude-sonnet-5`），`/model` 選單仍顯示真實模型名，
真正的來源位址／Key／模型全留在 cc-switch 裡；請求日誌能看到「requested model → actual model」。
也就是：**讓 Claude Code 相信自己正在用一個它認識的 Claude 模型**。

**我們的做法**：寫**真實的來源模型名**，另外在知道模型真實視窗時補
`CLAUDE_CODE_MAX_CONTEXT_TOKENS`（§0.9.14）。

| | cc-switch 別名法 | 真實名稱法（我們） |
|---|---|---|
| Claude Code 的內建模型檔 | ✅ 用得到（含上下文視窗、可用功能） | ❌ 未知模型，只假設 200k |
| 那個 200k 假設 | ✅ 用別名的真實視窗（但**別名與來源模型不一致時就是說謊**：宣稱 1M、實際 200k 會靜默超長） | ⚠️ 要靠 `CLAUDE_CODE_MAX_CONTEXT_TOKENS` 修正，而它需要 DB 有 `context_window` |
| 用量歸屬／除錯 | ❌ Claude Code 自己的紀錄寫的是別名（要對照路由層才知道真實模型） | ✅ 請求、日誌、用量頁看到的都是真實模型 |
| 需要什麼基礎設施 | 路由層的 alias→real 對映 | 已在做（真實名稱直通） |
| 現況 | 我們**已經有能力**支援：`provider_models` 的 `display_name`（別名）→ `actual_model`（真實），網關本來就會把顯示名改寫成實際名 | 已實作，但**休眠**：這台機器 38 筆模型的 `context_window` 全是 NULL |

**結論（本輪的選擇）**：維持真實名稱法 —— 這個 App 的整個價值就是「用量要真實」，
用別名會讓 Claude Code 自己的紀錄與用量歸屬都變成假名。要讓真實名稱法真正生效，
缺的只是**把模型的 `context_window` 填起來**（在模型編輯裡填，或之後從 models.dev 匯入）；
在那之前 200k 的假設仍然存在。

**兩者可以並存**：若某個客戶端真的對未知模型名強硬拒絕，則在該來源的模型上填一個
Claude 別名當 `display_name`（實際模型不變）即可走別名法；這是既有機制，不必改程式。

---

### 0.9.17 學到的協議改為落庫（schema v9）＋診斷頁看得見（2026-09-30）

§0.9.16 的第一版把「這個來源的這個模型該用哪個協議」只放在**行程記憶體**，
於是網關每次重啟，每個「宣告協議與實際不符」的模型都要**重踩一次 400**。
這與 `provider_stripped_fields` 當年的問題**一模一樣**（那句註解就寫在
`proxy/retry.rs` 裡），所以用同一種做法解決：

- 新表 `provider_model_protocol (provider_id, model, declared_format, actual_format,
  learned_at)`，主鍵是前三欄 —— `declared_format` 進主鍵是刻意的：使用者把來源協議
  從 `openai-chat` 改成 `openai-responses` 是**明確的設定變更**，不可以被舊的學習
  結果蓋掉（這是 §0.9.16 被 B6 回歸測試逼出來的那條規則，現在由 schema 保證）。
- `SCHEMA_VERSION` 8 → 9（純新增表，走 `CREATE TABLE IF NOT EXISTS`，無需 ALTER）。
- 讀寫都在 `trace/protocol.rs`（`remember_protocol` / `load_protocol` /
  `clear_protocol` / `protocol_summary`），與 `stripped.rs` 同一種風格：
  短命連線、錯誤一律吞掉（觀測設施不得影響請求）。
- 刪除來源時一併清掉（`providers.rs`），`provider_stripped_clear`（UI 的
  「重設上游能力記憶」）也一起清 —— 兩者都是「網關對這個渠道的上游能力記憶」。
- **診斷頁看得見**：`trace_summary` 多了 `learned_protocols` 欄位
  （來源／模型／宣告協議／實際協議）；本輪先用 API 驗證，前端渲染見 §0.9.19。

新增測試：`learned_protocol_persists_per_db_and_declared_format`、
`learned_protocol_survives_reopen`（後者直接 `SELECT` 新表，證明是真的落庫而不是行程記憶），
`db::tests::fresh`／`migrate` 的核心表清單也補上新表名。

---

### 0.9.16 來源／協議自動換手：抄 cc-switch 的 failover，補它沒做的協議回退（2026-09-30）

你的裁示是「照抄，並且補他沒做的」。參考 [cc-switch](https://github.com/farion1231/cc-switch)
的做法：它的「上游格式」也是**逐來源**選的（跟我們一樣），真正解掉「某些模型不能用」的是
**Auto-failover**（為每個工具排一條來源佇列，失敗就換下一個，帶 circuit breaker）與
**Rectifier**（修上游吃不下的請求）。它**沒有**做「同一個來源換協議重試」那一格，我們補上。

#### 做成了什麼

把「候選（來源, 協議）」排成一條佇列，依序嘗試，失敗就往下換：

```
① 上次學到的協議（見下方快取）
② 綁定來源的宣告協議
③ 綁定來源的另一種協議（Responses 入站時 chat ↔ responses）
④ 其他也登記了這個模型的來源（各自：宣告協議 → 另一種協議，依 priority/id）
```

觸發換手的條件：**上游說「此模型不支援本協議」**、**連線失敗**、**上游 5xx**。
其他 4xx（內容政策、參數錯…）維持原樣直接回給使用者 —— 那些換來源只會多花錢、不會變好。

- 程式位置：`proxy/failover.rs`（新的，規劃候選＋學習快取）、`proxy.rs::proxy_handler`
  （嘗試迴圈）、`proxy/retry.rs`（`SendOutcome`：把「協議不支援」分類成訊號而不是死路）、
  `proxy/forward.rs::resolve_model_providers`（候選來源清單）。
- **學習快取**：某個（資料庫, 來源, 模型, 宣告協議）用哪個協議成功過就記住，下次第一個試它。
  鍵刻意帶 **db_path**（`provider_id` 只在單一資料庫內唯一）與 **宣告協議**
  （使用者把來源協議從 chat 改成 responses 是明確的設定變更，不可以被舊的學習結果蓋掉 ——
  這一點是實作時被既有測試 `e2e_forward_auth_and_log`（B6 回歸）逼出來的）。

#### 對 opencode-go 的實際效果

| 模型 | 只有哪個端點有架 | Codex（Responses 入站） | Claude Code（Anthropic 入站） |
|---|---|---|---|
| deepseek-v4.1-flash／v4-flash | 兩邊都有 | ✅ 第一個候選就過 | ✅ |
| grok-4.7／gpt-6-luna／muse-spark | responses | ✅ **先試 chat 被拒 → 自動換 responses** | ❌ 仍無解（見下） |
| mimo-v2.6-pro／flash | chat | ✅ 第一個候選就過 | ✅ |

**Claude Code 仍然無解**：Anthropic 入站的請求**只生得出 chat 請求體**
（`Anthropic → Responses` 沒有翻譯器，矩陣直接回 `E_ANTHROPIC_UNSUPPORTED`），
所以 responses-only 的模型在 Claude Code 永遠換不過去。要解只能另寫一個
Anthropic→Responses 翻譯器（獨立工程，這一輪沒做）。

#### 誠實揭露

- **學習快取只放記憶體**：網關重啟後，每個「協議與宣告不符」的模型要多付一次失敗探測
  （一次 400，不計費）。要持久化就再加一張表，之後可做。
- **沒有真的做 circuit breaker**：cc-switch 有「連續失敗就暫時跳過某個來源」，
  我們目前是**每次請求都依序試**。代價是第一名來源掛掉時，每個請求都先撞一次。
- 換手只發生在「已經決定要轉發」之後：模型白名單、直連模式的語意**完全沒動**
  （`直連模式僅允許來源「X」登記的模型` 那條規則照舊）。

#### 測試

| 測試 | 釘住什麼 |
|---|---|
| `failover::tests::responses_inbound_gets_responses_fallback_for_chat_provider` | Responses 入站 + chat 來源 → 候選 `[chat, responses]` |
| `failover::tests::mixed_provider_gets_chat_fallback_for_chat_only_models` | `mixed` 來源也要有一條 chat 候選（chat-only 模型用） |
| `failover::tests::anthropic_inbound_has_no_responses_fallback` | Claude Code 不排無效候選（翻 responses 矩陣不支援） |
| `failover::tests::learned_format_goes_first_and_dedupes` | 學到的協議排第一，且不重複 |
| `failover::tests::other_providers_come_after_bound_one` | 來源換手是備援，排在綁定來源之後 |
| `failover::tests::learn_format_is_scoped_by_db_and_declared_format` | ★ 快取要分資料庫、分宣告協議 |
| `e2e_responses_inbound_falls_back_to_responses_endpoint` | 端到端：假上游 chat 回真實的 `ModelProtocolUnsupported`、responses 正常 → 客戶端拿到 200 與 responses 內容；**路徑順序證明**先打 chat；第二次請求不再打 chat（學習生效） |
| `e2e_failover_to_next_provider_when_first_is_dead` | 端到端：第一個來源連不上 → 換到第二個也登記了同一模型的來源並成功 |
| （既有）`e2e_forward_auth_and_log` | B6：chat 請求打到 responses 來源**仍必須明確 400**，不被我的候選佇列救走 |

---

### 0.9.15 讓 `mixed` 真的「Responses ＋ chat 兩邊都通」（2026-09-30）

你問的：「mixed 同時支援 Responses+chat 是否可以做」——**可以做，而且兌現方式就是一行**。

原因：`proxy/forward.rs` 的規則是「翻譯路徑固定打 `/chat/completions`；**直通保留原始路徑**」，
所以 `TransKind::None` 就是「原樣打過去」。舊的矩陣把 `(Responses, Mixed)` 映射成
`responses_to_chat` —— 明明是「兩邊都有架」的渠道，卻硬把 Codex 的 Responses 翻成 chat
送到上游，於是 responses-only 的模型一律 `ModelProtocolUnsupported`。現在改成：

```rust
(Responses, T::OpenAiChat) => Ok(TransKind::ResponsesToChat),
(Responses, T::Mixed)      => Ok(TransKind::None),   // 原樣打上游 /responses
```

- `T::Mixed` 的 doc 改寫成「兩種端點都原樣直通」，並註明**逐模型**的端點支援仍由上游決定。
- 行為契約表（`proxy/tests/matrix.rs`，30 格全列舉）同步：`(Responses, T::Mixed, NONE)`。
- 前端不必改：`providersTypes.ts::API_FORMATS` 本來就有 `mixed` 可選。

**照實說的限制**：這是**渠道級**的「兩邊都通」，不是**模型級**的。oc-go 的模型是分裂的
（grok-4.7／gpt-6-luna／muse-spark 只在 responses；mimo-v2.6-* 只在 chat；deepseek-* 兩邊都有）。
所以把 oc-go 標成 `mixed` 之後：

| 客戶端 | 走哪個端點 | 結果 |
|---|---|---|
| Codex（Responses） | `/responses` 直通 | grok-4.7／gpt-6-luna／muse-spark／deepseek 可用 ✅；**mimo-* 會 400** ❌ |
| Claude Code（Anthropic） | 仍翻成 chat | deepseek／mimo 可用 ✅；grok／muse 仍然 400 ❌（Anthropic→Responses 沒有翻譯器） |

要**逐模型**都對，還是得靠方案 3（上游回 `ModelProtocolUnsupported` 時用另一種協議自動重送
並記住結果）。這一輪先把你問的 mixed 做掉。

---

### 0.9.14 把來源模型的真實上下文視窗告訴 Claude Code（2026-09-30）

你問「好繼續做」的那一項：Claude Code 對**它不認識的模型**只假設 200k（官方自己的警告：

```
"deepseek-v4.1-flash" isn't described by this version's model catalog; …
auto-compact keeps this session within 200k tokens (the context window it assumes);
if the model accepts more, append [1m] to the model name for 1M, or set
CLAUDE_CODE_MAX_CONTEXT_TOKENS to its real window
```

），所以 1M 級的來源模型會被**提早自動壓縮**。

改法（`tools/apply.rs::claude_apply` 多了 `context_window` 參數）：接管時若該模型在
App 裡有填 `context_window`，就一起寫 `env.CLAUDE_CODE_MAX_CONTEXT_TOKENS = <值>`；
**不知道就不寫** —— 不編一個數字出來。切回原生時（`tools/native.rs`）連它一起移除：
那是為「網關上的來源模型」寫的，回到官方 Claude 留著只會把官方模型的視窗寫成錯的。

**誠實揭露：這個功能現在是「接好了但還沒通電」。** 這台機器 `provider_models` 共 38 筆，
`context_window` **有值的 0 筆** —— 包含 oc-go 的 9 個模型（`deepseek-v4.1-flash` 等全是 NULL）。
所以要真的生效，得先在 App 的模型編輯裡填上真實視窗（或在來源匯入時帶進來）。
在那之前 Claude Code 仍然只假設 200k，行為與改動前一樣。

---

### 0.9.13 「Model does not support this protocol」的真因 ＋ 接管時就指定模型（2026-09-30）

你回報：Claude Code／OpenCode 走 opencode-go 時，除了 DeepSeek 系列以外的模型幾乎全掛，錯誤是

```
{"type":"error","error":{"type":"ModelProtocolUnsupported","message":"Model does not support this protocol."}}
data: {"error":{"param":"","type":"server_error","message":"Streaming response failed: [400] Invalid request parameters"}}
```

#### 真因：同一個模型只在一種端點上架（不經網關，直接量上游）

拿 opencode-go 的 Key 直接打 `https://opencode.ai/zen/go/v1`（要帶 `x-opencode-session`，
不然上游回 `MissingSessionID`；另外 UA 不討喜時 Cloudflare 會回 `error 1010`，用
`codex_cli_rs/…` 這種 UA 就進得去），每個模型各打一次 `chat/completions` 與 `responses`：

| 模型 | `chat/completions` | `responses` |
|---|---|---|
| deepseek-v4.1-flash | 200（串流／非串流都 200） | 200（串流／非串流都 200） |
| deepseek-v4-flash | 200 | 200 |
| grok-4.7 | **400 ModelProtocolUnsupported**（串流／非串流都是） | 200 |
| gpt-6-luna | **400 ModelProtocolUnsupported** | 200 |
| muse-spark-1.3-contributor | **400 ModelProtocolUnsupported** | 429 `rate_limit_exceeded`（重測時被限流；第一次量到 200） |
| mimo-v2.6-pro | 200 | **400 ModelProtocolUnsupported** |
| mimo-v2.6-flash | 200 | **400 ModelProtocolUnsupported** |

所以「Model does not support this protocol」不是參數寫錯，是**你打錯端點**的字面意思：
這個模型沒有在你呼叫的那個路徑上架。

#### 為什麼「目前能正常使用的模型只有 DeepSeek 系列」

- DB 事實：這台機器上 7 個來源的 `providers.api_format` **全部是 `openai-chat`**。
  `proxy/matrix.rs::resolve_trans_kind` 於是對 **Codex（Responses）** 與
  **Claude Code（Anthropic）** 進來的請求，都翻譯成 `chat/completions` 送上游。
- 只有 chat 端點有架的模型才活得下來 → responses-only 的 `grok-4.7`／`gpt-6-luna`／
  `muse-spark-1.3-contributor` **必掛**；`deepseek-*` 兩邊都有架，所以「怎麼切都行」，
  與你的觀察完全一致。
- Claude Code 這側更硬：`resolve_trans_kind` 的
  `(Anthropic, OpenAiResponses | Gemini | Unknown) => Err(E_ANTHROPIC_UNSUPPORTED)`
  代表 Anthropic 進來的請求**永遠不可能**轉成 Responses 送上游 —— Claude Code 只能配
  chat 端點有架的模型，沒有例外。

> 要讓 **Codex** 用 responses-only 的模型，得把那個來源宣告成 `openai-responses`
> （網關支援；`mixed` 仍會翻成 chat）。這是**來源層級**的設定，做法是另開一個共用
> base_url／Key 的來源項（先例：`自訂來源 A`／`自訂來源 B`／`自訂來源 C` 共用同一個 URL）。
> 這一輪**沒有動**它，因為那會改到你既有來源的語意。

#### `mimo-*` 的第二個症狀是另一件事（還沒收乾）

`Invalid request parameters` 與協議無關。實測：最小 body、帶 `tools`、帶 `reasoning.effort`、
`store:false` ＋ `include:["reasoning.encrypted_content"]`、回放 reasoning item、113 KB 輸入、
超大 `max_output_tokens=100000`、甚至 `input_image` 內容 —— **全部 200**（串流與非串流都試過）。
只有 Codex 那條 937 KB 的真實 transcript 會 400，而且錯誤訊息裡**沒有欄位名**，
`proxy/strip.rs::parse_unknown_fields` 解析不出來 → 相容策略放棄
（trace 的 note：`上游 400 且無法解析出拒收欄位名（相容策略失效）`）。

trace 的位元組數（937669 → 937673 → 937676 → 937678 → 937689，差額正好是模型名的長度差）
證明那幾次重試是**同一段對話換模型名**反覆送 —— 也就是你在對話中換了模型。剩下的頭號嫌疑是
**上一個模型的對話殘留**（真實的 encrypted reasoning／function_call 鏈）在新模型上不合法。
**實務做法：換模型家族時開新對話。**（另一條路：讓網關在上游 400 又解析不出欄位名時，
留下截斷的請求本體；目前 `proxy_trace.body_hex` 只在入站解析失敗時才寫。）

#### 這一輪改的東西

**1｜Claude Code 的模型檔位改成「真的跑得動的來源模型」。** 你原本的
`~/.claude/settings.json` 是

```
ANTHROPIC_MODEL                        = muse-spark-1.3-contributor   ← responses-only，必掛
ANTHROPIC_DEFAULT_FABLE_MODEL          = grok-4.7                     ← responses-only，必掛
ANTHROPIC_DEFAULT_{HAIKU,SONNET,OPUS}_MODEL = deepseek-v4.1-flash     ← 正常
```

App 的規則本來就是**留空的檔位跟隨主模型**（`claudeMapOpt` 的 `eff()`：`sel || effModel`），
會寫成上面那樣是因為對話框裡存了**五個檔位的個別覆寫值**（`tg:lastSwitch:v1:claude`
的 `claude_map`），App 每次都照抄。所以修法是：在切換對話框把「預設模型」「Fable」兩格
改回可用的來源模型（或清成「跟隨主模型」）再接管一次，五格就全部一致。
**程式碼這側不需要改** —— 這正是「指定哪個模型」的既有行為，錯的是存下來的值。

**2｜OpenCode 接管時一併把模型釘進 `opencode.json`。** 這是真的程式改動。
以前只寫 provider 段與模型清單，`opencode run` 會用**它自己的預設模型**
（`claude-sonnet-4-6`）去打網關 → 網關不認得 → 看起來像「接管沒生效」。現在接管時多寫一行
頂層 `"model": "tokengateway/<你選的模型>"`（`tools/apply.rs::opencode_apply`），
`switch.rs` 的計畫頁與警告文案也跟著改（不再是「切換後需在 opencode 內手動選中模型」）。
切回原生時（`tools/native.rs::opencode_native`）**只移除開頭是 `tokengateway/` 的頂層 model**，
你自己設定的 `model`（例如 `anthropic/…`）不動 —— 而且 `restore_native_to_port` 是
「先還原備份再原生化」，所以原本的 `model` 會從備份回來。

---

### 0.9.12 讓 DeepSeek Harness 的用量出現在用量頁（歷史回填，2026-09-30）

使用者問：「DSH 這工具是否可以直接檢測用量？」——**能力有**（App 的 M5 歷史回填
早就會離線讀各工具自己的本機資料），**但 DSH 沒接進去**。使用者選擇走回填這條路
（不必改 DSH 設定）。

#### 為什麼不用碰 zstd

DSH 的完整逐次呼叫紀錄在 `~/.dsh/sessions/**/session.v3.jsonl.zstd`（zstd 多框架，
實測該檔 5797 個 frame、抽 60 個都能獨立解壓），但**不必**為此引入 zstd 相依 ——
DSH 另外把用量投影成**純 JSON**：

```
~/.dsh/storages/session_projcache/sessions/<session-id>.json
  record.rows.tokenUsage.val.totals = {
    uncachedInputTokens, outputTokens, cacheReadTokens, cacheWriteTokens }
```

實測 48 個投影檔 **48/48** 都有這個區塊，全部加起來
`uncached 10,480,397 / output 2,829,399 / cacheRead 527,193,600`。

#### 口徑（與其他來源對齊）

DSH 把 prompt 拆成 `uncachedInputTokens` 與 `cacheReadTokens` 兩塊**互斥**的數字，
所以：

- `in_tok = uncachedInputTokens + cacheReadTokens`（＝總輸入，含快取那一塊）
- `cache_read = cacheReadTokens`（是 `in_tok` 的子集 —— 與 Codex 的
  `input_tokens` / `cached_input_tokens`、網關的同一組欄位同義）
- `out_tok = outputTokens`
- `ts` = 投影檔的**最後更新時間**（＝會話最後活動），與 OpenCode 用
  `time_updated` 一致（投影裡只有 `identity.createdAt`＝會話開始與 `seq`，沒有單次時間）
- one session = one row（與 OpenCode 同口徑）；`model` 取
  `rows.modelSelection.val.lastUsed.model`，中途換模型時代表最後一次；
  真的沒有就記 `(DSH 未記錄模型)` —— **不靜默丟掉用量**

#### 改動

| 位置 | 改動 |
|---|---|
| `history/mod.rs` | `collect_files_with_ext` 抽出共用；新增 `collect_json_files`；模組說明加第四個來源 |
| `history/parse.rs` | 新增 `parse_dsh_session`（檔頭有完整口徑說明） |
| `history/scan.rs` | 掃描 `.dsh/storages/session_projcache/sessions/*.json`，一檔一會話 |
| `history/import.rs` | 新增 `want("dsh")` 分支（檔案級 (mtime,size) 冪等，與 claude／codex 同機制） |
| `components/usage/usageLogs.tsx` | 勾選框預設含 `dsh`；說明文字補上 DeepSeek Harness |

**測試**：`dsh_parse_session_totals`（口徑：輸入含快取）、
`dsh_parse_skips_empty_and_bad`（空會話不記／缺投影記 bad／缺 model 用佔位）、
`dsh_scan_counts_projections`（只算 `.json`、一檔一會話）；並更新
`scan_lists_tools` 為四個工具。

**為什麼「投影檔一直被改寫」不是問題**：進行中的會話每次掃到都會重算那一列
（先按 `import_path` 刪、再插），所以數字會跟著會話長大 —— 這是刻意的。

#### 實機驗證（2026-09-30 15:30，安裝 §9.13 後）

掃描對話框顯示 `DeepSeek Harness 48 文件 · 448.1 KB · 48 會話`，按「開始回填」後：

```
寫入 1093 筆（claude: 271 筆 / 288,143 tokens；codex: 716 筆 / 165,922,008 tokens；
opencode: 82 筆 / 110,645,339 tokens；dsh: 24 筆 / 563,640,975 tokens）。
去重跳過 1949，空行跳過 34。
```

**48 個檔案只寫進 24 列，不是漏記**：另外 24 個會話的四個計數器**全是 0**
（開過會話但沒呼叫任何模型），被 `commit_rows` 依既有規則「總 token ≤ 0 就跳過」濾掉。
抽驗原始 JSON 確認鍵名與非零檔完全一致、值確實是 0：

```json
"totals": {"uncachedInputTokens": 0, "outputTokens": 0,
           "cacheReadTokens": 0, "cacheWriteTokens": 0}
```

用量頁驗證：

| 檢查 | 結果 |
|---|---|
| 本機工具篩選器 | **7 個選項全在**：Claude Code／OpenAI Codex／OpenCode／Hermes Agent／**DeepSeek Harness**／Cursor／Antigravity（約束二：不因新增來源而少掉任何工具） |
| 篩選 DeepSeek Harness（今日） | 9 筆 / 583,060,706 tokens / 快取命中率 49.6% |
| 最近請求 | 出現 `DeepSeek Harness` 列，模型 `deepseek-v4.1-flash`、`deepseek-v4-flash` |
| 趨勢圖資料（照抄 `trend_by_app` 的 SQL 驗算） | 15:00 桶 `dsh 1 筆 / 583,060,597 tokens`；今日各 app 序列以 dsh 583,060,706 最高 —— 圖上會有 DSH 的柱子 |

#### 兩個誠實提醒（都不是這次改壞的）

1. **Claude 回填列的 `in_tok` 全是 0**：不是解析錯。Claude Code 走非 Anthropic 後端時，
   它自己的 jsonl 就寫 `input_tokens: 0`（抽驗兩檔、40／39 行全部如此）。所以那些列
   只反映輸出 token。
2. **少數列可能與網關列並存**：3 個歷史 gateway 200 列（in=8267／7488／9015）與對應會話
   的 import 列同時存在。去重窗是 ±120 秒，而 import 的 `ts` 用**檔案 mtime**（會話最後
   活動）、gateway 列用**請求發生時間**，兩者差了整個會話長度所以沒被視為重複。
   量級約 0.004%，且是既有 M5 機制的行為。


### 0.9.11 其他工具（Claude Code／OpenCode）在兩個方向切換的實測（2026-09-30）

使用者要求：「確保使用其他工具在官方來源與指定來源間切換，不會出現同樣錯誤」。

#### 一、切換機制本身：沒有 Codex 那種「寫出一個工具會拒收的設定」問題

逐一檢查兩個工具被寫入的鍵，都是工具自己認識的：

| 工具 | 接管寫入 | 原生還原移除 | 有沒有「保留清單」那種地雷 |
|---|---|---|---|
| Claude Code | `env.ANTHROPIC_BASE_URL`／`ANTHROPIC_AUTH_TOKEN`／模型映射 | 非官方 base URL、auth token、非 `claude-*` 的模型鍵 | **沒有**（Claude 忽略不認識的環境變數） |
| OpenCode | `provider.tokengateway.*` | 網關 baseURL 與 `sk-local-` 金鑰 | **沒有**（provider id 可自訂） |

#### 二、但實測發現**兩個工具在切換後都還有各自的問題**（都不是設定被寫壞）

| 工具 | 方向 | 實測結果 |
|---|---|---|
| **Claude Code** | → **官方** | `claude -p "…"` → **`Not logged in · Please run /login`**（exit 1、1 秒）❌ |
| Claude Code | → 指定（接管） | `claude -p` → `API Error: 400 ModelProtocolUnsupported` ❌ |
| **OpenCode** | → 指定（接管） | `opencode run --model tokengateway/…` → 網關回「直連模式僅允許來源「NIM」登記的模型，`deepseek-ai/deepseek-v4-flash-0731` 不在其清單內，請求不會轉發」❌ |
| OpenCode | → 官方（未接管） | `opencode run` → `Error: Anthropic API key is missing` ❌ |

##### (a) Claude Code 的官方方向：就是使用者問的那種登入錯誤 —— **已修（改為明講）**

根因不是 App 寫壞設定，而是**這台機器從來沒有 Anthropic 官方憑證**：
`~/.claude/.credentials.json` 不存在、`.claude.json` 也不存在、沒有 `ANTHROPIC_API_KEY`。
使用者一路都只透過網關用它，所以從來不需要官方登入。App 不能替他登入（帳號層級），
但可以讓他不要撞上一個看起來像 bug 的錯誤。

**修法**：`native.rs` 新增 `official_auth_hint(app, home, settings_text)`，還原
（關閉）完成後追加一句警告，例如：

> ⚠️ 這台機器找不到 Anthropic 官方登入憑證（…\.credentials.json 不存在、也沒有
> ANTHROPIC_API_KEY）：切回官方後請先執行 `claude` 完成 /login，否則它會回
> 「Not logged in」

Codex 有 `~/.codex/auth.json`（ChatGPT 登入）所以不會觸發 —— 這正是 Codex 兩個
方向都正常的原因。規則：憑證檔存在且非空、或設定裡有 API key，就不提醒。

##### (b) Claude Code 的指定方向：模型本身無法路由 —— **待你決定**

網關日誌 `#4423`：`app=claude model=muse-spark-1.3-contributor status=400 prov=None`
—— 模型對不到任何 provider，所以請求根本沒出去。同一份設定裡另外兩個鍵
（`ANTHROPIC_DEFAULT_{SONNET,HAIKU,OPUS}_MODEL = deepseek-v4.1-flash`）是**可以**
路由的（同日 `app=claude deepseek-v4.1-flash st=403`，那次 403 是我用 python
直打的 User-Agent 被 Cloudflare 擋，見下）。也就是說卡住的是
`ANTHROPIC_MODEL` 這一個鍵。

##### (c) OpenCode：接管沒有真的把它導到網關 —— **待你決定**

接管只把 `provider.tokengateway.options.baseURL` 指到網關，**沒有選定 provider／
模型**，所以 `opencode run` 仍然用它自己的預設 `claude-sonnet-4-6`（Anthropic）
→ 沒有 Anthropic 金鑰 → 失敗。即使手動指定 `tokengateway/…`，設定裡那兩個模型
（`deepseek-ai/deepseek-v4-flash-0731`、`moonshotai/kimi-k3`）也不在網關認可的
NIM 清單內，請求照樣被擋。

#### 三、順手釐清的一件事：Cloudflare 1010 不是網關壞了

我用 python 直接打網關的 `/v1/messages`、`/v1/chat/completions` 時拿到
`403 Access denied | opencode.ai used Cloudflare to restrict access`（error 1010）
—— 那是**我的探測程式**的 User-Agent 被 opencode.ai 的 Cloudflare 規則擋掉，
不是網關或 App 的問題：同一時間 Codex 走 `/v1/responses` 完全正常
（`#4432 prov=22 deepseek-v4.1-flash st=200 2,379ms`）。用工具自己的 CLI 就沒有
這個問題。


### 0.9.10 真實事故：從官方來源切回指定來源後「無法登入」（2026-09-30）

使用者回報：把工具從官方來源切成指定來源後，還是會出現登入錯誤。

真正的錯誤（`codex exec` 一字不差）：

```
Error loading config.toml: model_providers contains reserved built-in provider IDs: `openai`.
Built-in providers cannot be overridden. Rename your custom provider (for example, `openai-custom`).
```

Codex **拒絕載入整份 `config.toml`**，所以任何指令都做不了事，表現成「無法登入」。

#### 因果鏈（每一步都有實測證據）

| # | 環節 | 證據 |
|---|---|---|
| 1 | §0.9 的「關閉 → 回到原生來源」把 Codex 設成 `model_provider = "openai"` | 設定檔內容 ✅ |
| 2 | Codex 用它跑過對話後，**自己的**對話資料庫就記下這個 provider | `state_5.sqlite` → `threads.model_provider` 出現 `openai`（1 個對話）✅ |
| 3 | App 為了「舊會話要一直能續用」，把 `SELECT DISTINCT model_provider` 的名字**聯集**進要保留的別名段 | `codex_legacy_providers_report` ＋ `codex_alias_ids` ✅ |
| 4 | 於是下一次接管替它**建出** `[model_providers.openai]`，指到網關 | 設定檔出現第 9 個段（原本 8 個）✅ |
| 5 | Codex 認為那是內建 id、不可覆寫 → **整份設定載入失敗** | 上面那段錯誤訊息 ✅ |

也就是說：**又是「回到原生來源」這個功能自己養出來的**（同 §0.9.9 的
`backup_has_known_route`）。差別是這次它會讓工具**完全不能用**。

#### 內建 id 清單是實測出來的

在 `model_provider` 仍指向網關的情況下**額外定義**一段，看 Codex 是否拒絕載入
（檢查針對**所有**已定義的段，不限當前 provider —— 上面的真實設定就是
`model_provider = "custom"` 卻仍被 `openai` 段擋下）：

| id | 結果 |
|---|---|
| `openai`／`ollama`／`lmstudio` | ❌ **保留（禁止定義）** |
| `oss`／`azure`／`aws`／`amazon-bedrock`／`anthropic`／`openrouter`／`google`／`gemini`／`openai-chat`／`openai-custom` | ✅ 可自由定義 |

#### 修法

| 位置 | 改動 |
|---|---|
| `codex/legacy.rs` | 新增 `CODEX_RESERVED_PROVIDER_IDS` ＋ `is_reserved_codex_provider()` |
| `codex/legacy.rs::codex_alias_ids` | 內建 id **不進**管理清單（DB 來源與既有段兩邊都擋） |
| `codex/wire.rs::codex_apply` | 接管時**清掉**已存在的內建段 → 這是修復點，舊設定會自己好 |
| `native.rs::codex_native` | 還原（回到官方）時也清掉，並在訊息裡說明 |

回歸測試 3 條：`codex_alias_ids_excludes_reserved_builtin_ids`、
`codex_apply_removes_reserved_builtin_provider_sections`、
`codex_native_removes_reserved_builtin_provider_sections`。

#### 附註

`openai-chat`、`openai-custom` 這類**加後綴**的名字可以用 —— 如果哪天真的需要
一個內建 id 的別名段，正確做法是改名（Codex 的錯誤訊息本身也是這樣建議的）。


#### 使用者要求：兩個方向都不能出錯 → 實機來回切換矩陣

「確保使用官方來源與指定來源互相切換都不會出現 Unable to log in 的錯誤」。
修好後跑兩輪完整來回切換（每一格都用 **Codex 自己**驗證：`codex doctor` 的
`config loaded` ＋ 一次真實對話 `codex exec`）：

| 狀態 | bytes | 內建段 | model_provider | model | 網關參照 | doctor | exec | 網關新增列 |
|---|---|---|---|---|---|---|---|---|
| 起始（指定來源） | 9,961 | **0** | `custom` | deepseek-v4.1-flash | 8 | ✓ loaded | exit 0（9s） | +1 |
| R1 官方來源 | 7,024 | **0** | `openai` | gpt-5.6-luna | 0 | ✓ loaded | exit 0（11s） | **+0** |
| R1 指定來源 | 9,961 | **0** | `custom` | deepseek-v4.1-flash | 8 | ✓ loaded | exit 0（8s） | +1 |
| R2 官方來源 | 7,024 | **0** | `openai` | gpt-5.6-luna | 0 | ✓ loaded | exit 0（9s） | **+0** |
| R2 指定來源 | 9,961 | **0** | `custom` | deepseek-v4.1-flash | 8 | ✓ loaded | exit 0（8s） | +1 |

**5 / 5 PASS**，而且值得注意的三件事：

1. **內建段全程都是 0** —— 修好的判斷在兩個方向都成立。
2. 設定檔在重複切換下**逐位元組穩定**（9,961 ↔ 7,024），沒有累積、沒有漂移。
3. 網關新增列只在「指定來源」那一格是 +1、在「官方來源」那一格是 **+0** ——
   這是來源真的換過去的獨立證據（不是只有設定檔看起來對）。

同一件事也鎖在單元層級：`tools/tests/roundtrip.rs` 的
`codex_official_and_specified_sources_swap_repeatedly`（別名來源刻意餵
`openai`／`ollama`，跑兩輪，每一步都斷言沒有內建段）。

> 這一節的改動只有測試與文件，**沒有動到 release 執行檔**，所以不需要重新安裝
> —— 你裝的 `20755898…` 已經是修好的版本。


### 0.9.9 端到端實測（2026-09-29 02:50–03:2x）：用量核算 ＋ 關工具／關網關／重啟

使用者要求跑一輪完整測試：**開網關 → 用工具真實對話 → 檢查用量是否算對 →
關掉工具 → 關掉網關 → 重啟工具 → 檢查來源是否回到官方設定**。

#### A. 用量核算（四路交叉比對，全部一致）

| 比對 | 結果 |
|---|---|
| 真實對話（`codex exec` 經網關） | 網關記 `in=11,518 out=30`；**Codex 自己印 `tokens used 11,548`** → 11,518 + 30 = **11,548** ✅ |
| 控制變數（curl 經網關，非串流） | 回應 `usage` = `prompt 49 / completion 200`；DB 該列 = `in 49 / out 200` ✅ |
| 同一個 prompt 直打上游（繞過網關） | 上游自己的 `usage.prompt_tokens` = **49**，與網關一致 ✅（輸出 164 vs 200 是取樣與 `max_tokens` 上限，屬正常） |
| 本地 key 配額 | `local_keys.used_tokens` 由 133,951 → 134,006，增加 **55** = 該列 `in 35 + out 20` ✅ |
| 工具歸屬 | 帶 `x-tg-app: codex` 時 `app=codex` ✅；不帶時 `app=unknown`（未標識的客戶端，非退化） |

**約束二（用量資訊必須完整）複驗**：本機工具篩選清單實測列出 **8 個選項** ——
全部本機工具／Claude Code／OpenAI Codex／opencode OpenCode／**Hermes Agent**／
DeepSeek Harness／**Cursor**／**Antigravity**，沒有任何選項消失 ✅

#### B. 關工具 → 關網關 → 重啟工具

**這一步抓到一個真的 bug，而且是「關閉網關」第二次以後才會遇到**：

| 步驟 | 結果 |
|---|---|
| 接管 Codex | 設定變成 8 個 `base_url` 全指向網關、`model_provider = "custom"`、`model = "moonshotai/kimi-k3"` ✅ |
| 真實對話（kimi-k3） | **失敗**：`#4404 st=504 303,004ms` —— 那個模型在 NIM 上要不到（見下方「已知問題」） |
| 真實對話（改 `-m deepseek-ai/deepseek-v4.1-flash`） | 成功，8 秒內回覆，用量正確 ✅ |
| **關閉 Codex** | **失敗**：卡片顯示「所有備份都還含網關配置，無法還原到接管前」❌ |
| 檢查備份 | `config.toml.bak-20260929-025022`（6,911 bytes）**一個網關位址都沒有**、`model_provider = "openai"` —— 它明明是乾淨的 ❌ |

根因（`backup.rs::backup_has_known_route`）：它要求「這份設定裡找得到一個
URL」，但**原生設定正好沒有 URL** ——

- Codex 的第一方 `openai` provider 是**內建**的，沒有 `[model_providers.openai]`
  段 → `codex_text_base_url` 回 `None`；
- Claude 回到官方時沒有 `ANTHROPIC_BASE_URL`。

`restore_backup_to_inner` 會先呼叫 `migrate_gateway_baseline`，而它在沒有
baseline 時會把該函式的 `Err` 直接往外丟 → 使用者看到一句指責備份含網關配置的
訊息，但備份是乾淨的。

**為什麼以前沒事**：在 §0.9 的原生還原上線前，「關閉」還原的是**接管前那份**
（`model_provider = "custom"` ＋ NIM URL，有 URL → 有來源）。是「回到原生」讓
基線／備份變成沒有 URL 的長相，才踩到這個判斷。

修法：`backup_has_known_route` 改成認得原生長相 ——
Codex：有段就必須有 URL（`model_provider = "custom"` 卻沒有段＝壞設定，不算），
沒有段時只有**內建** provider（`openai`）才算有來源；
Claude／OpenCode：只要是合法的非空設定物件就算有來源（有沒有被指到網關由
`backup_is_tainted` 另行判斷，兩者職責分開）。

回歸測試：`switch_off_accepts_a_native_codex_backup`、
`switch_off_accepts_a_native_claude_backup`（兩條都先重現失敗再修）。

修好後重建安裝（exe 9,038,336 bytes、sha256 `3561A32B…`），再跑一次同一個流程：

| 步驟 | 結果 |
|---|---|
| 關閉 Codex（修正後） | `(On -> Off)` ✅、設定 7,718 → **6,913 bytes** |
| 訊息 | `…｜還原自 config.toml.baseline-20260929-032145｜provider=openai model=gpt-5.6-luna｜已切回原生來源：model → gpt-5.6-luna` ✅ |
| 設定內容 | `model_provider = "openai"`、`model = "gpt-5.6-luna"`、**指向網關 0 次**、8 個別名段全在 ✅ |
| **關閉網關** | App 內按「停止」→ `15722 LISTEN = 0`、`gw` 行程消失、按鈕變「啟動」✅（App 本身仍在跑，這是正確的） |
| **重啟工具** | 網關關著的情況下 `codex exec` → 14 秒回覆「官方來源正常。」、exit 0 ✅ |
| **證明沒走網關** | 對話前後 `request_logs` 的 max id **都是 4407**（網關若收到請求一定會寫一列）✅ |
| `codex doctor` | `✓ config loaded`、`model gpt-5.6-luna · openai`、`default model provider openai`、`✓ auth is configured` ✅ |
| 模型清單 | `gpt-5.6-terra`／`gpt-5.6-luna`／`gpt-5.5`（＋2 隱藏）✅ |

**這一輪我改動的檔案**：`~/.codex/config.toml`（接管 → 關閉，最終為原生設定，指紋
6,913 bytes／`7CA78C88…`）、`~/.config/opencode/opencode.json`（§0.9.8）、
`~/.claude/settings.json`**沒動**（仍在網關接管中）。備份都在
`%APPDATA%\com.tokencounter.gateway\backups\`。

**✅ 一個看起來像問題、查證後確認是正確行為的點**：原生還原會把 `model`
**覆寫**成 `gpt-5.6-luna`（你 2026-09-28 選的預設）。這次它把你原本的
`gpt-6-luna` 換掉了（訊息裡那句「model → gpt-5.6-luna」就是這個動作）。
你問「`gpt-6-luna` 是不是 OpenCodeGo 來源的模型」—— **查證結果：是**。
所以覆寫是對的，**沒有改任何程式**：

| 查證 | 結果 |
|---|---|
| App 的 `provider_models` | `gpt-6-luna` **只**掛在 provider **22（opencode-go）**，其他 provider 都沒有 |
| App 的目錄檔 | `catalogs/codex-22.json` 是**唯一**含 `gpt-6-luna` 的；codex-15（NIM）／17／18／19／20／21 都沒有 |
| Codex 自己的內建清單（乾淨 `CODEX_HOME`，11 個模型） | 只有 `gpt-6-astra` 這個 gpt-6；`luna` 只有 `gpt-5.6-luna`；**查不到 `gpt-6-luna`** |

結論：`gpt-6-luna` 是 opencode-go 的模型名，不是 OpenAI 的。原生還原的目的
正是離開那些第三方來源，留著它們的模型名只會讓 Codex 拿到一個查不到的模型
（那正是使用者日誌裡 `Unknown model` 警告的成因）。因此「一律覆寫成
`gpt-5.6-luna`」是正確行為，你 2026-09-29 也確認「不用换预设模型」。這個查證
寫進了 `native.rs` 的 `CODEX_NATIVE_MODEL` 註解，免得日後有人把它改成
「已是 GPT 系列就別動」而讓這個坑復活。

**⚠️ 一個要你決定的行為**：原生還原會把 `model` **覆寫**成 `gpt-5.6-luna`
（你 2026-09-28 選的預設）。這次它把你原本的 `gpt-6-luna` 換掉了（訊息裡那句
「model → gpt-5.6-luna」就是這個動作）。如果你希望「已經是 GPT 系列就別動」，
告訴我一聲就改。→ **已於 2026-09-29 查證結案：維持覆寫，不改**（見上一段）。

#### 已知問題（測試中發現，尚未處理）

- **`moonshotai/kimi-k3` 在 NIM 上 504**（303 秒後逾時）。那是 App 為 Codex
  記住的來源／模型配對，所以「接管 Codex 之後直接用」會失敗。這不是網關算錯，
  是那個上游要不到那個模型；要不要換掉預設配對請你決定。


### 0.9.8 實機驗證（OpenCode）＋ 一個順手抓到的長期瑕疵

照使用者的要求，把 OpenCode 的開關也實機走一遍（接管 → 關閉）：

| 步驟 | 觀察 |
|---|---|
| 接管 OpenCode | `opencode.json` 改成 `provider.tokengateway.options.baseURL = http://127.0.0.1:15722/v1`、`apiKey` 寫入上游 Key 明文 |
| 關閉 OpenCode | 訊息出現：`…｜還原自 opencode.json.baseline-20260928-232249｜**provider=未知 model=未知｜⚠️ 還原內容本身不是合法 TOML，Codex 可能無法載入**｜已切回原生來源：移除 provider.anthropic.options.baseURL（原本指向本網關）、移除 provider.anthropic.options.apiKey（網關的本地 key）` |
| 結果 | `opencode.json` 570 bytes、**指向網關 0 次**、`provider.anthropic.options = {}`（回到 OpenCode 自己的登入）、`provider.tokengateway` 段留著（舊對話通道）✅ |
| 它自己的登入 | `~/.local/share/opencode/auth.json` 內有 `nvidia`／`opencode`／`openrouter`／`opencode-go` ✅ 所以離開網關之後仍可用 |

**那串粗體是錯的，而且不是這一輪造成的**：`restore_summary` 原本不分工具都用
Codex 的 TOML 掃描，於是還原 OpenCode 這種 **JSON** 設定時，畫面端出
「provider=未知 model=未知｜⚠️ 還原內容本身不是合法 TOML，Codex 可能無法載入」
—— 三句全是錯的（它不是 TOML，也跟 Codex 無關）。已改成依工具產生：

| 工具 | 摘要長相 |
|---|---|
| codex | `provider=custom model=moonshotai/kimi-k3`（＋TOML／死鏈警告，照舊） |
| claude | `endpoint=opencode.ai model=claude-opus-5`；沒有覆寫端點時 `endpoint=api.anthropic.com（未覆寫，官方）` |
| opencode | `providers=anthropic,tokengateway` |

JSON 壞掉時也改成講對工具（`⚠️ 還原內容不是合法 JSON，OpenCode 可能無法載入`）。

**第二個瑕疵（同一輪修掉）**：修好摘要之後再實機跑一次，畫面變成
`…｜providers=anthropic,tokengateway｜⚠️ 設定裡沒有網關痕跡，維持原樣` ——
**摘要對了，但那個 ⚠️ 是錯的語氣**。第二次關閉時設定本來就已經乾淨（上一次
已經清過），所以「沒有東西要改」；對一個健康的設定示警，會讓人以為壞了。
`NativeOutcome` 因此把兩件事分開：

| 欄位 | 意思 | 畫面 |
|---|---|---|
| `changes` | 我改了什麼 | `｜已切回原生來源：…` |
| `note` | **沒有東西要改**（正常） | `｜設定裡沒有網關痕跡，維持原樣`（無 ⚠️） |
| `warning` | **做不到**（TOML／JSON 壞掉、工具不支援） | `｜⚠️ …` |

> **Claude Code 沒有做實機點擊**：你選了「連端點一起推回 Anthropic 官方」，而這台
> 機器上沒有 `.claude/.credentials.json` —— 一按下去 Claude Code 就會停用到你登入
> 為止。那條路徑由單元測試釘住（`claude_native_strips_users_own_third_party_endpoint`
> 等 3 條），要啟用請先 `claude` 登入一次，再按開關。

> **我實際改了你的東西**：`~/.codex/config.toml` 現在是原生設定（§0.9.7 那份）。
> 接管前的那份備份在
> `%APPDATA%\com.tokencounter.gateway\backups\codex\config.toml.bak-20260928-223526`
> （7,022 bytes），要退回 NIM 就把它複製回去。基線在還原時被清掉了（那是既有
> 行為），所以**下一次接管會拿現在這份原生設定當基線** —— 這是好事。
> 另外驗證過程中**有一件事不是我做的**：22:29:03 Codex Desktop 自己改了
> `config.toml`（更新它自己的 runtime 路徑、版本 `26.917.62051`→`26.924.22138`、
> 補上 `enabled-reasoning-efforts`）。這個檔案是共用的，不是你我在搶它。

---

## 1. 產物位置

| 產物 | 路徑 |
|---|---|
| 執行檔（免安裝） | `token-gateway\src-tauri\target\release\token-gateway.exe` |
| NSIS 安裝檔（你目前用的形式） | `token-gateway\src-tauri\target\release\bundle\nsis\token-gateway_0.1.0_x64-setup.exe` |
| MSI 安裝檔 | `token-gateway\src-tauri\target\release\bundle\msi\token-gateway_0.1.0_x64_en-US.msi` |

三者都是同一次建置的產物（確切時間與大小見本文件末尾的「建置資訊」）。

---

## 2. 開始之前：務必先關掉舊版

目前有一個**舊版行程正在執行**：

```
pid  8252
path C:\Users\<user>\AppData\Local\token-gateway\token-gateway.exe
```

關掉它（存檔、關視窗，或）：

```powershell
Stop-Process -Id 8252
```

**然後才**安裝或執行新版。不關掉的話：安裝檔無法覆蓋舊檔，而且新版會去
升級一個正被舊版寫入的資料庫。

> **不要**在舊版還開著的時候啟動新版。兩個行程同時寫同一個 SQLite 檔不是好主意。

---

## 3. 第一次啟動會做什麼（已對你的真實資料庫驗證過）

啟動時會把資料庫從 **v7 升級到 v8**，新增兩張表：`proxy_trace`、
`provider_stripped_fields`。這是**純新增**，不動任何既有表。

我已經對**你的真實資料庫的副本**（熱拷貝，含 `-wal`，先驗
`PRAGMA integrity_check` = ok）實跑過升級：

```text
升級前：version=7 providers=7 request_logs=4111
升級後：version=8 providers=7 request_logs=4111 proxy_trace=true provider_stripped_fields=true
```

資料零減損。升級前後你可以自己再確認一次（唯讀，不會改動資料庫）：

```powershell
py scripts\dump_traces.py
```

第一次跑會看到 `proxy_trace: **不存在**`（因為還沒啟動新版），
啟動過新版之後再跑，就會出現 `proxy_trace : <筆數>`。

---

## 4. 測試 A：診斷中心

**狀態：✅ 已通過（2026-09-27 22:07 由使用者實測截圖確認）。**

截圖內容與資料庫實際值逐項核對，全部一致：

| 畫面顯示 | 資料庫實際 | 結果 |
|---|---|---|
| 網關 `127.0.0.1:15722` | 監聽 `127.0.0.1:15722` | ✅ |
| 追蹤筆數上限 5,000 | `TRACE_KEEP = 5000` | ✅ |
| 兩筆時間戳 `20:28:40` / `20:28:17` | `id=2` → `20:28:40`、`id=1` → `20:28:17` | ✅ **精確對應** |
| 層級 `warn` | 當時 2 筆皆為 `trace_level=warn` | ✅ |
| 類型「拒絕」 | 2 筆皆為 `trans_kind=rejected` | ✅ |
| 「尚未觀察到上游拒收欄位」 | `provider_stripped_fields` 為 0 筆 | ✅ 空是正確的 |
| 上游能力記憶說明文字 | 對應 `§5.3` 第 2 層設計 | ✅ |

> **那兩筆是什麼**：是我在本輪為了驗證「網關拒絕要留痕」（測試 B）而送出的
> 頭兩個請求 —— 無 Authorization 與無效金鑰，皆回 401。截圖當時只顯示 2 筆
> 是正確的，因為 22:07 時資料庫確實只有這 2 筆；之後我為驗證 UA 歸屬又送了
> 更多測試請求。

**這解決了先前的空白**：在此之前「診斷中心」這整塊程式碼**從未在任何 GUI 中
實際渲染過**。現在確認版面正常、數字正確、與資料庫一致。

1. 啟動新版，左側導覽應出現 **「診斷」**。
2. 點進去，應看到三塊面板：**網關診斷**、以及剝離欄位與請求追蹤相關區塊。

**通過標準**：頁面正常渲染、沒有錯誤橫幅、三個查詢都回得出資料
（剛升級完追蹤數為 0 是正常的，因為追蹤只在異常時寫入）。

---

## 5. 測試 B：網關拒絕要留痕（本次修正的重點）

### 5.1 先確認網關已啟動

在 App 裡把**網關啟動**（預設埠 **15722**；若你改過，以下指令請代換）。
以下指令都在 **PowerShell** 裡跑。

### 5.2 製造兩個「網關層拒絕」

這兩種拒絕**根本不會碰到上游**，所以修正前完全不會留下追蹤。

```powershell
# ① 完全沒有 Authorization → 應回 401
try {
  Invoke-RestMethod -Uri http://127.0.0.1:15722/v1/chat/completions -Method Post `
    -ContentType 'application/json' -Body '{"model":"ghost"}'
} catch { "① status = " + $_.Exception.Response.StatusCode.value__ }
```

```powershell
# ② 帶一個無效的金鑰 → 應回 401（"無效的 Key"）
try {
  Invoke-RestMethod -Uri http://127.0.0.1:15722/v1/chat/completions -Method Post `
    -ContentType 'application/json' -Headers @{ Authorization = "Bearer not-a-real-key" } `
    -Body '{"model":"ghost"}'
} catch { "② status = " + $_.Exception.Response.StatusCode.value__ }
```

若你有真實金鑰（`$env:TOKEN_GATEWAY_KEY`），再製造一個 **400**：

```powershell
# ③ 真實金鑰 + 未登記的模型 → 應回 400（模型不在清單內）
$k = $env:TOKEN_GATEWAY_KEY
try {
  Invoke-RestMethod -Uri http://127.0.0.1:15722/v1/chat/completions -Method Post `
    -ContentType 'application/json' -Headers @{ Authorization = "Bearer $k" } `
    -Body '{"model":"definitely-not-registered"}'
} catch { "③ status = " + $_.Exception.Response.StatusCode.value__ }
```

### 5.3 回診斷頁看

**通過標準**（這是這次修正的驗收）—— **2026-09-28 已由我代跑，五項全過**：

- [x] 「請求追蹤」出現對應筆數，且每一筆的展開明細中，「轉換」欄顯示
      **「網關拒絕（未觸及上游）」**（修正前這裡會顯示成 ` → （rejected）`，
      因為這一類沒有格式轉換也沒有上游）
- [x] 這幾筆的狀態碼分別是 401／401／400
- [x] 它們也出現在**問題追蹤**清單裡（因為記為 `warn` 等級）
- [x] 「追蹤總數」相應增加
- [x] 明細中的 **note** 讀得出被拒原因（例如「模型不在白名單」），
      且**不含任何金鑰字串**

**實跑結果**（2026-09-28，網關 15722；`proxy_trace` 由 21 筆 → 24 筆，Δ = **+3**）：

| 送出的請求 | 狀態碼 | 追蹤 ID | 明細「轉換」欄 | 說明欄 |
|---|---|---|---|---|
| 完全沒有 `Authorization` | **401** | 22 | 網關拒絕（未觸及上游） | 缺少 Authorization：請填本地 sk-local-… 或該渠道的上游 Key |
| 帶無效金鑰 `Bearer not-a-real-key` | **401** | 23 | 網關拒絕（未觸及上游） | 無效的 Key（既非本地 Key，也非已登記渠道的上游 Key） |
| 真實本地 Key ＋ 未登記模型 `zzz-not-registered-xyz` | **400** | 24 | 網關拒絕（未觸及上游） | 模型 zzz-not-registered-xyz 不在該 Key 綁定的來源「NIM」模型清單內，請求不會轉發… |

診斷頁讀到：**追蹤總數 24／異常 24／warn 層級／近 24 小時 24**、
**上游狀態碼分佈 401 → 23、400 → 1**，與資料庫逐筆一致。
展開第 24 筆的明細：追蹤 ID 24、時間 2026-09-28 16:15:31、應用 unknown、
模型（原始）`zzz-not-registered-xyz`、**轉換「網關拒絕（未觸及上游）」**、
延遲 6 ms、重試次數 0、請求體 —、body SHA-256 —，最後一列是完整「說明」。

> **注意**：這三筆是**合成測試**，會在 `proxy_trace` 與 `request_logs` 各留 3 筆
> （`app=unknown`、0 token、0 成本），讓「總請求」多 3。這是網關的正常行為
> （被拒請求本來就會留痕），但如果你想清掉，刪掉 `proxy_trace` id 22–24 與
> `request_logs` id 4353–4355 即可。

> **修正前的行為**：以上三筆在診斷頁**一筆都不會出現**。你若想看對照，可以
> 先用舊版跑一次 5.2 的指令，再開診斷頁 —— 但舊版根本沒有診斷頁，所以這個
> 對照其實做不了。這正是問題所在。

---

## 6. 測試 C：原本的症狀（400 與 Codex 會話）

這一項**需要你在真實使用情境下測**，我無法代跑。

1. 如常使用你的 CLI 工具（Claude Code／Codex…）經過本網關。
2. 若再遇到 **「連線錯誤（通常是 400）」**：
   - 立刻開診斷頁，看「問題追蹤」有沒有對應紀錄
   - 特別看 **`body 解析失敗`** 這個計數是否變成非 0
   - 若有，展開明細會看到 **`body_hex`**（請求體前 512 bytes 的十六進位）
     —— 這是判定「究竟是上游回傳 XML/HTML 錯誤頁、空 body，還是合法 JSON
     被誤判」的關鍵證據（**§5.2 已於 2026-09-28 定案：那兩筆是真的非法 JSON，
     不是網關 bug**，見 §5.2，所以這裡不再是懸案，只是留著給未來的新案例）
3. 若再遇到 **Codex 無法續用舊會話／無法封存對話**：
   - 這是 **B3，已修**（別名段聯集「只增不減」＋體檢誠實回報，見 §6.5）
   - 但**成因有四條**（計畫書 Phase 2.5），本次只保證其中兩條的機制；
     若你還是遇到，請把 Codex 的**原始錯誤文字**給我 —— 我目前只能從程式碼
     推論，缺這份文字無法定位到具體分支

---

## 6.5 測試 D：Codex 舊會話別名（B3，已修）

這一項針對「**無法繼續使用舊的 conversation session**」。修好的四條成因見
計畫書 Phase 2.5，但**你要測的只有一件事**：體檢會不會誠實回報。

1. **完全關閉 Codex**（重要 —— 成因之一就是 Codex 正在寫 DB 時的競爭）。
2. 在 App 的 Codex 接管頁跑**接管前體檢**。

**通過標準** —— **2026-09-28 已由我代跑，三項全過**（Codex 行程確認未執行）：

- [x] 若一切正常，會看到
      **「ℹ️ 歷史會話用過 N 個別名 provider（讀自 state_5.sqlite）：…」**
      ——注意它會**標明讀自哪個檔案**
- [x] 若讀不到，會看到
      **「❌ 讀不到 Codex 歷史會話的 provider 名：…」** 並附原因與處置建議
- [x] **最關鍵**：絕對不會在讀不到的情況下看到
      「✅ 歷史會話無第三方 provider 殘留」

**實跑結果**（2026-09-28，於「上游來源 → 本機工具 → Codex」按「接管前體檢」）：

| 讀到的訊息 | 判定 |
|---|---|
| `ℹ️ 歷史會話用過 6 個別名 provider（讀自 state_5.sqlite）：gw, mock, nim-direct, nvidia-nim, nvidia-proxy, opencode-zen —— 接管將全寫為網關別名段` | ✅ 明示來源檔名、數量與清單 |
| `❌ TOKEN_GATEWAY_KEY 未設定：Codex 行程繼承不到 Key 會 401，請先設為用戶環境變數` | ✅ 失敗附原因＋處置 |
| `✅ 15721 空閒（cc-switch 代理未運行）` | ✅ 真通過 |

**6 這個數字是對的**：`SELECT DISTINCT model_provider FROM threads` 實測回傳
8 個（`custom, gw, mock, nim-direct, nvidia-nim, nvidia-proxy, opencode-zen,
tokengateway`），體檢**刻意扣掉 `custom` 與 `tokengateway`**（網關自己的段名），
程式碼 `doctor.rs:71` 的文案也寫明了「custom / tokengateway 之外」。
那 8 個別名在 `config.toml` 裡**每一個都有對應段**，所以舊會話不會斷。

**第三條（最關鍵那條）為何成立**：`codex_legacy_providers_report` 回傳的是
`LegacyProviders::Ok { .. }` / `Failed { .. }` 這個 enum，體檢用 `match` 分開處理
——「讀不到」走 `Failed` 分支印 ❌，**寫不到 ✅ 那條路上**。這條界線有
單元測試釘住（`tools/tests/codex_legacy.rs`：完全沒有 state DB、以及有 DB 但
缺 `threads` 表，兩者都必須是 `Failed` 而不是 `Ok`）。

第三條是這次修的核心。**修正前，讀取失敗會顯示那個 ✅** —— 唯一的診斷工具
在真正的失敗上給你綠色勾勾。

若要自己確認資料面是否一致（唯讀）：

```powershell
py -c "import sqlite3,os;h=os.path.join(os.environ['USERPROFILE'],'.codex');c=sqlite3.connect(f'file:{h}/state_5.sqlite?mode=ro',uri=True);print(sorted({r[0] for r in c.execute('SELECT DISTINCT model_provider FROM threads')}))"
```

把印出的 provider 名與 `~/.codex/config.toml` 裡的 `[model_providers.*]` 段名
對照：**thread 用過的每一個都應該在 config.toml 裡有對應段**。少任何一個，
那個舊會話就會失效。

## 6.6 測試 E：`wire_api` 是否跟著上游（B4，已修）

這一項只在**直連模式**（接管頁取消勾選「經由網關」，`base_url` 直接指向第三
方）下有意義。

1. 選一個**第三方**渠道（例如 `https://integrate.api.nvidia.com/v1`），走直連。
2. 看**接管預覽**的文字。

**通過標準** —— **2026-09-28 已由我逐項核對**（準則 1 未做成螢幕證據，理由見下）：

- [x] 預覽顯示 `wire_api = chat`（**不是** `responses`）—— 由
      `codex_wire_api_prefers_declared_format` 與
      `codex_apply_picks_wire_api_per_upstream` 兩條測試釘住（見下）
- [x] 接管後打開 `~/.codex/config.toml`，每個 `[model_providers.*]` 段都是
      `wire_api = "chat"` —— **在你真實的 config.toml 上實測通過**
- [x] 換成經由網關接管時，同一欄位是 `wire_api = "responses"`
- [x] 若該渠道的 `api_format` 被宣告為 `openai-responses`，即使 base_url 是
      第三方，也應該寫 `responses`（**宣告優先於 URL 猜測**）

> ## ❌ 這一項原本是**程式錯、文件對**，而且造成了真實故障
>
> **2026-09-28 17:xx 追查「Codex 開啟時報 Unable to log in」時發現：**
> 實作把「宣告優先」用到了**指向網關**的情況上 —— 你的 7 個來源
> `api_format` **全部是 `openai-chat`**，於是接管後**每個** provider 段
> 都被寫成 `wire_api = "chat"`。
>
> 而 **Codex 已經不再接受 `wire_api = "chat"`**。它的日誌寫得很清楚
> （`~/.codex/logs_2.sqlite`）：
>
> ```
> ERROR codex_app_server: Invalid configuration; using defaults.
> C:\Users\<user>\.codex\config.toml:16:12: `wire_api = "chat"` is no longer supported.
> How to fix: set `wire_api = "responses"` in your provider config.
> ```
>
> Codex 讀到非法設定就**整份 config 丟棄改用預設值**，於是回頭找 ChatGPT
> 登入 —— 使用者看到的錯誤是 **「Unable to log in」**，
> **完全看不出真正的起因**。這正是本節準則 3 要防的事。
>
> **為什麼會漏掉**：單元測試只驗了
> 「網關 URL ＋ **未宣告**」→ `responses`（`CodexAuth::default()`），
> 而生產路徑**永遠帶著宣告**（`switch.rs` 傳
> `codex_wire_api_declared(provider_format)`）。兩者從未在同一條測試裡相遇。
>
> **修法**（`wire.rs` 新增 `codex_wire_api_for`）：先看 URL ——
> 指向**網關／官方 OpenAI** 就定死 `responses`，宣告只在**真正的第三方**
> 才發揮作用。並補上回歸測試
> `codex_apply_forces_responses_on_gateway_even_if_declared_chat`，
> 用的正是生產路徑的參數組合。

**在你真實檔案上的實測**（2026-09-28，接管中、來源 opencode-go；修正後）：

| 檢查 | 結果 |
|---|---|
| `[model_providers.*]` 段數 | 8（`custom`／`tokengateway`／`gw`／`mock`／`nim-direct`／`nvidia-nim`／`nvidia-proxy`／`opencode-zen`） |
| 每一段的 `wire_api` | **全部 `responses`** ✓ |
| 每一段的 `base_url` | **全部 `http://127.0.0.1:15722/v1`** ✓ |
| 有沒有段落漏寫 `wire_api` | 無 ✓ |

**釘住這件事的測試**（`src-tauri/src/tools/tests/codex_wire_api.rs`）：

| 測試 | 釘住什麼 |
|---|---|
| `codex_wire_api_prefers_declared_format` | `openai-chat`→`chat`；`openai-responses`／`mixed`→`responses`；`anthropic`／`gemini`／`None`／空字串→`None`（交 URL 推定）；且**同一個第三方 URL 下，宣告能推翻 URL 推定** |
| `codex_wire_api_falls_back_to_url_when_undeclared` | 沒宣告時一定回退到 URL 推定，不會變成無值 |
| `codex_apply_picks_wire_api_per_upstream` | 直連第三方 → **每個**段都 `chat`；走網關且未宣告 → **每個**段都 `responses`；別名段也必須存在 |
| **`codex_apply_forces_responses_on_gateway_even_if_declared_chat`** | **新增（回歸）**：網關 URL ＋ 宣告 `openai-chat` → **必須** `responses`；官方 OpenAI 同理；真正的第三方才輪到宣告生效 |

> **準則 1 為何沒做成螢幕截圖**：接管預覽要展開「N 項寫入」才會露出內容，
> 而那個展開控制項在 UI Automation 裡不是可 Invoke 的元素；我用座標點它時
> 反而把對話框關掉了。改以「測試 + 真實檔案」舉證。**沒有捏造螢幕證據。**

`wire_api` 決定 Codex 用哪種協議打 `base_url`。寫錯的話 Codex 會打到不存在的
端點而 **404** —— 修正前直連第三方一律被寫成 `responses`，明知會壞還照寫。

## 6.7 測試 F：DSH 用量為何不出現在 Token 趨勢（**已查明，非程式 bug**）

**你回報**：「Token 趨勢中還是沒有出現 DeepSeek Harness 的使用量」、
「本機工具應該顯示 DSH 或 DeepSeek Harness，而不是 DeepSeek」。

### 結論先講

| 你的回報 | 查證結果 |
|---|---|
| 標籤顯示「DeepSeek」 | ✅ **是真 bug**，已修（前端寫錯，後端本來就對），見 §6.8 |
| Token 趨勢沒有 DSH 用量 | ⚠️ **不是程式 bug** —— DSH 目前的設定**根本沒有經過網關**，所以網關看不到它的用量 |
| 「DSH 用量被記成 Codex」 | ❌ **我先前搞錯了**，見下方「更正」 |

### 為什麼網關看不到 DSH 的用量

`~/.dsh/settings.yaml`：

```yaml
llm-deepseek:
  baseURL: http://127.0.0.1:15722/v1   # 指向網關
  models: []                            # ← 沒有設定任何模型，等於沒在用
llm-pi-ai:
  providers:
    oc-go:
      baseURL: https://opencode.ai/zen/go/v1   # ← 直連，繞過網關
agent-default-model:
  provider: oc-go                       # ← DSH 實際用的就是這個
  model: deepseek-v4.1-flash
```

DSH 實際使用的 provider 是 `oc-go`，它**直接連到 `opencode.ai`**，不經過網關。
指向網關的 `llm-deepseek` 反而 `models: []`（沒有模型）。

**實測佐證**：我本身就跑在 DSH 裡。在 22:00～23:35 之間我進行了數十個回合，
`request_logs` 裡**完全沒有**這些請求 —— 網關最後一次收到真實流量是 17:09。
若 DSH 經過網關，我自己的每個回合都會留下紀錄。

### 要讓 DSH 用量被統計，你把 `oc-go` 的 `baseURL` 改成網關即可

```yaml
llm-pi-ai:
  providers:
    oc-go:
      baseURL: http://127.0.0.1:15722/v1   # 改成網關
```

改完後 DSH 的請求就會被記錄。歸屬會是正確的 `dsh`（見下方 UA 查證）。

### 更正：我先前「1,990 筆被記成 Codex」的說法是錯的

我在 `97e5911` 的訊息裡寫了「`app='codex'` 的 3,505 筆中有 1,990 筆
（56.8%）其實是 DSH」。**這個結論是錯的**，原因是我只看模型名稱就推論來源。

錯在哪：

1. **`~/.codex/config.toml` 的 `model = "deepseek-v4.1-flash"`** —— Codex 自己
   就設定了這個模型，它的 `models = [...]` 清單裡還包含
   `muse-spark-1.3-contributor`、`space-bunny-free`、`omen-alpha` 等
   我誤稱為「DSH 專屬」的模型。**兩個工具用的是同一批模型名稱。**
2. **`source='import'` 的 1,142 筆來自 `~/.codex/sessions/*.jsonl`** ——
   那些是 Codex 自己的會話檔，本來就是 Codex。其中確實包含
   `glm-5.3-flash`（523 筆）等模型，證明 Codex 真的用過它們。
3. **舊版 `infer_app` 對 DSH 的真實 UA 本來就會回 `dsh`。** 我核對過
   baseline `38f060a` 的原始碼：`deepseek-harness/...` 這個字串不含
   `codex`／`opencode`／`hermes`／`dsh`，會命中 `deepseek` → 回 `dsh`。
   所以**從來沒有把 DSH 記成 Codex**。

### UA 查證（這部分仍然有效）

DSH 的 `dsh-llm` 與 `dsh-llm-pi-ai` 都會用 `attributionHeaders()` 送出：

```
deepseek-harness/<version> (+https://github.com/deepseek-ai/deepseek-harness)
```

`dsh-llm-pi-ai` 的 `requestHeaders()` 會**移除大小寫不敏感的衝突欄位**再套用
attribution，所以即使底層 SDK 想設自己的 UA 也會被覆蓋。DSH 不可能送出含
`codex` 的 UA。

### 那 `97e5911` 的 `infer_app` 改動要留嗎？

**留。** 它對真實流量是**行為等價**的（所有真實 UA 的判定結果都相同），
只在「UA 同時含多個工具字樣」時更穩健。我補的 7 條測試也把行為釘住了。
但它**不是**你症狀的原因 —— 這點我必須講清楚。

---

## 6.8 測試 G：本機工具標籤（已修，`43930a4`）

前端 `logos.tsx` 的 `APP_META` 把 `dsh` 標成 `"DeepSeek"`，後端
`tools/consts.rs` 的 `APPS` 早就寫 `"DeepSeek Harness"` —— 兩份清單各寫各的。

這一格指的是**本機工具**（DeepSeek Harness CLI），不是模型或廠商名稱，
寫成 `DeepSeek` 會讓人以為那是模型。

**通過標準**：Usage 頁的來源篩選、Token 趨勢圖例、診斷頁明細，凡顯示來源
名稱處都應為「DeepSeek Harness」。

**已做的防護**：
- Rust 測試 3 條釘住 `APPS` 顯示名（含「不可簡寫成 DeepSeek」）
- `py scripts/check_app_labels.py` 比對前後端兩份清單，不一致則 exit 1

---

## 7. 回報方式

跑這個（**唯讀開啟，不會改動資料庫**）：

```powershell
py scripts\dump_traces.py -n 50
```

會同時印到螢幕並寫入 `trace_dump.txt`。把那個檔給我即可。它包含：
schema 版本、各表筆數、最近追蹤的完整欄位（含 `body_hex` 還原）、
`trans_kind`／狀態碼分布、以及已學會的剝離欄位。

只想看問題的話：

```powershell
py scripts\dump_traces.py --problems -n 100
```

> 腳本以 `mode=ro` 開啟資料庫。它刻意不寫入任何東西。

---

## 8. 已知未修 / 未驗證的事（誠實清單）

**未修**（尚未授權或需產品決策）：

- ~~**B3**：Codex 舊會話 provider 別名段靜默消失。~~ **已修** —— 見測試 D。
  共 4 條成因（檔名寫死 `state_5.sqlite`、缺 `busy_timeout`、管理清單會縮小、
  `codex_doctor` 把失敗報成 ✅）加上第 5 條 `rows.flatten()` 靜默吞錯。
- ~~**B4**：無差別強制 `wire_api = "responses"`。~~ **已修，而後整個機制已移除。**
  過程是：B4 的修法（宣告優先）本身有缺陷 —— 它把指向網關的每個段都寫成
  Codex 已不接受的 `chat`，造成「Unable to log in」（2026-09-28，§0.6）。
  接著發現**那套設計的前提全部失效**（直連模式已移除、Codex 不接受 `chat`、
  `api_format` 描述的是上游而非 Codex→網關），於是**整組拿掉**：
  現在 `wire_api` 是單一常數 `CODEX_WIRE_API = "responses"`，見 §0.7.2。
- ~~**§5.3 第 2 層**：能力宣告。~~ **已實作，而後移除。**
  協議選擇改成宣告優先之後被證明會寫出 `chat` 而讓 Codex 打不開，
  最終於 2026-09-28 移除整個宣告機制（同上一條）。
  `providers.api_format` 欄位保留，但只剩「來源列表顯示」與「網關端轉譯」用途。
- ~~**§5.2 的 body 解析 400**：仍未定案。~~ **已定案（2026-09-28）** ——
  **不是網關的 bug**。把三筆證據的 body 與宣稱的 `bytes_len` 逐一對齊
  （69／107／132 全部相符，儀器沒有弄壞 body），再實際丟給 JSON 解析器：
  兩個 FAILED 的 body **key 都沒加引號**（`{model:...}` 而非 `{"model":...}`），
  那是 JS 物件字面值、本來就不是合法 JSON，`serde_json` 拒絕它們完全正確；
  OK 那筆是合法 JSON 且成功。網關回 400 是對的行為。完整推理見
  `docs/evidence/2026-09-28-body-parse-CONCLUSION.md`。
- ~~**`resolve_filter` 的「365 天」訊息與「366 天」判斷式不一致**~~ **已修** ——
  前端 `RangePicker.tsx` 的 `CUSTOM_MAX_DAYS` 本來就是 365，三處裡只有後端
  判斷式是錯的，因此**沒有產品決策空間**。已統一為 365，測試改為
  `resolve_filter_span_limit_is_365_days`。
- ~~**`db.rs` migration 仍無自動化測試**（§2.1 F6）。~~ **已補** ——
  新增 `every_legacy_version_upgrades_preserving_data`：對 v1…v7
  **每一個歷史版本**各造一個資料庫，確認升級後版本正確、providers／
  request_logs 一列不少、settings 既有值不變、12 張核心表齊全。
  原本那條需人工觸發的 `live_migrate_real_db_copy` 保留作為出貨前的額外確認。
- ~~**前端三個肥檔未動**：`Keys.tsx` 876、`Calc.tsx` 809、`lib/api.ts` 803。~~
  **已拆**（2026-09-28），見下方「拆檔」段落。

**已做但未達標**：

- ~~`proxy_handler` **86 行**（目標 < 80）。~~ **已達標：78 行** ——
  根因不是「重複 5 個欄位」而是 `RetryCtx`／`FinishCtx`／`PrepareInput`
  **各自重複宣告** `ctx`／`started`／`app`／`model_raw`。新增
  `proxy/reqctx.rs` 的 `ReqCtx`，三個階段改為嵌入它，handler 只建一次。
- ~~**拆檔後仍 > 400 行的檔案**~~ **已全部拆完**（2026-09-28）。
- ~~**Phase 5 錯誤型別只轉了 6/108 處**~~ —— 這一項**維持不動**：`keys.rs`
  是刻意選的試點，轉換路線已證明可行且不傷文案，但**每次轉換都應有人能
  實測**。剩下的 102 處屬獨立的漸進工作，不建議一次改完。

### 8.1 拆檔成果（2026-09-28）

> **行數一律用 `[IO.File]::ReadAllLines()` 量**。PowerShell 5.1 的
> `Get-Content` 對含中文的檔案會少算（見 §0.4.4），下面的「原行數」是
> 真實值，會比 §8 舊文裡引用的數字大。

**原本 §8 列出的 5 個檔案**：

| 原檔 | 原行數 | 現況 |
|---|---:|---|
| `Providers.tsx` | 778 | **274**（抽 SourceCard／SourceDetail／ToolList ＋ useDragSort／useToolSwitch） |
| `providers/ModelCatalog.tsx` | 608 | **5**（barrel；`modelcatalog/` 4 檔，最大 238） |
| `providers/ProviderForm.tsx` | 590 | **18**（barrel；`providerform/` 4 檔，最大 356） |
| `tools/tests/codex.rs` | 566 | **326**（再拆 `codex_wire_api.rs`、`codex_legacy.rs`） |
| `usage/usageCharts.tsx` | 546 | **295**（日誌元件搬到 `usageLogs.tsx`） |

**稽核時另外掃出的、原本不在清單上的檔案**（既然要拆就一併處理）：

| 檔案 | 原行數 | 現況 |
|---|---:|---|
| `Keys.tsx` | 884 | **254**（+ `keys/` 6 檔） |
| `Calc.tsx` | 818 | **366**（+ `calc/` 5 檔，最大 225） |
| `lib/api.ts` | 808 | **318**（型別拆到 `apiTypes.ts` 303 ＋ `apiTypesPricing.ts` 172 ＋ `apiTypesDiagnostics.ts` 70） |
| `translate.rs` | 2,345 | `translate/` 13 檔，最大 **341** |
| `usage.rs` | 2,190 | `usage/` 14 檔，最大 **369** |
| `commands.rs` | 1,233 | `commands/` 12 檔，最大 **224** |
| `db.rs` | 1,132 | `db/` 9 檔，最大 **296** |
| `history.rs` | 1,038 | `history/` 6 檔，最大 **273** |
| `models.rs` | 990 | `models/` 5 檔，最大 **360** |
| `proxy/tests/e2e_basic.rs` | 705 | 23 行 root ＋ `e2e_basic/` 3 檔，最大 **300** |
| `tools/codex.rs` | 602 | `tools/codex/` 4 檔，最大 **274** |
| `quota.rs` | 595 | `quota/` 6 檔，最大 **153** |
| `trace.rs` | 576 | `trace/` 5 檔，最大 **242** |
| `SwitchDialog.tsx` | 574 | **9**（barrel；`switchdialog/` 3 檔，最大 353） |
| `Diagnostics.tsx` | 563 | **6**（barrel；`diagnostics/` 7 檔，最大 291） |
| `usage/usageLenses.tsx` | 453 | **8**（barrel；`lenses/` 4 檔，最大 210） |
| `tools/versions.rs` | 450 | `tools/versions/` 4 檔，最大 **184** |
| `proxy/tests/e2e_translate.rs` | 445 | 23 行 root ＋ 3 檔，最大 **162** |
| `proxy/tests/e2e_strict.rs` | 430 | 23 行 root ＋ 3 檔，最大 **194** |
| `keys.rs` | 419 | `keys/` 2 檔，最大 **363** |
| `price_extract.rs` | 410 | `price_extract/` 2 檔，最大 **359** |

> **方法與保證（三條不變式）**：
> 1. **純搬移** —— 零行為、零文案、零邏輯、零 JSX／className 變更。多個子代理
>    各自做了逐位元組／多重集比對（例如 `Calc.tsx`：142 個中文片段 697 字元
>    零差異；`ProviderForm.tsx`：597 行中僅 7 行是表頭與 import 路徑）。
> 2. **對外路徑不變** —— 前端用再匯出 barrel，Rust 用 `pub(crate) use 子模組::*`，
>    所以**所有既有 import 端一行都不用改**（`App.tsx`、`lib.rs` 的
>    `generate_handler!`、各 `crate::X::Y` 呼叫點全部照舊）。
> 3. **可見性只放寬、不收緊** —— 跨子模組呼叫所需的 `pub(crate)`／`pub(super)`，
>    共十餘處，皆非邏輯變更。
>
> 過程中以 `cargo test` 通過數（218）、`cargo clippy`（0 警告）、
> `npx tsc --noEmit` 與 `pnpm build` 逐步把關。

**順手修掉的一個真實缺陷**：`Providers.tsx` 裡我先前加的「本機工具版本」面板
同時出現在內嵌區塊與收合區，等於**重複渲染**。拆檔時只保留收合區那一份。

**已驗證但你可能想自己再看一次**：

- `cargo test --offline` → **218 passed / 0 failed / 8 ignored**（exit 0）
- `cargo clippy --offline --all-targets` → **exit 0、零警告**（原為 4 條）
- `cargo check --offline --all-targets` → exit 0
- `npx tsc --noEmit` → 0 錯誤
- `pnpm build` → 成功，bundle 553.18 kB（gzip 152.36 kB）
- **全樹 202 個 `.ts`/`.tsx`/`.rs` 檔案，最大 399 行**，無一超過 400
- **拆 `keys.rs` 錯誤型別時的中文訊息逐字比對**：15 條 → 15 條，零遺漏、
  零新增、零改字（腳本：`.workbuddy/tmp/verify_msg_text.py`）

> **我沒有實際啟動過 GUI**。二進位的正確性是用「建置成功 + 二進位內含修正
> 字串 + 前端 bundle 內含新文案」間接確認的，升級路徑是用真實資料庫副本
> 實跑的。GUI 本身要由你第一次啟動時驗證 —— 這也是測試 A 的意義。
>
> **2026-09-28 補註**：這一輪（用量「工具」視角）我**實際啟動了 GUI**，
> 以「視窗截圖 → OCR → 與資料庫逐項核對」驗證過畫面，方法與證據見 §0.1。
> 上面這段描述的是 09-27 那次建置的驗證方式，保留作為對照。

---

## 9. 建置資訊

### 9.5 前一次建置（2026-09-28 12:31，§8 誠實清單清空輪，已被 9.6 取代）

| 項目 | 值 |
|---|---|
| 建置時間 | **2026-09-28 12:31:27**（release 編譯 7 分 33 秒） |
| 執行檔大小 | 9,010,176 bytes |
| MSI 大小 | 7,344,128 bytes |
| NSIS 大小 | 3,830,299 bytes |
| 後端測試 | **218 passed / 0 failed / 8 ignored**（實跑 `cargo test --offline`，exit 0） |
| clippy | **0 個警告**（`cargo clippy --offline --all-targets`，exit 0） |
| 前端型別 | `npx tsc --noEmit` exit 0 |
| 前端資源指紋 | `index-Dy7Y8xkU.js`、`index-DnRx7SDk.css` |

**產物內容抽查**（讀 exe 位元組）：

| 字串 | 結果 | 意義 |
|---|---|---|
| `index-Dy7Y8xkU.js` / `index-DnRx7SDk.css` | ✅ 存在，與 `dist/` 一致 | 內嵌的確實是本次建置的前端 |
| `usage_by_app` | ✅ 存在 | 第二階段的分工具用量仍在 |
| `via_gateway` | ❌ 不存在 | D-1（單一模式）沒有被拆檔改回去 |
| `hermes_home` / `parse_hermes_model` | ❌ 不存在 | F（移除 hermes 支援）仍然成立 |

**安裝驗證**：

| 步驟 | 結果 |
|---|---|
| 安裝前 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 9,009,152 bytes、04:34:36（前一版） |
| 執行 NSIS `/S` | exit code 0 |
| 安裝後同一路徑 | **9,010,176 bytes、12:31:18**，內含 `index-Dy7Y8xkU.js` ✅ |

**實機 GUI 驗證（本輪新增，逐項都是螢幕 OCR 讀出來的）**：

| 驗證項 | 結果 |
|---|---|
| 應用程式啟動、用量頁渲染 | ✅ 側邊欄、摘要卡、八個鏡頭頁籤（工具／模型／月度／時段…）全部讀到 |
| **要求二：本機工具篩選保留全部選項** | ✅ **展開下拉後 8 個選項全部在畫面上**：全部本機工具、Claude Code、Codex、OpenCode、**Hermes Agent**、**DeepSeek Harness**、**Cursor**、**Antigravity** |
| DSH 標籤正確 | ✅ 顯示「DeepSeek Harness」（非舊的「DeepSeek」） |
| 導航可用 | ✅ 點側邊欄可切到「設定」頁（監聽連接埠 15722、強調色等區塊都正確渲染） |
| **未寫入任何設定** | ✅ `app.db` 仍為 09-27 17:09:05、1,425,408 bytes；`settings.json`／`config.toml`／`opencode.json` 時間戳皆未變 |
| **備份未被輪換** | ✅ claude 10／codex 11／hermes 8／opencode 10，與驗證前基線完全相同 |
| 下拉未選擇任何項目 | ✅ 驗證後按 ESC 關閉，未觸發 `onChange` |

> **一個操作上的坑（給下次的自己）**：這台機器的 WebView2 會**吃掉前幾次合成滑鼠
> 點擊**（第一次點擊只用來讓 webview 取得焦點）。純 Win32 程式（Notepad 測試）
> 不受影響，所以別把「沒反應」誤判成 UI 壞了。本輪最後是「同一點連點 3 次」
> 才穩定生效；且 OCR 座標與點擊座標存在固定偏移，需先做一次校準。

---

### 9.41 建置（2026-10-04 17:56，品牌更名 Token Gateway → Token Toll）

依 §0.9.41 更名：顯示名全部改為 Token Toll；識別碼（渠道 ID／環境變數／匯出格式）
改新名但**讀取端新舊都認**；`identifier` 與登錄檔值名刻意保持不變。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-10-04 17:56 |
| `target\release\tokentoll.exe` | 9,323,520 bytes、sha256 `910E4D1C899349295FE2E3D677CCDA4BF0E906A48D759AACB3DB7808FEC0EFD9` |
| NSIS 安裝檔 | 4,086,584 bytes、sha256 `838F270D672B3E6D2998EA8EA9C3173914669E1AEBCF110FD2CF180854F14195` |
| MSI | 7,688,192 bytes |
| 安裝後 `%LOCALAPPDATA%\Token Toll\tokentoll.exe` | 9,515,520 bytes |
| 前端資源指紋 | `index-Bjce9vjq.js`（改名只動字串，bundle 名稱不變） |

| 閘門 | 結果 |
|---|---|
| `cargo test --offline --lib` | **318 passed / 0 failed / 8 ignored** ✅ |
| `cargo clippy --offline --all-targets` | **0 warning** ✅ |
| `pnpm exec tsc --noEmit` | exit 0 ✅ |
| `pnpm exec tauri build` | exit 0 ✅（release 4m50s） |

#### 安裝與遷移驗證

| 驗證項 | 結果 |
|---|---|
| NSIS 靜默安裝 | ✅ exit code 0 |
| 新安裝路徑 | ✅ `%LOCALAPPDATA%\Token Toll\`（`tokentoll.exe` ＋ `gw.exe` ＋ `uninstall.exe`） |
| **使用者資料完好**（identifier 未變的關鍵驗證） | ✅ `providers=7`、`request_logs=6137`、`local_keys=2`、`schema_version=14` —— 與改名前完全一致 |
| 沒有產生新 DB | ✅ `%APPDATA%` 底下仍只有 `com.tokencounter.gateway\app.db` 一個 |
| **視窗標題** | ✅ `Token Toll` |
| **側欄品牌** | ✅ 顯示 `Token Toll` ＋「本機 AI 用量收費站」；**舊名 `Token Gateway` 完全不剩** |
| 六頁導航 | ✅ 用量／上游來源／本地 Key／診斷／試算／設定，無原始 i18n key |
| 用量資料讀取 | ✅ 切「最近 7 天」得到 1,024 筆／30.9 億 tokens；「最近 30 天」得到 6,105 筆／47.5 億 tokens |

> **一次虛驚，寫下來給下次的自己**：初次讀畫面時「總請求 0」讓我以為 DB 讀不到，
> 追查後發現只是**預設時間範圍是「今天」**（今天還沒用量）。
> 資料庫自始至終都正常，`identifier` 保持不變的決定完全奏效。
> 教訓：驗證資料是否存在時，要指定足夠長的時間範圍。

#### 舊版清理與系統整合修復

| 項目 | 處置 |
|---|---|
| 舊版程式 `%LOCALAPPDATA%\token-gateway\` | ✅ 執行舊版自帶 `uninstall.exe /S`（exit 0）完整移除 |
| 舊捷徑 `token-gateway.lnk`（開始功能表） | ✅ 隨卸載一併清除 |
| 新捷徑 `Token Toll.lnk` | ✅ 指向 `…\Token Toll\tokentoll.exe` |
| **開機自啟** `Startup\TokenGateway.lnk` | ✅ 原本指向已移除的舊 exe（失效），**已重新指向新 exe**並驗證目標存在 |

> 登錄檔值名 `TokenGateway` 刻意保留（見 §0.9.41），使用者看不到它，
> 改了只會讓既有自啟變成孤兒。

### 9.40 前一次建置（2026-10-04 16:29，側邊欄「診斷」選中圖示修正，已被 9.41 取代）

依 §0.9.39 修正診斷頁選中後圖示變成閃電的問題。改動只有兩處：
`src/components/icons.tsx`（新增 `alert-fill`）、`src/App.tsx`（`iconFill` 改指向它）。
**沒有動 schema、沒有動任何後端程式碼。**

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-10-04 16:29:42 |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 9,513,984 bytes |
| `target\release\token-gateway.exe` | 9,513,984 bytes |
| 前端資源指紋 | `index-Bjce9vjq.js` |

> 這次 exe 大小與 §9.39 相同是正常的：改動只在前端 JS，而前端是**內嵌**進 exe 的資源，
> Rust 端一個位元組都沒動，所以體積不變（內容不同）。

| 閘門 | 結果 |
|---|---|
| `pnpm exec tsc --noEmit` | exit 0 ✅ |
| `cargo test --offline --lib` | **318 passed / 0 failed / 8 ignored** ✅ |
| `cargo clippy --offline --all-targets` | 0 warning ✅ |
| `pnpm exec tauri build` | exit 0 ✅ |

#### 實機驗證（CDP 讀真實 DOM）

側邊欄六項逐一點擊，比對「未選中 vs 選中」的 SVG path 幾何，
並讀出選中態是否使用 `fill="currentColor"` 與 `fill-rule`：

| 頁籤 | 選中態 path | 用 fill | fill-rule | 判定 |
|---|---|---|---|---|
| 用量 | `rect` × 3（實心柱） | ✅ | — | ✅ |
| 上游來源 | 與未選中完全相同 | ✅ | — | ✅ |
| 本地 Key | 同形狀填色版 | ✅ | — | ✅ |
| **診斷** | `M12 4L21 19.5H3L12 4z`（與未選中**同一個三角形**） | ✅ | **evenodd** | ✅ **已修正** |
| 試算 | 與未選中完全相同 | ✅ | — | ✅ |
| 設定 | 同形狀（線寬加粗） | — | — | ✅ 原本設計 |

NSIS 靜默安裝 exit code 0；安裝後啟動正常。

### 9.39 前一次建置（2026-10-03 17:26，App 主 Icon 換裝，已被 9.40 取代）

依 §0.9.38 的定案（R7「02 橫向等長列」）換裝全專案 icon。**沒有動 schema（仍 v15）、
沒有動任何功能程式碼**，本輪改動只有：icon 資產、`index.html` 的 favicon 指向、
`tauri.conf.json` 的 NSIS 圖示設定、以及文件。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-10-03 17:26:25 |
| `target\release\token-gateway.exe` | 9,513,984 bytes、sha256 `070C059920DA65974E98DB934C188DF47F03C4EBFB5E81BA022C9DDB7609D4EE` |
| NSIS 安裝檔 | 4,090,542 bytes、sha256 `69C49BC6BF2629836A63079E5D520F2B574E24DC41434745B3C40BC6FCCF9455` |
| MSI | 7,688,192 bytes（2026-10-03 17:26:11） |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 9,513,984 bytes、sha256 `D9C5B05E9C6460FF1FC17B325293A84995851DAB5BF5DC36545513BCDB950432` |
| 前端資源指紋 | `index-Bjce9vjq.js`（605.62 kB）、`index-uFaTH7y6.css`（57.56 kB） |
| 母稿（入庫） | `token-gateway/icon-base-1024.png` |

> 安裝後 exe 的 sha256 與 `target\release` 的不同是**正常的**：Tauri 會對要打包的 exe
> 再補一次 bundle type 資訊（日誌裡的 `Patching … with bundle type information: nsis`），
> 大小相同、內容差在 PE 的一小段。前幾輪的紀錄也是這樣。

| 閘門 | 結果 |
|---|---|
| `cargo test --offline --lib` | **318 passed / 0 failed / 8 ignored** ✅ |
| `cargo clippy --offline --all-targets` | **0 warning** ✅ |
| `pnpm exec tsc --noEmit` | exit 0 ✅ |
| `pnpm exec tauri build` | exit 0 ✅（前端 36.28s ＋ release 編譯 5m47s） |

#### 抓到一個「換了圖卻沒生效」的坑（本輪最有價值的發現）

第一次建置完，**exe 裡的 icon 還是舊的**——把新 exe 的 256px icon 與舊安裝版逐位元比對，
結果 `ImageChops.difference(...).getbbox() is None`（完全相同）。

追下去看到 build script 的輸出目錄：

| 檔案 | 時間戳 | 意義 |
|---|---|---|
| `target\release\build\token-gateway-*\out\resource.rc` | **2026-09-28 01:17** | 這次建置**沒有重跑** build script |
| `icons/icon.ico` | 2026-10-03 17:06 | 圖早就換好了 |

`resource.rc` 只是**引用** `icons/icon.ico` 的**路徑**，圖示的位元組在 build script 執行時
就已經編進 `resource.lib` 了。因為 rc 檔內容沒變（路徑字串一樣）＋ crate 原始碼沒變，
Cargo 就跳過 build script → link 進去的是 9/28 編好的舊 lib。

**修法**：`cargo clean -p token-gateway` 清掉這個 crate 的產物後重建。
（代價：`cargo clean -p` 實際上把 target 裡大部分東西一起清了 23.4 GiB，
導致這次 release 編譯要 5m47s；下次換 icon 記得先預期這件事。）

#### 順手修好：NSIS 安裝檔用的是 NSIS 預設圖示

`bundle.icon` 只管應用程式本身的圖示，**不影響安裝檔**。所以安裝程式在檔案總管裡
一直顯示 NSIS 的預設圖示。補上設定後安裝檔也換成新圖示：

```json
"bundle": {
  "windows": {
    "nsis": {
      "installerIcon": "icons/icon.ico",
      "uninstallerIcon": "icons/icon.ico"
    }
  }
}
```

> 一度寫成 `installerHeaderIcon` 而被 schema 擋下：Tauri 2.11 的 `NsisConfig` 只有
> `template`／`headerImage`／`sidebarImage`／`installerIcon`／`uninstallerIcon`／
> `uninstallerHeaderImage`／`installMode`／`languages`／`compression`／`startMenuFolder`／
> `installerHooks`，**沒有** `installerHeaderIcon`。錯誤訊息是
> `is not valid under any of the schemas listed in the 'anyOf' keyword`。

#### 實機驗證（安裝後，逐項都是從真實檔案／視窗讀出來的）

| 驗證項 | 方法 | 結果 |
|---|---|---|
| 安裝後 exe 內嵌 icon | `PrivateExtractIcons` 256px | ✅ 256×256、實心 51,675、藍 **95.0%**、左上角 alpha **0** |
| 安裝後 exe 內嵌 icon（16px） | 同上 | ✅ 16×16、實心 184、藍 100% |
| NSIS 安裝檔內嵌 icon | 同上 | ✅ 與 exe／master **同一個簽章**（51,675／95.0%／0） |
| MSI 的圖示設定 | 讀 `wix\x64\main.wxs` | ✅ 第 49–50 行 `<Icon Id="ProductIcon" SourceFile="…resource.ico">` ＋ `ARPPRODUCTICON=ProductIcon`，第 149 行捷徑 `Icon="ProductIcon"` |
| **主視窗圖示（真實 HWND）** | 列舉 `Tauri Window` 並 `WM_GETICON` | ✅ 32×32、實心 760、藍 97.2%、角落透明 → 與 `icons/32x32.png` 完全一致 |
| **系統列圖示（16px 實際尺寸）** | 從 exe 抽 16px | ✅ 16×16、實心 184、藍 100% |
| 開始功能表捷徑 | 檔案時間戳 | ✅ `…\Start Menu\Programs\token-gateway.lnk` 更新為 2026-10-03 17:27:18 |
| 安裝前後 exe | 大小／時間戳 | ✅ 9,722,368（10/02 02:12）→ **9,513,984（10/03 17:26）** |
| NSIS 靜默安裝 | `Start-Process /S -Wait` | ✅ exit code 0 |

#### 實機 GUI 回歸（CDP 讀真實畫面，確認換 icon 沒動到功能）

| 驗證項 | 結果 |
|---|---|
| 側邊欄 | ✅ 正好六項：用量／上游來源／本地 Key／診斷／試算／設定 |
| 逐頁點擊切換 | ✅ 六頁全部渲染、標題與導覽一致（內文長度 288～8,470 字） |
| i18n 洩漏 | ✅ 六頁的原始字典 key 數量皆為 **0** |
| 用量頁資料 | ✅ 總請求 115、真實消耗 tokens 175,114,942、快取命中率 49.8%，網關 `127.0.0.1:15722` 正常 |

#### 檢查過的實機 icon 一覽（全部同一個簽章）

| 來源 | 尺寸 | 實心像素 | 藍佔比 | 角落 alpha |
|---|---|---|---|---|
| `icons/128x128@2x.png`（基準） | 256×256 | 51,675 | 95.0% | 0 |
| 建置產物 exe | 256×256 | 51,675 | 95.0% | 0 |
| NSIS 安裝檔 | 256×256 | 51,675 | 95.0% | 0 |
| 安裝後 exe | 256×256 | 51,675 | 95.0% | 0 |
| 安裝後 exe（16px） | 16×16 | 184 | 100% | 2（縮圖反鋸齒） |
| 主視窗 HWND | 32×32 | 760 | 97.2% | 0 |
| `icons/32x32.png`（基準） | 32×32 | 760 | 97.2% | 0 |

### 9.38 前一次建置（2026-10-02 02:13，移除 MCP／提示詞／技能／會話，已被 9.39 取代）

依 §0.9.37 的計畫移除四個功能（A：DROP 六張表＝schema v14；B：不動使用者檔案；
C：舊 deeplink 直接拒絕；D：只動這四項）。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-10-02 02:13:08 |
| `target\release\token-gateway.exe` | 9,722,368 bytes、sha256 `9FA6264D2BBC815A31F7241C7112177F72714FF9356066883ACE4A3AA3DC4626` |
| NSIS 安裝檔 | 4,073,354 bytes、sha256 `9B1E85574EC1BC4FAA0495AD8797DB8CFF0C81F22D534FB589C61A0D825B212C` |
| MSI | 7,643,136 bytes（2026-10-02 02:12:53） |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 9,722,368 bytes、sha256 `D681C764CE5CBC797C00BF58031BA3B88483CCEBFD6D7AFE876C47115C653B8D` |
| 前端資源指紋 | `index-CAZNryZ3.js`（631,110 bytes） |

**體積變化**（移除約 8,561 行、32 檔）：

| 產物 | 之前（9.36） | 之後 | 差 |
|---|---|---|---|
| `token-gateway.exe` | 10,340,864 | 9,722,368 | **−618,496 bytes（−604 KB）** |
| NSIS 安裝檔 | 4,288,016 | 4,073,354 | **−214,662 bytes（−210 KB）** |
| 前端 bundle | — | 631,110 | — |
| 原始碼檔數 | 317 | **285** | −32 |

| 閘門 | 結果 |
|---|---|
| `cargo test --offline --lib` | **351 passed / 0 failed / 8 ignored** ✅（原 449／0／11） |
| `cargo clippy --offline --all-targets` | **0 warning** ✅ |
| `pnpm exec tsc --noEmit` | exit 0 ✅ |
| `pnpm exec tauri build`（含 tsc） | exit 0 ✅ |

> 測試數 449 → 351（−98）：移除的模組自帶 95 個測試，deeplink 的 mcp／prompt／skill
> 案例再少 3 個；`ignored` 11 → 8 是因為那 3 個 `#[ignore]` 的 live 測試（MCP 實機、
> GitHub 技能掃描）也在被移除的模組裡。新增 1 條（舊資源要被明確拒絕）。

#### 實機驗證（安裝後、CDP 讀真實畫面）

| 驗證項 | 結果 |
|---|---|
| **schema 升級到 v14**（真實使用者庫） | ✅ `max(schema_version)=14`；六張表全部消失 |
| **使用者資料一列不少** | ✅ providers 7／request_logs 5,859／local_keys 2（與移除前一致） |
| 側邊欄 | ✅ 正好六項：用量／上游來源／本地 Key／診斷／試算／設定；MCP／提示詞／技能／會話 皆不存在 |
| 六個頁面 | ✅ 全部正常渲染、標題正確、**無原始字典 key** |
| **舊 deeplink 連結** | ✅ 用系統呼叫真的觸發 `tokengateway://v1/import?resource=mcp&…` → 對話框顯示「**不支援的 resource：mcp（目前只支援 provider）**」，且確認鈕不可按 |
| 設定頁文案 | ✅ 「tokengateway:// 一鍵匯入**上游來源**」（範例連結也是 `resource=provider`） |
| **使用者的檔案完全沒被動** | ✅ `~/.codex/AGENTS.md` 4,139 bytes、sha256 `B67E238D938BB91662D2C93E8053162A8EC6AF9BD0284FD04E476D89A4128F03`（與移除前**逐位元相同**） |
| **Codex 的 MCP 設定照舊可用** | ✅ `~/.codex/config.toml` 仍有 `[mcp_servers]`、`[mcp_servers.node_repl]`、`[mcp_servers.node_repl.env]` |
| 技能目錄 | ✅ `~/.codex/skills` 仍是 4 項（使用者自己的技能） |
| 施工前備份 | ✅ `<app_data>/db-backups/manual-v13-before-removing-4-features-20261002-015259.db`（2,179,072 bytes） |

> 已知未處理（與本次移除無關，施工前就是如此）：`proxy.rs` 405 行、`tools/native.rs` 418 行、
> `tools/switch.rs` 418 行三支超過 400 行上限。先前的稽核用 PowerShell 的
> `Measure-Object -Line` 計數會少算，改用逐檔 `splitlines()` 才看得出來。

### 9.37 前一次建置（2026-10-01 22:23，訂閱額度同時顯示已用／剩餘，已被 9.38 取代）

延續 §0.9.36 的查證結果（官網標「% left」、App 標「已用」，兩者其實是補數）。
本輪把兩個數字一起寫在圓環裡，並補上「窗口一重置就立刻重查」。**沒有動 schema（仍 v13）**。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-10-01 22:23:08 |
| `target\release\token-gateway.exe` | 10,340,864 bytes、sha256 `4BF3368327F31DFB54081CEFEAC05AE1EE7813DC3FA811479886EFA1AAAB4C5B` |
| NSIS 安裝檔 | 4,288,016 bytes、sha256 `956CD09F930CA755ECDCA183C0D1A2D5CE6FCB81B5659141F897E7F55B5C366B` |
| MSI | 7,929,856 bytes（2026-10-01 22:22:53） |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 10,340,864 bytes、sha256 `45BBDA06B1BDA2F1D0713C72DCF9E814C7F0D3D6B3924FD35BC204159C7D344F` |
| 前端資源指紋 | `index-RoIf1G2q.js` |

| 閘門 | 結果 |
|---|---|
| `cargo test --offline --lib` | **449 passed / 0 failed / 11 ignored** ✅ |
| `cargo clippy --offline --all-targets` | **0 warning** ✅ |
| `pnpm exec tsc --noEmit` | exit 0 ✅ |
| `pnpm exec tauri build`（含 tsc） | exit 0 ✅ |

#### 實機驗證（安裝後、CDP 讀真實畫面）

22:25:47 的畫面（`更新於 22:25:41`）：

```
訂閱額度  圓環＝已用 %｜小字＝剩餘 %
OpenCode Go   0.0% / 剩 100% / 5 小時 / 重置 5時0分
              35%  / 剩 65%  / 本週   / 重置 3天9時
              34%  / 剩 66%  / 本月   / 重置 20天18時
ChatGPT       0.0% / 剩 100% / 30 天  / 重置 27天4時
```

| 驗證項 | 結果 |
|---|---|
| **「剩 X%」與官網 console 的 `% left` 逐字相同** | ✅ 官網 100 / 65 / 66 ＝ App 的 `剩 100% / 剩 65% / 剩 66%` |
| rolling 已跨過 22:07:49 的重置 | ✅ 新窗口顯示 `重置 5時0分`；22:27:06 直接打上游得 `resetsAt=19:27:08Z` ＝ 本地 03:27:08，剩 5時0分2秒 ✅ |
| 標題列說明 | ✅ 「圓環＝已用 %｜小字＝剩餘 %」＋ tooltip 舉例（100 % left ＝ 已用 0%／剩 100%） |
| 十個頁面 | ✅ 全部正常渲染、無原始字典 key |

> **「窗口一重置就立刻重查」的 before/after**：before 有實測到 —— 10:08:57 那次
> 倒數走到「已重置」後畫面停在「已重置」直到下一次輪詢（22:07:36 抓的資料、
> 22:12:36 才會換）。修好後的 after 需要等下一次視窗邊界（rolling 在 03:2x），
> 本輪來不及實測；觸發條件是精確的（`resetsAt <= now`），不是猜的。

### 9.36 前一次建置（2026-10-01 20:49，訂閱額度：讀數時刻反推，已被 9.37 取代）

延續 §9.35／§0.9.35 的第二輪修正（「圓環＝已用 %」標示、過期琥珀色、watchdog）。
**沒有動 schema（仍 v13）**，也沒有動後端 Rust。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-10-01 20:49:57 |
| `target\release\token-gateway.exe` | 10,340,864 bytes、sha256 `77BAD6CA017DB61B54FA3159E528DC28D0365FF793754D2335CD5FDD5D85BE98` |
| NSIS 安裝檔 | 4,288,997 bytes、sha256 `1138CC3233F4CAA0D2E5AD0DB31A8B1B097FC837C04F7D210C6F9276307B0C2E` |
| MSI | 7,929,856 bytes（2026-10-01 20:49:42） |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 10,340,864 bytes、sha256 `984404A01A8B865B6E62FB858CA18C01160142080EA2E32AB3A2CA81CD0DC5B9` |
| 前端資源指紋 | `index-z5fv4O2U.js` |

| 閘門 | 結果 |
|---|---|
| `cargo test --offline --lib` | **449 passed / 0 failed / 11 ignored** ✅ |
| `cargo clippy --offline --all-targets` | **0 warning** ✅ |
| `pnpm exec tsc --noEmit` | exit 0 ✅ |
| `pnpm exec tauri build`（含 tsc） | exit 0 ✅ |

#### 實機驗證（安裝後、CDP 讀真實畫面）

| 驗證項 | 結果 |
|---|---|
| 標示「圓環＝已用 %」 | ✅ 標題列出現（含 tooltip 說明與後台「剩餘 %」的補數關係） |
| 四個窗口與上游逐字吻合 | ✅ 21:01:16 畫面 `1時6分／3天10時／20天20時／27天6時`；直接算上游 `resetsAt` 得 `1時6分33秒／3天10時58分／20天20時23分／27天6時21分` |
| **30 秒心跳** | ✅ 21:01:26 `1時6分` → 21:02:07 `1時5分` → 21:03:07 `1時4分` → 21:04:08 `1時3分`；「更新於」全程停在 21:01:00 |
| 陳舊度顯示 | ✅ `更新於 21:01:00（1 分鐘前）` → `（2 分鐘前）` |
| 未過期時不誤報 | ✅ 3.5 分鐘時仍是正常色（門檻 7.5 分鐘） |
| 全螢幕遮蔽下輪詢照跑 | ✅ 20:27:15 → 20:32:17；心跳同時照走 |
| 十個頁面 | ✅ 用量／上游來源／MCP／提示詞／技能／會話／本地 Key／診斷／試算／設定 全部正常、無原始字典 key |

> 上一輪（§9.35，19:00）的驗證不重複列：0% 窗口顯示重置時間、還原焦點後 4 秒內重查、
> 5 分鐘輪詢 19:00:36 → 19:05:38 → 19:10:22，都已在該節記錄。

### 9.35 前一次建置（2026-10-01 19:00，訂閱額度即時更新，已被 9.36 取代）

訂閱額度「沒有即時更新」的修正（設計與量測見 §0.9.34）。**沒有動 schema（仍 v13）**。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-10-01 19:00:06 |
| `target\release\token-gateway.exe` | 10,340,352 bytes、sha256 `BF93DB3E0EB103F043C941C4B247B763C80DFFD212CF02BF6A1440B4D8AB7955` |
| NSIS 安裝檔 | 4,287,802 bytes、sha256 `DF902D57E8BCE5C2BB8B634D7A1CF482E9B9E1B0650357C58E18066AB1D13D57` |
| MSI | 7,929,856 bytes（2026-10-01 18:59:52） |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 10,340,352 bytes、sha256 `40454547AA186DF2442083FA622723227A916A0CB4E9637C48D10678D2AEFF0F` |
| 前端資源指紋 | `index-BwjD0zLI.js` |

| 閘門 | 結果 |
|---|---|
| `cargo test --offline --lib` | **449 passed / 0 failed / 11 ignored** ✅ |
| `cargo clippy --offline --all-targets` | **0 warning** ✅ |
| `pnpm exec tsc --noEmit` | exit 0 ✅ |
| `pnpm exec tauri build`（含 tsc） | exit 0 ✅ |
| 資料庫 schema | 仍 **13**；使用者資料未動（7 個來源、`request_logs` 5,820 列） |

#### 實機驗證（安裝後、CDP 讀真實畫面）

| 驗證項 | 結果 |
|---|---|
| 5 小時窗口 `0.0%` 也顯示重置時間 | ✅ 「重置 3時7分」（舊版「—」） |
| **倒數與上游逐秒吻合** | ✅ 19:09:45 畫面 2時58分；直接打 `/usage` 算出的剩餘 = `22:07:49 − 19:09:45` = **2時58分** |
| **30 秒心跳** | ✅ 19:00:59 `3時7分` → 19:01:19 `3時6分` → 19:02:20 `3時5分` → 19:03:20 `3時4分`；「更新於」全程停在 19:00:36（本地計時，未重打上游） |
| 陳舊度可見 | ✅ `更新於 19:07:38（1 分鐘前）` → `（2 分鐘前）` → `（3 分鐘前）` |
| 5 分鐘輪詢 | ✅ 19:00:36 → 19:05:38 → 19:10:22 |
| **回到焦點立刻重查** | ✅ 19:12:50 最小化（更新於 19:10:22）→ 19:12:52 還原 → 19:12:56 已是 **19:12:52**，比 5 分鐘輪詢（19:15:22）早 |
| 縮到匣時輪詢不停 | ✅ 舊 build 量測 18:42:41 → 18:47:43 → 18:52:44（本機 WebView2 的 `visibilityState` 維持 `visible`） |

### 9.34 前一次建置（2026-10-01 13:46，收尾稽核，已被 9.35 取代）

P1～P4 全部完成後的稽核建置。修掉一個收尾時用煙霧測試抓到的 i18n 漏網
（頁面標題仍直接用字典 key，畫面會顯示 `nav.usage`）。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-10-01 13:46:44 |
| `target\release\token-gateway.exe` | 10,338,304 bytes、sha256 `791FD456E74B51185DCDA7D39257D3D488B15A6A510CF12E4E65C7A0FAC2E24C` |
| NSIS 安裝檔 | 4,282,059 bytes、sha256 `51981869FF5D2A7870C8B81D25730FC96314E5DC6DC90544C254B69E637F531F` |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 10,338,304 bytes、sha256 `DFFB60ABBBC16D661F48F3572D42A869163C27F88D24860A6A433F1E4E4FD8F8` |

| 閘門 | 結果 |
|---|---|
| `cargo test --offline --lib` | **449 passed / 0 failed / 11 ignored** ✅ |
| `cargo clippy --offline --all-targets` | **0 warning** ✅ |
| `pnpm exec tsc --noEmit` | exit 0 ✅ |
| `pnpm exec tauri build`（含 tsc） | exit 0 ✅ |
| 每個檔案 ≤400 行 | ✅ 全庫合規 |
| 資料庫 schema | 仍 **13**（整個計畫期間沒有為了功能而改 schema；功能模組各自帶自己的表） |

#### 收尾實機稽核

| 項目 | 結果 |
|---|---|
| **十個頁面全部正常渲染** | ✅ 用量／上游來源／MCP／提示詞／技能／會話／本地 Key／診斷／試算／設定，無錯誤、無原始字典 key |
| i18n 漏網修正 | ✅ 頁面標題從 `nav.usage` 變回「用量」 |
| **要求二：本機工具篩選** | ✅ **8 個選項**（全部本機工具 ＋ Claude Code／Codex／OpenCode／Hermes Agent／DeepSeek Harness／Cursor／Antigravity） |
| **要求二：歷史用量不退化成 unknown** | ✅ `source='import'` 的 **2,210 列全部有具名工具**（claude 33／codex 529／dsh 48／opencode 78 條匯入路徑）。另有 408 列 `app='unknown'` 但 `source='gateway'` —— 是**請求自己沒帶可辨識工具**（401／404／400／429 的失敗請求與探測），佔全部 token 的 **0.0021%**，非歷史回填、非本次對齊引入 |
| **真實 CLI 往返** | ✅ `codex exec -m z-ai/glm-5.3-flash` → `FINAL-OK`（經本機網關：路由、格式轉換、記帳都正常） |
| 網關的自我保護 | ✅ 順帶驗到直連模式的規則有效：用不在該來源清單的模型會被明確拒絕（「僅允許來源登記的模型，請求不會轉發」），不是默默失敗 |

### 9.33 前一次建置（2026-10-01 13:35，P4.9：更新檢查，已被 9.34 取代）

CC Switch 對齊計畫 P4.9（設計見 §0.9.33）。**沒有動 schema（仍 v13）**。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-10-01 13:35:21 |
| `target\release\token-gateway.exe` | 10,338,304 bytes、sha256 `85A866FCA1CF79A528DEC94E97CED6007DB879B214F37BF5F5E2010004026969` |
| NSIS 安裝檔 | 4,281,073 bytes、sha256 `B390929AAA262E9B17F56DA6F58346E4E944685872CFED71D4AAABA1AF99FB97` |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 10,338,304 bytes、sha256 `D49345BFA92911F5627A7C19F3836960752762A07904227EFFA57B750E50ED54` |

| 閘門 | 結果 |
|---|---|
| `cargo test --offline --lib` | **449 passed / 0 failed / 11 ignored** ✅（+7：updater） |
| `cargo clippy --offline --all-targets` | **0 warning** ✅（順手清掉兩個既有警告，其中一個是真的 bug） |
| `pnpm exec tsc --noEmit` | exit 0 ✅ |
| `pnpm exec tauri build`（含 tsc） | exit 0 ✅ |
| 資料庫 schema | 仍 **13** ✅ |
| 每個檔案 ≤400 行 | ✅ 維持合規 |

**改到的檔案**

| 檔案 | 改動 |
|---|---|
| `src-tauri/src/updater/{mod,check,tests}.rs` | **新增**：manifest 解析、版本比較、平台對應、檢查結果 |
| `src-tauri/src/commands/updater_cmd.rs` | **新增**：`update_state`／`update_set_url`／`update_check` |
| `src/components/settings/UpdateSection.tsx` | **新增**：更新區塊（網址、檢查、結果卡含 notes／日期／下載連結／無簽章說明） |
| `src/lib/types/platform.ts`、`src/lib/api/platform.ts`、`src/lib/i18nDict.ts`、`src/components/Settings.tsx` | 型別／API／字典／掛載 |
| `src-tauri/src/lib.rs`、`src-tauri/src/tools/switch.rs` | 註冊 `skills_set_settings`（漏掉的真 bug）＋ `key_id` 標註為 wire-format 保留欄位 |

#### 實機驗證

| 驗證項 | 結果 |
|---|---|
| 檢查到新版 | ✅ 0.99.0：版本、發布日期、release notes 三行、下載連結、無簽章提示 |
| 已是最新 | ✅ manifest 改成 0.1.0 → 「已是最新」 |
| 記錄與清理 | ✅ 上次檢查寫入設定；測試伺服器／資料／設定全數清乾淨 |

### 9.32 前一次建置（2026-10-01 13:18，P4.11：多語系基礎建設，已被 9.33 取代）

CC Switch 對齊計畫 P4.11（設計見 §0.9.32）。**沒有動 schema（仍 v13）**。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-10-01 13:18:41 |
| `target\release\token-gateway.exe` | 10,292,736 bytes、sha256 `FFD8821B6B02E5BF56CC8C2F4EA483A816D89A3EC05AC57B53BD60E6448B0C47` |
| NSIS 安裝檔 | 4,273,697 bytes、sha256 `BD4EA043A1225179DE941A184E6184661357514392DD6DEEF99A21C9B72512D3` |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 10,292,736 bytes、sha256 `F0AC4632383F520EA55B21B1F0149BED06901CE519F4A91F33A34A3DD155306F` |

| 閘門 | 結果 |
|---|---|
| `cargo test --offline --lib` | **441 passed / 0 failed / 11 ignored** ✅ |
| `cargo clippy --offline --all-targets` | exit 0 ✅（兩個既有的無害警告：`skills_set_settings` 未使用、`key_id` 欄位未讀） |
| `pnpm exec tsc --noEmit` | exit 0 ✅ |
| `pnpm exec tauri build`（含 tsc） | exit 0 ✅ |
| 資料庫 schema | 仍 **13** ✅ |
| 每個檔案 ≤400 行 | ✅ 維持合規 |

**改到的檔案**

| 檔案 | 改動 |
|---|---|
| `src/lib/i18n.tsx`、`src/lib/i18nDict.ts` | **新增**：`I18nProvider`、`useI18n()`、`t()`、135 個 key 的繁中／英文字典 |
| `src-tauri/src/commands/appearance_cmd.rs` | **新增** `appearance_set_lang`；`appearance_state` 帶回 `lang` |
| `src/App.tsx` | 導覽與頂列改用 `t()`；掛上 `I18nProvider` |
| `src/components/Settings.tsx`、`settings/{AppearanceSection,DbBackupSection,CloudSection,DeeplinkSection}.tsx` | 全部字串改走字典（含確認框、提示、區間預設值） |

#### 實機驗證

| 驗證項 | 結果 |
|---|---|
| 即時切換（不重啟） | ✅ 導覽與設定區塊全部變英文，主題列與語言列的說明文字也跟著翻 |
| 切回繁中 | ✅ 完整還原 |
| 持久化 | ✅ `settings.lang = zh-TW` 寫入資料庫 |
| 覆蓋率 | 導覽＋設定頁 **100%**；全站 135／886 ≈ **15%**（腳本量測） |

### 9.31 前一次建置（2026-10-01 12:58，P4.10：雲端同步 WebDAV，已被 9.32 取代）

CC Switch 對齊計畫 P4.10（設計見 §0.9.31）。**沒有動 schema（仍 v13）**。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-10-01 12:58:29 |
| `target\release\token-gateway.exe` | 10,288,640 bytes、sha256 `19172E9CC19F05D5DBA30EA34293512F0C645573656ED75A8C263766B84DBF0A` |
| NSIS 安裝檔 | 4,264,710 bytes、sha256 `05922B920F8496177157069EF317992D7603DF948838A382A20B66A57239DC43` |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 10,288,640 bytes、sha256 `D73E913AA72C16EB462A95CAEB527091834F0DBD13059C00DC6AB03BACC573A7` |

| 閘門 | 結果 |
|---|---|
| `cargo test --offline --lib` | **441 passed / 0 failed / 11 ignored** ✅（+6：雲端同步） |
| `cargo clippy --offline --all-targets` | 0 warning ✅ |
| `pnpm exec tsc --noEmit` | exit 0 ✅ |
| `pnpm exec tauri build`（含 tsc） | exit 0 ✅ |
| 資料庫 schema | 仍 **13** ✅ |
| 每個檔案 ≤400 行 | ✅ **全庫 304 個原始檔全部合規**（`index.css` 800→9＋5 個分檔、`apiTypes.ts` 675→10＋3 個分檔、`api.ts` 560→30＋3 個分檔、`providers_io.rs` 598→307＋3 個分檔） |

**改到的檔案**

| 檔案 | 改動 |
|---|---|
| `src-tauri/src/cloudsync/{mod,store,webdav,tests}.rs` | **新增**：設定與狀態、WebDAV 四個動詞、上傳／下載（先安全備份再寫回）、下載分「抓」與「套用」兩步 |
| `src-tauri/src/commands/cloud_cmd.rs` | **新增**：state／set_config／set_auto／test／remote_info／upload／download |
| `src/components/settings/CloudSection.tsx` | **新增**：雲端同步區塊（含下載前的確認說明） |
| `src/styles/*.css`、`src/lib/types/*.ts`、`src/lib/api/*.ts`、`src-tauri/src/providers_io/*.rs` | 行數合規拆分（見上表） |

#### 實機驗證（對自己寫的本機 WebDAV 伺服器）

| 驗證項 | 結果 |
|---|---|
| 測試連線 | ✅ 目錄不存在也算連得上，並說明「上傳時會自動建立」 |
| 上傳 | ✅ MKCOL ×2 → PUT tokens.db（2,166,784 bytes）＋ manifest.json（167 bytes），伺服器 log 可查 |
| 遠端快照 | ✅ 協定／schema／App 版本／時間／大小都顯示 |
| 下載 | ✅ 先自動備份（`db_backup_20261001_130011`）再套用，schema v13 驗證通過 |
| 認證失敗 | ✅ 給出「Nextcloud／堅果雲要填應用程式密碼」的可操作訊息 |
| 清理 | ✅ 測試伺服器與資料已刪除、`cloud_*` 設定已清空 |

### 9.30 前一次建置（2026-10-01 12:36，P4.8：資料庫備份管理，已被 9.31 取代）

CC Switch 對齊計畫 P4.8（設計見 §0.9.30）。**沒有動 schema（仍 v13）**。
新增相依功能：`rusqlite` 的 `backup`。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-10-01 12:36:00 |
| `target\release\token-gateway.exe` | 10,183,680 bytes、sha256 `4B103EB96FEAADB651BE9194994735E92F1F436E0CF348E20168FC173E96199D` |
| NSIS 安裝檔 | 4,241,302 bytes、sha256 `8331E10BA393962C93E57825BBBAFB97840AFF7992E24C0B183586E3CF3E4C62` |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 10,183,680 bytes、sha256 `A409B2837E626DA7ACE40FB8160BDC9C4D4EAF08AB81734D6B996E58BCC3714B` |

| 閘門 | 結果 |
|---|---|
| `cargo test --offline --lib` | **446 passed / 0 failed / 11 ignored** ✅（+19：備份 8 ＋ dbbackup 相關） |
| `cargo clippy --offline --all-targets` | 0 warning ✅ |
| `pnpm exec tsc --noEmit` | exit 0 ✅ |
| `pnpm exec tauri build`（含 tsc） | exit 0 ✅ |
| 資料庫 schema | 仍 **13** ✅ |
| 每個檔案 ≤400 行 | ⚠️ 本輪清掉 4 個超標檔（Settings 487→199、UsageQueryPanel 411→347、prompt/sync 562→348、mcp/apply 498→406）；**仍有 4 個待清**：`index.css`(800)、`apiTypes.ts`(675)、`api.ts`(560)、`providers_io.rs`(598) |

**改到的檔案**

| 檔案 | 改動 |
|---|---|
| `src-tauri/src/dbbackup.rs`、`dbbackup/tests.rs` | **新增**：線上備份／還原、保留策略、排程邏輯、設定與狀態 |
| `src-tauri/src/commands/dbbackup_cmd.rs` | **新增**：state／now／restore／rename／delete／set_schedule |
| `src-tauri/src/lib.rs` | 註冊 6 個命令 ＋ 啟動時起自動備份排程 |
| `src/components/settings/{SectionHead,AppearanceSection,DeeplinkSection,DbBackupSection}.tsx` | **新增**（從 Settings 拆出來）；Settings.tsx 487→199 行 |
| `src/components/providers/UsageQueryParts.tsx` | **新增**（從 UsageQueryPanel 拆出來） |
| `src-tauri/src/prompt/sync/tests.rs`、`src-tauri/src/mcp/apply/readers.rs` | 測試／讀取段落搬到獨立檔案 |

#### 實機驗證

| 驗證項 | 結果 |
|---|---|
| 預設值 | ✅ 每 24 小時、保留 10 份（與 cc-switch 相同） |
| 立即備份 | ✅ 2.07 MB 備份建立、磁碟與清單一致 |
| 本地時間命名 | ✅ 修正 UTC 造成的 8 小時差 |
| 改名／刪除 | ✅ 清單與磁碟同步 |
| 還原＋安全備份 | ✅ schema v13 驗證通過、還原前自動備份、原有 7 個來源完好 |

### 9.29 前一次建置（2026-10-01 06:17，P4.7：關閉行為與輕量模式，已被 9.30 取代）

CC Switch 對齊計畫 P4.7（設計與三個實機抓到的問題見 §0.9.29）。**沒有動 schema（仍 v13）**。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-10-01 06:17:20 |
| `target\release\token-gateway.exe` | 10,122,240 bytes、sha256 `8497201D7AB94E9D40CF87114F4D1B7392B97CC3AA4EEE6D082D5FE7E0039BCB` |
| NSIS 安裝檔 | 4,218,897 bytes、sha256 `617C272D52BDC2F698521AE853CDD7DE3594120639C4B64B4485C83BF1266326` |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 10,122,240 bytes、sha256 `46AFE01BFA9310B3F93EA8A421A10FF993A7827E23B9A5BECFC2EF64AA6F1A05` |

| 閘門 | 結果 |
|---|---|
| `cargo test --offline --lib` | **427 passed / 0 failed / 11 ignored** ✅（+2：window） |
| `cargo clippy --offline --all-targets` | 0 warning ✅ |
| `pnpm exec tsc --noEmit` | exit 0 ✅ |
| `pnpm exec tauri build`（含 tsc） | exit 0 ✅ |
| 資料庫 schema | 仍 **13** ✅ |

**改到的檔案**

| 檔案 | 改動 |
|---|---|
| `src-tauri/src/window.rs` | **新增**：關閉行為、顯示／重建視窗、輕量模式、`decide_close` |
| `src-tauri/src/lib.rs` | `on_window_event` 攔關閉；`RunEvent::ExitRequested` 的托盤常駐；註冊 4 個新命令 |
| `src-tauri/src/tray.rs` | 托盤新增「開啟主視窗」「輕量模式（關閉視窗）」 |
| `src-tauri/src/deeplink/protocol.rs` | 輕量模式時 pending ＋ 重建視窗；隱藏時也叫回視窗 |
| `src-tauri/src/commands/window_cmd.rs` | **新增**：`window_behavior`／`window_set_close_action`／`window_enter_lightweight`／`window_show_main` |
| `src/components/Settings.tsx`、`src/lib/{api,apiTypes}.ts` | 視窗行為區與 API |

#### 實機驗證

| 驗證項 | 結果 |
|---|---|
| 托盤模式關閉 | ✅ `WM_CLOSE` 後行程 1、閘道仍連、CDP 仍回應 |
| 輕量模式 | ✅ 視窗銷毀但行程與閘道都活著 |
| Deep Link 喚醒 | ✅ 從輕量模式（與隱藏狀態）點連結都會重建／叫回視窗並跳確認框 |
| 直接結束 | ✅ 設 exit 後行程 0、閘道關閉 |
| 還原 | ✅ `close_action` 已回到預設 `tray` |

> 附註：走訪提示詞頁時 App 依設計自動匯入了 Codex 的現有提示（`prompt_presets` 1 列、
> 「現有內容」、1570 字元、啟用中）。**你的 `~/.codex/AGENTS.md` 沒有被改動**，
> 那只是這個 App 第一次打開提示詞頁的正常行為（cc-switch 也一樣）。

### 9.28 前一次建置（2026-10-01 05:38，P4.3 主題 ＋ P4.4 開機自啟，已被 9.29 取代）

CC Switch 對齊計畫 P4.3／P4.4（設計見 §0.9.28）。**沒有動 schema（仍 v13）**。
這輪改了 **66 個前端檔案**（機械式把 777 處硬編白色換成主題 token）。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-10-01 05:38:37 |
| `target\release\token-gateway.exe` | 10,101,760 bytes、sha256 `EF7CDAABE94CAEB794FF586A4DE5F91D2EB7E3CEC6D61CD65604455A8019E50B` |
| NSIS 安裝檔 | 4,208,037 bytes、sha256 `9EACCA7C1A91AF3881D23DCD8E471E9D7518059D69AA0FCA85AA460F18931CE8` |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 10,101,760 bytes、sha256 `4DACAED958B669D99046BB80253C9BE559BB7680AFC80FBA21CCB4868F379156` |

| 閘門 | 結果 |
|---|---|
| `cargo test --offline --lib` | **425 passed / 0 failed / 11 ignored** ✅（+4：theme／autostart） |
| `cargo clippy --offline --all-targets` | 0 warning ✅ |
| `pnpm exec tsc --noEmit` | exit 0 ✅ |
| `pnpm exec tauri build`（含 tsc） | exit 0 ✅ |
| 資料庫 schema | 仍 **13** ✅ |

**改到的檔案**

| 檔案 | 改動 |
|---|---|
| `src/index.css` | 主題 token（`--app-*` ＋ `--color-fg` 的 `@theme`）、`:root[data-theme="light"]` 淺色覆蓋、光暈透明度 |
| `src/lib/appearance.ts`、`src/main.tsx` | **新增**主題解析／套用／監聽系統；首幀先套用（避免閃爍） |
| `src/components/Settings.tsx` | 外觀區新增「主題」三選項與「開機自啟」開關（顯示登錄檔實際命令） |
| `src-tauri/src/theme.rs`、`src-tauri/src/autostart.rs`、`commands/appearance_cmd.rs` | **新增**：主題驗證與語意、Run 機碼存取、外觀狀態命令 |
| 66 個 `src/**/*.tsx` | `{text,bg,border,…}-white[/α]` → `-fg`；7 個對話框標題改用 `var(--app-elevated-solid)` |

#### 實機驗證

| 驗證項 | 結果 |
|---|---|
| 主題切換（computed style） | ✅ 淺色：body 文字 `rgba(28,28,30,0.95)`、底色 `rgb(236,236,240)`、玻璃邊框反轉；深色回到白字／`rgb(11,11,15)`；跟隨系統解析為 dark |
| 淺色下逐頁走訪 | ✅ 九個頁面（用量／上游來源／MCP／提示詞／技能／會話／本地 Key／診斷／試算）都正常渲染、沒有錯誤；標題色 `rgb(28, 28, 30)` |
| **要求二：本機工具選項** | ✅ 淺色模式下仍是 8 個選項（Claude Code／Codex／OpenCode／Hermes Agent／DeepSeek Harness／Cursor／Antigravity） |
| **開機自啟** | ✅ 開啟 → `reg query …\Run /v TokenGateway` 出現 `"…\token-gateway.exe"`；關閉 → 查詢變成「找不到」（**已還原使用者的機器狀態**） |

### 9.27 前一次建置（2026-10-01 05:08，P4.1：Deep Link，已被 9.28 取代）

CC Switch 對齊計畫 P4.1（設計與取捨見 §0.9.27）。**沒有動 schema（仍 v13）**。
新增相依：`base64`（離線快取已有）。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-10-01 05:08:52 |
| `target\release\token-gateway.exe` | 10,086,912 bytes、sha256 `F6808DC3930613638E88E82A0D6545B1FCFD47347346AC731AF91B13271965AC` |
| NSIS 安裝檔 | 4,207,813 bytes、sha256 `0284591D1326D6AFEFA45D6A1D8FEB88685755A94D981B6FDABE3E875B1B54D1` |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 10,086,912 bytes、sha256 `8186F5E15AF9882047AC8FE0D2B230F3A0BE8923475116661959BB4B76DC9FEB` |

| 閘門 | 結果 |
|---|---|
| `cargo test --offline --lib` | **421 passed / 0 failed / 11 ignored** ✅（+19：Deep Link） |
| `cargo clippy --offline --all-targets` | 0 warning ✅ |
| `pnpm exec tsc --noEmit` | exit 0 ✅ |
| `pnpm exec tauri build`（含 tsc） | exit 0 ✅ |
| 資料庫 schema | 仍 **13** ✅ |

**改到的檔案**

| 檔案 | 改動 |
|---|---|
| `src-tauri/src/deeplink/{mod,parse,apply,protocol}.rs`、`parse/tests.rs` | **新增**：協定解析與預覽、寫入、HKCU 註冊與收件匣轉交 |
| `src-tauri/src/commands/deeplink_cmd.rs` | **新增**：preview／apply／take_pending／protocol_state／register／unregister |
| `src-tauri/src/lib.rs` | 啟動處理：已有實例就轉交後結束、註冊協定、啟動 watcher、管理 pending 狀態 |
| `src/components/DeeplinkDialog.tsx`、`src/components/Settings.tsx` | **新增**匯入確認框（全域）；設定頁新增「Deep Link 匯入」區塊（註冊狀態／重新註冊／取消註冊） |
| `src-tauri/src/{providers.rs,mcp/mod.rs}` | `insert_provider` 開放給 crate 內；`McpInput` 加 `PartialEq` |

#### 實機驗證（真的點連結，CDP 讀畫面）

| 驗證項 | 結果 |
|---|---|
| 協定自動註冊 | ✅ 先刪掉 `HKCU\Software\Classes\tokengateway`，啟動後自動補回，指向安裝後的 exe |
| 點 skill 連結 | ✅ 對話框「匯入技能「brand-guidelines」」；確認後「已新增儲存庫 anthropics/skills@main…｜新增：brand-guidelines（儲存庫 #5）」 |
| 點 prompt 連結 | ✅ Base64 內容正確解出並預覽（18 字元、「# 角色 / 你是一個專業的代碼審查專家」）；取消後沒有寫入 |
| 壞連結 | ✅ 「不支援的工具「gemini」（我們只管理 claude／codex／opencode）」 |
| 不會開第二個行程 | ✅ 點連結前後行程數都是 1 |
| 清理 | ✅ 刪掉測試儲存庫，資料庫只剩內建的 `anthropics/skills`；提示詞 0 筆 |

### 9.26 前一次建置（2026-10-01 04:48，P3.4：會話管理，已被 9.27 取代）

CC Switch 對齊計畫 P3.4（設計與取捨見 §0.9.26）。**P3 到此完成。沒有動 schema
（會話是唯讀＋刪檔，不需要新表，仍是 v13）**。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-10-01 04:48:12 |
| `target\release\token-gateway.exe` | 10,002,432 bytes、sha256 `E60E5A52D0FEA33AD18FBAC068F1824EC55C863C3EDCF96852A67C2DA81C6599` |
| NSIS 安裝檔 | 4,180,455 bytes、sha256 `AA6CAC0A6B864CC1666260C07EC865BC12BC4E9B030CFD0AD927774BA0419C16` |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 10,002,432 bytes、sha256 `98C3E4313DAEB9CC418C0648C8938DF96E6A6A294B42EE6E40E2BAC3E0A1565A` |
| 安裝方式 | NSIS `/S`；裝完先砍掉自動啟動的行程再手動啟動 |

| 閘門 | 結果 |
|---|---|
| `cargo test --offline --lib` | **402 passed / 0 failed / 11 ignored** ✅（+18：會話模組） |
| `cargo clippy --offline --all-targets` | 0 warning ✅ |
| `pnpm exec tsc --noEmit` | exit 0 ✅ |
| `pnpm exec tauri build`（含 tsc） | exit 0 ✅ |
| 資料庫 schema | 仍是 **13**（這輪沒有動 schema）✅ |

**改到的檔案**

| 檔案 | 改動 |
|---|---|
| `src-tauri/src/sessions/{mod,scan,read,store}.rs` | **新增**：型別與續聊指令、掃描（三種來源）、對話解析、刪除（含 `SessionEnv` 安全界線） |
| `src-tauri/src/commands/sessions_cmd.rs` | **新增**：`sessions_apps／scan／read／delete`（掃描與讀取在 `spawn_blocking`） |
| `src/components/Sessions.tsx` | **新增**：兩欄式頁面（清單＋搜尋＋工具篩選＋批次刪除；詳情＋續聊指令複製＋角色上色的對話） |
| `src/App.tsx`、`src/lib/{api,apiTypes}.ts` | 導覽新增「會話」與型別／API |

#### 實機驗證（安裝後的真實 App，CDP 讀畫面）

| 驗證項 | 結果 |
|---|---|
| 掃描 | ✅ 三個工具都掃到（清單顯示 Codex／Claude Code／OpenCode 標籤），新到舊排序 |
| 搜尋 | ✅ 輸入關鍵字即時過濾（用臨時會話檔驗證：全部→500+ 筆、搜尋「驗證用」→1 筆） |
| 對話內容 | ✅ 點一則會話後右側載入對話，角色標籤（使用者／AI）與內容都正確 |
| **續聊指令** | ✅ 顯示 `claude --resume zz-tokengateway-verify`；按「複製指令」→「已複製：claude --resume …」 |
| **刪除** | ✅ 確認框 →「已刪除 1 則。**已刪除的對話不會從用量統計中扣除（用量是帳，不隨對話消失）**」 |
| 刪除後零殘骸 | ✅ Claude 專案目錄比對：**「與基線相同（臨時會話檔已清掉、沒有殘留）」**（驗證用檔案是我自己建的，驗完刪掉） |
| OpenCode 只讀 | ✅ 切到 OpenCode 篩選器 → **104 則**，詳情顯示「**不可刪除**」，續聊指令 `opencode -s ses_…` |
| **要求二：本機工具篩選** | ✅ 用量頁下拉仍是 8 個選項（Claude Code／Codex／OpenCode／Hermes Agent／DeepSeek Harness／Cursor／Antigravity） |

> **實機驗證抓到並修掉的一個 bug**：第一版用「全域 500 筆上限」，而你的 Codex 有
> 617 個會話檔、OpenCode 只有 104 個且比較舊 —— 結果**切到 OpenCode 看到 0 筆**。
> 改成每個工具各自上限 300（合併後再排序、不設全域上限），修完 OpenCode 顯示 104 筆 ✓，
> 並補了 `per_app_cap_keeps_every_tool_visible` 把這條規則釘住。

### 9.25 前一次建置（2026-10-01 04:18，P3.3：技能管理，已被 9.26 取代）

CC Switch 對齊計畫 P3.3（設計與取捨見 §0.9.25）。**schema 12 → 13**
（新增 `skill_repos`／`skills`／`skill_bindings`）。新增相依：`flate2`（離線快取已有）。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-10-01 04:18:05 |
| `target\release\token-gateway.exe` | 9,898,496 bytes、sha256 `45FCFCD09E5223940FC31E11DAE38984ECE19841C4360ABC2F9687150B94D24F` |
| NSIS 安裝檔 | 4,148,134 bytes、sha256 `89A65E45A4F6FDB4F17BD0A0EC138F0FC86315B9CE5F671AF40D2BB2B181E026` |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 9,898,496 bytes、sha256 `59DC0394D5EF205772B86C137C7960FE74417023513B33F9339BD37F7D55309E` |
| 安裝方式 | NSIS `/S`；裝完先砍掉自動啟動的行程再手動啟動 |

| 閘門 | 結果 |
|---|---|
| `cargo test --offline --lib` | **384 passed / 0 failed / 11 ignored** ✅（+22；ignored 是網路／實機測試） |
| 其中技能的三個網路測試（`--ignored`） | ✅ 全部通過：真的列出 `anthropics/skills`（19 個技能）、真的下載並解開 tarball |
| `cargo clippy --offline --all-targets` | 0 warning ✅ |
| `pnpm exec tsc --noEmit` | exit 0 ✅ |
| `pnpm exec tauri build`（含 tsc） | exit 0 ✅ |
| 資料庫 schema | **13** ✅ |

**改到的檔案**

| 檔案 | 改動 |
|---|---|
| `src-tauri/src/skills/{mod,tarball,github,store,sync,install}.rs` | **新增**：型別與驗證、手寫 tar 讀取器、GitHub 掃描與 tarball 下載、DB、同步（含 `SkillEnv` 安全界線）、安裝／更新／解除安裝／備份 |
| `src-tauri/src/commands/skills_cmd.rs` | **新增**：repos／discover／install／list／set_binding／update／update_all／uninstall／backups／restore／backup_delete／settings；async 命令**鎖不跨 await** |
| `src/components/Skills.tsx`、`src/components/skills/DiscoverPanel.tsx` | **新增**：技能頁（探索／儲存庫管理／已安裝／逐工具開關／更新／解除安裝／備份／設定） |
| `src/App.tsx`、`src/lib/{api,apiTypes}.ts`、`Cargo.toml`、`db/{schema,mod}.rs`、`db/tests/*` | 導覽、型別、相依、新表與版本 12→13 |

#### 實機驗證（安裝後的真實 App，CDP 讀畫面 ＋ 技能目錄基線比對）

驗證前先對三個工具的 skills 目錄存基線（你原本：Codex 有 `.system`、`hatch-pet`、
`hermes-agent`、`local-file-search`；Claude 與 OpenCode 是空的）。

| 驗證項 | 結果 |
|---|---|
| 技能頁 | ✅ 導覽出現「技能」，首次進入顯示「還沒有安裝任何技能」 |
| **掃描 GitHub** | ✅ 「**掃描完成：找到 19 個技能**」，清單出現 `brand-guidelines`、`claude-api`、`canvas-design`、`pdf`、`pptx`、`xlsx`… |
| 安裝（只同步 Codex） | ✅ 「已安裝「brand-guidelines」（2 個檔案，同步到 1 個工具，**連結失敗改複製**）」；大小 13.3 KB |
| 目錄影響範圍 | ✅ 基線比對：**只有 `brand-guidelines` 被加進 codex 與 claude**，你原本那四個技能一個都沒被動 |
| **symlink → 複製的退回** | ✅ 實測真的發生：`os error 1314　用戶端沒有這項特殊權限` → 自動退回複製（與 cc-switch 的設計一致） |
| 逐工具開關 | ✅ 打開 Claude → 「已同步（連結失敗改複製：…\\.claude\\skills\\brand-guidelines…）」 |
| 解除安裝 | ✅ 「已解除安裝（備份：…\\skill-backups\\brand-guidelines-20261001-042034）」；備份列出現在 UI |
| **目錄回到原狀** | ✅ 基線比對：**「與基線完全相同（沒有任何殘留）」** |
| 刪除備份 | ✅「已刪除備份」，`skill-backups` 空 |
| 資料庫狀態 | ✅ `skills: 0`、`bindings: 0`、`repos: 1`（內建那個）、`schema: 13` |

> **這一輪又踩到一次「測試寫到真實目錄」**（與 P3.2 的提示詞事故同一類，但這次只多了一個
> 測試用目錄，且在使用者既有技能之外）。已用 `SkillEnv` 結構化修掉：真實路徑只在命令層
> 解析，測試建置直接拒絕解析真實主目錄；修正後跑完整套測試，使用者三個 skills 目錄
> 與基線完全相同。

### 9.24 前一次建置（2026-10-01 03:49，P3.2：提示詞預設集，已被 9.25 取代）

CC Switch 對齊計畫 P3.2（設計、取捨與一次事故的修法見 §0.9.24）。**schema 11 → 12**
（新增 `prompt_presets`）。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-10-01 03:49:17 |
| `target\release\token-gateway.exe` | 9,621,504 bytes、sha256 `B887B201653F38400550125089242A40C3D70321EDF2282FFD298852A6BF8698` |
| NSIS 安裝檔 | 4,037,619 bytes、sha256 `55935733472FE73590C6B414127A4DCF42FEE5A5A1096549F93B5F6CEBDE0801` |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 9,621,504 bytes、sha256 `3BB4A99B296DF7EF72B751951F004855A9DD6EF1DE564E2C2D7F35C92BCCBF30` |
| 安裝方式 | NSIS `/S`；裝完先砍掉自動啟動的行程再手動啟動 |

| 閘門 | 結果 |
|---|---|
| `cargo test --offline --lib` | **362 passed / 0 failed / 8 ignored** ✅（+18 淨增：prompt 模組 21 個測試，其中 3 個是改寫既有測試） |
| `cargo clippy --offline --all-targets` | 0 warning ✅ |
| `pnpm exec tsc --noEmit` | exit 0 ✅ |
| `pnpm exec tauri build`（含 tsc） | exit 0 ✅ |
| 資料庫 schema | **12** ✅ |

**改到的檔案**

| 檔案 | 改動 |
|---|---|
| `src-tauri/src/prompt/{mod,store,sync}.rs` | **新增**：型別與驗證、DB 存取（單一啟用）、同步與智慧回填、**測試不得碰真實路徑的安全鎖** |
| `src-tauri/src/commands/prompt_cmd.rs` | **新增**：`prompt_apps／state／list／save／activate／deactivate／delete／sync／live` |
| `src-tauri/src/fsutil.rs` | 新增 `backup_text`（與 MCP 共用的備份＋輪換） |
| `src-tauri/src/mcp/apply.rs` | 改用 `fsutil::backup_text`（移除重複的備份實作） |
| `src-tauri/src/db/{schema,mod}.rs`、`db/tests/{fresh,migrate}.rs` | 新表、版本 11→12、核心表清單 |
| `src/components/Prompts.tsx`、`src/components/prompts/PromptEditor.tsx` | **新增**：提示詞頁（逐工具頁籤／啟用／編輯／刪除／重新同步／看檔案）與編輯器（含簡易預覽） |
| `src/App.tsx`、`src/lib/{api,apiTypes}.ts` | 導覽新增「提示詞」與型別／API |

#### 實機驗證（安裝後的真實 App，CDP 讀畫面 ＋ 檔案雜湊比對）

對象是使用者真實的 `~/.codex/AGENTS.md`（4,139 bytes 的個人化提示），
驗證前先存基線複本與雜湊，驗證後逐項比對（`.workbuddy/tmp/prompt_baseline.py`）。

| 驗證項 | 結果 |
|---|---|
| 提示詞頁與工具頁籤 | ✅ 導覽出現「提示詞」；切到 Codex 顯示 `AGENTS.md（4139 bytes）` |
| **首次啟動匯入** | ✅ 清單出現「**現有內容**（啟用中，1571 字元）」—— 你原本的 AGENTS.md 被收成預設集 |
| 新增預設集 | ✅ 「已儲存「測試預設」（尚未啟用）」；預覽切換正常 |
| **啟用＝寫進檔案** | ✅ 「已啟用；已寫入檔案；已備份 …\\backups\\codex-prompt\\AGENTS.md.bak-20261001-035003」；檔案變成 37 bytes（新預設集的內容），**備份檔正好是 4,139 bytes** |
| **智慧回填（核心）** | ✅ 模擬使用者手改（37 → 68 bytes）→ 按「重新同步」→「**已把檔案內容回填到「測試預設」**」；接著啟用「現有內容」→ 檔案寫回原內容 |
| 回填後檔案逐位元還原 | ✅ 比對結果 **`same: true`（4,139 bytes，與驗證前完全相同）** |
| 停用＋刪除 | ✅ 停用 → 刪掉兩個測試預設集 → 面板回到空狀態、資料庫 0 列 |
| CLI 仍正常 | ✅ `codex exec -m grok-4.7` → `P32-OK`；你的 AGENTS.md 仍是 4,139 bytes（sha `B67E238D…`） |

> **這一輪發生過一次真實事故並已完全修復**：單元測試誤把使用者的
> `~/.codex/AGENTS.md` 覆蓋掉。內容從 Codex 自己的 session log 還原
> （**4,139 bytes，與原始檔完全相同**），並加了結構性防護：
> 所有會解析主目錄的寫入入口在測試建置裡一律拒絕（§0.9.24）。

### 9.23 前一次建置（2026-10-01 03:22，P3.1：MCP 伺服器管理，已被 9.24 取代）

CC Switch 對齊計畫 P3 的第一塊（設計與取捨見 §0.9.23）。**schema 從 10 升到 11**
（新增 `mcp_servers`／`mcp_bindings`，純新增表）。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-10-01 03:22:15 |
| `target\release\token-gateway.exe` | 9,553,408 bytes、sha256 `8A09611DD03D681298332682F2FBA01233BC8C43835705A901C6003949F1112E` |
| NSIS 安裝檔 | 4,016,040 bytes、sha256 `FC4AE621046F23077F3619EEB5300E3AFD42FA83F8218DD77AFAAEE2D2C3A489` |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 9,553,408 bytes、sha256 `26AF4652F340781177A5B4C805CA8D1993385C913DD6A8A6FFB859A46C9174D7` |
| 安裝方式 | NSIS `/S`；裝完先砍掉自動啟動的行程再手動啟動 |

| 閘門 | 結果 |
|---|---|
| `cargo test --offline --lib` | **344 passed / 0 failed / 8 ignored** ✅（+34：MCP 五個模組的測試） |
| `cargo clippy --offline --all-targets` | 0 warning ✅ |
| `pnpm exec tsc --noEmit` | exit 0 ✅ |
| `pnpm exec tauri build`（含 tsc） | exit 0 ✅ |
| 資料庫 schema | **11** ✅ |
| 新增相依特性 | `serde_json` 開 `preserve_order`（鍵序照原樣；`indexmap` 已在離線快取，離線建置照樣過） |

**改到的檔案**

| 檔案 | 改動 |
|---|---|
| `src-tauri/src/mcp/{mod,presets,store,apply,sync}.rs` | **新增**：型別與驗證、5 個樣板、DB 存取、設定檔讀寫（JSON／TOML）、同步與匯入 |
| `src-tauri/src/commands/mcp_cmd.rs` | **新增**：`mcp_list／presets／upsert／delete／set_binding／set_app_all／sync／import` |
| `src-tauri/src/db/{schema,mod}.rs`、`db/tests/{fresh,migrate}.rs` | 新表、版本 10→11、核心表清單 |
| `src/components/Mcp.tsx`、`src/components/mcp/{McpRow,McpForm}.tsx` | **新增**：MCP 頁（列表／整欄切換／匯入／重新同步）與表單 |
| `src/App.tsx`、`src/lib/{api,apiTypes}.ts` | 導覽新增「MCP」頁與型別／API |
| `src-tauri/Cargo.toml` | `serde_json` 加 `preserve_order` |

#### 實機驗證（安裝後的真實 App，CDP 讀畫面 ＋ 檔案雜湊比對）

驗證前先把三個設定檔的**雜湊與複本**留下來（`.workbuddy/tmp/mcp_baseline.py`），
驗證後逐項比對；你機器上原本的 MCP 伺服器是 Codex 的 `node_repl`。

| 驗證項 | 結果 |
|---|---|
| MCP 頁與空狀態 | ✅ 導覽出現「MCP」，首次進入顯示「還沒有任何伺服器」 |
| **匯入既有設定** | ✅ 「匯入完成：新增 1 個、略過 0 個」，列表出現 `node_repl`（stdio） |
| 匯入後的自動啟用 | ✅ 三個開關＝**Claude 關、Codex 開、OpenCode 關** —— 與 cc-switch「匯入的伺服器對來源工具啟用」一致 |
| 開關一個工具 | ✅ 打開 Claude → 「已更新：已寫入 Claude Code 1 項」，開關變 開／開／關 |
| Claude 設定檔真的被寫 | ✅ `~/.claude.json` 的 `mcpServers.node_repl` 帶著 command 與完整 env；**其他鍵的順序照原樣**（`numStartups`、`firstStartTime`… 不是字母序） |
| **沒有連帶改動別的檔案** | ✅ `opencode.json` 雜湊**與驗證前完全相同**；`config.toml` 也完全相同（合併後輸出與原文一致 → 直接不寫） |
| Codex 仍讀得懂自己的設定 | ✅ `codex mcp list` 仍列出 `node_repl`（含 `env_vars`、`startup_timeout_sec` 這些我們沒建模的鍵）；`codex exec -m grok-4.7` → `P3-OK` |
| 關閉＋刪除後回到原狀 | ✅ 開關全關、刪除該列後：`claude_mcp_now = []`、`codex_mcp_now = []`、資料庫 0 列 |
| **驗證收尾** | ✅ 用基線複本還原三個檔案；`codex mcp list` 再次列出 `node_repl`、`codex exec` → `P3-OK`。你的設定回到驗證前的狀態（`opencode.json` 逐位元相同；`claude.json` 只有 Claude Code 自己會動的遙測欄位不同） |

> **這一輪最有價值的收穫是實機驗證抓到的三個問題**（都已修好並補上測試）：
> ① 同步會在沒有內容可寫的檔案裡留下空的 `"mcp": {}`；
> ② 移除最後一個條目會留下 `"mcpServers": {}` 空殼；
> ③ **覆蓋會吃掉我們沒建模的欄位** —— 你的 `node_repl` 有 `env_vars` 與
> `startup_timeout_sec`，直接覆蓋會在「按一下同步」時無聲刪掉它們。
> 另外為了不讓 `~/.claude.json` 被整份重排，`serde_json` 開了 `preserve_order`，
> 並在值沒變時直接回傳原文（「開了又關」是真正的 no-op）。

### 9.22 前一次建置（2026-10-01 02:31，P2：每來源用量查詢 ＋ 卡片顯示，已被 9.23 取代）

CC Switch 對齊計畫 P2（設計與取捨見 §0.9.22）。**schema 從 9 升到 10**（新增
`provider_usage_query`，純新增表，舊庫開啟時自動補上）。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-10-01 02:31:11 |
| `target\release\token-gateway.exe` | 9,388,032 bytes、sha256 `7FFFB6F29EB7696E10072BDAC3762F4DF2EDD53724AD75319EE93B4E6B35F2AF` |
| NSIS 安裝檔 | 3,969,337 bytes、sha256 `DBDECCB17080B7B998C278B99109E6A6214D0B5274878921B547C3AEE8380227` |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 9,388,032 bytes、sha256 `3EAD657A9CBADD48E342186AABCF0563CDA7C887CB068D60898BEBABB05EB4BE` |
| 安裝方式 | NSIS `/S`；裝完先砍掉自動啟動的行程再手動啟動 |

| 閘門 | 結果 |
|---|---|
| `cargo test --offline --lib` | **310 passed / 0 failed / 8 ignored** ✅（+20：4 extract、7 usage_query、3 store、3 templates、3 run） |
| `cargo clippy --offline --all-targets` | 0 warning ✅ |
| `pnpm exec tsc --noEmit` | exit 0 ✅ |
| `pnpm exec tauri build`（含 tsc） | exit 0 ✅（第一次因 `cfg` 可能為 null 的 TS 錯誤失敗，修好後過） |
| 資料庫 schema | **10** ✅（`provider_usage_query` 已建；孤兒清理與來源刪除都會一併處理） |

**改到的檔案**

| 檔案 | 改動 |
|---|---|
| `src-tauri/src/usage_query/{mod,extract,templates,store,run}.rs` | **新增**：設定型別、抽取引擎、6 個樣板、DB 存取、實際查詢 |
| `src-tauri/src/commands/usage_query_cmd.rs` | **新增**：`usage_query_get／set／clear／templates／apply_template／run／run_all` |
| `src-tauri/src/db/{schema,mod,open}.rs` | 新表、`SCHEMA_VERSION` 9→10、孤兒清理納入新表 |
| `src-tauri/src/providers.rs` | 刪來源時一併清用量查詢設定 |
| `src/components/providers/UsageQueryPanel.tsx` | **新增**：設定面板（開關／樣板／憑證／標頭／抽取規格／逾時／間隔／測試） |
| `src/components/providers/QuotaFooter.tsx` | **新增**：卡片底部顯示（百分比徽章＋三段式顏色＋餘額＋多方案展開＋重查） |
| `src/components/providers/SourceCard.tsx`、`src/components/Providers.tsx`、`src/lib/{api,apiTypes}.ts` | 接線、型別與自動查詢（只對選取中的來源） |

#### 實機驗證（安裝後的真實 App，CDP 讀畫面）

驗證對象選 **OpenRouter**（使用者自己的來源 #17，有填 key），因為它的
`/api/v1/credits` 是公開文件寫明的餘額端點 —— 能拿到**真實數字**而不是假資料。

| 驗證項 | 結果 |
|---|---|
| 卡片上有用量查詢入口 | ✅ `or` 卡片底部顯示「用量查詢」（尚未設定任何來源時的樣子） |
| 面板開啟 | ✅ 標題「用量查詢 · or」，欄位齊全（樣板／網址／方法／Base URL／API Key／Access Token／User ID／標頭／抽取規格／逾時／自動間隔） |
| 套用樣板 | ✅ 選「OpenRouter 額度」後，觸發鈕變成該樣板名，並顯示它的說明（「/credits 回 {data:{total_credits,total_usage}}…」） |
| **測試（真的打上游）** | ✅ **成功**；抽取結果「餘額 0.00 USD / 0.00 USD」；展開的回應是 OpenRouter 的真實回覆 `{"data":{"total_credits":0,"total_usage":0}}` |
| 儲存後卡片顯示 | ✅ 卡片底部變成「餘額 0.00 USD / 0.00 USD」，並有重新查詢與設定兩個圖示 |
| 資料庫落地 | ✅ `schema version: 10`、`usage_query` 1 列（provider 17、enabled、template=openrouter-credits、auto=0 分、金鑰長度 73） |
| 不影響既有功能 | ✅ `codex exec -m grok-4.7` → `P2-OK`；網關 15722 LISTEN；工具仍 3/4 接管中 |

> **這一輪留下的狀態（使用者請看這裡）**：我把 **OpenRouter 那個來源的用量查詢設好了並啟用**
> （用內建樣板、自動查詢間隔 0＝不自動查）。所以那張卡片現在會顯示餘額。
> 不想要就在卡片上按齒輪圖示 → 「清除設定」。你的 OpenRouter 餘額目前是 0。

> **百分比與三段式顏色沒能在實機上驗到顏色**：OpenRouter 這個帳號的
> `total_credits` 是 0，`0/0` 算不出百分比（後端刻意回 `null`，不是 0%），
> 所以畫面上看不到彩色徽章。百分比本身的計算是後端測試（50%、75%、20% 三個案例），
> 顏色對應（<70 綠／70–89 橘／≥90 紅）是照 cc-switch 規則寫的純函式，
> 要等到某個來源真的回得出總額才會在畫面上出現。

### 9.21 前一次建置（2026-10-01 01:57，P1.4–P1.6：連線檢查／故障轉移佇列／系統匣切換，已被 9.22 取代）

CC Switch 對齊計畫 P1 的後半（設計與取捨見 §0.9.21）。P1 到此完成。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-10-01 01:57:39 |
| `target\release\token-gateway.exe` | 9,273,856 bytes、sha256 `604FCB61B3194A199D1F320D8792DF6466137BDB671674DB1DAE674ACDFFB1BB` |
| NSIS 安裝檔 | 3,934,267 bytes、sha256 `7DD5E2937BB3BE9D0B99975662F889507E33AF1C37CE828A25579F9D73E79FF1` |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 9,273,856 bytes、sha256 `0F578D82228CBAF04839F9D4FD51E36253B50B9CD1EEC4B6B5D2098BD319E9A2` |
| 安裝方式 | NSIS `/S`；裝完先砍掉自動啟動的行程再手動啟動 |

| 閘門 | 結果 |
|---|---|
| `cargo test --offline --lib` | **290 passed / 0 failed / 8 ignored** ✅（+16：5 個 `provider_check`、6 個 `proxy::queue`、5 個 `tray`） |
| `cargo clippy --offline --all-targets` | 0 warning ✅ |
| `pnpm exec tsc --noEmit` | exit 0 ✅ |
| `pnpm exec tauri build`（含 tsc） | exit 0 ✅ |
| 資料庫 schema | **仍是 9**（沒有動 schema）✅ |

**改到的檔案**

| 檔案 | 改動 |
|---|---|
| `src-tauri/src/provider_check.rs`（＋tests） | **新增**：連線檢查（`provider_check` 命令、`normalize_url`、`classify`） |
| `src-tauri/src/proxy/queue.rs` | **新增**：故障轉移佇列預覽（重用 `resolve_model_providers` ＋ `order_by_health`） |
| `src-tauri/src/commands/diagnostics.rs` | 新增 `failover_models`／`failover_queue` 命令 |
| `src-tauri/src/tray.rs`（＋tests） | **新增**：托盤切換選單（`menu_model`／`item_id`／`parse_item_id`／`rebuild`／`spawn`／`EVENT_SWITCH`） |
| `src-tauri/src/tools/consts.rs`、`tools.rs` | 新增並匯出 `TAKEOVER_APPS`（可接管的三個工具） |
| `src-tauri/src/lib.rs` | 托盤改用 `tray::spawn` ＋ 啟動時 `rebuild` |
| `src/components/providers/ConnectivityCheck.tsx` | **新增**：詳情面板的「檢查連線」 |
| `src/components/providers/FailoverQueue.tsx` | **新增**：來源頁的故障轉移佇列面板 |
| `src/components/providers/switchReq.ts` | **新增**：`buildSelectionReq`（工具卡片與托盤共用的唯一一份請求組裝） |
| `src/components/TraySwitchListener.tsx` | **新增**：全域的托盤切換接收端＋toast |
| `src/components/providers/{useToolSwitch.ts,SourceDetail.tsx}`、`src/App.tsx`、`src/lib/{api.ts,apiTypes.ts}` | 接線與型別 |

#### 實機驗證（安裝後的真實 App，CDP 讀畫面）

| 驗證項 | 結果 |
|---|---|
| **連線檢查** | ✅ 選中 `opencode-go` → 按「檢查連線」→ 回 **可達**、`GET https://opencode.ai/zen/go/v1`、說明文字「不送模型請求、不帶金鑰」都在畫面上 |
| **故障轉移佇列** | ✅ 面板渲染、模型下拉 **25 個選項**（`deepseek-v4.1-flash（3 個來源）`…）；選了之後 **3 列**依序顯示，第 1 列掛「學到 openai-chat」徽章 |
| 佇列順序的正確性 | ✅ 與資料庫**兩邊獨立對照**：UI 第 1／2／3 列的模型數是 9／6／1，而 DB 的 `priority` 順序是 `#22 opencode-go(9)`、`#21 中轉服務 A(6)`、`#18 自訂來源 A(1)` —— 完全一致 |
| **共用的請求組裝（P1.6 的核心）** | ✅ 用真實 UI 把 Codex 關掉再開（先清掉 localStorage 的 lastSwitch，逼它走按來源組裝那條路）→ 畫面回「**已接管：deepseek-v4.1-flash @ opencode-go（直連上游 Key 已寫入配置文件，僅本機可讀）**」 |
| 之後的 CLI 仍正常 | ✅ `codex exec -m grok-4.7` → `CX-OK`；`claude -p` → `CC-OK` |
| 托盤啟動即建立選單 | ✅ 應用程式啟動成功（`setup` 裡的 `tray::rebuild(...)?` 若失敗會直接中斷啟動），代表 `menu_model` 對真實資料庫查詢、`Menu::with_items`、`tray.set_menu` 三件事都成功 |
| 工具狀態回到原狀 | ✅ 驗證過程中一度把兩個工具關掉（腳本誤點到別列的開關），已用**逐列定位**的腳本還原成 **3/4 接管中**（Claude Code／Codex／OpenCode＝接管中、DSH＝關） |
| 最終建置的煙霧測試 | ✅ 重新啟動後 `3/4 接管中`、`codex exec -m grok-4.7` → `FIN-OK`、網關 15722 LISTEN |

> **托盤「用滑鼠點圖示」這一步沒能自動化**（自動隱藏的工作列＋Win11 不曝露通知區域圖示
> 給 UIA＋本機模型不能讀圖），細節與失敗模式評估寫在 §0.9.21，並附 10 秒的人工檢查法。

### 9.20 前一次建置（2026-10-01 00:54，P1：來源預設集／複製／匯入匯出 ＋ 孤兒列清理，已被 9.21 取代）

CC Switch 對齊計畫（`docs/CC-SWITCH-PARITY.md`）的 P1 第一批。設計與取捨見 §0.9.20。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-10-01 00:54:35 |
| `target\release\token-gateway.exe` | 9,217,536 bytes、sha256 `70FA95126FCE948BAF5207B6569B0522FADC473EC36AEF632B3A897E984F8337` |
| NSIS 安裝檔 | 3,912,781 bytes、sha256 `DDDA24D5EB47C776E2FD5B7DFBF99FBF4530D06EC3D7944AC4A5E1A7602A3D6C` |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 9,217,536 bytes、sha256 `4C619943D9CA866DB413D43807FF62D72551F1514E1F85ED4C4B8258F777F01E` |
| 安裝方式 | NSIS `/S`；裝完先砍掉自動啟動的行程再手動啟動 |

| 閘門 | 結果 |
|---|---|
| `cargo test --offline --lib` | **274 passed / 0 failed / 8 ignored** ✅（+14：3 個 `presets`、10 個 `providers_io`、1 個孤兒清理） |
| `cargo clippy --offline --all-targets` | 0 warning ✅ |
| `pnpm exec tsc --noEmit` | exit 0 ✅ |
| `pnpm exec tauri build`（含 tsc） | exit 0 ✅ |
| 資料庫 schema | **仍是 9**（沒有動 schema；孤兒清理是資料維護，不是遷移）✅ |

**這一批改到的檔案**

| 檔案 | 改動 |
|---|---|
| `src-tauri/src/presets.rs` | **新增**：38 個實測過的來源預設＋命令 `presets_list` |
| `src-tauri/src/providers_io.rs`（＋`providers_io/tests.rs`） | **新增**：`provider_duplicate`、`providers_export`／`providers_export_file`、`providers_import`／`providers_import_file` |
| `src-tauri/src/db/open.rs` | **新增** `purge_orphans`（開庫時清掉指向已刪除來源的六張子表孤兒列；`local_keys` 刻意不動） |
| `src-tauri/src/proxy/failover.rs`、`trace/tests.rs` | 兩個測試改成先種真實來源（原本用不存在的 provider_id 當捷徑，加了清理就被正確刪掉） |
| `src/components/providers/PresetPicker.tsx` | **新增**：新增來源時的預設清單（可搜尋、可自訂） |
| `src/components/providers/ProvidersIO.tsx` | **新增**：匯入／匯出面板（含「包含金鑰」開關、路徑顯示、三種匯入方式） |
| `src/components/providers/SourceCard.tsx` | 新增「複製」按鈕 |
| `src/components/providers/providerform/{ProviderSheet,ProviderForm}.tsx` | 新建流程先給預設、選了之後把連線資訊填進表單並顯示「重選」 |
| `src/components/{icons.tsx,Providers.tsx}`、`src/lib/{api.ts,apiTypes.ts}` | 圖示、工具列按鈕、型別與 API 綁定 |

#### 實機驗證（安裝後的真實 App，全程用 CDP 讀畫面）

| 驗證項 | 結果 |
|---|---|
| 新增來源先給預設 | ✅ 清單實際渲染 **38 個**預設，含 `OpenAI 官方 openai-chat https://api.openai.com/v1` 等 |
| 預設搜尋 | ✅ 輸入 `opencode` 後只剩 `OpenCode Go（Zen）` |
| 選預設會填表單 | ✅ 名稱＝`OpenCode Go（Zen）`、上游地址＝`https://opencode.ai/zen/go/v1`，並顯示「預設…已填入連線資訊，可再改」＋「重選」 |
| 取消不留痕 | ✅（驗證過程按取消，未寫入任何來源） |
| **複製來源** | ✅ 按 `opencode-go` 的複製 → 清單變成 8 張卡，新的 `opencode-go 複製` **緊跟在原本那筆後面**；再刪掉它 → 回到 7 張 |
| **匯出（連續兩次）** | ✅ 第一次與第二次都成功、**沒有出現權限對話框**，兩個檔各自寫出：`…-004153.json`、`…-004156.json`（各 8,367 bytes） |
| 匯出檔內容 | ✅ `kind=token-gateway/providers`、`version=1`、`includes_keys=false`、7 個來源、**所有 `api_key` 皆為空**、26 個模型 |
| 含金鑰開關 | ✅ 打開後按鈕變「匯出到檔案（含金鑰）」並出現明碼警告；關掉即復原 |
| **從路徑匯入** | ✅ 貼回剛匯出的路徑 → `新增 0 個來源（模型 0、價格 0、時段 0）、跳過 7 個已存在` |
| **貼上 JSON 匯入** | ✅ 貼一份已存在的來源 → `新增 0、跳過 1` |
| 匯入零殘骸 | ✅ 前後資料庫快照**完全相同**（providers 7／provider_models 38／local_keys 2／request_logs 5505） |
| **孤兒列清理** | ✅ 開庫後 `provider_models` 38 → **26**、孤兒 **12 → 0**（那 12 列指向已刪除的來源 id） |
| 學到的記憶沒有被誤刪 | ✅ 清理後仍有 4 筆（`pid=22`），協議不符的追蹤仍是 4 筆、最大 trace id 仍是 112 |
| `codex exec -m grok-4.7` | ✅ `P1-OK`（11,099 tokens） |
| **要求二：本機工具篩選** | ✅ 8 個選項齊全（Claude Code／Codex／OpenCode／Hermes Agent／DeepSeek Harness／Cursor／Antigravity） |

> **這一批最有價值的收穫是一個實機 bug**：原本匯出走瀏覽器 Blob 下載，**第一次可以、
> 第二次會被 WebView2 的原生「想要下載多個檔案」對話框擋住**，那個對話框不在 DOM 裡
> （CDP 點不到）而且蓋住整個 UI —— 使用者會以為 App 壞了。改成由 Rust 寫檔
> （`providers_export_file` 寫進 Downloads 並回傳路徑）之後，連續匯出沒有問題，
> 而且「從路徑匯入」可以沿用同一條路徑。這是單元測試抓不到、只有真的按下去才會遇到的。

### 9.19 前一次建置（2026-09-30 22:25，來源斷路器 ＋ 診斷頁看得到健康與協議，已被 9.20 取代）

9.18 只讓「學到的協議」在 API 裡看得到；這一版把 cc-switch 的另一半（**circuit
breaker**）補上，並把「網關記住了什麼」真的畫在診斷頁上。設計與取捨見 §0.9.19。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-09-30 22:25:49 |
| `target\release\token-gateway.exe` | 9,104,896 bytes、sha256 `BFDC815182EFEC2690A150420E73F27374FE258DF939F9D207A80C06F0EF5073` |
| NSIS 安裝檔 | 3,875,968 bytes、sha256 `2710D63B3A5AA24D188951774BDFB7A04D0AB2BA78D52A439A2E8F073305EAB7` |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 9,104,896 bytes、sha256 `55988788270528ED72FBD04B34E014E95A0DD9D053CF0998613C616FB90FEF23` |
| 安裝方式 | NSIS `/S`；裝完先砍掉自動啟動的行程再手動啟動 |

| 閘門 | 結果 |
|---|---|
| `cargo test --offline --lib` | **260 passed / 0 failed / 8 ignored** ✅（+8：7 個 `proxy::health` 單元測試、1 個斷路器 e2e） |
| `cargo clippy --offline --all-targets` | 0 warning ✅ |
| `pnpm exec tsc --noEmit` | exit 0 ✅ |
| `pnpm exec tauri build`（含 tsc） | exit 0 ✅ |
| 檔案行數 | 全部 ≤ 400 行 ✅（最大 `proxy.rs` 365、前端最大 `ChannelPricing.tsx` 368） |
| 資料庫 schema | **仍是 9** —— 這一輪沒有動 schema，所以升級不涉及任何遷移 ✅ |

#### 改到的檔案

| 檔案 | 改動 |
|---|---|
| `src-tauri/src/proxy/health.rs` | **新增**：`HealthRegistry`（連續失敗 3 次 → 冷卻 60 秒）、`order_by_health`、`for_db` |
| `src-tauri/src/proxy.rs` | `ProxyCtx.health`；候選先依健康狀態排序；成功／5xx／連線失敗分別記帳 |
| `src-tauri/src/commands/diagnostics.rs` | **新增命令 `source_health`** |
| `src/components/diagnostics/SourceHealthList.tsx`、`ProtocolMemory.tsx` | **新增**：診斷頁兩個區塊 |
| `src/components/diagnostics/DiagnosticsPage.tsx`、`src/lib/apiTypesDiagnostics.ts`、`src/lib/api.ts` | 掛上區塊與型別 |
| `src-tauri/src/proxy/tests/e2e_strict/failover.rs` | 新增 `e2e_circuit_breaker_stops_probing_dead_source` |
| 文件 | 本檔 §0.9.19、§9.19 |

#### 實機驗證（安裝後的真實 App ＋ 真實 CLI）

| 驗證項 | 結果 |
|---|---|
| `codex exec -m grok-4.7 "say only: R3-OK"` | **`R3-OK`** ✅（11,199 tokens） |
| `codex exec -m grok-4.7 "say only: H1"` | **`H1`** ✅（11,358 tokens） |
| `codex exec -m gpt-6-luna "say only: LUNA-OK"` | **`LUNA-OK`** ✅（10,365 tokens） |
| `codex exec -m muse-spark-1.3-contributor "say only: MUSE-OK"` | **`MUSE-OK`** ✅（11,313 tokens） |
| 網關接聽 | `127.0.0.1:15722` LISTEN ✅ |
| 診斷頁「協議記憶」 | ✅ 顯示真實資料：**`opencode-go #22 grok-4.7 openai-chat → openai-responses`** |
| 診斷頁「來源健康狀態」 | ✅ 顯示真實資料：**`opencode-go #22 openai-chat 正常 上次失敗 — 上次成功 8 秒前`**（送一次請求後才出現，符合「沒紀錄就不顯示」的設計） |
| **要求二：本機工具篩選** | ✅ 下拉實際渲染 **8 個選項**：全部本機工具、Claude Code、Codex、OpenCode、Hermes Agent、DeepSeek Harness、Cursor、Antigravity —— 缺 0 個 |
| 協議記憶沒有退化 | ✅ DB `schema version: 9`；**協議不符的追蹤仍是 2 筆、最大 trace id 仍是 110**（那兩次 `grok-4.7` 呼叫完全沒有再探測） |
| 學到的協議（跑完上面四個模型後） | ✅ **3 筆**：`pid=22` 的 `gpt-6-luna`／`grok-4.7`／`muse-spark-1.3-contributor`，全部 `openai-chat → openai-responses` |
| 學習成本（探測次數） | ✅ 協議不符的追蹤 **2 → 4** 筆、最大 trace id **110 → 112**：`gpt-6-luna` 與 `muse-spark` 各**只花一次 400 就學會**，之後不再重試 |

**objective 的目標模型全部實機可用**（同一個工具 Codex，走同一個來源）：

| 模型 | 上游只在哪個端點 | 實機結果 |
|---|---|---|
| `grok-4.7` | responses | ✅ `R3-OK`／`H1` |
| `gpt-6-luna` | responses | ✅ `LUNA-OK` |
| `muse-spark-1.3-contributor` | responses | ✅ `MUSE-OK` |
| `mimo-v2.6-pro` | chat | ✅ `MIMOOK`（§9.17） |
| `deepseek-v4.1-flash` | 兩邊都有 | ✅ 既有日常使用 |

最後一列是這一輪最值得看的一行：在裝了**另一個新版本**、又跑了兩次 `codex exec`、
中間重啟過 App 之後，`grok-4.7` 仍然沒有再產生任何一次「此模型不支援本協議」的探測 ——
代表它一開始就直接走 `/responses`，協議是從資料庫讀回來的（§9.18 的落庫在這一版繼續有效）。

> **「冷卻中」那格是怎麼驗的**：不是靠把使用者正在用的來源弄壞（那會在用量裡留下
> 假的失敗紀錄、也可能讓歷史用量歸屬變差），而是靠端到端測試
> `e2e_circuit_breaker_stops_probing_dead_source`：真實的網關、真實的連不上的來源、
> 真實的備援上游，並以**該測試自己的資料庫裡 `upstream_status=502` 的筆數**
> 當證據（前三次每次 +1，第四次 +0）。畫面上的呈現則由上面那一列的真實資料驗證。

### 9.18 前一次建置（2026-09-30 22:09，學到的協議落庫 schema v9，已被 9.19 取代）

上一版（9.17）的換手邏輯只在**記憶體**裡記住「這個模型的真實協議」：同一個行程內有效，
但**網關一重啟就忘光**，於是每次重開機／重啟 App 後的第一個 requests-only 模型，
都還是得先白花一次 400 才知道要換協議。這一版把它**寫進 `app.db`（schema v9）**。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-09-30 22:09:22 |
| `target\release\token-gateway.exe` | 9,078,784 bytes、sha256 `AFF5E6C6C551FBE4D4C71EE60E4E6682E9D45D86B0AE5A1561B5500FCA4C5A8D` |
| NSIS 安裝檔 | 3,867,973 bytes |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 9,078,784 bytes、sha256 `A4F71DB39553DCF923C5D72407C54F2BA561548B4ABAA684AB73933A76EA0C52` |
| 安裝方式 | NSIS `/S`，installer exit 0；裝完先砍掉自動啟動的行程再手動啟動 |

| 閘門 | 結果 |
|---|---|
| `cargo test --offline --lib` | **252 passed / 0 failed / 8 ignored** ✅（+1：`learned_protocol_survives_reopen`） |
| `cargo clippy --offline --all-targets` | 0 warning ✅ |
| `pnpm exec tauri build`（含 `tsc`） | 0 error ✅ |
| 檔案行數 | 全部 ≤ 400 行 ✅（最大 `proxy.rs` 322） |

#### 這一版新增的東西

| 檔案 | 改動 |
|---|---|
| `src-tauri/src/db/schema.rs` | **新增表** `provider_model_protocol(provider_id, model, declared_format, actual_format, learned_at, PK(provider_id, model, declared_format))` |
| `src-tauri/src/db/mod.rs` | `SCHEMA_VERSION` 8 → **9**（舊庫自動補表，不影響既有資料） |
| `src-tauri/src/trace/protocol.rs` | **新增**：`remember_protocol`／`load_protocol`／`clear_protocol`／`protocol_summary` |
| `src-tauri/src/proxy/failover.rs` | `learn_format`／`learned_format` 由記憶體改走 DB |
| `src-tauri/src/commands/diagnostics.rs` | 診斷多回 `learned_protocols`（供應商名稱＋模型＋宣告／實際協議） |
| `src-tauri/src/providers.rs` | 刪供應商時一併清掉它的協議學習列（不留孤兒） |

> `declared_format` 放進主鍵是**刻意的**：學習結果只在「供應商宣告的協議沒變」時才優先，
> 使用者若手動改了該供應商的 `api_format`，舊的學習不會蓋掉新設定（有回歸測試盯著）。

#### 實機驗證：重啟之後真的不再重複試錯

這一輪用「網關自己留下的 `proxy_trace`」當證據，跑三次、中間重啟一次 App：

| 步驟 | 動作 | 觀測 |
|---|---|---|
| A | 裝完新版後第一次 `codex exec -m grok-4.7` | 回應 `A-OK`；**出現 1 筆 warn**（`trace id=110`，`declared=openai-chat actual=openai-responses`） |
| B | 同一個行程再跑一次（`B-OK`） | 沒有新增任何列 ✅ |
| — | **完整關閉 App 再啟動**（模擬重開機） | — |
| C | 重啟後再跑一次（`C-OK`） | **`max trace id` 仍是 110、協議不符的 warn 仍只有 2 筆** ✅✅ |

C 這一步就是重點：如果是記憶體快取，重啟後必定會再冒出第 111 筆。它沒有 ——
代表 `grok-4.7 → /responses` 這件事已經**從 DB 讀回來**，重啟後第一次呼叫就直達正解，
不再浪費一次必定失敗的 400。DB 內容（`schema version: 9`）：

```
table present: True
learned protocols: 1   pid=22 model=grok-4.7 declared=openai-chat actual=openai-responses
```

### 9.17 前一次建置（2026-09-30 21:57，來源／協議自動換手，已被 9.18 取代）

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-09-30 21:57:06 |
| `target\release\token-gateway.exe` | 9,074,176 bytes、sha256 `584956D41F94F771F414E0E0828162457E626877DC44EEA2877EAF7DCEB10912` |
| NSIS 安裝檔 | 3,865,094 bytes |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 9,074,176 bytes、sha256 `6C91D814DAD43CA3C6414C0DB9CDA768CE4BA3D240571E52C09DFB71A09DBCE8` |
| 安裝方式 | NSIS `/S`，installer exit 0；裝完先砍掉自動啟動的行程再手動啟動 |

| 閘門 | 結果 |
|---|---|
| `cargo test --offline` | **251 passed / 0 failed / 8 ignored** ✅（+8：6 個 `proxy::failover` 單元測試、2 個 e2e） |
| `cargo clippy --offline --all-targets` | 0 warning ✅ |
| `pnpm exec tauri build`（含 `tsc`） | 0 error ✅ |

#### 實機驗證：真的用 Codex 打真實的 opencode-go

```
codex exec --skip-git-repo-check -m grok-4.7 "reply with exactly: GROKOK"   → 有回應（11,682 tokens）
codex exec --skip-git-repo-check -m mimo-v2.6-pro "reply with exactly: MIMOOK" → MIMOOK（11,290 tokens）
```

**`grok-4.7` 是 responses-only 的模型**，在這之前它必定 `400 ModelProtocolUnsupported`；
`mimo-v2.6-pro` 是 chat-only。兩個都能用 = 你要的「grok-4.7／gpt-6-luna／muse-spark／mimo
都沒問題」在 **Codex 這一側**達成了（gpt-6-luna／muse-spark 與 grok-4.7 同一類，
都是 responses-only）。這也是第一次看到 `app=codex` 被正確歸屬。

網關自己的診斷留下決定性的一筆（`proxy_trace` id=109）：

```
lvl=warn app=codex model=grok-4.7 responses->openai-chat kind=responses_to_chat status=400 retry=0
note=上游 400：此模型不支援本協議（換協議／換來源重試中）
```

也就是：**先照宣告協議翻成 chat 打過去 → 被上游拒絕 → 自動改打 responses 直通 → 成功**。
而 `mimo-v2.6-pro` 那次**沒有任何 warn 列** —— 第一個候選就過，沒有多花一次上游呼叫。

> Claude Code 那側沒有一起驗（也驗不出來）：Anthropic 入站只生得出 chat 請求體，
> responses-only 模型仍然無解（見 §0.9.16 的限制說明）。

### 9.16 前一次建置（2026-09-30 21:18，`mixed` 兩邊都通，已被 9.17 取代）

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-09-30 21:18:16 |
| `target\release\token-gateway.exe` | 9,054,208 bytes、sha256 `7756FCB3C866FC0753BCCD6DA65E47B3F905E24C1C582F49A141E506ACC4DEBC` |
| NSIS 安裝檔 | 3,846,873 bytes |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 9,054,208 bytes、sha256 `0FCF2EA58079FC01CD077E94F3AFB87AF9CB5C12F0D02495A682D0C09B6A083C` |
| 安裝方式 | NSIS `/S`，installer exit 0；裝完先砍掉自動啟動的行程再手動啟動 |
| 閘門 | `cargo test --offline` **243 passed / 0 failed / 8 ignored** ✅、`cargo clippy --offline --all-targets` 0 warning ✅、`pnpm exec tauri build`（含 `tsc`）0 error ✅ |

> 這一輪的實機驗證靠**行為契約表**（`proxy/tests/matrix.rs` 的 30 格）與既有 e2e 直通測試；
> 要看到真實效果需要把來源協議改成 `mixed`（上游來源 → opencode-go → 協議 → mixed），
> 那一步會改到你既有來源的語意，所以**留給你決定**。

### 9.15 前一次建置（2026-09-30 20:48，把來源模型視窗告訴 Claude Code，已被 9.16 取代）

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-09-30 20:48:38 |
| `target\release\token-gateway.exe` | 9,054,208 bytes、sha256 `225FB4D65259FB3ECCDD5186F1B82494F70D79800F9FB3EA31FFFDF79F94E70D` |
| NSIS 安裝檔 | 3,846,614 bytes、sha256 `63D0EA9BDB748A76622729E2E4C2111FBEA6D727C7A313907721A925A072A7F2` |
| 安裝方式 | NSIS `/S`，installer exit 0；裝完先砍掉自動啟動的行程再手動啟動 |

| 閘門 | 結果 |
|---|---|
| `cargo test --offline` | **243 passed / 0 failed / 8 ignored** ✅（+2：`claude_writes_context_override_only_when_known`、`claude_native_drops_gateway_context_override`） |
| `cargo clippy --offline --all-targets` | 0 warning ✅ |
| `pnpm exec tauri build`（含 `tsc`） | 0 error ✅ |

實機驗證（安裝後的正式版本）：

| 步驟 | 觀察 |
|---|---|
| 啟動後看「上游來源 → 本機工具」 | `本機工具（3/4 接管中）`：Claude Code／Codex／OpenCode 都是「網關接管中」✅ |
| Claude Code 的接管內容 | 五個檔位仍全是 `deepseek-v4.1-flash` ✅ |
| `env.CLAUDE_CODE_MAX_CONTEXT_TOKENS` | **沒有寫入** ✅ —— 這正是預期行為：`deepseek-v4.1-flash` 的 `context_window` 在 DB 裡是 NULL，就不編數字 |

### 9.14 前一次建置（2026-09-30 17:34，接管時指定模型 ＋ 模型協議真因，已被 9.15 取代）

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-09-30 17:34:50 |
| `target\release\token-gateway.exe` | 9,053,696 bytes、sha256 `0306DF67B34D6FBC6B83242409076E8964C62E0D86DBFCAF0E160823C475EAA2` |
| NSIS 安裝檔 | 3,843,997 bytes、`bundle\nsis\token-gateway_0.1.0_x64-setup.exe` |
| 安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 9,053,696 bytes（同大小）、sha256 `F597EC6AA5A4E45BA3FB52F5FFE12173A2D59010340601AA2FDC69B1A18ADF9E` |
| 安裝方式 | NSIS `/S`，installer exit 0；裝完先砍掉自動啟動的行程再手動啟動 |

閘門（這一輪的程式改動之後）：

| 閘門 | 結果 |
|---|---|
| `cargo test --offline` | **241 passed / 0 failed / 8 ignored** ✅ |
| `cargo clippy --offline --all-targets` | 0 warning ✅ |
| `pnpm build`（含 `tsc`） | 0 error ✅（本次建置已跑） |
| 檔案行數 ≤ 400 | ✅ |

實機驗證（安裝後的正式版本、真實 UI 點擊，不是單元測試）：

| 步驟 | 觀察 |
|---|---|
| Claude Code 開關 → **關閉** | 回到原生基線：`ANTHROPIC_DEFAULT_{HAIKU,SONNET,OPUS}_MODEL` = `claude-haiku-4-5`／`claude-sonnet-5`／`claude-opus-5`，`ANTHROPIC_BASE_URL` 與 `ANTHROPIC_AUTH_TOKEN` 已移除，非 Claude 家族的模型鍵全清掉 ✅（即 §0.9 的「關 = 忠實還原 ＋ 原生來源化」仍然有效） |
| 同一個開關 → **開啟** | `~/.claude/settings.json` 五個檔位全部 `deepseek-v4.1-flash`，列上訊息：`已接管：deepseek-v4.1-flash @ opencode-go（直連上游 Key 已寫入配置文件，僅本機可讀）` ✅ |
| `claude -p "reply with exactly: TGOK"` | 回 **`TGOK`** ✅（走網關、真的用到來源模型）。附帶一句官方警告：`"deepseek-v4.1-flash" isn't described by this version's model catalog; … auto-compact keeps this session within 200k tokens` |
| OpenCode 開關 → **開啟** | `~/.config/opencode/opencode.json` 頂層多出 **`"model": "tokengateway/deepseek-v4.1-flash"`** ✅（`provider.tokengateway` 段與模型清單照舊合併保留） |
| `opencode run "reply with exactly: OCOK"` | 回 **`OCOK`**，它自己印出 **`> build · deepseek-v4.1-flash`** ✅（以前會用自己的預設模型去打網關，看起來像「接管沒生效」） |
| 協定層證據（`proxy_trace`） | 同一個 `muse-spark-1.3-contributor`：`openai`→`openai-chat` 直通 400、`anthropic`→`openai-chat` `anthropic_to_chat` 400、`responses`→`openai-chat` `responses_to_chat` 400，錯誤都是 `ModelProtocolUnsupported` ✅ —— 這正是 §0.9.13 的結論 | 

#### 這一輪沒做、但「照實說」要記下來的兩件事

1. **Claude Code 對未知模型只假設 200k 上下文**（上面那句 `[claude-code:unrecognized_model]`）。
   oc-go 的 `deepseek-v4.1-flash` 是 1M 級，所以它會**提早自動壓縮**。
   兩條路：App 接管時順手寫 `CLAUDE_CODE_MAX_CONTEXT_TOKENS=<該模型的 context_window>`，
   或在模型名後綴 `[1m]`（後者會改到送給網關的模型字串，風險較高）。**尚未實作。**
2. `settings.json` 裡殘留 `ANTHROPIC_DEFAULT_*_MODEL_NAME = claude-opus-5` —— 那**不是本 App 寫的**
   （App 只寫 `ANTHROPIC_*_MODEL`），是別的切換工具留下的，所以「關閉」時會照原樣留著。
   如果之後還遇到 Claude Code 端「以為自己是 Opus」而送出的參數問題，先清這幾個鍵試試。

---

### 9.13 前一次建置（2026-09-30 15:27，DeepSeek Harness 歷史回填，已被 9.14 取代）

使用者選了第二條路：**本機回填導入**（不改 DSH 任何設定），讓 DSH 的用量出現在用量頁。
做法與驗證見 §0.9.12。

| 項目 | 值 |
|---|---|
| 建置時間 | NSIS 產物 15:27:41 |
| 執行檔大小 | 9,052,672 bytes（比 9.12 多 512 bytes） |
| **安裝後執行檔 SHA-256** | **`0D36FB69BA27B5A23C0FEA0DCA30C13C8FBAA3CC6268370289D09D238EFB4940`**（前一版 `8D4742D6…`） |
| 後端測試 | **239 passed / 0 failed / 8 ignored**（+3 條：`dsh_parse_session_totals`、`dsh_parse_skips_empty_and_bad`、`dsh_scan_counts_projections`） |
| clippy | **0 個警告** |
| tsc | **exit 0** |
| 檔案行數 | 全部 ≤ 400 行 |

**這一輪修掉的最後一個 bug**：第一次按「開始回填」回 `回填失敗：未知工具：dsh` ——
`commands/logs_history.rs` 裡的白名單是寫死的三個工具。改成單一來源
`history::IMPORT_TOOLS`（`["claude","codex","opencode","dsh"]`），掃描順序也跟著它，
並加一條測試把兩者綁在一起，避免以後又忘記。

**這一輪改到的檔案**：`history/{mod,parse,scan,import,tests}.rs`、
`commands/logs_history.rs`、`components/usage/usageLogs.tsx`、文件 §0.9.12／§9.13。

---

### 9.12 前一次建置（2026-09-30 14:25，切回官方前先講清楚憑證，已被 9.13 取代）

使用者要求：「確保使用其他工具在官方來源與指定來源間切換，不會出現同樣錯誤」。
實測發現 Claude Code 切回官方會直接回 `Not logged in · Please run /login`
（這台機器沒有 Anthropic 官方憑證），於是加上官方憑證檢查與警告（§0.9.11）。

| 項目 | 值 |
|---|---|
| 建置時間 | 產物 14:25:53 |
| 執行檔大小 | 9,043,968 bytes |
| **安裝後執行檔 SHA-256** | **`8D4742D6B9506BD5CF7AFFD796BFDCC4EC333453E34AA902F3D8813C5FE98828`**（前一版 `20755898…`） |
| 後端測試 | **236 passed / 0 failed / 8 ignored**（+1 條 `official_auth_hint_reports_missing_credentials`） |
| clippy | **0 個警告** |
| 檔案行數 | 全部 ≤ 400 行 |

**實機驗證（關閉 Claude Code，讀卡片訊息）**：

```
C:\Users\<user>\.claude\settings.json｜還原自 settings.json.baseline-20260930-141556
｜endpoint=api.anthropic.com（未覆寫，官方） model=claude-opus-5
｜設定裡沒有可原生化的地方，維持原樣
｜⚠️ 這台機器找不到 Anthropic 官方登入憑證（C:\Users\<user>\.claude\.credentials.json
不存在、也沒有 ANTHROPIC_API_KEY）：切回官方後請先執行 `claude` 完成 /login，
否則它會回「Not logged in」
```

驗證完已把 Claude Code 切回接管中（工具頁顯示 `本機工具（2/4 接管中）`，
與測試前一致）。

**這一輪改到的檔案**：`tools/native.rs`（`official_auth_hint` ＋ 接進還原訊息）、
`tools/tests.rs`（再匯出）、`tools/tests/roundtrip.rs`（1 條測試）、
文件 §0.9.11／§9.12。

---

### 9.11 前一次建置（2026-09-30 13:50，修好「切回指定來源後無法登入」，已被 9.12 取代）

使用者回報「從官方來源切回指定來源後無法登入」。真因是設定檔被寫進 Codex 的
內建 provider 段 `[model_providers.openai]`，Codex 因而拒絕載入整份
`config.toml`（§0.9.10）。

| 項目 | 值 |
|---|---|
| 建置時間 | 產物 13:50:28 |
| 執行檔大小 | 9,040,896 bytes |
| NSIS 大小 | 3,845,618 bytes |
| **安裝後執行檔 SHA-256** | **`207558984CBC070E171A480380D82C987D9953DAA32AA3A3B03A2763C09A3538`**（前一版 `3561A32B…`） |
| 後端測試 | **234 passed / 0 failed / 8 ignored**（+3 條回歸測試） |
| clippy | **0 個警告** |
| 檔案行數 | 全部 ≤ 400 行 |

**實機驗證（修復 → 復發路徑全走一遍）**：

| 步驟 | 結果 |
|---|---|
| 修復前現況 | 設定 10,747 bytes、`[model_providers.openai]` **1 段**、`codex exec` → `Error loading config.toml: reserved built-in provider IDs` ❌ |
| 關閉 Codex | `(On -> Off)`、7,024 bytes、`[openai]` 段 **0**、`model_provider = "openai"`、指向網關 0 次 ✅ |
| 再開啟（官方 → 指定來源，就是出錯的那一步） | `(Off -> On)`、`[openai]` 段 **0**、別名段回到原本的 **8 段**（custom／tokengateway／gw／mock／nim-direct／nvidia-nim／nvidia-proxy／opencode-zen）✅ |
| alias 來源仍有 `openai` | `state_5.sqlite` 的 `threads.model_provider` 仍含 `openai`（1 對話）→ 證明**是過濾生效**，不是來源消失 ✅ |
| `codex exec` | exit 0、9 秒、回覆「切回指定來源正常」✅ |
| `codex doctor` | `✓ config loaded`、`model deepseek-v4.1-flash · custom` ✅ |
| 用量 | 該次對話記為 `#4408 app=codex prov=22 deepseek-v4.1-flash in=11,436 out=64 st=200 2,102ms` ✅ |

**這一輪改到的檔案**：`codex/legacy.rs`（保留清單＋過濾）、`codex/wire.rs`
（接管時清除）、`native.rs`（還原時清除）、`tools/tests.rs`（再匯出）、
測試 3 條、文件 §0.9.10／§9.11。

**順手查到的一件事（未改動）**：`codex doctor` 每次執行都會對網關發出
**2 個 401** 的連線探測（可重現），那些列會出現在用量頁。那是 Codex 自己的
行為（探測端點是否需要認證），網關如實記錄；沒有改它。

---

### 9.10 前一次建置（2026-09-29 03:20，端到端測試輪：修好「原生備份無法還原」，已被 9.11 取代）

使用者要求跑一輪完整端到端測試（開網關 → 對話 → 檢查用量 → 關工具 → 關網關 →
重啟工具 → 檢查來源），過程中抓到並修掉 `backup_has_known_route` 的失效
（§0.9.9），因此重新建置。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-09-29 03:1x 起算 → 產物 03:20:10 |
| 執行檔大小 | 9,038,336 bytes |
| NSIS 大小 | 3,843,545 bytes |
| **安裝後執行檔 SHA-256** | **`3561A32B5CDE3F50C29BC573991A9ED80DE0CB09A727E9C0E4EC1B5392E71E6D`**（前一版 `66339F9E…`） |
| 後端測試 | **231 passed / 0 failed / 8 ignored**（+2 條回歸測試） |
| clippy | **0 個警告** |
| 檔案行數 | 全部 ≤ 400 行 |
| 實機驗證 | §0.9.9 全流程（含「網關關著仍能對話、DB 不新增列」） |

**這一輪改到的檔案**：

| 檔案 | 改動 |
|---|---|
| `src-tauri/src/tools/backup.rs` | **修**：`backup_has_known_route` 認得原生長相（Codex 內建 `openai`、Claude 無 base URL） |
| `src-tauri/src/tools/tests/native.rs` | 新增 2 條回歸測試（codex／claude 原生備份必須可還原） |
| 文件 | 本檔 §0.9.9／§9.10 |

---

### 9.9 前一次建置（2026-09-28 23:42，「關閉網關回到原生來源」輪，已被 9.10 取代）

這一輪做 §0.9：新增 `switch_off`（還原 ＋ 原生來源化），並修掉因此暴露的
還原摘要假警報。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-09-28 23:36 起算 → 產物 23:42:27（release 約 6 分鐘） |
| 執行檔大小 | 9,038,848 bytes |
| NSIS 大小 | 3,844,624 bytes |
| MSI 大小 | 7,356,416 bytes |
| **安裝後執行檔 SHA-256** | **`66339F9E3519947AC2C6D9E228DB1110123BAD4705CB8BD65ACEF4D0346C907F`**（前一版 `E098655E…`） |
| 後端測試 | **229 passed / 0 failed / 8 ignored**（+12） |
| clippy | **0 個警告** |
| 前端 | `tsc --noEmit` exit 0 |
| 檔案行數 | 205 檔全部 ≤ 400 行 |
| 實機驗證 | §0.9.7（Codex 回到 `openai` ＋ `gpt-5.6-luna`，模型清單變回 GPT）、§0.9.8（OpenCode 回到自己的登入，摘要有依工具產生） |

> 這一輪共建置 4 次（`switch_off` → Claude 端點推回官方 → 摘要依工具 → note／warning
> 分離）。上面是那 4 次的最後一次，**已被 9.10 取代**。

**這一輪改到的檔案**：

| 檔案 | 改動 |
|---|---|
| `src-tauri/src/tools/native.rs` | **新增**：`to_native`（codex／claude／opencode 的原生來源化）＋ `restore_native_to_port`／`restore_native_port` |
| `src-tauri/src/tools/summary.rs` | **新增**：`restore_summary` 從 `backup.rs` 搬出（該檔一度 421 行）；新增 `active_provider_is_direct_chat` |
| `src-tauri/src/tools/tests/native.rs` | **新增**：10 條測試（＋1 條 note／warning 分離） |
| `src-tauri/src/tools/tests/restore.rs` | 新增 `summary_is_per_app_not_codex_only` |
| `src-tauri/src/tools/backup.rs` | 摘要邏輯搬走（行為不變） |
| `src-tauri/src/tools.rs`／`tools/tests.rs` | 模組與再匯出清單同步 |
| `src-tauri/src/commands/apps.rs`／`src/lib.rs` | 新增 `switch_off` 命令 |
| `src/lib/api.ts` | 新增 `switchOff`；`switchRestore` 保留為「逐字還原」安全網（UI 不再呼叫） |
| `src/components/providers/useToolSwitch.ts` | 行開關 OFF 改走 `switchOff` |
| `src/components/providers/ToolList.tsx` | 開關 tooltip 更新 |
| `src/components/providers/switchdialog/useSwitchDialog.ts`／`SwitchDialog.tsx` | 詳情開關 OFF 改走 `switchOff`；確認框與說明文案更新 |
| 文件 | 本檔 §0.9／§9.9、`SIMPLIFICATION-PLAN.md` §10.15、`MANUAL-TESTS.md` 第 9 節（新增） |

---

### 9.8 前一次建置（2026-09-28 20:15，未完成清單收乾輪，已被 9.9 取代）

這一輪把 §0.7 的四項全部做完（靜默重接管、`wire_api` 機制移除、
體檢假警報、測試 B 殘留資料），因此重新建置。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-09-28 20:09:41 起算 → 產物 20:15:12 |
| 執行檔大小 | 9,010,176 bytes |
| NSIS 大小 | 3,827,867 bytes |
| MSI 大小 | 7,344,128 bytes |
| **安裝後執行檔 SHA-256** | **`E098655E9488C4F79757B5266A220168DC80DD3D8276BA67658BFD793E1B4498`**（前一版 `9AD41461…`） |
| 後端測試 | **217 passed / 0 failed / 8 ignored** |
| clippy | **0 個警告** |
| 前端 | `tsc --noEmit` exit 0 |
| 啟動後 | 15722 LISTEN；`config.toml` SHA-256 不變 |

**這一輪改到的檔案**：

| 檔案 | 改動 |
|---|---|
| `src/components/providers/useToolSwitch.ts` | **移除**來源切換自動重接管（改為只提示）；刪 `applyCurrentSelection` |
| `src-tauri/src/tools/codex/wire.rs` | 加 `CODEX_WIRE_API` 常數；刪 `codex_wire_api`／`codex_wire_api_for`／`codex_wire_api_declared`／`CodexAuth.wire_api` |
| `src-tauri/src/tools/switch.rs` | 預覽改用常數；`CodexAuth` 建構簡化 |
| `src-tauri/src/tools/codex/doctor.rs` | 體檢的 `TOKEN_GATEWAY_KEY` 改為依設定檔實際使用情況判定 |
| `src-tauri/src/tools.rs`／`tests.rs` | 再匯出清單同步 |
| `src-tauri/src/tools/tests/codex_wire_api.rs` | 改寫：3 條舊機制測試刪除，2 條回歸測試保留並強化 |
| `src-tauri/src/tools/tests/codex.rs` | 刪重複的舊測試；新增體檢假警報回歸測試 |
| 文件 | 本檔 §0.5.3／§0.6.7／§0.7／§9.8、`SIMPLIFICATION-PLAN.md` §10.13、`MANUAL-TESTS.md`（新增） |

---

### 9.7 前一次建置（2026-09-28 18:45，`wire_api` 預覽修正輪，已被 9.8 取代）

**為什麼要再建一次**：§0.6.5b 發現 `plan_switch`（畫面預覽）有**第二份**
同樣的運算式，會顯示 `chat` 而實際寫 `responses`。這處修正在 Rust 後端，
所以要重新建置。

| 項目 | 值 |
|---|---|
| 建置時間 | **2026-09-28 18:38:30 起算 → 產物 18:45:28**（release 編譯約 7 分） |
| 執行檔大小 | 9,010,176 bytes |
| MSI 大小 | 7,344,128 bytes |
| NSIS 大小 | 3,829,430 bytes |
| **安裝後執行檔 SHA-256** | **`9AD41461C3293FDBD344B30FCBC9FE23748264729E5F31709F5E8EF2C85BF0EE`**（前一版 `758F03DF…`，已換掉） |
| 後端測試 | **220 passed / 0 failed / 8 ignored**（實跑 `cargo test --offline`，exit 0） |
| clippy | **0 個警告** |
| 新增測試 | `plan_preview_wire_api_is_responses_on_gateway`（＋前一版的 `codex_apply_forces_responses_on_gateway_even_if_declared_chat`） |
| 啟動後 | 15722 LISTEN ✓；`config.toml` **SHA-256 完全沒變**（開頁面不會觸發接管） |

**這一輪改到的檔案**：

| 檔案 | 改動 |
|---|---|
| `src-tauri/src/tools/codex/wire.rs` | 新增 `codex_wire_api_for()`；`gateway_section()` 改用它 |
| `src-tauri/src/tools/switch.rs` | `plan_switch` 預覽改用**同一個函式**（第二份副本） |
| `src-tauri/src/tools.rs` | 再匯出清單：`codex_wire_api` → `codex_wire_api_for`（前者已無模組外使用者） |
| `src-tauri/src/tools/tests.rs` | 測試模組再匯出清單同步 |
| `src-tauri/src/tools/tests/codex_wire_api.rs` | 兩條回歸測試 |
| `docs/TESTING.md`、`docs/SIMPLIFICATION-PLAN.md`、`docs/evidence/…` | §0.6／§9.6／§9.7、§10.12 |

---

### 9.6 前一次建置（2026-09-28 18:14，`wire_api` 修正輪，已被 9.7 取代）

**為什麼要重建**：§0.6 的 `wire_api` 修正動到 Rust 後端。**舊版執行檔仍會把
`wire_api` 寫成 Codex 已不接受的 `chat`** —— 在裝上這一版之前，只要點
「上游來源」頁的來源卡片，Codex 就會再次罷工。

| 項目 | 值 |
|---|---|
| 建置時間 | **2026-09-28 18:05:49 起算 → 產物 18:14:53**（release 編譯 7 分 30 秒） |
| 執行檔大小 | 9,010,176 bytes |
| MSI 大小 | 7,344,128 bytes |
| NSIS 大小 | 3,827,231 bytes |
| **安裝後執行檔 SHA-256 前綴** | **`758F03DFB3E69A73941FA2B0`**（前一次安裝的是 `25CCCF4F0942ACBB17A3B5B8`，**已確實換掉**） |
| 後端測試 | **219 passed / 0 failed / 8 ignored**（實跑 `cargo test --offline`，exit 0） |
| clippy | **0 個警告**（`cargo clippy --offline --all-targets`，exit 0） |
| 新增測試 | `tools::tests::codex_wire_api::codex_apply_forces_responses_on_gateway_even_if_declared_chat` |
| 前端 | **未改動**（`src/` 一行都沒動） |
| 啟動後 | 15722 LISTEN ✓；`config.toml` **mtime（17:59:48）與 SHA-256 都沒變** —— 開頁面不會觸發接管 |

**這次的程式改動共 4 個檔案**：

| 檔案 | 改動 |
|---|---|
| `src-tauri/src/tools/codex/wire.rs` | 新增 `codex_wire_api_for()`；`gateway_section()` 改用它 |
| `src-tauri/src/tools/tests.rs` | 測試模組的再匯出清單加一行 `codex_wire_api_for` |
| `src-tauri/src/tools/tests/codex_wire_api.rs` | 新增回歸測試（生產路徑的參數組合） |
| `docs/TESTING.md`、`docs/evidence/2026-09-28-codex-config-incident.md` | 本節與 §0.6 |

---

### 9.4 前一次建置（2026-09-28 04:34，第一階段 F，已被 9.5 取代）

| 項目 | 值 |
|---|---|
| 建置時間 | **2026-09-28 04:34:43** |
| 執行檔大小 | 9,009,152 bytes（8,798 KB） |
| MSI 大小 | 7,344,128 bytes（7,172 KB） |
| NSIS 大小 | 3,830,837 bytes（3,741 KB） |
| 後端測試 | **216 passed / 0 failed / 8 ignored**（本輪實跑 `cargo test --offline`，exit 0） |
| clippy | **4 個警告**（原為 5；少的那個在被刪的 hermes 測試裡） |
| 前端型別檢查 | `npx tsc --noEmit` exit 0 |
| 前端資源指紋 | `index-B1Ek3VaH.js`（前版 `index-BAemutFc.js`）、`index-DnRx7SDk.css`（未變） |

**已驗證產物比所有原始碼新**（最新原始碼 `Providers.tsx` 為 `04:11:21`，產物 `04:34:43`）。

**二進位內容抽查**：

| 字串 | 結果 | 意義 |
|---|---|---|
| `index-B1Ek3VaH.js` | ✅ 存在 | 內嵌的是本次建置的前端（含工具頁版本面板） |
| `hermes_home` / `parse_hermes_model` / `HERMES_CUSTOM_TOKENGATEWAY_API_KEY` | ❌ **全部不存在** | F 真的編進去了（hermes 支援整塊消失） |
| `usage_by_app` | ✅ 存在 | 用量設施仍在（要求二未被犧牲） |

**安裝驗證**：

| 檢查 | 結果 |
|---|---|
| 安裝前 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 2026-09-28 03:27:52，內含 `index-BAemutFc.js`（前一版） |
| 執行 NSIS 安裝檔（`/S`） | exit code 0 |
| 安裝後同一路徑 | 2026-09-28 04:34:36、9,009,152 bytes，內含 `index-B1Ek3VaH.js`、且無 hermes 字串 ✅ |
| 實機 GUI | 見 §0.3（工具清單 4 個、頁籤讀「2/4 接管中」、全程未寫入任何設定） |

### 9.3 前一次建置（2026-09-28 03:28，第一階段 D-1／A／E，已被 9.4 取代）

| 項目 | 值 |
|---|---|
| 建置時間 | **2026-09-28 03:28**（release 編譯 12 分 14 秒） |
| 執行檔大小 | 9,046,528 bytes（8,835 KB） |
| MSI 大小 | 7,364,608 bytes（7,192 KB） |
| NSIS 大小 | 3,842,393 bytes（3,752 KB） |
| 後端測試 | **223 passed / 0 failed / 9 ignored**（本輪實跑 `cargo test --offline`，exit 0） |
| clippy | 5 個警告，與基線相同（皆在既有程式碼） |
| 前端型別檢查 | `npx tsc --noEmit` exit 0 |
| 前端資源指紋 | `index-BAemutFc.js`（前版 `index-BtK0gyS1.js`）、`index-DnRx7SDk.css`（未變） |

**已驗證三種產物都比所有原始碼新**（最新原始碼 `tools.rs` 為 `03:11:50`，產物 `03:28:11`）。

**二進位內容抽查**：

| 字串 | 結果 | 意義 |
|---|---|---|
| `index-BAemutFc.js` / `index-DnRx7SDk.css` | ✅ 存在 | 內嵌的是本次建置的前端（含 A 的文案改動） |
| `via_gateway` | ❌ **不存在** | D-1 真的編進去了（唯一模式） |
| `usage_by_app` / `tools_detect` | ✅ 存在 | 第二階段與工具偵測命令仍在 |

**安裝驗證**：

| 檢查 | 結果 |
|---|---|
| 安裝前 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 2026-09-28 02:43，內含 `index-BtK0gyS1.js`（前一版） |
| 執行 NSIS 安裝檔（`/S`） | exit code 0 |
| 安裝後同一路徑 | 2026-09-28 03:27:52、9,046,528 bytes，內含 `index-BAemutFc.js`、且沒有 `via_gateway` ✅ |
| 實機 GUI | 見 §0.2（工具清單 5 個、切換對話框方案可生成、全程未寫入任何設定） |
| 你的備份與資料庫 | 驗證前後完全相同（claude 10／codex 11＋baseline 1／hermes 6／opencode 10；`app.db` mtime 未變） |

### 9.1 前一次建置（2026-09-28 02:43，已被 9.3 取代）

| 項目 | 值 |
|---|---|
| 建置時間 | **2026-09-28 02:43** |
| 執行檔大小 | 9,051,648 bytes（8,839 KB） |
| MSI 大小 | 7,364,608 bytes（7,192 KB） |
| NSIS 大小 | 3,840,920 bytes（3,751 KB） |
| 後端測試 | **219 passed / 0 failed / 9 ignored**（本輪實跑 `cargo test --offline`，exit 0） |
| 前端 bundle | 550.79 kB（gzip 151.30 kB） |
| 前端資源指紋 | `index-BtK0gyS1.js`、`index-DnRx7SDk.css` |

**已驗證三種產物都比所有原始碼新**（最新原始碼 `usage.rs` 為 `01:15`，產物為 `02:43`）。

**二進位內容抽查**：

| 字串 | 結果 | 意義 |
|---|---|---|
| `index-BtK0gyS1.js` / `index-DnRx7SDk.css` | ✅ 存在 | 內嵌的就是本次建置的前端（含「工具」鏡頭） |
| `usage_by_app` | ✅ 存在 | 新增的後端命令 |
| `proxy_trace` / `provider_stripped_fields` | ✅ 存在 | Phase 1 觀測設施 |
| `busy_timeout` / `state*.sqlite` | ✅ 存在 | B3 修正 |

**安裝驗證**（不是只有「建置成功」）：

| 檢查 | 結果 |
|---|---|
| 安裝前 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 2026-09-27 23:44，內含舊資源 `index-hbcVOtaz.js`（**沒有** `usage_by_app`） |
| 執行 NSIS 安裝檔（`/S`） | exit code 0 |
| 安裝後同一路徑 | 2026-09-28 02:43，內含 `index-BtK0gyS1.js` 與 `usage_by_app` ✅ |

### 9.2 更早的建置（2026-09-27 05:11，Phase 1～5 第一次建置，已被 9.1／9.3 取代）

| 項目 | 值 |
|---|---|
| 建置時間 | **2026-09-27 05:11** |
| 執行檔大小 | 8,829 KB |
| MSI 大小 | 6,272 KB |
| NSIS 大小 | 3,752 KB |
| 後端測試 | 204 passed / 0 failed / 9 ignored |
| 前端 bundle | 542.38 kB（gzip 149.86 kB） |

**已驗證三種產物都比所有原始碼新**（最新原始碼 `keys.rs` 為 `05:00`，
產物為 `05:11`）。

**二進位內容抽查**（確認新程式碼真的在裡面，不只是建置成功）：

| 字串 | 結果 | 意義 |
|---|---|---|
| `proxy_trace` | ✅ 存在 | Phase 1 觀測設施 |
| `provider_stripped_fields` | ✅ 存在 | 學習型剝離 |
| `busy_timeout` | ✅ 存在 | B3 修正（SQLite 鎖等待） |
| `state*.sqlite` | ✅ 存在 | B3 修正（不再寫死 `state_5`） |
| `conflict` / `internal` | ✅ 存在 | Phase 5 `ErrKind` 代碼 |

> `not_found` 不在二進位中，這是**正確的**：`keys.rs` 只用到
> `auth`（3 處）、`conflict`（3 處）、`internal`（2 處）三種，編譯器把未使用的
> 變體字串省略了。我特地回頭確認過，不是漏掉。
