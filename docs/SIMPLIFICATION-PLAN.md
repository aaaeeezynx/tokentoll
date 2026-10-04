# 工具區精簡 ＋ 用量資訊強化方案

> 狀態（**2026-09-28 更新**）：
> - **第二階段（用量資訊強化）已完成、已重新建置、已實機驗證** —— 程式 commit
>   `d417b02`，安裝版 2026-09-28 02:43，證據見 §10 與 `TESTING.md` §0.1。
> - **第一階段：D-1、A、E、F 四個方向全部完成**（各自一個 commit：`3908fcd`／
>   `71d51a6`／`38c5bcd`／`c0c3f41`，見 §10.4–10.9）；D-1／A／E 已重新建置＋
>   安裝＋實機驗證（§10.7、`TESTING.md` §0.2），F 的產物驗證見 §10.9.5。
> - **hermes 已從網關殘留中外科清理乾淨**（你的 opencode-go 設定未動），詳見 §10.9.2。
>
> 擬定日期：2026-09-27

---

## 1. 你的要求，我理解成兩件事

| # | 要求 | 我的解讀 |
|---|---|---|
| **一** | 工具功能大量精簡（A + E + F + D-1） | 砍掉沒在用、走不到的程式碼，讓「工具」這塊變簡單 |
| **二** | 用量資訊**必須完整**：Token 用量、請求數、快取命中率、**而且要分工具** | 精簡**不可以**犧牲任何現有資訊；而且缺的「分工具」要補上 |

**最高原則：這兩件事不能互相衝突。** 所以本方案把「工具區（接管功能）」與「用量區（資訊）」徹底分開處理 —— 工具區大砍，用量區只加不減。

---

## 2. 現況查核：用量資訊到底有什麼（不得缺少清單）

我把「用量」分頁現在**實際顯示**的東西全部列出來。這份清單就是驗收標準。

### 2.1 概覽鏡頭

| 區塊 | 內容 |
|---|---|
| 7 張統計卡 | 總請求、真實消耗 Tokens、**快取命中率**、請求費用 $、成功率、本月訂閱費 $、合計成本 $ |
| 配額區 | 本地 Key 配額用量 |
| 活動熱區 | 貢獻度方格圖（可切換範圍） |
| Token 趨勢 | 分工具堆疊柱狀圖 ＋ 行內「輸入／輸出／**快取讀**／**快取建**」 |
| 趨勢浮層 | 該時間桶的請求數、費用、各工具 tokens 與佔比 |

### 2.2 其他 6 個鏡頭

| 鏡頭 | 內容 |
|---|---|
| 模型 | 模型排行 |
| 月 | 月度統計 |
| 日 | 每日統計 |
| 小時 | 24 小時分布 |
| 統計 | Top 模型／來源、**快取讀／快取建** |
| 渠道 | 分來源（請求數、tokens、費用、成功率） |

### 2.3 篩選與工具

| 項目 | 內容 |
|---|---|
| 範圍 | 今天／最近 7 天／最近 30 天／自訂區間 |
| 本機工具 | 全部／claude／codex／opencode／hermes／dsh／cursor／antigravity |
| 來源 | 全部／各上游來源 |
| 模型 | 全部／各模型 |
| 其他 | 最近紀錄表、CSV 匯出、歷史匯入對話框 |

### 2.4 資料庫底層

`request_logs` 已儲存：`requests`、`in_tok`、`out_tok`、`cache_read`、`cache_write`、`cost_usd`、`status`、`latency_ms`、`app`、`provider_id`、`model_raw`、`is_stream`、`source`。

**結論：資料層很完整，沒有缺欄位。**

---

## 3. 我發現的缺口

**「分工具」目前只能一次看一個，不能並排比較。**

現況機制：

```
使用者在「本機工具」下拉選一個工具
  → 整個用量頁（含快取命中率）都跟著變成那個工具的數字
  → 所以分工具的快取命中率「看得到」，但要一個一個切
```

**缺的是「一張表同時列出所有工具」的比較視圖。**

技術上的原因（後端 6 個統計結構，只有 1 個有快取欄位）：

| 結構 | 請求數 | tokens | cache_read | cache_write | 快取命中率 |
|---|---|---|---|---|---|
| `UsageSummary`（整體） | ✅ | ✅ | ✅ | ✅ | ✅ |
| `TrendAppRow`（分工具） | ✅ | ✅ | ❌ | ❌ | ❌ |
| `TrendBucket`（整體趨勢） | ✅ | ✅ | ❌ | ❌ | ❌ |
| `HourlyBucket`（小時） | ✅ | ✅ | ❌ | ❌ | ❌ |
| `ProviderStat`（分來源） | ✅ | ✅ | ❌ | ❌ | ❌ |
| `ModelStat`（分模型） | ✅ | ✅ | ❌ | ❌ | ❌ |

**「分工具的統計結構」根本不存在** —— 所以不是「隱藏了」，是**還沒做**。

---

## 4. 第一階段：工具區精簡（A + E + F + D-1）

### 4.1 A —— 移除 cursor / antigravity 的偵測與接管

**依據**：兩者都沒安裝、0 筆請求、0 份備份，而且本來就只能偵測不能寫入。

**改動**：

| 位置 | 動作 |
|---|---|
| `tools/detect.rs:187-222` | 刪 `detect_cursor()` + `detect_antigravity()`（約 34 行） |
| `tools/detect.rs:241-242` | 刪兩個呼叫 |
| `tools/switch.rs:279-292` | 刪兩個「不支援」arm |
| 測試 | 更新 `live.rs:234`、`restore.rs:45` 等 |

**⚠️ 關鍵決定（我建議保留）**：`APPS`、`APP_META`、`APP_COLORS`、`infer_app` 裡的 cursor／antigravity **不刪**。

理由：那四處是**用量歸屬與顯示**用的，不是工具偵測用的。刪掉的話：
- 歷史資料若有 cursor 請求會變成 `unknown`
- 用量篩選下拉會少兩個選項

保留成本是 4 行，換取「資訊不缺少」。**這正是你要求二的具體落實。**

**省：約 50 行**

**執行結果（2026-09-28，見 §10.5）**：實際 −68／+20 行（commit `71d51a6`）。
`APPS`／`APP_META`／`APP_COLORS`／`infer_app`／`APP_ORDER` **全部照建議保留**。
另外發現並順手修掉一個潛在崩潰：兩個 arm 一刪，`plan_switch` 的
`_ => unreachable!()` 就會被 cursor／antigravity 命中而 panic（因為 `APPS`
仍留著這兩個 id），所以改寫成一個誠實的拒絕 arm。

---

### 4.2 E —— 備份只留最新一份

**改動**：

| 位置 | 動作 |
|---|---|
| `tools/consts.rs:23` | `BACKUP_KEEP: 10 → 1` |
| `tools/backup.rs:79-90` | 精簡輪替迴圈 |
| `fsutil.rs:46-49` | 精簡 `rotate_backups` |
| 測試 | 更新 `restore.rs:115`、`restore.rs:176` |

**⚠️ 這個改動有真實邊界風險，我不會草率做**：

現有邏輯（`backup.rs:60-91`）不是單純「砍最舊」，而是：

> **優先刪「已被接管污染」的備份，永遠保留「乾淨（接管前）」的那份** ——
> 因為那是關閉接管的唯一退路。

`keep = 1` 時若連續兩次都是乾淨備份，會刪掉較舊的（可能含你最初的原始設定）。
所以**必須驗證**：`keep=1` 下「接管 → 還原」仍然正確。

**省：約 30–60 行**（老實說不多）。
**E 的價值主要在磁碟與心智簡化**（你現在 claude 10 份、codex 11 份），不是大幅瘦身。

**執行結果（2026-09-28，見 §10.6）—— 與上面的計畫有兩處不同，如實記錄**：

1. **輪換迴圈沒有精簡**：讀完程式後確認那不是可砍的複雜度，而是**必要語意**
   （優先淘汰污染備份、乾淨的留到最後）。`keep = 1` 讓它**更重要**，所以邏輯
   一字不動，只改常數。
2. **多了一個安全網**（原方案沒有）：查核你的備份目錄後發現
   **claude／opencode／hermes 都沒有 baseline 檔**（只有 codex 有），它們的還原
   完全依賴那串 `bak-`。因此在輪換前新增 `ensure_baseline_before_prune()`：
   當前內容乾淨 → 直接寫 baseline；當前是接管態 → 從歷史備份遷移最新的乾淨
   備份成 baseline。失敗不報錯（盡力而為）。

實際 **−10／+187 行**（測試佔多數）：**沒有省到行數，但換到「只留 1 份也不會
失去還原能力」**，這才是這個方向該有的成果。新增 4 條回歸測試釘住它。

---

### 4.3 F —— 移除 hermes 支援

**依據**：只有 1 筆請求（但有 6 份備份，所以你確實試過）。`hermes.rs` 367 行是為它寫的專屬 YAML 邏輯。

**改動**：

| 位置 | 行數 |
|---|---|
| `tools/hermes.rs` | **刪除整檔 367 行** |
| `tools/switch.rs:254-265, 402-405, 441-459` | hermes 分支（約 40 行） |
| `tools/backup.rs:46-50, 118-122, 256, 315` | 污染偵測與 `.env` 還原（約 20 行） |
| `tools/apply.rs:16` | `config.yaml` 路徑分支 |
| `tools.rs:83-88` | 6 個 re-export |
| `tools/consts.rs` | `HERMES_CUSTOM_ENV_KEY` |
| 測試 | 55 處提及（apply 18／live 16／restore 11／tests 9／misc 1） |

**⚠️ 動手前我會先做兩件事**：

1. **檢查你目前的 hermes 設定狀態** —— 若 `~/.hermes/config.yaml` 現在指向網關，移除後 App 就無法幫你還原。我會先查給你確認。
2. **保留 `APPS`／`APP_META`／`infer_app` 的 hermes 項**（理由同 4.1）—— 你現有的 1 筆 hermes 請求才不會變成 `unknown`。

**省：約 490 行** ← 這是最大的一刀

**執行結果（2026-09-28，見 §10.9）—— 實際 −733 行（+50／−783），比估計更多，
而且動手前的查核推翻了原本的假設**：

先查核後發現三件事：

1. **hermes 現在根本沒走網關**：`model.provider` 是 `opencode-go`（base_url 指向
   `https://opencode.ai/zen/go/v1`），App 自己也沒把它標成「網關接管中」。
   資料庫那 1 筆 hermes 請求是 **09-23 的歷史紀錄**。（我先前口頭說的「正被接管」
   是錯的，這裡更正。）
2. 真正殘留的只有兩處：`config.yaml` 裡一段**沒有任何地方引用**的
   `providers.tokengateway`（第 25–30 行）與 `.env` 的
   `HERMES_CUSTOM_TOKENGATEWAY_API_KEY`（第 549 行）。
3. 因此「先還原到 09-23 備份」是**錯的解法** —— 那會把你目前可用的
   opencode-go／gpt-6-luna 設定換回舊的 nvidia／flatkey 設定。

**實際做法（你選的 A）**：以外科方式只切掉那兩處殘留（動手前先把兩個檔備份到
App 的 `backups/hermes/`，並用 `hermes doctor` 驗證改完仍正常），**你的
opencode-go 設定一個字都沒動**。詳見 §10.9.1。

**保留項全部照原計畫**：`APPS`／`APP_META`／`APP_COLORS`／`APP_ORDER`／
`infer_app` 的 hermes 都留著 —— 用量歸屬、篩選選項、歷史資料一個不少
（`scripts/check_app_labels.py` 驗證前後端 7 個顯示名仍一致）。

**額外發現**：`tools/util.rs` 的 `strip_quotes` 只被 hermes 用到，一起刪（−13 行）；
工具版本清單從 5 項變 4 項，`live_tool_versions_shape` 的契約數字同步更新。

---

### 4.4 D-1 —— 刪掉「真·直連上游」這條死路

**依據（我查證過）**：

```
SwitchDialog.tsx:213   base_url: via ? gatewayUrl(...) : provider.base_url
buildReq(via) 呼叫點：230、306、327 行 —— 全部傳 true
Providers.tsx:301、361 —— 也寫死 via_gateway: true
→ 後端 11 處讀 via_gateway 的分支，永遠走 true 那一邊
```

**前端從來沒送過 `false`**，所以「把工具設定檔的網址直接寫成廠商地址」這條路走不到。

**改動**：

| 位置 | 動作 |
|---|---|
| `tools/switch.rs:17` | 刪 `via_gateway` 欄位 |
| `tools/switch.rs` 74/79/113/193/234/299/305/337/426 | 11 處分支簡化 |
| `commands.rs:658-669` | `normalize_switch_request` 簡化 |
| `SwitchDialog.tsx` / `Providers.tsx` | 拿掉 `via` 參數 |
| `lib/api.ts:77` | 刪型別欄位 |

**省：約 60–80 行**

**執行結果（2026-09-28，見 §10.4）**：實際 −107／+67 行（commit `3908fcd`）。
除了上表，還一併刪掉兩個只可能在 `via_gateway = false` 時出現的警告字串，
以及在 `plan_switch` 裡**唯一**的 `detect_tools()` 呼叫。`direct_upstream`
（Key 欄留空＝用上游 Key）刻意保留：那是獨立機制，與「經網關／直連」無關。

**附帶效果**：程式中「經網關／直連」的雙模式概念消失，只剩一條路。**你目前唯一真正直連的是 DSH，而那是你手動改 `settings.yaml` 造成的，不受影響。**

---

### 4.5 第一階段合計

| 方向 | 省下 |
|---|---:|
| A（cursor／antigravity 偵測） | ~50 |
| E（備份留 1 份） | ~30–60 |
| F（移除 hermes） | **~490** |
| D-1（刪死分支） | ~60–80 |
| **合計** | **約 630–680 行** |

---

## 5. 第二階段：用量資訊強化（補上「分工具」）—— **已完成**

> 本節目標已於 2026-09-28 完成並實機驗證。實作與驗證結果見 §10；
> 下表保留作為當初的設計依據與驗收對照。

### 5.1 後端新增

**新增結構 `AppStat`**（放在 `usage.rs`）：

```rust
pub struct AppStat {
    pub app: String,
    pub requests: i64,
    pub ok_requests: i64,
    pub success_rate: f64,
    pub in_tok: i64,
    pub out_tok: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub total_tokens: i64,
    pub cache_hit_rate: f64,   // 與 UsageSummary 同公式：cache_read / (in_tok + cache_read)
    pub cost_usd: f64,
}
```

**新增函式 `by_app()`** ＋ **新增命令 `usage_by_app`**。

沿用現有 `where_sql(f)`，所以**範圍／工具／來源／模型四種篩選全部自動生效**，與現有統計口徑完全一致。

### 5.2 前端新增「本機工具」鏡頭

在「概覽」與「模型」之間新增一個鏡頭：

```
概覽 | 工具 | 模型 | 月 | 日 | 小時 | 統計 | 渠道
```

內容：

**A. 分工具比較表**（可依各欄排序）

| 工具 | 請求數 | 成功率 | 輸入 | 輸出 | 快取讀 | 快取建 | 總計 Tokens | **快取命中率** | 費用 $ |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|

**B. 每個工具一張摘要卡**（三個頭條數字，一眼可見）

```
┌─ DeepSeek Harness ────────────┐
│ Token 用量   591,609,773      │
│ 請求數             2,834      │
│ 快取命中率          87.3%     │
└───────────────────────────────┘
```

**C. 點表格任一列 → 自動套用該工具的篩選**（下鑽到現有單一工具視圖）

### 5.3 順帶強化（可選）

- 趨勢圖浮層加上「該工具的**快取命中率**」（需 `TrendAppRow` 補 `cache_read`）
- 「渠道」鏡頭與「模型」鏡頭補上快取讀／快取建／命中率（同樣口徑）

---

## 6. 不變更承諾（驗收標準）

**工具區精簡後，以下必須一字不少**（2026-09-28 第一階段全部完成後逐項複驗）：

- [x] 用量頁 7 個鏡頭全部保留（工具為新增第 8 個）
      —— `usageTypes.ts::LENSES` 8 項：總覽／工具／模型／月／日／時段／統計／來源
- [x] 概觀 7 張卡全部保留
      —— `usageLenses.tsx:56-63`：總請求／真實消耗 Tokens／快取命中率／請求費用／成功率／本月訂閱費／合計成本
- [x] 配額區、活動熱區、Token 趨勢圖保留（實機畫面皆在）
- [x] 趨勢圖浮層的請求數／費用／分工具 tokens 與佔比保留（`usageActivity.tsx` 的 tooltip）
- [x] 「輸入／輸出／快取讀／快取建」行內文字保留（`usageLenses.tsx:96-97`）
- [x] 篩選四項（範圍／工具／來源／模型）＋自訂區間保留
- [x] 本機工具下拉**選項一個不少（含 cursor／antigravity／hermes）**
      —— **實機驗證**：2026-09-28 打開該下拉，讀到 7 個選項
      （Claude Code／Codex／OpenCode／**Hermes Agent**／DeepSeek Harness／**Cursor**／**Antigravity**）。
      這一項先前因滑鼠事件打不開彈出層而只能靠程式碼推論，**現已補上實機證據**。
- [x] 最近紀錄表、CSV 匯出、歷史匯入保留（`RecentLogs`／`HistoryImportDialog` 均在）
- [x] 其他 5 個分頁（來源／Key／診斷／試算／設定）完全不動（`App.tsx::TABS` 6 項不變）
- [x] `cache_hit_rate` 計算公式不變（本輪未觸及）

> 最後一項原本的驗證方式寫的是「bundle 逐字比對中文文案」，實際上我改用
> **更強的方式**：逐項對照程式碼來源 ＋ 實機 OCR 畫面。實機證據見
> `TESTING.md` §0.1～§0.3。

---

## 7. 順序與回退

**每個方向一個獨立 commit，任何一步都能單獨回退。**

| 步驟 | 內容 | 風險 |
|---|---|---|
| 0 | 備份資料庫 ＋ 建立分支 | — |
| 1 | **第二階段先做**（用量強化）| 低 —— 只加不減 |
| 2 | D-1（刪死分支） | 低 |
| 3 | A（cursor／antigravity） | 極低 |
| 4 | E（備份留 1 份） | 低（需驗還原） |
| 5 | F（移除 hermes） | 中（先查你現況） |

> **執行順序（實際）**：0 → 1 → 2（D-1）→ 3（A）→ 4（E）→ 5（F），全部完成。
> 每一項都是獨立 commit，任何一項都能單獨 `git revert`。

**為什麼用量強化先做？** 因為它是**只加不減**的改動，先做完並讓你確認「資訊齊全」之後，再動精簡 —— 這樣你才能清楚區分「精簡造成的變化」與「新功能的變化」。

---

## 8. 需要你確認的決定點（2026-09-28 現況）

| # | 決定點 | 現況 |
|---|---|---|
| 1 | 「本機工具」做成獨立鏡頭還是塞進概覽？ | **已決定並完成**：做成獨立鏡頭「工具」（`UsageAppsLens.tsx`） |
| 2 | D-1、E 是否確認要做？ | **已完成**：D-1（`3908fcd`）、A（`71d51a6`）、E（`38c5bcd`），見 §10.4–10.7 |
| 3 | F 動手前先查 hermes 現況？ | **已查、已處理、已完成**（`c0c3f41`）。查核推翻了「正被接管」的假設：hermes 走的是 opencode-go，真正殘留只有一段沒被引用的 provider 與一行 `.env` Key，已外科清理，見 §10.3 與 §10.9 |
| 4 | 「渠道」與「模型」鏡頭要不要一併補快取欄位？ | **已完成**（兩者都補上快取讀／快取建／總計／命中率） |
| 5 | §6 不變更承諾是否複驗？ | **已完成**：10 項逐項複驗通過，含實機驗證「本機工具下拉仍有 7 個選項」，見 §6 |

> **四個決定點全部結案，本計畫範圍內已無待你確認的項目。** 結案總表見 §10.10。

---

## 9. 我對這個方案的誠實說明

- **A 與 E 省的行數不多**（合計約 100 行）。真正的大刀是 **F（490 行）** 與 **D-1（70 行）**。
- **E 有真實的邊界風險**，我不會只改常數就宣稱完成，會實測「接管 → 還原」。
- **「分工具快取命中率」目前嚴格說是「看得到但要一個一個切」**，不是完全沒有。我把它寫成「缺口」是因為**並排比較**確實不存在。如果你覺得一次看一個就夠，第二階段可以縮小。
- **本方案不動用量頁任何現有內容**，只新增。若你實測發現任何一項資訊消失了，那是我的錯，請立刻回報。

---

## 10. 執行紀錄

### 10.1 已完成：第二階段（用量資訊強化）

- commit `d417b02`（9 檔、+708/−26）：後端 `AppStat`／`by_app()`／`usage_by_app`／
  全檔唯一 `hit_rate()`；前端新增「工具」鏡頭（摘要卡＋可排序並排比較表＋點列下鑽），
  「渠道」「模型」鏡頭補上快取欄位。
- 另外修掉一個顯示 bug：`request_logs.app` 的 schema 是 `TEXT NOT NULL DEFAULT ''`，
  原本 `COALESCE(app,'unknown')` 永遠不會觸發，空字串會變成空白標籤；
  改用 `COALESCE(NULLIF(app,''),'unknown')`（`by_app` 與 `trend_by_app` 一起改）。
- 建置與安裝：2026-09-28 02:43 的 exe／MSI／NSIS，並實際安裝到
  `%LOCALAPPDATA%\token-gateway`、啟動 GUI 驗證。
- 驗收數字：`cargo test --offline` → **219 passed / 0 failed / 9 ignored**（exit 0）；
  畫面數字與獨立重算的 SQLite 值逐格相符（詳表見 `TESTING.md` §0.1）。

### 10.2 已完成：第一階段全部四個方向（工具區精簡）

D-1／A／E／F 各一個 commit，詳見 §10.4–10.6、§10.9；建置／安裝／實機驗證見
§10.7（D-1／A／E）與 §10.9.5（F）。

### 10.3 F 的前置查核結果（hermes 現況）—— **已更正並結案**

**第一次查核（2026-09-28 初）的結論是錯的**，原文如下（保留以示負責）：

> hermes 現在正被本網關接管。若照原方案移除 hermes 支援（F），App 就再也無法幫
> hermes 還原成接管前的設定。

**錯在哪**：我只看到 `config.yaml` 裡**存在** `tokengateway` 這個 provider 區塊，
就推論「hermes 在走網關」。但 `model.provider` 是 **`opencode-go`** —— 那個區塊
**沒有任何地方引用**，App 自己也從來沒把 hermes 標成「網關接管中」。

**複查（實際讀檔）**：

| 檢查 | 結果 |
|---|---|
| hermes 家目錄 | `%LOCALAPPDATA%\hermes`（不是 `~/.hermes`） |
| `model.provider` / `base_url` | **`opencode-go` / `https://opencode.ai/zen/go/v1`** ← 沒走網關 |
| App 的判定 | 工具頁只有 Claude Code／Codex 顯示「網關接管中」 |
| 真正的殘留 | `config.yaml` 第 25–30 行未被引用的 `providers.tokengateway`；`.env` 第 549 行那一行 Key |
| hermes 用量 | DB：**1 筆 / 24,632 tokens**，時間是 **09-23**（歷史紀錄，不是現在） |
| 備份 | claude 10／codex 11（＋baseline 1）／opencode 10／hermes 6 |

**因此原「三條路」的第 1 條（先還原到 09-23 備份）是錯的解法** —— 那會把你目前
可用的 opencode-go／gpt-6-luna 設定，換成 09-23 的 nvidia／flatkey 舊設定。

**你最終選的做法（A）**：只外科清掉那兩處殘留，**你的 opencode-go 設定一字不動**，
`hermes doctor` 驗證通過後才做 F。實際執行與可回復方式見 §10.9.1–10.9.2。

---

### 10.4 D-1 執行紀錄（commit `3908fcd`，11 檔、+67/−107）

**刪掉的**：`SwitchRequest.via_gateway` 欄位、後端 11 處分支、兩段只可能在
`via_gateway = false` 時出現的警告（「疑似 cc-switch 接管」與「直連第三方…無用量
審計」）、`plan_switch` 裡**唯一**的 `detect_tools()` 呼叫、`normalize_switch_request`
的「直連模式需先選擇來源」錯誤與對 `providers.base_url` 的查詢。

**改成無條件**：`base_url` 一律覆寫為 `gateway_url(port, app)`（`plan_switch`／
`apply_switch`）、接管前基準備份一定寫、格式相容提示一定評估。

**刻意保留**：`direct_upstream`（Key 欄留空＝用上游來源 Key）。它是獨立機制，
與「經網關／直連」無關，計畫裡也沒有要動它。

**驗證**：`cargo test --offline` → 219 passed（當時的數字）／0 failed／9 ignored，
exit 0；`npx tsc --noEmit` exit 0；全域搜尋 `via_gateway` 只剩 4 處說明性註解；
**實機**：新 binary 內已找不到 `via_gateway` 字串。

### 10.5 A 執行紀錄（commit `71d51a6`，3 檔、+20/−68）

**刪掉的**：`detect_cursor()`／`detect_antigravity()` 與 `detect_tools()` 裡的兩個
呼叫（工具偵測清單 7 → 5）、`switch.rs` 兩個「請手工配置」arm。

**順手修的**：那兩個 arm 一刪，`plan_switch` 的 `_ => unreachable!()` 就會被
cursor／antigravity 命中而 **panic**（`APPS` 仍保留這兩個 id）。改成一個誠實的
拒絕 arm（`supported = false` ＋ 說明）。

**保留的**：`APPS`／`APP_META`／`APP_COLORS`／`infer_app`／`APP_ORDER` 裡的
cursor／antigravity —— 用量歸屬、篩選選項、歷史資料一個不少（見 §4.1）。

**驗證**：219 passed／`tsc` exit 0／`scripts/check_app_labels.py` 前後端 7 個顯示名
一致；**實機**：上游來源頁籤讀到「**本機工具（2/5 接管中）**」（原本會是 7），
五張卡片是 Claude Code／Codex／OpenCode／Hermes Agent／DeepSeek Harness，
底部文案讀到「Cursor／Antigravity 不提供接管…」。

### 10.6 E 執行紀錄（commit `38c5bcd`，6 檔、+187/−10）

- `BACKUP_KEEP: 10 → 1`。
- **新增 `ensure_baseline_before_prune()`**：輪換前先確保接管前基準備份存在
  （當前乾淨→用它；當前是接管態→從歷史備份遷移最新的乾淨備份）。這是因為查核
  發現 **claude／opencode／hermes 都沒有 baseline、只有 codex 有**，它們的還原
  完全靠那串 `bak-`。
- **輪換邏輯一字不動**（見 §4.2 的執行結果說明）。
- 新增 4 條測試：`prune_keep_one_keeps_the_clean_backup`、
  `keep_one_still_restores_original_after_repeated_takeover`、
  `keep_one_salvages_baseline_from_history_before_prune`、
  `backup_keep_is_one_by_design`。
- **驗證**：`cargo test --offline` → **223 passed**／0 failed／9 ignored，exit 0；
  `cargo clippy --offline --all-targets` → 5 個警告（與基線相同、皆在既有程式碼）。
- 過程中一條新測試先失敗，暴露一個既有性質：原始設定若太簡略（沒有
  `model_provider`／`base_url`），`migrate_gateway_baseline` 會視為「不是已知路由」
  而不採用。已把這個性質寫進測試註解（**不是本階段改的**）。
- **對你現有備份的影響**：下次對某工具按「接管」時，該工具的 `bak-` 會被輪換到
  1 份（乾淨那份或最新那份），baseline 不受影響。不想被輪換就不要按接管。

### 10.7 建置／安裝／實機驗證（2026-09-28 03:28 產物）

| 項目 | 結果 |
|---|---|
| 原始碼最新時間 | 03:11:50（`tools.rs`） |
| exe | 2026-09-28 03:28:11、9,046,528 bytes（8,835 KB） |
| MSI | 03:27:41、7,364,608 bytes（7,192 KB） |
| NSIS setup | 03:28:11、3,842,393 bytes（3,752 KB） |
| 前端資源 | `index-BAemutFc.js`（前版 `index-BtK0gyS1.js`）、`index-DnRx7SDk.css`（未變） |
| 安裝 | NSIS `/S` exit 0；安裝後 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` 為 03:27:52、同一組資源 |
| 舊字串已消失 | exe 內搜不到 `via_gateway`（D-1 真的進了產物） |
| 工具清單 | 實機讀到「本機工具（2/5 接管中）」＝ **5 個**（A 生效） |
| 切換對話框（方案路徑） | 點 Claude Code 卡片開出對話框，方案區讀到 `C:\Users\<user>\.claude\settings.json · 自動備份`、`8 項寫入`、以及「⚠ ANTHROPIC_AUTH_TOKEN 將明文寫入 settings.json…網關模式下請使用可吊銷的本地 sk Key」→ **新產物的 `switch_plan` 仍能生成方案**（這條警告在 D-1 後改為無條件出現；舊版因前端一律送 `true` 也會出現，所以本項證明的是「方案路徑在新產物上正常」，D-1 本身由上一列的 `via_gateway` 消失來證明） |
| 未寫入任何設定 | 全程沒按「套用／接管」；驗證前後備份目錄完全相同（claude 10／codex 11＋baseline 1／hermes 6／opencode 10），`app.db` mtime 仍是 2026-09-27 17:09:05 |
| **沒能 GUI 驗證的** | 用量頁那個自製「本機工具」下拉，我的滑鼠事件打不開它的彈出層；範圍切換也沒點動。改用**程式碼與 diff 證明**：三次 commit 都沒動 `APP_META`／`APP_ORDER`／用量頁任何一行，`Usage.tsx` 的選項來源就是 `APP_META`（含 cursor／antigravity），所以選項一個不少 |

### 10.10 結案總表（2026-09-28 複驗）

| 項目 | 狀態 | 證據 |
|---|---|---|
| 第二階段：用量「工具」鏡頭 | ✅ 完成 | `d417b02`；`TESTING.md` §0.1 |
| 第一階段 D-1（刪死路） | ✅ 完成 | `3908fcd`；§10.4 |
| 第一階段 A（cursor／antigravity） | ✅ 完成 | `71d51a6`；§10.5 |
| 第一階段 E（備份留 1 份） | ✅ 完成 | `38c5bcd`；§10.6 |
| 第一階段 F（移除 hermes） | ✅ 完成 | `c0c3f41`＋`ba635c0`；§10.9 |
| hermes 設定的網關殘留 | ✅ 已清理（可回復） | §10.9.2；`hermes doctor` 全綠 |
| §6 不變更承諾 10 項 | ✅ **逐項複驗通過** | 見 §6（含「本機工具下拉選項」實機證據） |
| 每個方向一個 commit | ✅ | 6 個 commit，`git status` 乾淨 |
| 最新建置已安裝並實機驗證 | ✅ | `TESTING.md` §9.4 |
| 測試／lint／型別 | ✅ | 218 passed／clippy **0** warnings／tsc 0 errors |

**本計畫範圍內已無未完成項。**

### 10.11 後續清理輪（2026-09-28，commit `b0b5a34` / `99f14b3` / `1091b4e` / `10b3f4c`）

§10.10 原本把 `TESTING.md` §8 那份「誠實清單」列為「不在本計畫授權範圍」。這一輪
取得授權後**一次全部處理完**，除了明確保留的一項：

| §8 項目 | 結果 |
|---|---|
| `resolve_filter` 365/366 口徑不一致 | ✅ 統一為 365（沒有產品決策空間，前端本來就是 365） |
| `db.rs` migration 無自動化測試 | ✅ 新增 v1…v7 逐版本升級測試 |
| `proxy_handler` 86 行未達 80 | ✅ **78 行**（抽出 `ReqCtx`；根因與原判斷不同） |
| §5.2 body 解析 400「未定案」 | ✅ **定案：不是 bug**（證據見 `docs/evidence/`） |
| 前端三個肥檔未拆 | ✅ `Keys.tsx` 876→254、`Calc.tsx` 818→366、`api.ts` 808→318 |
| 「拆檔後仍 >400 行」5 檔 | ✅ 全拆完，且**稽核另外掃出的 15 檔也一併拆完** |
| clippy 4 個既有警告 | ✅ **0 個** |
| Phase 5 錯誤型別只轉 6/108 | ⛔ **刻意維持不動** —— 每次轉換都應有人能實測，屬獨立的漸進工作 |

**拆檔的統一作法**：純搬移，零行為／零文案／零邏輯變更；每個模組的對外路徑
都不變（前端用再匯出 barrel，Rust 用 `pub(crate) use 子模組::*`），所以
**所有既有 import 端一行都不用改**。以 `cargo test` 通過數（218）、
`cargo clippy`（0 警告）、`npx tsc --noEmit`、`pnpm build` 逐步把關。

> **一個值得記下的量測陷阱**：PowerShell 5.1 的 `Get-Content` 在 codepage 936 下
> **會少算含中文的檔案行數**（DBCS 前導位元組會吃掉後面的 `0x0A`）。
> 先前 §8 記的 876／809／803 其實是 884／818／808。之後量行數請用
> `[IO.File]::ReadAllLines($p).Count`。

---

### 10.12 `wire_api` 回歸事故（2026-09-28，commit `b092772` ＋ 後續）

**這一輪不是新功能，是修一個由 §10.9 的 E 階段（`38c5bcd`）引入的迴歸。**

**症狀**：使用者反映「開啟 Codex 有這個錯誤：Unable to log in」。

**真因**：E 階段把 `wire_api` 的判定改成「**provider 宣告的 `api_format`
優先於 URL 推定**」。但宣告描述的是**上游**說哪種協議，而寫進 `config.toml`
的 `base_url` **永遠是本網關**（唯一的模式就是經網關 —— 直連分支已於
§4.4 移除）。使用者 7 個來源的 `api_format` **全部是 `openai-chat`**，
於是接管後**每一個** provider 段都被寫成 `wire_api = "chat"`。

而 **Codex 已不再接受 `chat`**：

```
ERROR codex_app_server: Invalid configuration; using defaults.
config.toml:16:12: `wire_api = "chat"` is no longer supported.
```

Codex 讀到非法設定就整份 config 丟棄、改用預設值 → 回頭找 ChatGPT 登入 →
使用者看到「Unable to log in」，**完全看不出真正起因**。

**為何測試沒攔住**：單元測試只驗「網關 URL ＋ **未宣告**」→ `responses`
（用 `CodexAuth::default()`），而生產路徑（`switch.rs`）**永遠帶著宣告**。
兩個條件從未在同一條測試裡相遇 —— 測了元件，沒測接線。

**修正**：

| 位置 | 改動 |
|---|---|
| `wire.rs` | 新增 `codex_wire_api_for(base_url, declared)`：指向網關／官方 OpenAI 一律 `responses`（宣告不得推翻），真正的第三方才輪到宣告 |
| `wire.rs::gateway_section` | 改用它（寫入端） |
| `switch.rs::plan_switch` | **同一類錯誤的第二處** —— 預覽也各自算一份，會顯示 `chat` 而實際寫 `responses`。已改為呼叫同一個函式 |
| 測試 | 新增 `codex_apply_forces_responses_on_gateway_even_if_declared_chat`（寫入端）與 `plan_preview_wire_api_is_responses_on_gateway`（預覽端） |

**教訓**：同一個決策運算式出現在兩個地方就會漂移。修一處時必須全域搜尋
同型運算式（`codex_wire_api_for` 就是靠搜尋 `codex_wire_api(` 的使用處
才找到第二份副本的）。

**仍待決策**：`chat` 在新版 Codex 已**完全**不能用，代表 E 那套「依上游能力
選 chat/responses」的**前提已經消失**。目前只修了實際會走到的路
（網關 → `responses`）；「真正的第三方 → `chat`」那條分支在生產路徑已
不可達，但留著就是地雷。要否整組拿掉、永遠只寫 `responses`，需產品決策。

**閘門**：`cargo test --offline` → **220 passed / 0 failed / 8 ignored**；
`cargo clippy --offline --all-targets` → **0 警告**。

---

### 10.13 未完成清單收乾輪（2026-09-28，`TESTING.md` §0.7）

使用者指示「先把全部未完成的部分完成」。四項全部做完，並補上先前一直缺的
**端到端實機證據**。

| 項目 | 結果 |
|---|---|
| 點來源卡片會**靜默重寫** `config.toml` | ✅ 自動重接管**整段移除**；改為只提示，寫入一律要明確動作（行開關／詳情「套用」） |
| `wire_api` 的「依上游能力判斷」 | ✅ **整組移除**，改成單一常數 `CODEX_WIRE_API`，寫入端與預覽端共用 |
| 體檢的 `TOKEN_GATEWAY_KEY` 假警報 | ✅ 改為依設定檔是否真的用 `env_key` 判定 |
| 測試 B 留下的 6 筆合成資料 | ✅ 已刪（先備份 DB），證據保留在 §5.3 |

**為什麼 `wire_api` 的判斷該整組拿掉**：那套設計的三個前提**全部失效** ——
①「真·直連第三方」模式已於 §4.4 移除，`plan_switch`／`apply_switch` 都
強制把 base_url 改成網關；②`chat` 這個值 **Codex 已完全不接受**（§0.7.5 用
`codex doctor` 直接重現：`config could not be loaded`）；③`api_format` 描述的是
上游，與「Codex→網關」這一段無關。留著只會是地雷。

**用常數而不是函式**是刻意的：同一天稍早，寫入端與預覽端各算一次同樣的
運算式，就出現過「寫入寫 responses、預覽顯示 chat」。常數讓漂移在型別層面
不可能發生。

**新增的實機驗證**（安裝後的正式版本 + 真實 UI 點擊）：把 8 段手動改回
`chat` → `codex doctor` 顯示設定載入失敗 → 在 App 按「套用」→ 8 段全部變回
`responses` → `codex doctor` 全綠。這條把「App 會修好壞掉的設定」變成
可重現的觀測，而不是推論。

**閘門**：`cargo test --offline` → **217 passed / 0 failed / 8 ignored**；
`cargo clippy --offline --all-targets` → **0 警告**；`tsc --noEmit` → exit 0。

**新增文件**：`docs/MANUAL-TESTS.md` —— 只列「要使用者自己動手」的測試，
含唯一還沒跑過的測試 C 的完整步驟與通過標準。

---

### 10.14 場外：Codex「無法封存對話」的根因（2026-09-28 第三輪）

使用者回報舊對話可以開、可以用，但**不能封存**。追查結論：**與本專案無關**，
是 Codex 自己的殘留鎖檔 `~/.codex/thread-writer-locks/<thread-id>.lock`。
證據鏈與操作步驟見 `TESTING.md` §0.8、`MANUAL-TESTS.md` §8。

本輪唯一新增的程式碼是一支**唯讀優先**的清理工具
`scripts/codex_clear_stale_locks.py`：預設只報告，`--apply` 才刪，刪前備份，
且**有行程開著 handle 的鎖一律不碰**（用 `CreateFileW` 以 share mode 0 探測）。

記在這裡是因為它驗證了一個一直只存在於註解裡的推論：`switch.rs` 當年寫著
「清單只來自 DB 的話，封存之後 provider 段會消失 → 舊會話無法續用／無法封存」。
**那個推論是錯的** —— 真正的封存失敗原因與 provider 段無關（本次失敗的
session 其 provider 段完全正常）。註解已據此修正，免得後人照著錯的線索查。

---

### 10.8 行數對照（誠實版）

| 方向 | 計畫估算 | 實際 | 說明 |
|---|---:|---:|---|
| D-1 | 60–80 | **−40**（+67/−107） | 達成 |
| A | ~50 | **−48**（+20/−68） | 達成，並修掉一個 panic 風險 |
| E | 30–60 | **+177**（+187/−10） | **沒省到**：輪換邏輯必須留，新增的是安全網＋測試＋註解 |
| F | ~490 | **−733**（+50/−783） | 超額達成：連帶刪掉只服務 hermes 的 `strip_quotes` 與測試 |
| **合計** | ~630–680 | **−644** | 第一階段四項全部完成 |

> 唯一「沒省到」的是 E，而那是刻意的：我沒有為了行數去砍它的必要語意。
> F 一項就把整個第一階段的淨行數從 +89 翻成 −644。

---

### 10.10 結案總表（2026-09-28 複驗）

| 項目 | 狀態 | 證據 |
|---|---|---|
| 第二階段：用量「工具」鏡頭 | ✅ 完成 | `d417b02`；`TESTING.md` §0.1 |
| 第一階段 D-1（刪死路） | ✅ 完成 | `3908fcd`；§10.4 |
| 第一階段 A（cursor／antigravity） | ✅ 完成 | `71d51a6`；§10.5 |
| 第一階段 E（備份留 1 份） | ✅ 完成 | `38c5bcd`；§10.6 |
| 第一階段 F（移除 hermes） | ✅ 完成 | `c0c3f41`＋`ba635c0`；§10.9 |
| hermes 設定的網關殘留 | ✅ 已清理（可回復） | §10.9.2；`hermes doctor` 全綠 |
| §6 不變更承諾 10 項 | ✅ **逐項複驗通過** | 見 §6（含先前未能驗證的「本機工具下拉選項」實機證據） |
| 每個方向一個 commit | ✅ | 6 個 commit，`git status` 乾淨 |
| 最新建置已安裝並實機驗證 | ✅ | `TESTING.md` §9.4 |
| 測試／lint／型別 | ✅ | 216 passed／clippy 4 warnings／tsc 0 errors |

**本計畫範圍內已無未完成項。** 剩下的都是 `TESTING.md` §8 那份「誠實清單」裡的
**既有、且不在本計畫授權範圍**的項目（例如 `proxy_handler` 86 行未達 80、
`resolve_filter` 365/366 訊息不一致、Phase 5 錯誤型別只轉 6/108、
前端三個肥檔未拆）—— 那些需要你另行決定，不在這次的兩個要求內。
（→ **已於 §10.11 全部處理完畢**，僅 Phase 5 那一項刻意保留。）

---

### 10.9 F 執行紀錄（commit `c0c3f41`，17 檔、+50/−783）

#### 10.9.1 動手前的查核（推翻了原本的假設）

| 查核項 | 結果 |
|---|---|
| hermes 目前走哪條路 | `model.provider = opencode-go`、base_url = `https://opencode.ai/zen/go/v1` → **根本沒走網關** |
| App 怎麼看它 | 工具頁只有 Claude Code／Codex 是「網關接管中」，hermes 不是 |
| 那 1 筆請求 | DB 查詢：hermes 共 **1 筆／24,632 tokens**，是 09-23 的歷史紀錄 |
| 真正的殘留 | `config.yaml` 第 25–30 行未被引用的 `providers.tokengateway`、`.env` 第 549 行 `HERMES_CUSTOM_TOKENGATEWAY_API_KEY` |
| 6 份備份 | 最新兩份（config 與 .env）**都是乾淨的**，還原來源沒有問題 |

⇒ 「先還原到 09-23 備份」會把你現在可用的設定換成舊設定，**不是你要的**。
你選了 A：只做外科清理。

#### 10.9.2 外科清理（可完全回復）

1. 先把 `config.yaml` 與 `.env` 複製到 App 的 `backups/hermes/`
   （`config.yaml.bak-20260928-035729`、`.env.bak-20260928-035729`）。
   **這一步刻意不經 App 的輪換邏輯**，所以這兩份不會被 `BACKUP_KEEP = 1` 砍掉。
2. `config.yaml`：`providers:` 區塊（6 行）→ `providers: {}`（343 行，原 349）。
3. `.env`：刪掉那一行（549 行，原 550）。
4. 驗證：`yaml.safe_load` 可解析、`model` 段逐字未動、全檔再無
   `tokengateway`／`15722` 痕跡；再跑 **`hermes doctor`** → 配置區全綠
   （「API key or custom endpoint configured ✓」「No deprecated config keys ✓」），
   42 項連線檢查中 **OpenCode Go ✓ (key configured)**，沒有任何一項因這次改動失敗。
5. `hermes --version` 仍正常（v0.21.4）。

> 想回頭：把 `backups/hermes/` 那兩份 `*.bak-20260928-035729` 覆蓋回去即可。

#### 10.9.3 程式改動

刪除（−783 行）：

| 位置 | 內容 |
|---|---|
| `tools/hermes.rs` | **整檔 367 行** |
| `tools/switch.rs` | hermes 接管分支、`hermes_apply` 呼叫、接管後寫 `.env` 的整段（56 行） |
| `tools/versions.rs` | hermes 版本探測／更新命令／`hermes_up_to_date`／`tool_latest` 補位（79 行） |
| `tools/backup.rs` | hermes 污染偵測兩處、可還原清單、還原 `.env` 連帶處理（29 行） |
| `tools/util.rs` | `strip_quotes`（只被 hermes 用，13 行） |
| `tools/detect.rs` | `detect_hermes` 與清單項（44 行） |
| 測試 | apply 5 條、restore 2 條、`live_hermes_roundtrip`；契約 5 → 4 |

保留（要求二：**用量資訊必須完整**）：

- 後端 `APPS`／`APP_META` 的 hermes 項、`proxy/util.rs::infer_app` 的
  `("hermes", "hermes")` 對應
- 前端 `logos.tsx`／`usageTypes.ts` 的 `APP_META`／`APP_COLORS`／`APP_ORDER`
- `switch.rs`／`apply.rs` 各留一條**誠實的拒絕**（hermes 不再能被接管）

#### 10.9.4 驗證

| 項目 | 結果 |
|---|---|
| `cargo test --offline` | **216 passed / 0 failed / 8 ignored**（少掉的 7 條正是 hermes 專屬） |
| `cargo clippy --offline --all-targets` | **4 個警告**（原為 5；少的那個就在被刪的測試裡） |
| `npx tsc --noEmit` | exit 0 |
| `scripts/check_app_labels.py` | 前後端 7 個顯示名一致（含 hermes） |
| `hermes doctor` | 配置全綠、OpenCode Go 連線 ✓（見 §10.9.2） |

#### 10.9.5 建置／安裝／實機驗證（2026-09-28 04:34 產物）

| 項目 | 結果 |
|---|---|
| exe | 2026-09-28 04:34:43、9,009,152 bytes（比 D-1／A／E 版又小 37,376 bytes） |
| MSI | 04:34:29、7,344,128 bytes |
| NSIS setup | 04:34:43、3,830,837 bytes |
| 前端資源 | `index-B1Ek3VaH.js`、`index-DnRx7SDk.css`（未變） |
| 安裝 | NSIS `/S` exit 0；安裝後 04:34:36、同一組資源 |
| **hermes 程式真的消失** | exe 內搜 `hermes_home`／`parse_hermes_model`／`HERMES_CUSTOM_TOKENGATEWAY_API_KEY` → **全部 False** |
| 用量設施仍在 | exe 內搜 `usage_by_app` → True |
| 工具清單縮減 | 實機頁籤讀到「**本機工具（2/4 接管中）**」 |
| 四張工具卡 | Claude Code（網關接管中）、Codex（接管中 ＋ 接管前體檢鈕）、OpenCode、DeepSeek Harness —— **沒有 Cursor／Antigravity／Hermes** |
| 底部文案 | 讀到「Cursor／Antigravity／Hermes 不提供接管…」 |
| 新增版本面板 | 工具區新增「本機工具版本」面板，含「可接管／還原：Claude Code、Codex、OpenCode」與四列工具版本 |
| 未寫入任何設定 | 全程未按套用／接管；備份目錄 claude 10／codex 11＋baseline 1／opencode 10 完全未變，`app.db` mtime 仍是 2026-09-27 17:09:05 |

> hermes 的備份目錄由 6 份變成 8 份 —— 多出來的正是這次外科清理的兩份安全備份
> （`*.bak-20260928-035729`），**刻意不經 App 的輪換邏輯**，所以不會被砍。

### 10.15 「關掉網關卻沒回到原生來源」（2026-09-28 第四輪）

你回報：關閉工具的網關之後，Codex 的來源還是 NIM 的來源／模型。詳細根因與
閘門見 `TESTING.md` §0.9，這裡只記這次的**設計決定**。

#### 10.15.1 兩個獨立的原因

1. **基線只在「進入接管」的那一刻拍一次。** `ensure_baseline_before_prune` 的語義
   是「接管前的乾淨設定」，寫進 `{stem}.baseline-*` 之後就不動。之後在 App 裡換
   來源／模型只改「指向網關」的那份設定，基線永遠停在最後一次離開接管態之前的
   樣子（本例：09-26 的 NIM）。只要不曾真的離開接管態，下一次「關」又會回到同
   一個舊快照，而那個舊快照又被拍成新基線 —— 自我延續。
   （基線在還原後會被清掉，所以修正上線後，下一次接管的基線就會是剛拿回來的
   原生設定。）
2. **「關」的語義太窄。** 「還原接管前配置」把「接管前」當成使用者要的終點，
   但使用者要的是「這個工具回到它自己的來源」。

#### 10.15.2 決定：`switch_off` = 忠實還原 ＋ 原生來源化

- **還原那一步不動**（`restore_backup_to_inner` 逐字寫回備份的性質被測試釘住，
  而那個性質本身是安全網）。原生化是**額外**的一步，由命令層串起來。
- **只改來源相關的鍵**：使用者自己的設定、以及 `.codex` 的別名 provider 段一律
  保留。後者是舊對話能續用的唯一依靠（rollout 記著 provider 名）。
- **Codex 的目標值由使用者選定**：`model_provider = "openai"`、
  `model = "gpt-5.6-luna"`（他自己舊對話用過的模型）、移除
  `model_catalog_json`（不移除的話 Codex 的模型清單仍是 NIM 那幾個）。
- **Claude／OpenCode 只清除網關痕跡**… 但 2026-09-28 追加決定：**Claude Code 連
  端點也推回 Anthropic 官方** —— 使用者原本的 router（`opencode.ai/zen/go/v1`）
  一樣移除。代價是他必須先 `claude` 登入一次（這台機器沒有
  `.claude/.credentials.json`）。OpenCode 則只清網關痕跡：它有自己的登入
  （`~/.local/share/opencode/auth.json`），清掉之後就能用。
- **`switch_restore` 保留**：UI 不再呼叫，但它是「逐字還原、不加工」的安全網，
  需要時可從命令層直接叫。這是刻意留下的第二條路，不是忘記刪的死碼。

#### 10.15.3 場外修復：`restore_summary` 的假警報

「直連第三方 URL 卻走 responses 協議（會 404）」原本掃整份文本找 `https://`
＋ `wire_api = "responses"`。別名段留著之後，即使來源已切回 `openai`，那句
「不可用請改走網關接管」還是會出現 —— 正好在使用者剛回到官方來源時嚇他。
改成只看 `model_provider` 指名的那個段。

#### 10.15.4 行數自律

`backup.rs` 加上新檢查後到 **421 行**（超過 400），所以把還原摘要整組搬到新的
`tools/summary.rs`。搬移是**純搬移**，`restore_summary` 經 `tools.rs` 再匯出，
呼叫端寫法不變。

#### 10.15.5 閘門

| 項目 | 結果 |
|---|---|
| `cargo test --offline` | **226 passed / 0 failed / 8 ignored**（exit 0；+9 條新測試） |
| `cargo clippy --offline --all-targets` | **0 警告** |
| `pnpm exec tsc --noEmit` | exit 0 |
| 檔案行數 | 205 檔全部 ≤ 400 行 |
| 實機驗證 | 安裝後用真實 UI 做了一遍「接管 → 關閉」：Codex 的 `model_provider` 變 `openai`、`model` 變 `gpt-5.6-luna`、`model_catalog_json` 消失、8 個別名段全留、`codex doctor` 全綠、`codex debug models` 只剩 GPT 模型（`gpt-5.6-terra`／`gpt-5.6-luna`／`gpt-5.5`）。逐項證據見 `TESTING.md` §0.9.7 |

#### 10.15.6 「其他應用也能還原預設來源嗎？」—— 查核結果

使用者接著問了這一句，所以逐個查清楚（結論已寫進 `MANUAL-TESTS.md` §9）：

| 應用 | 能力 | 依據 |
|---|---|---|
| Codex | ✅ OpenAI 官方 | 有 `openai` 內建 provider ＋ chatgpt 登入 |
| OpenCode | ✅ 自己的登入 | 有 `~/.local/share/opencode/auth.json` |
| Claude Code | ⚠️ 只有模型，端點原本不會回官方 | 備份裡的是他自己的 `opencode.ai/zen/go/v1`；後續依他的決定改成連端點一起推回官方 |
| DSH | — 不適用 | `supported_switch: false`，App 從未寫入它的來源 |
| Cursor／Antigravity | — 不適用 | 不提供接管，只當用量標籤 |

兩個查核中發現的**事實**（都不是 bug，但會讓人誤判）：

1. **Claude Code 的端點是他自己設的**，不是 App 硬塞的 —— 還原後會回到
   `https://opencode.ai/zen/go/v1` ＋ 他那把 `oc_sk_…`（他來源清單裡的
   opencode-go）。所以他第一次會覺得「怎麼沒回到 Claude」。
2. **OpenCode 沒有任何一份「接管前」的備份**：10 份全都（最早 09-22 02:27）有
   provider 指向網關 —— 那是早期版本的 App 寫進 `provider.anthropic` 的。
   `backup_is_tainted("opencode")` 只看 `provider.tokengateway`（現行接管寫入的
   位置），所以那些備份被判成乾淨。**我試過收緊規則，結果 10 份全變污染、
   還原直接失敗**（已實測列出），所以刻意維持寬鬆 —— 真正把 OpenCode 清乾淨的
   是原生化那一步（它掃所有 provider）。
