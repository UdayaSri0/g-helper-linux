## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

When the user types `/graphify`, use the installed graphify skill or instructions before doing anything else.

Rules:
- For codebase questions, first run `graphify query "<question>"` when graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- Dirty graphify-out/ files are expected after hooks or incremental updates; dirty graph files are not a reason to skip graphify. Only skip graphify if the task is about stale or incorrect graph output, or the user explicitly says not to use it.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).

## Issue automation

- Work on one GitHub issue at a time, from a branch named `fix/issue-NUMBER-short-description`.
- Keep orchestration changes separate from issue fixes; use a worktree when the main worktree is dirty.
- The reusable workflow lives in `scripts/issue-orchestrator.py` and defaults to read-only dry-run mode.
- Durable state is written below `.issue-orchestrator/`, which is ignored and must never contain tokens.
- Do not run the GUI or daemon as root, perform physical hardware writes, change host configuration, or bypass branch protection.
- Required validation for Rust changes is `cargo fmt --all -- --check`, locked workspace build/test, and locked Clippy with warnings denied. Missing local tooling is a blocker, not a passing result.
- Open a PR only after the relevant local checks and independent review pass. Use `Fixes #NUMBER` only for a complete fix; otherwise use `Refs #NUMBER` and leave the issue open.
- Resume with `python3 scripts/issue-orchestrator.py --resume --list`; use `--apply` only after reviewing the dry-run plan.
