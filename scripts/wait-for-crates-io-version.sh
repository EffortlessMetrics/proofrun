#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 || $# -gt 4 ]]; then
  echo "usage: $0 <crate> <version> [timeout-seconds] [interval-seconds]" >&2
  exit 64
fi

crate="$1"
version="$2"
timeout_seconds="${3:-300}"
interval_seconds="${4:-10}"

if ! [[ "$timeout_seconds" =~ ^[0-9]+$ ]]; then
  echo "timeout-seconds must be a non-negative integer" >&2
  exit 64
fi

if ! [[ "$interval_seconds" =~ ^[0-9]+$ ]]; then
  echo "interval-seconds must be a non-negative integer" >&2
  exit 64
fi

crate_url="https://crates.io/api/v1/crates/${crate}"
deadline=$((SECONDS + timeout_seconds))

while true; do
  if response="$(curl -fsSL "$crate_url" 2>/dev/null)"; then
    if printf '%s' "$response" | grep -F "\"num\":\"${version}\"" >/dev/null; then
      echo "${crate} ${version} is visible on crates.io"
      exit 0
    fi
  fi

  if (( SECONDS >= deadline )); then
    echo "${crate} ${version} is not yet visible on crates.io" >&2
    exit 1
  fi

  sleep "$interval_seconds"
done
