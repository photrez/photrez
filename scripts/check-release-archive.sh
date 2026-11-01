#!/bin/sh
# Guard: a release archive is only valid if it is a real build and it carries
# the licence set. AGPL-3.0 requires the licence to accompany the conveyed copy,
# so a missing LICENSE is a licensing defect, not a cosmetic one; a sub-megabyte
# archive means the binary never made it in.
#
# This is the single source of truth for the release smoke test: `.github/
# workflows/release.yml` calls it, and a developer can run it against any
# locally built archive. Prove it can fail:
#
#   truncate -s 0 broken.tar.gz
#   sh scripts/check-release-archive.sh broken.tar.gz linux   # must exit 1
#
# Usage: sh scripts/check-release-archive.sh <archive> <windows|macos|linux>
set -u

ARCHIVE=${1:-}
PLATFORM=${2:-}

if [ -z "$ARCHIVE" ] || [ -z "$PLATFORM" ]; then
  echo "usage: $0 <archive> <windows|macos|linux>" >&2
  exit 2
fi
if [ ! -f "$ARCHIVE" ]; then
  echo "::error::Archive not found: $ARCHIVE" >&2
  exit 1
fi

# The release binary alone is an order of magnitude larger than this floor, so
# anything under it is a missing or truncated payload.
MIN_BYTES=1000000
SIZE=$(wc -c < "$ARCHIVE" | tr -d ' ')
if [ "$SIZE" -lt "$MIN_BYTES" ]; then
  echo "::error::Archive is $SIZE bytes, below the ${MIN_BYTES} byte floor: $ARCHIVE" >&2
  exit 1
fi

LISTING=$(mktemp)
trap 'rm -f "$LISTING"' EXIT
case "$ARCHIVE" in
  *.zip) unzip -l "$ARCHIVE" > "$LISTING" ;;
  *.tar.gz) tar -tzf "$ARCHIVE" > "$LISTING" ;;
  *)
    echo "::error::Unrecognised archive type (expected .zip or .tar.gz): $ARCHIVE" >&2
    exit 1
    ;;
esac

for required in LICENSE NOTICE THIRD_PARTY.md; do
  if ! grep -q "$required" "$LISTING"; then
    echo "::error::Archive is missing the required notice file: $required" >&2
    exit 1
  fi
done

# The executable name and its path inside the archive differ per platform.
case "$PLATFORM" in
  windows) BINARY=photrez.exe ;;
  macos|linux) BINARY=photrez ;;
  *)
    echo "::error::Unknown platform: $PLATFORM" >&2
    exit 2
    ;;
esac
if ! grep -q "$BINARY" "$LISTING"; then
  echo "::error::Archive is missing the release binary: $BINARY" >&2
  exit 1
fi

echo "release archive ok: $ARCHIVE ($SIZE bytes, $PLATFORM)"
