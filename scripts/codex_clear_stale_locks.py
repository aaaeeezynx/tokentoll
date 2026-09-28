"""List (and optionally clear) stale Codex thread writer locks.

WHY THIS EXISTS
---------------
`codex archive <session>` can fail with:

    Error: failed to archive session

even though the session exists, its rollout file is present and complete, and
`codex doctor` only warns about unrelated inventory counts.

The cause is a leftover lock file:

    %USERPROFILE%\\.codex\\thread-writer-locks\\<thread-id>.lock

Codex records "somebody is writing this thread" by creating that file.  When a
conversation is opened in Codex Desktop and the lock is never released again,
every later archive of that thread fails.  Removing the stale file makes the
archive succeed immediately (verified 2026-09-28 -- see docs/TESTING.md).

This tool only ever deletes 0-byte `<uuid>.lock` marker files, only when
`--apply` is given, and it copies each one into a backup directory first.
It never touches `thread-writer-locks\\.coordination.lock` and never touches
rollout files.

USAGE
-----
    py scripts\\codex_clear_stale_locks.py            # report only (safe)
    py scripts\\codex_clear_stale_locks.py --apply    # delete, after backing up
    py scripts\\codex_clear_stale_locks.py --apply --min-age 10
"""
from __future__ import annotations

import argparse
import ctypes
import datetime
import os
import shutil
import sqlite3
import sys
import uuid

HOME = os.path.expanduser("~")
CODEX = os.path.join(HOME, ".codex")
LOCK_DIR = os.path.join(CODEX, "thread-writer-locks")
STATE_DB = os.path.join(CODEX, "state_5.sqlite")
COORDINATION = ".coordination.lock"

GENERIC_READ = 0x80000000
GENERIC_WRITE = 0x40000000
OPEN_EXISTING = 3
INVALID_HANDLE_VALUE = ctypes.c_void_p(-1).value


def file_is_open_by_a_process(path: str) -> bool:
    """True when some process holds the file open (sharing violation).

    Conservative on purpose: if we cannot tell, we report True so the caller
    leaves the file alone.  This is a *sharing-mode* check, so a process using
    byte-range locks only may still slip through.
    """
    try:
        CreateFileW = ctypes.windll.kernel32.CreateFileW
        CreateFileW.restype = ctypes.c_void_p
        h = CreateFileW(
            ctypes.c_wchar_p(path),
            GENERIC_READ | GENERIC_WRITE,
            0,  # no sharing at all
            None,
            OPEN_EXISTING,
            0,
            None,
        )
        if h == INVALID_HANDLE_VALUE or h is None:
            err = ctypes.windll.kernel32.GetLastError()
            # 32 = ERROR_SHARING_VIOLATION, 33 = ERROR_LOCK_VIOLATION
            return err in (32, 33)
        ctypes.windll.kernel32.CloseHandle(ctypes.c_void_p(h))
        return False
    except Exception:
        return True


def load_threads():
    if not os.path.exists(STATE_DB):
        return {}
    con = sqlite3.connect("file:%s?mode=ro" % STATE_DB.replace("\\", "/"), uri=True)
    out = {}
    try:
        for tid, arch, title, model in con.execute(
                "SELECT id, archived, coalesce(title,''), coalesce(model,'') FROM threads"):
            out[tid] = (arch, title, model)
    finally:
        con.close()
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--apply", action="store_true",
                    help="actually delete the stale locks (default: report only)")
    ap.add_argument("--min-age", type=int, default=0, metavar="MINUTES",
                    help="only consider locks older than this many minutes (default 0)")
    ap.add_argument("--backup", default=os.path.join(CODEX, "thread-writer-locks-backup"),
                    help="directory to copy removed locks into")
    args = ap.parse_args()

    print("lock dir =", LOCK_DIR)
    if not os.path.isdir(LOCK_DIR):
        print("  (does not exist -- nothing to do)")
        return 0

    threads = load_threads()
    now = datetime.datetime.now()
    stale, busy, skipped = [], [], []

    for name in sorted(os.listdir(LOCK_DIR)):
        path = os.path.join(LOCK_DIR, name)
        if name == COORDINATION:
            skipped.append((name, "coordination lock (not a thread)"))
            continue
        if not name.endswith(".lock"):
            skipped.append((name, "not a .lock file"))
            continue
        stem = name[: -len(".lock")]
        try:
            uuid.UUID(stem)
        except ValueError:
            skipped.append((name, "not a thread UUID"))
            continue
        if os.path.getsize(path) != 0:
            skipped.append((name, "not empty (%d bytes)" % os.path.getsize(path)))
            continue

        age_min = (now - datetime.datetime.fromtimestamp(os.path.getmtime(path))).total_seconds() / 60.0
        if age_min < args.min_age:
            skipped.append((name, "younger than --min-age (%d min)" % age_min))
            continue

        if file_is_open_by_a_process(path):
            busy.append((name, age_min))
            continue
        stale.append((name, stem, age_min, path))

    print("\n%-40s %-9s %-6s %s" % ("lock", "age(min)", "arch", "title"))
    print("-" * 100)
    for name, stem, age_min, _ in stale:
        arch, title, model = threads.get(stem, ("?", "(not in state DB)", ""))
        print("%-40s %-9.1f %-6s %s" % (stem, age_min, arch, title[:52]))
    for name, age_min in busy:
        print("%-40s %-9.1f %-6s %s" % (name[:-5], age_min, "-", "<-- IN USE, will not touch"))
    for name, why in skipped:
        print("%-40s %-9s %-6s %s" % (name, "-", "-", "<-- skipped: " + why))

    print("\nstale=%d  in-use=%d  skipped=%d" % (len(stale), len(busy), len(skipped)))

    if not stale:
        print("\nNothing to clear.")
        return 0

    if not args.apply:
        print("\nReport only. Re-run with --apply to delete the %d stale lock(s)." % len(stale))
        print("(Codex should ideally be closed first; the check above already skips")
        print(" any lock a running process still holds open.)")
        return 0

    os.makedirs(args.backup, exist_ok=True)
    removed = 0
    for name, stem, age_min, path in stale:
        shutil.copy2(path, os.path.join(args.backup, name))
        os.remove(path)
        removed += 1
    print("\nRemoved %d stale lock(s); copies are in %s" % (removed, args.backup))
    print("You can now archive those sessions, e.g.:")
    for name, stem, age_min, _ in stale[:3]:
        print("    codex archive %s" % stem)
    return 0


if __name__ == "__main__":
    sys.exit(main())
