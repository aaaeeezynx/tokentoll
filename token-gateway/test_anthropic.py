import http.client, json

body = json.dumps({
    "model": "claude-opus-5",
    "max_tokens": 256,
    "messages": [{"role": "user", "content": "What is 2+2? Reply only with the number."}]
})

conn = http.client.HTTPConnection("127.0.0.1", 15722, timeout=60)
conn.request("POST", "/v1/messages", body=body, headers={
    "x-api-key": "sk-local-h6M09v4fhGx9h7qIX3pVB5OkMIjslkNd",
    "anthropic-version": "2023-06-01",
    "Content-Type": "application/json",
})
resp = conn.getresponse()
data = resp.read().decode()
print(f"HTTP {resp.status}")
parsed = json.loads(data)
print(json.dumps(parsed, indent=2, ensure_ascii=False))
conn.close()
