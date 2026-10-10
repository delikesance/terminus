#!/usr/bin/env bash
# Fails when a Terminus source file exceeds the hard line budget and is not
# in the allowlist. The allowlist may only shrink: an entry whose file is now
# under budget is reported as stale so it gets removed.
set -euo pipefail

SOFT=300
HARD=500
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ALLOWLIST="$ROOT/scripts/file-size-allowlist.txt"
EXEMPT='(/icons\.rs|/os_icons\.rs)$'

allowed() { [[ -f "$ALLOWLIST" ]] && grep -qxF "$1" "$ALLOWLIST"; }

status=0
soft=0
while IFS= read -r file; do
  [[ "$file" =~ $EXEMPT ]] && continue
  lines=$(wc -l <"$ROOT/$file")
  if ((lines > HARD)); then
    allowed "$file" && continue
    echo "error: $file has $lines lines (hard limit $HARD)"
    status=1
  elif ((lines > SOFT)); then
    soft=$((soft + 1))
  fi
done < <(cd "$ROOT" && git ls-files 'crates/*.rs' 'frontends/rioterm/src/*.rs')

if [[ -f "$ALLOWLIST" ]]; then
  while IFS= read -r file; do
    [[ -z "$file" ]] && continue
    if [[ ! -f "$ROOT/$file" ]] || (($(wc -l <"$ROOT/$file") <= HARD)); then
      echo "error: $file is under budget, remove it from the allowlist"
      status=1
    fi
  done <"$ALLOWLIST"
fi

echo "files over soft limit ($SOFT): $soft"
exit "$status"
