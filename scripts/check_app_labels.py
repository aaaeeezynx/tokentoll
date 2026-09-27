"""確認前端 APP_META 與後端 APPS 的 app 顯示名一致。

由來：使用者回報「本機工具應該是顯示 DSH 或 DeepSeek Harness，而不是
DeepSeek」。後端早就寫 "DeepSeek Harness"，是前端寫成 "DeepSeek" ——
兩份清單各寫各的，沒有任何機制防止漂移。

這支腳本同時被用在開發流程中，直接執行即可：

    py scripts/check_app_labels.py

不一致時以非零 exit code 結束。
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CONSTS = ROOT / "token-gateway" / "src-tauri" / "src" / "tools" / "consts.rs"
LOGOS = ROOT / "token-gateway" / "src" / "components" / "logos.tsx"


def parse_backend() -> list[tuple[str, str]]:
    t = CONSTS.read_text(encoding="utf-8")
    m = re.search(r"pub const APPS:\s*\[\(&str,\s*&str\);\s*\d+\]\s*=\s*\[(.*?)\];", t, re.S)
    if not m:
        sys.exit(f"找不到 APPS 定義：{CONSTS}")
    return re.findall(r'\("([^"]+)",\s*"([^"]+)"\)', m.group(1))


def parse_frontend() -> list[tuple[str, str]]:
    t = LOGOS.read_text(encoding="utf-8")
    m = re.search(r"export const APP_META.*?=\s*\[(.*?)\];", t, re.S)
    if not m:
        sys.exit(f"找不到 APP_META 定義：{LOGOS}")
    return re.findall(r'\{\s*id:\s*"([^"]+)",\s*label:\s*"([^"]+)"\s*\}', m.group(1))


def main() -> int:
    be = dict(parse_backend())
    fe = dict(parse_frontend())

    print("後端 APPS：")
    for k, v in be.items():
        print(f"   {k:12} -> {v}")
    print("\n前端 APP_META：")
    for k, v in fe.items():
        print(f"   {k:12} -> {v}")

    problems = []
    for k, v in be.items():
        if k not in fe:
            problems.append(f"前端缺少 app「{k}」")
        elif fe[k] != v:
            problems.append(f"app「{k}」顯示名不一致：後端 {v!r} vs 前端 {fe[k]!r}")
    for k in fe:
        if k not in be:
            problems.append(f"前端多出 app「{k}」（後端沒有）")

    # 針對本次回報的具體要求給出明確診斷
    print("\n針對本次回報：")
    dsh = be.get("dsh")
    if dsh == "DeepSeek":
        problems.append("後端 dsh 顯示名不可簡寫成 DeepSeek")
    if fe.get("dsh") == "DeepSeek":
        problems.append("前端 dsh 顯示名不可簡寫成 DeepSeek")
    print(f"   dsh 顯示名：後端 {dsh!r} / 前端 {fe.get('dsh')!r}")

    if problems:
        print("\n不一致：")
        for p in problems:
            print(f"   - {p}")
        return 1

    print("\n一致（OK）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
