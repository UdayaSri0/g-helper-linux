# Branching policy

Lowercase `dev` is the canonical integration branch. `main` is the stable production branch, the default branch and the source for official releases. It must remain buildable; incomplete, experimental or hardware-dependent work stays on `dev` until validated.
Branch names are case-sensitive: legacy uppercase `Dev` is preserved for history,
but new branches and PRs must not use it as their base or target.

## Feature and fix work

1. Fetch the remote and start a focused branch from `origin/dev`.
2. Implement the change, run the [required checks](DEVELOPMENT.md#build-and-validation-commands),
   and open a PR targeting `dev`.
3. Merge after review and passing checks on the current PR head.
4. Promote the reviewed integration state with a `dev` → `main` PR using a merge
   commit. Preserve shared ancestry; do not squash or rebase this promotion.

Do not force-push shared branches or bypass protection. Existing work based on
`Dev` should be reviewed and retargeted to `dev`, resolving any differences before
merging; the legacy branch can remain available without receiving new work.

## Keeping work current and resolving conflicts

Use `feature/`, `fix/`, `chore/`, `docs/`, `ci/` or `hotfix/` followed by a short description.
Merge updated `origin/dev` into a shared feature branch when needed; preserve other
contributors’ commits. Respond to review comments, push focused fixes and rerun
checks on the new head. Resolve review threads only after addressing them.

Read both sides of conflicts and preserve tests, validation and privilege boundaries.
Do not resolve an entire merge by blindly selecting one side. Recheck callers,
configuration, packaging and affected docs, then rerun the required checks.
Synchronize `main` back into `dev` with a merge PR after production-only changes.
Never delete legacy branches without owner approval.

## Urgent hotfixes

When a fix must reach `main` before the next promotion, branch from `origin/main`
and open a narrowly scoped hotfix PR to `main`. Apply the same review and checks.
After merging, promptly open a `main` → `dev` back-sync PR and merge it with a merge
commit so the fix and its ancestry remain in the integration branch. Resolve
conflicts explicitly and validate the resulting tree before merging.

## Releases

Promoting or back-syncing branches does not authorize version changes, tags,
release workflow dispatches, artifact uploads, or publication. Release actions
require separate explicit authorization and the [release checklist](RELEASE_CHECKLIST.md).
CI success establishes the recorded checks only; it does not establish untested
hardware compatibility or approval to release.
