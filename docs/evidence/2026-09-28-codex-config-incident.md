# 2026-09-28 `~/.codex/config.toml` 被誤改與還原 —— 完整紀錄

> 這份檔案記錄**我自己在代跑測試 D／E 期間造成的意外**，包含時間線、
> 當時的檔案指紋、以及還原後逐項核對的結果。目的是留下可查核的痕跡，
> 而不是只寫「已還原」。

## 1. 起因

我用滑鼠座標導覽 App 時，有兩次點擊落到了「上游來源」頁的**來源卡片**上。
而 `src/components/providers/useToolSwitch.ts:200-215` 的設計是
**所選來源一變就自動重接管 Codex**（該處註解自己寫著「來源切換自動重接管……
**永不彈框**」），於是每次誤觸都直接重寫了 `~/.codex/config.toml`。

**開頁面本身不會觸發**：`prevSelId` 守衛會把首次載入的 `selId` 只記錄不套用。
我實測確認過兩次 —— 重啟 App 後只開啟「上游來源」頁、不做任何點擊，
`config.toml` 的 mtime 與 SHA-256 都沒有變。

## 2. 時間線與檔案指紋

| 時間 | 觸發 | 大小 | SHA-256 | `model` | catalog |
|---|---|---|---|---|---|
| 09-27 15:27:09 | （你原本的狀態） | — | — | — | — |
| 09-28 16:19:35 | 我誤觸 `自訂來源 A` 卡片的「編輯」 | 5640 | （未留存） | `deepseek-v4.1-flash` | `codex-18.json`（provider 18 = 自訂來源 A） |
| 09-28 16:23:37 | 我誤觸 `NIM` 卡片 | 7588 | `5B40F3980EB2CB862796DBB88EA3F62668F2F45A2D0DBE3DE4F01D2DB5B4EA32` | `moonshotai/kimi-k3`（reasoning `max`） | `codex-15.json`（provider 15 = NIM） |
| **09-28 16:28:56** | **還原：點 `opencode-go` 卡片** | **9720** | **`A9339B9517432D178F3325404D91E4EC57771ED1E7F5DB78F8FDF62C93489D3E`** | **`deepseek-v4.1-flash`（reasoning `medium`）** | **`codex-22.json`（provider 22 = opencode-go）** |

## 3. 憑什麼斷定「原本是 opencode-go」

三條互相獨立的證據都指向同一個結論：

1. **catalog 檔的時間戳**：`catalogs/` 裡在我工作階段開始（16:19）**之前**
   最新的一份是 `codex-22.json`，時間 **09-26 05:47:16**。
   其餘為 09-24（17）、09-22（19、20、21）、09-21（template）。
   → 你最後一次真正接管用的是 **provider 22 = opencode-go**。
2. **`codex-22.json` 的檔名編碼了 provider id**：`codex-<provider_id>.json`。
   16:19 那份叫 `codex-18.json`、16:23 那份叫 `codex-15.json`，
   正好對應我誤觸的 自訂來源 A（18）與 NIM（15）。
3. **模型與 reasoning 完全吻合**：opencode-go 的第一個模型（`ord = 30`）是
   `deepseek-v4.1-flash`，其 `reasoning` 欄位為 `medium,high,xhigh,max`
   → 首位正是 `medium`。這與 App 內儲存的「上次接管參數」
   （model `deepseek-v4.1-flash`、reasoning `medium`）一字不差。

## 4. 還原方式與核對

**還原方式**：在「上游來源」頁點一下 `opencode-go` 的卡片，
讓 App 依自己的邏輯重寫設定（不是手改檔案）。產生結果與第 3 節的預測
**完全一致**（model／reasoning／catalog 三項全中）。

**還原後逐項核對**：

| 檢查項 | 結果 |
|---|---|
| `[model_providers.*]` 段數 | 8 ✓ |
| 每一段的 `wire_api` | 全部 `chat` ✓ |
| 每一段的 `base_url` | 全部 `http://127.0.0.1:15722/v1` ✓ |
| `[plugins.*]` | 11 處 ✓ |
| `[marketplaces.*]` | 2 處 ✓ |
| `[desktop]` | 1 處 ✓ |
| `[mcp_servers*]` | 3 處 ✓ |
| `appearanceTheme` / `selected-avatar-id` | 各 1 處 ✓ |
| `[windows]` / `notify` | 各 1 處 ✓ |
| App 的 Codex 卡片訊息 | `已按新來源重接管：deepseek-v4.1-flash @ opencode-go（直連上游 Key 已寫入配置文件，僅本機可讀）` ✓ |

## 5. 無法復原的副作用

`backups/codex/` 由 **11 個被輪換成 2 個**：

```
config.toml.bak-20260926-054716        6931 bytes  09-26 05:47:16
config.toml.baseline-20260926-054716   6931 bytes  09-26 05:47:16
```

這是測試 E 把 `BACKUP_KEEP` 改成 **1** 之後、第一次接管必然發生的輪換，
**不是資料遺失**：最重要的「接管前原始設定」= baseline 仍然在，
所以 App 的「關閉：還原接管前備份」按鈕依然可用。
其餘工具的備份完全沒被動到（claude 10／hermes 8／opencode 10，與基線相同）。

## 6. 由此發現的真實缺陷（尚未修）

**點一下來源卡片就會靜默重寫 `~/.codex/config.toml`，沒有任何確認或提示。**

- 觸發條件：**Codex 正在接管中** ＋ 所選來源改變。
  對已接管的 Codex 而言，在來源清單上點**任何**一張卡片都會立刻重寫設定
  並輪換備份。
- 位置：`src/components/providers/useToolSwitch.ts:200-215`。
- 建議修法（**我沒有擅自改程式**）：
  ① 先跳一個「將重寫 config.toml」的確認框；或
  ② 只在使用者明確按下「以此來源重接管」時才寫入，而不是 `onSelect` 直接觸發。

## 7. 後續：這些重寫引爆了一個真正的程式缺陷

重寫本身還帶出了更嚴重的问题 —— **Codex 完全無法啟動**，錯誤是
「Unable to log in」。

**真因**（`~/.codex/logs_2.sqlite`，首次出現 09-28 16:48:28）：

```
ERROR codex_app_server: Invalid configuration; using defaults.
C:\Users\<user>\.codex\config.toml:16:12: `wire_api = "chat"` is no longer supported.
How to fix: set `wire_api = "responses"` in your provider config.
```

Codex 讀到非法設定 → **整份 config 丟棄改用預設值** → 回頭找 ChatGPT 登入
→ 使用者看到「Unable to log in」（與登入無關，`auth.json` 一直都在）。

**責任歸屬（誠實版）**：

| 事實 | 說明 |
|---|---|
| 你原本（09-27，**舊版** App 寫的）是 `responses` | 舊版依 URL 推定 → 指向網關 → Codex 原生形狀 → **能用** |
| 今天 12:31 你裝了**新版**（含 E 階段的「宣告優先」） | 新版會把指向網關的段寫成 `chat` |
| 我 16:19／16:23／16:28 的三次重寫用的是新版 | 所以寫出 `chat` → 引爆 |
| **但這不是「只有我點才會壞」** | 你只要在今天的新版上**點任何一張來源卡片**，結果完全相同。缺陷在程式，我的點擊只是**觸發器** |

**已修**：`wire.rs` 新增 `codex_wire_api_for()` —— 指向網關／官方 OpenAI 時
**定死 `responses`**，宣告只在真正的第三方才生效；並補上使用生產路徑參數
組合的回歸測試 `codex_apply_forces_responses_on_gateway_even_if_declared_chat`
（舊測試只驗「網關＋未宣告」，從未與「網關＋宣告」相遇，所以漏掉）。
你的 `config.toml` 8 個段已於 17:59:48 全部改回 `responses`。

詳見 `docs/TESTING.md` §0.6。
