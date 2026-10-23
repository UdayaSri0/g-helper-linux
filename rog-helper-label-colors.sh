#!/usr/bin/env bash
# ROG Helper Linux: apply a consistent GitHub Issues label color scheme.
# This script changes ONLY the 34 explicitly listed existing labels.
# It does not create/delete labels or edit issue contents.
set -euo pipefail

REPO="UdayaSri0/g-helper-linux"
MODE="${1:---preview}"

case "$MODE" in
  --preview|--apply) ;;
  *) printf 'Usage: %s [--preview|--apply]\n' "$0" >&2; exit 2 ;;
esac

if [[ "$MODE" == "--apply" ]]; then
  if ! command -v gh >/dev/null 2>&1; then
    echo "GitHub CLI (gh) is required. On Linux Mint: sudo apt install gh" >&2
    exit 1
  fi
  if ! gh auth status --hostname github.com >/dev/null 2>&1; then
    echo "GitHub CLI is not authenticated. Run: gh auth login" >&2
    exit 1
  fi
fi

ok=0
failed=0
printf '%-31s %-9s %s\n' 'LABEL' 'COLOR' 'DESCRIPTION'
printf '%s\n' '-------------------------------------------------------------------------------'
while IFS='|' read -r label color description; do
  [[ -n "$label" ]] || continue
  if [[ "$MODE" == "--preview" ]]; then
    printf '%-31s #%-8s %s\n' "$label" "$color" "$description"
    ((ok+=1))
  elif gh label edit "$label" --repo "$REPO" --color "$color" --description "$description"; then
    printf 'Updated %-31s (#%s)\n' "$label" "$color"
    ((ok+=1))
  else
    printf 'FAILED: %s (verify that the label exists and you have permission)\n' "$label" >&2
    ((failed+=1))
  fi
done <<'LABELS'
type:bug|D73A4A|A confirmed or reproducible defect
type:feature|0E8A16|Request for new functionality or enhancement
type:chore|6E7781|Maintenance, refactoring, or cleanup work
type:docs|8250DF|Documentation update or correction
prio:P0|B60205|Critical: immediate safety or release-blocking problem
prio:P1|D93F0B|High: prioritize for the next safety or release cycle
prio:P2|FBCA04|Medium: important but not currently release-blocking
prio:P3|C2E0C6|Low: minor improvement or backlog item
area:ui|1D76DB|GTK user interface and user interactions
area:daemon|0052CC|Session daemon, lifecycle, and policy engine
area:providers|5319E7|Hardware provider integrations and backends
area:core|0E8A16|Shared Rust models, validation, and contracts
area:packaging|006B75|DEB, RPM, AppImage, Flatpak, and installers
area:ci|0D9488|CI workflows, automated tests, and builds
area:docs|8250DF|Project documentation and help content
area:security|B60205|Security boundaries, vulnerabilities, and mitigations
area:hardware|D93F0B|Hardware compatibility, writes, and device safety
feature:telemetry|1D76DB|CPU, GPU, fan, battery, and system readings
feature:profiles|0E8A16|Saved profiles, presets, and performance modes
feature:fans|E67E22|Fan control, fan curves, and safety recovery
feature:gpuswitch|5319E7|GPU mode switching and display topology
feature:battery|2DA44E|Battery charge thresholds and charging state
feature:lighting|D93F9B|ASUS Aura, keyboard brightness, and RGB
feature:automation|7057FF|AC and battery event-driven automation
feature:diagnostics|6A737D|Diagnostics, reports, and troubleshooting
feature:cpu|0969DA|CPU governor, boost, cores, and clock limits
feature:memory|C5DEF5|RAM and memory telemetry
feature:setup|008672|Installation, dependencies, and initial setup
status:needs-triage|FBCA04|Needs initial assessment or prioritization
status:blocked|B60205|Cannot proceed until a dependency is resolved
status:needs-hw-logs|E99695|Requires real hardware logs or physical validation
status:needs-design|7057FF|Design or architecture decision is pending
good first issue|C2E0C6|Well-scoped task suitable for a first contribution
help wanted|008672|External contributions are welcome
LABELS

printf '\n%s: %s labels processed, %s failed for %s\n' "$MODE" "$ok" "$failed" "$REPO"
if [[ "$MODE" == "--preview" ]]; then
  printf 'No GitHub changes were made. Apply with: bash %s --apply\n' "$0"
fi
[[ "$failed" == 0 ]]
