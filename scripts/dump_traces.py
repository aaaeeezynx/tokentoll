#!/usr/bin/env python3
"""傾印網關診斷資料，供回報問題時附上。

**唯讀開啟**，不會修改資料庫（SQLite 以 `mode=ro` 開啟）。

用法：
    py scripts/dump_traces.py                    # 最近 30 筆追蹤
    py scripts/dump_traces.py -n 100             # 最近 100 筆
    py scripts/dump_traces.py --problems         # 只看問題（warn／有重試／有剝離）
    py scripts/dump_traces.py -o report.txt      # 另存檔案
    py scripts/dump_traces.py --db <路徑>        # 指定其他資料庫

輸出同時寫到 stdout 與 `-o` 指定的檔案（預設 `trace_dump.txt`）。
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import sqlite3
import sys

DEFAULT_DB = os.path.join(
    os.environ.get("APPDATA", ""), "com.tokencounter.gateway", "app.db"
)


def connect_ro(path: str) -> sqlite3.Connection:
    """唯讀開啟。WAL 資料庫在 ro 模式下可能因 -shm 不可寫而失敗，故有退路。"""
    try:
        return sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    except sqlite3.Error:
        return sqlite3.connect(path)


def has_table(c: sqlite3.Connection, name: str) -> bool:
    return (
        c.execute(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?", (name,)
        ).fetchone()
        is not None
    )


def scalar(c: sqlite3.Connection, sql: str, default=0):
    try:
        r = c.execute(sql).fetchone()
        return r[0] if r and r[0] is not None else default
    except sqlite3.Error:
        return default


def dump(args) -> str:
    if not os.path.exists(args.db):
        return f"找不到資料庫：{args.db}\n（用 --db 指定路徑）\n"

    c = connect_ro(args.db)
    o = []
    w = o.append
    now = dt.datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    w("# 網關診斷傾印")
    w(f"產生時間：{now}")
    w(f"資料庫　：{args.db}")
    w(f"schema_version：{scalar(c, 'SELECT COALESCE(MAX(version),0) FROM schema_version')}")
    w("")

    w("## 概況")
    w(f"  providers    : {scalar(c, 'SELECT COUNT(*) FROM providers')}")
    w(f"  request_logs : {scalar(c, 'SELECT COUNT(*) FROM request_logs')}")
    w(f"  local_keys   : {scalar(c, 'SELECT COUNT(*) FROM local_keys')}")

    for t in ("proxy_trace", "provider_stripped_fields"):
        if not has_table(c, t):
            w(f"  {t}: **不存在**（資料庫尚未升級到 v8，請先啟動一次新版程式）")

    if not has_table(c, "proxy_trace"):
        return "\n".join(o) + "\n"

    w(f"  proxy_trace  : {scalar(c, 'SELECT COUNT(*) FROM proxy_trace')}")
    w("")

    # ---- 追蹤 ----
    where = ""
    if args.problems:
        where = "WHERE trace_level='warn' OR retry_count>0 OR stripped_fields<>'[]'"
    rows = c.execute(
        f"""SELECT id, ts, trace_level, app, model_raw, in_fmt, target_fmt, trans_kind,
                   upstream_status, latency_ms, retry_count, stripped_fields,
                   content_length, content_type, body_sha256, body_hex,
                   upstream_error, note
            FROM proxy_trace {where} ORDER BY id DESC LIMIT ?""",
        (args.n,),
    ).fetchall()

    w(f"## 追蹤記錄（{'問題' if args.problems else '最近'} {len(rows)} 筆，新→舊）")
    w("")
    if not rows:
        w("  （無）")
    for r in rows:
        (rid, ts, lvl, app, model, inf, tf, tk, st, lat, retry,
         stripped, clen, ctype, sha, body_hex, uerr, note) = r
        when = dt.datetime.fromtimestamp(ts / 1000).strftime("%m-%d %H:%M:%S")
        w(f"### #{rid}  {when}  [{lvl}]  {app or '-'}  status={st}  {lat}ms")
        w(f"    model      : {model or '-'}")
        if tk == "rejected":
            w(f"    trans_kind : rejected（網關拒絕，未觸及上游）")
        else:
            w(f"    trans_kind : {tk}   {inf} → {tf}")
        if retry:
            w(f"    retry      : {retry}")
        if stripped and stripped != "[]":
            try:
                w(f"    stripped   : {', '.join(json.loads(stripped))}")
            except Exception:
                w(f"    stripped   : {stripped}")
        if clen is not None:
            w(f"    content    : {clen} bytes  {ctype}")
        if sha:
            w(f"    sha256     : {sha}")
        if note:
            w(f"    note       : {note}")
        if uerr:
            w(f"    upstream   : {uerr[:600]}")
        if body_hex:
            w(f"    body_hex   : {body_hex[:512]}{'…' if len(body_hex) > 512 else ''}")
            try:
                b = bytes.fromhex(body_hex)
                w(f"    body 還原  : {b[:400]!r}")
            except Exception as e:
                w(f"    body 還原  : <hex 解碼失敗：{e}>")
        w("")

    # ---- 分布 ----
    w("## 分布")
    w("  trans_kind:")
    for k, n in c.execute(
        "SELECT trans_kind, COUNT(*) FROM proxy_trace GROUP BY trans_kind ORDER BY 2 DESC"
    ):
        w(f"    {k or '(空)':24} {n}")
    w("  status（回給客戶端／上游）:")
    for k, n in c.execute(
        "SELECT upstream_status, COUNT(*) FROM proxy_trace GROUP BY 1 ORDER BY 2 DESC"
    ):
        w(f"    {k:24} {n}")
    if has_table(c, "provider_stripped_fields"):
        w("  已學會的剝離欄位（provider_id, field）:")
        any_row = False
        for pid, fld, n in c.execute(
            "SELECT provider_id, field, COUNT(*) FROM provider_stripped_fields "
            "GROUP BY provider_id, field"
        ):
            w(f"    provider {pid}: {fld}  (x{n})")
            any_row = True
        if not any_row:
            w("    （無）")

    return "\n".join(o) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser(description="傾印網關診斷資料（唯讀）")
    ap.add_argument("-n", type=int, default=30, help="筆數（預設 30）")
    ap.add_argument("--problems", action="store_true", help="只看問題追蹤")
    ap.add_argument("--db", default=DEFAULT_DB, help="資料庫路徑")
    ap.add_argument("-o", "--out", default="trace_dump.txt", help="輸出檔（預設 trace_dump.txt）")
    args = ap.parse_args()

    text = dump(args)
    try:
        with open(args.out, "w", encoding="utf-8", newline="") as f:
            f.write(text)
        print(f"已寫入 {os.path.abspath(args.out)}")
    except OSError as e:
        print(f"寫檔失敗（{e}），以下僅列於 stdout", file=sys.stderr)

    try:
        print(text)
    except UnicodeEncodeError:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
        print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
