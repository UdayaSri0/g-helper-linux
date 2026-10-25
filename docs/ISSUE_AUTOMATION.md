# Issue automation

`scripts/issue-orchestrator.py` coordinates the repository's conservative
issue workflow. It is read-only by default and uses the authenticated `gh`
CLI; tokens are never written to project files or logs.

## Inspect and resume

```sh
python3 scripts/issue-orchestrator.py --dry-run --list
python3 scripts/issue-orchestrator.py --resume --list
```

State and the single-process lock are stored in the ignored
`.issue-orchestrator/` directory. State updates are atomic and include issue,
branch, status, checkpoint, and error fields. Remove a stale lock only after
confirming no process with its recorded PID is running.

## Execute one issue

Review the dry-run queue and local worktree first, then explicitly run:

```sh
python3 scripts/issue-orchestrator.py --issue 78 --run-one --apply
```

The implementation hook invokes the installed `codex exec` command using its
current documented flags. Launch this from an independent terminal/session;
do not recursively invoke it from the active Codex session. The hook does not
merge PRs or close issues. Human/maintainer review remains required for
hardware, privilege, updater trust, GPU, fan, and CPU-control changes.

`--run-all-safe` is bounded by `--max-issues N` and should only be used for
explicitly classified low-risk work. `--auto-merge-safe` is intentionally a
reserved no-op flag until repository branch-protection policy permits a
verified normal merge.

## Gates

Each Rust change must pass the repository's locked format, build, test, and
Clippy commands plus applicable packaging checks. Missing local Rust or system
dependencies are recorded as unavailable; they are not treated as passing.
Use `Fixes #N` only when all acceptance criteria are met. Use `Refs #N` for a
partial result and leave the issue open.
