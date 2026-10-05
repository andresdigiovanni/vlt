#!/usr/bin/env bash
# Print the notes of a `## [<version>]` block from a Keep-a-Changelog file
# to stdout, with leading/trailing blank lines trimmed.
#
# Usage: scripts/extract-release-notes.sh <version|Unreleased> [changelog-path]
#
# Exits 1 (with a message on stderr) when the block is missing or has no
# content — the release workflow uses that as the fallback signal to try
# `Unreleased`.
set -euo pipefail

if [ $# -lt 1 ]; then
  echo "usage: $0 <version|Unreleased> [changelog-path]" >&2
  exit 2
fi

V="$1"
FILE="${2:-CHANGELOG.md}"

if [ ! -f "$FILE" ]; then
  echo "error: $FILE not found" >&2
  exit 1
fi

NOTES=$(awk -v ver="$V" '
  index($0, "## [" ver "]") == 1 && inblock == 0 {
    inblock = 1
    next
  }
  inblock && /^## / { exit }
  inblock && /^[[:space:]]*$/ { if (saw) pending++; next }
  inblock {
    saw = 1
    while (pending > 0) { print ""; pending-- }
    print
  }
' "$FILE")

if [ -z "$NOTES" ]; then
  echo "error: no notes found for [${V}] in ${FILE}" >&2
  exit 1
fi

printf '%s\n' "$NOTES"
