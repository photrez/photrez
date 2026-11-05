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
#   cp dist/photrez.exe dist/photrez-desktop.exe
#   sh scripts/check-release-archive.sh ok.tar.gz windows     # must exit 1
#
# Usage: sh scripts/check-release-archive.sh <archive> <windows|macos|linux>
#        sh scripts/check-release-archive.sh --binary <path> <platform>
#        sh scripts/check-release-archive.sh --imports <listing> <platform>
set -u

# Second mode: inspect the shipped binary's dynamic imports for a runtime this
# repository no longer contains. The archive checks above cannot see this case:
# a binary can carry the licence set, clear the size floor and still link an
# engine the product does not use, which is what a person who installs the
# release would receive. A captured listing is accepted so the macOS and Linux
# answers can be checked on any machine, and so the refusal rule below is
# provable without those hosts.
#
#   sh scripts/check-release-archive.sh --binary target/release/photrez windows
#   sh scripts/check-release-archive.sh --imports photrez-macos-imports.txt macos
case "${1:-}" in
  --binary)
    BIN=${2:-}
    PLATFORM=${3:-}
    [ -n "$BIN" ] && [ -n "$PLATFORM" ] || { echo "usage: $0 --binary <path> <windows|macos|linux>" >&2; exit 2; }
    [ -f "$BIN" ] || { echo "::error::Binary not found: $BIN" >&2; exit 1; }
    LIST=$(mktemp)
    trap 'rm -f "$LIST"' EXIT
    case "$PLATFORM" in
      windows)
        TOOL=$(command -v objdump || true)
        [ -n "$TOOL" ] || { echo "objdump is not on this machine, so the import listing did not run" >&2; exit 2; }
        "$TOOL" -p "$BIN" > "$LIST" || { echo "::error::objdump failed on $BIN" >&2; exit 1; }
        ;;
      macos)
        TOOL=$(command -v otool || true)
        [ -n "$TOOL" ] || { echo "otool is only available on macOS, so the import listing did not run" >&2; exit 2; }
        "$TOOL" -L "$BIN" > "$LIST" || { echo "::error::otool failed on $BIN" >&2; exit 1; }
        ;;
      linux)
        TOOL=$(command -v ldd || true)
        [ -n "$TOOL" ] || { echo "ldd is only available on Linux, so the import listing did not run" >&2; exit 2; }
        "$TOOL" "$BIN" > "$LIST" || { echo "::error::ldd failed on $BIN" >&2; exit 1; }
        ;;
      *) echo "::error::Unknown platform: $PLATFORM" >&2; exit 2 ;;
    esac
    BINARY_LABEL=$BIN
    ;;
  --imports)
    LIST_ARG=${2:-}
    PLATFORM=${3:-}
    [ -n "$LIST_ARG" ] && [ -n "$PLATFORM" ] || { echo "usage: $0 --imports <listing> <windows|macos|linux>" >&2; exit 2; }
    [ -f "$LIST_ARG" ] || { echo "::error::Import listing not found: $LIST_ARG" >&2; exit 1; }
    LIST=$LIST_ARG
    BINARY_LABEL=$LIST_ARG
    ;;
  *) ;;
esac

if [ "${1:-}" = "--binary" ] || [ "${1:-}" = "--imports" ]; then
  # The guard polices these three lines because they name the retired shell's
  # libraries; the refusal below is the reason they exist.
  case "$PLATFORM" in
    windows) RETIRED='webview2loader|webkit|tauri|wry' ;;  # trace-exempt: library names this guard refuses in a shipped binary
    macos) RETIRED='webkit|webview|tauri|wry' ;;           # trace-exempt: library names this guard refuses in a shipped binary
    linux) RETIRED='webkit2gtk|webkitgtk|javascriptcoregtk|appindicator|tauri|wry' ;;  # trace-exempt: library names this guard refuses in a shipped binary
  esac
  HITS=$(grep -iE "$RETIRED" "$LIST" || true)
  if [ -n "$HITS" ]; then
    printf '%s\n' "$HITS" >&2
    echo "::error::$BINARY_LABEL links a runtime this repository no longer contains (pattern: $RETIRED)" >&2
    exit 1
  fi
  # A scan over an empty input is not a pass. If nothing that looks like a
  # library was read, the listing came from the wrong tool, the wrong file type
  # or a stripped binary, and "no retired runtime found" would describe the
  # input rather than the payload.
  case "$PLATFORM" in
    windows) LIBRARY_MARKER='\.(dll|ocx)' ;;
    macos) LIBRARY_MARKER='\.(dylib|framework)' ;;
    linux) LIBRARY_MARKER='\.so|statically linked' ;;
  esac
  SEEN=$(grep -icE "$LIBRARY_MARKER" "$LIST" || true)
  if [ "${SEEN:-0}" -lt 1 ]; then
    echo "::error::no libraries parsed from $BINARY_LABEL (expected a line matching $LIBRARY_MARKER), so this scan cannot report a pass" >&2
    exit 2
  fi
  echo "imports ok: $BINARY_LABEL carries none of the retired runtime libraries ($PLATFORM, $SEEN library lines read)"
  exit 0
fi

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

# The archive carries the application and nothing else. A second runtime in the
# staging directory conveys a stale engine to the person who installs it, and it
# is invisible in CI because everything else still passes, so it is refused here
# by name rather than left to the size floor.
for retired in photrez-desktop; do
  if grep -q "$retired" "$LISTING"; then
    echo "::error::Archive carries a retired runtime artifact: $retired" >&2
    exit 1
  fi
done

# A browser runtime reaches a user as files, not as a link, so the payload rule
# needs both halves: the library scan looks at what the binary asks for, and
# this loop looks at what sits next to it in the archive. Photrez ships a native
# binary plus three text files, so any of these is a second renderer travelling
# with it.
for payload in '\.html$' '\.htm$' '\.js$' '\.css$' '\.wasm$'; do
  HITS=$(grep -E "$payload" "$LISTING" || true)
  if [ -n "$HITS" ]; then
    printf '%s\n' "$HITS" >&2
    echo "::error::Archive carries a browser runtime payload (pattern: $payload): $ARCHIVE" >&2
    exit 1
  fi
done

echo "release archive ok: $ARCHIVE ($SIZE bytes, $PLATFORM)"
