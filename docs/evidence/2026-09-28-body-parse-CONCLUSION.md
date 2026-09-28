# §5.2「body 解析失敗 400」定案

日期：2026-09-28（第一階段結案後的稽核）
狀態：**已定案 —— 不是網關的 bug，是那兩個請求送的 body 本來就不是合法 JSON。**

## 為什麼先前無法判斷

原本的疑慮是「三份證據用了兩種不同的 `bytes_len` 算法，無法區分『真的解析失敗』
與『舊的除錯儀器弄壞了 body』」。

## 這次怎麼定案的

把三筆紀錄的 body 與它宣稱的 `bytes_len` 逐一對齊，並實際丟給 JSON 解析器：

| 證據 | 紀錄宣稱 `bytes_len` | 該字串實際 byte 數 | JSON 解析 |
|---|---:|---:|---|
| `2026-09-26-body-parse-FAILED-openai.log` | 69 | **69** ✓ | ❌ 失敗 |
| `2026-09-26-body-parse-FAILED.log` | 107 | **107** ✓ | ❌ 失敗 |
| `2026-09-26-body-parse-OK.txt` | 132 | **132** ✓ | ✅ 成功 |

**三個都對得上。** 所以「兩種不同的 bytes_len 算法」這個前提是錯的 ——
紀錄裡的長度就是 body 的原始長度，儀器沒有弄壞 body。

## 真正的原因

看 body 的內容就一目了然：

```text
FAILED: {model:deepseek-v4-flash,messages:[{role:user,content:What is 2+2?}]}
        ↑ key 沒有引號
FAILED: {model:claude-opus-5,max_tokens:32,messages:[...]}
        ↑ key 沒有引號
OK:     {"model": "claude-opus-5", "max_tokens": 256, "messages": [...]}
        ↑ 合法 JSON
```

兩個失敗案例的 **key 全都沒有加引號**（`model:` 而非 `"model":`），
這是 **JavaScript 物件字面值的寫法，不是 JSON**。`serde_json` 拒絕它們完全正確。

推測來源：當時是用手寫的 `curl`／測試腳本送的 body，把 JS 物件字面值
誤當成 JSON 送出。**這不是網關的問題，網關回 400 是對的行為。**

## 結論與後續

- 網關的 `BodyPrep::Unparsable` 分支**行為正確**，不需修改。
- 既有的留痕設施（記錄原始位元組的 hex）**保留**：萬一未來真的出現
  「合法 JSON 卻解析失敗」，那份 hex 仍是唯一的定案依據。這次只是
  **沒有等到那種案例**，而手上這三筆已經可以解釋完畢。
- 因此 `TESTING.md` §8 的這一項由「仍未定案」改為「已定案」。
