"""對本地網關發一次 Anthropic 格式請求（POST /v1/messages），手動驗證翻譯路徑。

金鑰**不硬編**：從環境變數 `TOKEN_GATEWAY_KEY`（本專案既有的標準變數名）
或第一個命令列參數讀取。可在 App 的「本地 Key」頁簽發，明文只顯示一次。

用法：
    # PowerShell
    $env:TOKEN_GATEWAY_KEY="sk-local-…"; python scripts/test_anthropic.py
    python scripts/test_anthropic.py sk-local-… [port] [model]

備註：回應不是 JSON 時會印出原始位元組的 hex —— 這正是 400 排查最需要
看到的資訊（見 docs/REFACTORING-PLAN.md §5.2：先前的診斷之所以無法結案，
就是因為沒有留下原始 bytes）。
"""

import http.client
import json
import os
import sys

key = os.environ.get("TOKEN_GATEWAY_KEY") or (sys.argv[1] if len(sys.argv) > 1 else "")
if not key:
    sys.exit(
        "缺少金鑰。請設環境變數 TOKEN_GATEWAY_KEY，或傳入第一個參數。\n"
        "（可在 App 的「本地 Key」頁簽發；明文只顯示一次。）"
    )

port = int(sys.argv[2]) if len(sys.argv) > 2 else 15722
model = sys.argv[3] if len(sys.argv) > 3 else "claude-opus-5"

body = json.dumps(
    {
        "model": model,
        "max_tokens": 256,
        "messages": [
            {"role": "user", "content": "What is 2+2? Reply only with the number."}
        ],
    }
)

conn = http.client.HTTPConnection("127.0.0.1", port, timeout=60)
conn.request(
    "POST",
    "/v1/messages",
    body=body,
    headers={
        "x-api-key": key,
        "anthropic-version": "2023-06-01",
        "Content-Type": "application/json",
    },
)
resp = conn.getresponse()
raw = resp.read()
print(f"HTTP {resp.status}")

try:
    print(json.dumps(json.loads(raw.decode()), indent=2, ensure_ascii=False))
except (UnicodeDecodeError, json.JSONDecodeError):
    # 非 JSON 回應（HTML 錯誤頁、空 body、被截斷的 JSON…）
    print(f"（非 JSON 回應，{len(raw)} bytes；前 512 bytes 的 hex）")
    print(raw[:512].hex())
    print("---- 以文字盡量解讀 ----")
    print(raw[:2000].decode("utf-8", errors="replace"))

conn.close()
