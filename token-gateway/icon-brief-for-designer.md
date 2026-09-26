# Token Gateway — 主 Icon 設計需求書（給設計師）

> 目的：為 Windows 桌面 App 換掉預設 Tauri icon，設計一個能一眼看懂「Token 計量 × 網關中樞」的 App 主 icon。

## 1. 這個專案在做什麼（一句話）

**Token Gateway = 跑在本機的 AI 用量網關**：讓 Claude Code / Codex / OpenCode / Cursor 等 AI coding 工具的 API 請求都經過它，再轉發到各家上游模型商，順便做計費、限流、統計。

## 2. 誰在用、在哪用

- 使用者：個人開發者、重度 AI coding 工具用戶（很在意 token 花了多少錢）
- 平台：**Windows 桌面 App（Tauri 2 + WebView）**，視窗 1100×720，常駐在工作列 / Dock
- 使用場景：開機自啟、縮小放著跑，只在查帳、加 Key、切上游時打開

## 3. App 有哪 5 個分頁（理解產品語言）

| 分頁 | 在做什麼 |
|---|---|
| 用量 | Token 趨勢堆疊圖、貢獻格熱力圖、7 種透鏡（趨勢/工具/模型/小時…） |
| 上游來源 | 接多家模型商（OpenAI 格式為主）、切換開關、定價管理 |
| 本地 Key | 簽發 `sk-local-…` 給本機工具用，設配額 / 限流 / 模型白名單 |
| 試算 | 按量 vs 訂閱比價、閒聊 / 程式 / 長文場景試算 |
| 設定 | 網關端口（預設 15722）、外觀 Accent、CSV 匯出 |

核心概念關鍵字：**gateway（中樞/分流）、token（計量）、switch（切換）、quota（配額）**

## 4. 現在的視覺風格（icon 必須融入）

主題：**macOS 27 HIG / Liquid Glass / 深色模式 only**

- 背景：石墨黑漸層 `#121216 → #0B0B0F`，5 顆環境光暈（ambient orbs，blur 90）
- 表面：Liquid Glass 卡片 — `blur 20~28 + saturate 1.4~1.6`，`1px 白色 10~14% 描邊`，內側 `1px 高光`，圓角 **12 / 14 / 16px（圓角矩形，不是全圓膠囊）**
- 字：SF Pro / Inter，純白 95% / 72% / 58% 三級透明度
- 強調色（預設藍，用戶可切換 4 色）：
  - 藍 `#0A84FF`（主）/ 深 `#0070E0` / hover `#3395FF`
  - 綠 `#30D158` / 紫 `#A78BFA` / 粉 `#F472B6`
- 圖示語言：SF Symbols 同形線性圖示（2px 線寬、圓角端點），彩色只用在各家模型商 logo（LobeHub Icons）
- 動效：`0.32s cubic-bezier(0.32,0.72,0,1)` 全站同步，hover 上浮 1~2px + 描邊變亮

一句話給設計師：**深色玻璃擬態 + 藍色發光 + 圓角矩形 + 極簡線條**，不要扁平 Material、不要漸層彩虹、不要細字小字。

## 5. 主 Icon 設計方向（建議）

- 隱喻二選一（擇一深化即可，不要全放）：
  1. **閘道 / 分流**：圓角方形節點 → 左右分叉線條（入口 1 → 出口 N）
  2. **Token 計量**：`T` 字母變形 + 計量刻度 / 堆疊柱狀（呼應用量頁的堆疊趨勢圖）
- 構圖：單一實心主形 + 一個負空間切口；16px 小尺寸仍可辨認，**不要文字、不要 sk-、不要電路板碎線**
- 質感：深色底（`#1D1D23` 附近）+ 藍色主體 `#0A84FF → #0070E0` 微漸層 + 頂部 `1px 白色高光` + 柔和外發光（呼應玻璃卡片的 `shadow` token）
- 需同時耐看於：Windows 工作列深色/淺色、安裝程式、標題列 16px、控制台大圖
- 避開：與 OpenAI / Anthropic / Claude 官方 logo 過近、綠色主色（會撞狀態燈）、純白底（深色模式會刺眼）

## 6. 技術規格（交付物）

Tauri 讀的是 `src-tauri/tauri.conf.json > bundle.icon`，目前指向：

```
src-tauri/icons/icon.ico       （Windows 主用）
src-tauri/icons/icon.icns      （mac 打包用，可順手出）
src-tauri/icons/32x32.png
src-tauri/icons/128x128.png
src-tauri/icons/128x128@2x.png
```

請交付（PNG + SVG 原稿）：

1. `icon.svg` — 向量母稿（圓角矩形建議 22~24% 圓角，留 10% 安全邊距）
2. `icon-base-1024.png` — 1024×1024 無圓角底（由工程轉各尺寸；Windows 會自己切圓角/遮罩，不要預切）
3. `preview-dark.png / preview-light.png` — 放在深 `#141417` 與淺 `#F5F5F7` 底上的 64px / 32px / 16px 並排預覽各一張

命名/流程：設計師只需給 **1024 母稿 + 預覽圖**，工程用 `pnpm tauri icon icon-base-1024.png` 自動生成全尺寸覆蓋 `src-tauri/icons/`。

## 7. 驗收 Checklist

- [ ] 16px（工作列小圖）下輪廓清晰、無糊邊
- [ ] 深色 / 淺色工作列都不髒邊、不顯白框
- [ ] 與 App 內藍色 Accent `#0A84FF` 同色系、不跳色
- [ ] 無文字、無過細線條（<2px @1024 等比會消失）
- [ ] 與 Claude / OpenAI / Cursor 等第三方 logo 並排時不混淆

## 8. 工程交接備註（設計師可忽略）

- App 名：`Token Gateway`，identifier `com.tokencounter.gateway`
- 換圖步驟：替換母稿 → `pnpm tauri icon` → `pnpm build` → 裝 `bundle/msi` 看工作列實機效果
- 現有 icon 均為 Tauri 預設佔位圖，可直接覆蓋，無需保留
