#!/usr/bin/env bash
set -euo pipefail

root="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
registry="$root/docs/assets/registry.md"

if [ ! -f "$registry" ]; then
  echo "missing asset registry: $registry" >&2
  exit 1
fi

unregistered=0

while IFS= read -r -d '' file; do
  name="${file##*/}"
  if ! grep -Fq -- "$name" "$registry"; then
    echo "unregistered asset: $file" >&2
    unregistered=1
  fi
done < <(find "$root/assets/external" "$root/assets/mareforge" -type f ! -name .gitkeep -print0)

if [ "$unregistered" -ne 0 ]; then
  exit 1
fi

echo "OK"
