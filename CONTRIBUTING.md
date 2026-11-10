# Contributing

Start with [development setup and validation](docs/DEVELOPMENT.md) and the
[branching policy](docs/BRANCHING.md). Build prerequisites and packaging details
are in the [build guide](docs/BUILD.md).

- Branch from lowercase `dev` and open feature or fix PRs against `dev`.
- For issue work, use `fix/issue-NUMBER-short-description` and handle one issue per PR.
- Keep changes focused, include relevant regression coverage, and update affected docs.
- Run the required [Rust checks](docs/DEVELOPMENT.md#build-and-validation-commands)
  before requesting review; include results and any untested platform or hardware behavior.
- Use `Fixes #NUMBER` only for a complete fix; otherwise use `Refs #NUMBER`.
- Run the UI and session daemon as your normal user. Hardware writes and host
  configuration changes require explicit authorization.

Maintainers promote reviewed changes from `dev` to `main` with a merge commit.
The legacy uppercase `Dev` branch is preserved for history; do not target it with
new work. A merge does not authorize a release, tag, or publication; follow the
[release checklist](docs/RELEASE_CHECKLIST.md) with separate release authorization.
