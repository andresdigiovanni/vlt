#!/usr/bin/env bash
# Promote the `## [Unreleased]` section of a Keep-a-Changelog file into a
# dated `## [<version>] - <today>` section, leaving a fresh empty
# `## [Unreleased]` on top.
#
# Usage: scripts/prepare-release.sh <version> [changelog-path]
#
# Idempotent: exits 0 without modifying the file when `[<version>]` already
# exists or when `[Unreleased]` has no content. The release workflow pairs
# this with `git diff --quiet -- CHANGELOG.md` to decide about committing.
set -euo pipefail

if [ $# -lt 1 ]; then
  echo "usage: $0 <version> [changelog-path]" >&2
  exit 2
fi

V="$1"
FILE="${2:-CHANGELOG.md}"

if [ ! -f "$FILE" ]; then
  echo "error: $FILE not found" >&2
  exit 1
fi

if grep -q "^## \[${V}\]" "$FILE"; then
  echo "CHANGELOG already has [${V}] — nothing to promote."
  exit 0
fi

BODY=$(awk '/^## \[Unreleased\]/{f=1; next} f && /^## /{f=0} f' "$FILE")
if [ -z "$(printf '%s' "$BODY" | tr -d '[:space:]')" ]; then
  echo "[Unreleased] is empty — nothing to promote."
  exit 0
fi

TODAY=$(date +%Y-%m-%d)
awk -v ver="$V" -v today="$TODAY" '
  /^## \[Unreleased\]/ {
    print "## [Unreleased]"
    print ""
    print "## [" ver "] - " today
    inunrel = 1; pending = 0; saw = 0
    next
  }
  inunrel && /^## / {
    if (saw) print ""
    inunrel = 0
    print
    next
  }
  inunrel && /^[[:space:]]*$/ { pending++; next }
  inunrel {
    saw = 1
    while (pending > 0) { print ""; pending-- }
    print
    next
  }
  { print }
' "$FILE" > "$FILE.tmp"
mv "$FILE.tmp" "$FILE"
echo "Promoted [Unreleased] into [${V}] - ${TODAY}."
