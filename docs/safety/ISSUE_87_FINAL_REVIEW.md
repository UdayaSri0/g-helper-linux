# Issue 87 final review gate

This document is a release gate, not a declaration of completion.

## Required before epic closure

- [ ] All thirteen child issue acceptance criteria are individually evidenced.
- [ ] Fan write eligibility is exact-model, firmware, kernel, driver, and
  channel scoped.
- [ ] Fan Apply, Auto restore, failure recovery, and restart behavior have
  dated supervised observations.
- [ ] CPU and GPU transactions have deterministic rollback/readback evidence.
- [ ] Aura and packaging lifecycle evidence covers installed artifacts.
- [ ] AppImage tools are verified before execution and release permissions are
  minimized.
- [ ] A maintainer independent of the implementation signs off residual risk.
- [ ] GitHub issue checklists are updated with links to the actual evidence.

## Current decision

**BLOCKED / DO NOT CLOSE #87.** Source changes and automated tests cannot
substitute for physical testing, independent review, or installed-runtime
evidence. No release is published by this workstream.
