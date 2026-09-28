# 要求二 實機驗證證據：用量頁「本機工具」篩選保留全部選項

- 日期：2026-09-28
- 產物：`%LOCALAPPDATA%\token-gateway\token-gateway.exe`（9,010,176 bytes，12:31:18）
  內嵌前端指紋 `index-Dy7Y8xkU.js` / `index-DnRx7SDk.css`
- 方法：視窗截圖 → Windows OCR；座標為截圖像素（視窗左上為原點）

## 展開下拉後的畫面（OCR 原始輸出）

篩選列本身（下拉關閉時）：

```
811  147  97  15  全部本機工具      <- 本機工具 PopSelect
996  147  64  15  全部來源          <- 來源 PopSelect
378  148  30  14  今日
443  148  65  14  近 7 天
542  148  74  14  近 30 天
```

**展開「本機工具」下拉後**（面板區域 x 800–1010、y 190–540）：

```
817  206  97  15  全部本機工具
844  252  92  12  Claude Code
844  296  46  12  Codex
844  340  77  16  OpenCcxle        <- OCR 誤讀，實為 OpenCode
845  385 102  15  Hermes Agent
845  428 129  16  DeepSeek Hame …  <- OCR 截斷，實為 DeepSeek Harness
844  473  48  11  Cursor
844  516  78  15  AntIgra\'ItY     <- OCR 誤讀，實為 Antigravity
```

⇒ **8 個選項全部在畫面上**，包含 `cursor`／`antigravity`／`hermes`
這三個「不提供接管、只作為用量歸屬標籤」的項目，且 DSH 顯示為
「DeepSeek Harness」（不是舊的「DeepSeek」）。

## 對應的程式來源

`src/components/Usage.tsx`：

```tsx
options={[
  { value: "", label: "全部本機工具" },
  ...APP_META.map((m) => ({ value: m.id, label: m.label, logo: m.id })),
]}
```

`src/components/logos.tsx` 的 `APP_META` 共 7 筆：
`claude` / `codex` / `opencode` / `hermes` / `dsh` / `cursor` / `antigravity`
—— 加上「全部本機工具」正好 8 個，與畫面完全一致。

後端一致性由 `scripts/check_app_labels.py` 把關，本輪實跑 exit 0：
7 個顯示名前後端一致，其中 `dsh` 兩邊都是 `DeepSeek Harness`。

## 非破壞性證明

| 檢查 | 驗證後的值 | 與驗證前 |
|---|---|---|
| `app.db` | 2026-09-27 17:09:05、1,425,408 bytes | 未變 |
| `~/.claude/settings.json` | 2026-09-24 18:37:56 | 未變 |
| `~/.codex/config.toml` | 2026-09-27 15:27:09 | 未變 |
| `~/.config/opencode/opencode.json` | 2026-09-22 14:23:33 | 未變 |
| 備份 claude / codex / hermes / opencode | 10 / 11 / 8 / 10 | 與基線相同 |

下拉全程只展開、未點選任何項目，最後以 ESC 關閉（未觸發 `onChange`）。

## 操作附註（給下次驗證的人）

1. **WebView2 會吃掉前幾次合成滑鼠點擊**（第一次點擊只用來讓 webview 取得焦點）。
   同一點連點 3 次才穩定生效。純 Win32 程式（用 Notepad 做對照實驗）不受影響，
   所以別把「沒反應」誤判成 UI 壞掉。
2. **OCR 座標與點擊座標有固定偏移**，需先用一個已知目標校準（本輪約 y 差 45px）。
3. 量行數不要用 PowerShell 5.1 的 `Get-Content`：codepage 936 下含中文的檔案會
   少算（`usage.rs` 報 2109、實為 2190）。請用
   `[IO.File]::ReadAllLines($p).Count`。
