#!/usr/bin/env python3
"""Conservative, resumable GitHub issue workflow coordinator.

Dry-run is the default. This utility records planning/checkpoint state and can
optionally hand a single issue to an independently launched Codex CLI session.
It never stores credentials and never performs hardware or host configuration
changes itself.
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from contextlib import contextmanager
from pathlib import Path

REPO = "UdayaSri0/g-helper-linux"
ROOT = Path(__file__).resolve().parents[1]
STATE_DIR = ROOT / ".issue-orchestrator"
STATE_FILE = STATE_DIR / "state.json"
LOCK_FILE = STATE_DIR / "run.lock"


def run(*args: str) -> str:
    result = subprocess.run(args, cwd=ROOT, check=True, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return result.stdout


def load_state() -> dict:
    if not STATE_FILE.exists():
        return {"schema": 1, "repo": REPO, "issues": {}, "last_checkpoint": None}
    return json.loads(STATE_FILE.read_text())


def save_state(state: dict) -> None:
    STATE_DIR.mkdir(mode=0o700, exist_ok=True)
    temporary = STATE_FILE.with_suffix(".tmp")
    temporary.write_text(json.dumps(state, indent=2, sort_keys=True) + "\n")
    os.replace(temporary, STATE_FILE)


@contextmanager
def lock():
    STATE_DIR.mkdir(mode=0o700, exist_ok=True)
    try:
        fd = os.open(LOCK_FILE, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    except FileExistsError as exc:
        raise SystemExit(f"another orchestrator is active: {LOCK_FILE}") from exc
    try:
        os.write(fd, f"pid={os.getpid()}\n".encode())
        os.close(fd)
        yield
    finally:
        LOCK_FILE.unlink(missing_ok=True)


def issues() -> list[dict]:
    return json.loads(run("gh", "issue", "list", "--repo", REPO, "--state", "open",
                          "--limit", "200", "--json", "number,title,body,labels,updatedAt,url"))


def priority(issue: dict) -> str:
    names = {label["name"] for label in issue.get("labels", [])}
    return next((value for value in ("P0", "P1", "P2") if f"prio:{value}" in names), "P2")


def print_queue(found: list[dict], state: dict) -> None:
    for issue in sorted(found, key=lambda item: (priority(item), item["number"])):
        record = state["issues"].get(str(issue["number"]), {})
        print(f"#{issue['number']} {priority(issue)} {issue['title']}"
              f" [{record.get('status', 'discovered')}] {issue['url']}")


def checkpoint(state: dict, number: int, **values) -> None:
    record = state["issues"].setdefault(str(number), {})
    record.update(values)
    record["last_successful_checkpoint"] = int(time.time())
    state["last_checkpoint"] = {"issue": number, **values}
    save_state(state)


def run_codex(issue: dict) -> int:
    prompt = (f"Work only on GitHub issue #{issue['number']} in {REPO}. "
              "Inspect current main, use an isolated branch/worktree, reproduce, "
              "test, and report evidence. Do not merge, close issues, modify hardware, "
              "or bypass required review gates. Return a durable summary.")
    command = ["codex", "exec", "--cd", str(ROOT), "--sandbox", "danger-full-access",
               "--ask-for-approval", "never", prompt]
    return subprocess.run(command, cwd=ROOT).returncode


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dry-run", action="store_true", help="inspect only (default)")
    parser.add_argument("--list", action="store_true", help="display the issue queue")
    parser.add_argument("--run-one", action="store_true")
    parser.add_argument("--run-all-safe", action="store_true")
    parser.add_argument("--resume", action="store_true")
    parser.add_argument("--issue", type=int)
    parser.add_argument("--max-issues", type=int, default=1)
    parser.add_argument("--apply", action="store_true", help="permit Codex implementation")
    parser.add_argument("--auto-merge-safe", action="store_true",
                        help="reserved; never merges without explicit policy checks")
    args = parser.parse_args()
    if not (args.list or args.run_one or args.run_all_safe or args.issue):
        args.list = True
    state = load_state()
    try:
        with lock():
            found = issues()
            if args.list:
                print_queue(found, state)
            if not (args.run_one or args.run_all_safe or args.issue):
                return 0
            if not args.apply:
                print("dry-run: would process the selected issue(s); pass --apply to execute")
                return 0
            selected = [item for item in found if args.issue is None or item["number"] == args.issue]
            if args.run_one or args.issue:
                selected = selected[:1]
            else:
                selected = selected[:max(args.max_issues, 1)]
            for issue in selected:
                number = issue["number"]
                checkpoint(state, number, title=issue["title"], status="delegated",
                           category="unclassified", branch=f"fix/issue-{number}")
                code = run_codex(issue)
                checkpoint(state, number, status="codex-complete" if code == 0 else "error",
                           last_error=None if code == 0 else f"codex exit {code}")
                if code != 0:
                    return code
            return 0
    except subprocess.CalledProcessError as exc:
        print(exc.stderr.strip() or str(exc), file=sys.stderr)
        return exc.returncode or 1


if __name__ == "__main__":
    raise SystemExit(main())
