#!/usr/bin/env bash
set -euo pipefail

TAG="${1:?release tag required}"
COMMIT="${2:?release commit required}"
refs="$(git ls-remote --exit-code origin refs/heads/main "refs/tags/$TAG" "refs/tags/$TAG^{}")"
main="$(awk '$2 == "refs/heads/main" {print $1}' <<< "$refs")"
tag="$(awk -v ref="refs/tags/$TAG" '$2 == ref {print $1}' <<< "$refs")"
peeled="$(awk -v ref="refs/tags/$TAG^{}" '$2 == ref {print $1}' <<< "$refs")"
if [[ "$(git rev-parse HEAD)" != "$COMMIT" || "$main" != "$COMMIT" || "${peeled:-$tag}" != "$COMMIT" ]]; then
  echo "Release must use the live tag pointing to the verified main tip" >&2
  exit 1
fi
