#!/bin/sh
# File-size ratchet for the Rust trees.
#
# Thresholds: 500 lines for a non-test module, 1000 for a test module. A file
# that is already above its threshold is grandfathered in
# scripts/rust-file-size-baseline.txt at its recorded size and may not grow
# past that size. Any other oversized file fails. The ratchet only moves down:
# shrinking is always allowed, growing a grandfathered file never is.
#
# Test module = path contains a tests/ directory or the file is named tests.rs.
set -u

root=$(git rev-parse --show-toplevel 2>/dev/null) || {
    echo "check-rust-file-size: not inside a git work tree" >&2
    exit 1
}
cd "$root" || exit 1

baseline="scripts/rust-file-size-baseline.txt"

recorded_for() {
    # $1 = path -> recorded line count, or empty when not grandfathered
    [ -f "$baseline" ] || return 0
    awk -v p="$1" '!/^#/ && NF==2 && $2==p { print $1; exit }' "$baseline"
}

files=$(find crates apps -name '*.rs' -not -path '*/target/*' 2>/dev/null)
if [ -z "$files" ]; then
    echo "check-rust-file-size: no Rust sources found under crates/ or apps/" >&2
    exit 1
fi

fail=0
old_ifs=$IFS
IFS='
'
for f in $files; do
    IFS=$old_ifs
    lines=$(wc -l <"$f" | tr -d '[:space:]')
    case "$f" in
    */tests/* | tests/* | */tests.rs | tests.rs) cap=1000 ;;
    *) cap=500 ;;
    esac
    base=$(recorded_for "$f")
    if [ -n "$base" ]; then
        if [ "$lines" -gt "$base" ]; then
            echo "FAIL (ratchet): $f grew to $lines lines (recorded $base)"
            fail=1
        fi
    elif [ "$lines" -gt "$cap" ]; then
        echo "FAIL (> cap): $f is $lines lines (cap $cap)"
        fail=1
    fi
    IFS='
'
done
IFS=$old_ifs

if [ "$fail" -eq 0 ]; then
    echo "check-rust-file-size: OK (caps 500/1000, baseline ratchet honored)"
fi
exit "$fail"
