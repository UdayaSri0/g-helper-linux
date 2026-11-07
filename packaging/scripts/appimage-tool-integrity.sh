#!/usr/bin/env bash

download_verified_tool() {
  local url="$1"
  local dest="$2"
  local expected_sha256="$3"
  local actual_sha256
  local tmp

  if [[ ! "$expected_sha256" =~ ^[[:xdigit:]]{64}$ ]]; then
    echo "a 64-character SHA-256 pin is required for $url" >&2
    return 1
  fi

  mkdir -p "$(dirname -- "$dest")"
  if [[ -e "$dest" || -L "$dest" ]]; then
    if [[ ! -f "$dest" || -L "$dest" ]]; then
      echo "cached tool is not a non-symlink regular file: $dest" >&2
      return 1
    fi
    actual_sha256="$(sha256sum "$dest" | awk '{print $1}')"
    if [[ "$actual_sha256" != "$expected_sha256" ]]; then
      echo "cached tool checksum mismatch: $dest" >&2
      return 1
    fi
    if [[ ! -x "$dest" ]]; then
      echo "cached tool is not executable: $dest" >&2
      return 1
    fi
    return 0
  fi

  tmp="$(mktemp "$(dirname -- "$dest")/.${dest##*/}.XXXXXX")"
  if ! curl --fail --location --retry 3 --show-error --silent "$url" -o "$tmp"; then
    rm -f "$tmp"
    return 1
  fi
  actual_sha256="$(sha256sum "$tmp" | awk '{print $1}')"
  if [[ "$actual_sha256" != "$expected_sha256" ]]; then
    rm -f "$tmp"
    echo "downloaded tool checksum mismatch: $url" >&2
    return 1
  fi
  chmod 0755 "$tmp"
  mv "$tmp" "$dest"
}
