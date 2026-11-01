#!/bin/sh
# File-size ratchet for the Rust trees.
#
# User-selected local policy: active non-baseline Rust sources have a 1000-line
# hard cap and warnings from 900 lines; warnings do not fail the check. Existing
# baseline entries receive ratchet-only enforcement. The baseline records the
# native sources that were already over the cap when it was set; splitting them
# is the obligation, and no entry may grow.
#
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

files=$(find crates -name '*.rs' -not -path '*/target/*' 2>/dev/null)
if [ -z "$files" ]; then
    echo "check-rust-file-size: no Rust sources found under crates/ or apps/" >&2
    exit 1
fi

hard_cap=1000
warning_at=900
fail=0
warnings=0
old_ifs=$IFS
IFS='
'
for f in $files; do
    IFS=$old_ifs
    lines=$(wc -l <"$f" | tr -d '[:space:]')
    base=$(recorded_for "$f")
    if [ -n "$base" ]; then
        if [ "$lines" -gt "$base" ]; then
            echo "FAIL (ratchet): $f grew to $lines lines (recorded $base)"
            fail=1
        fi
    elif [ "$lines" -gt "$hard_cap" ]; then
        echo "FAIL (> hard cap): $f is $lines lines (hard cap $hard_cap)"
        fail=1
    elif [ "$lines" -ge "$warning_at" ]; then
        echo "WARN (near cap): $f is $lines lines (warning starts at $warning_at; hard cap $hard_cap)"
        warnings=$((warnings + 1))
    fi
    IFS='
'
done
IFS=$old_ifs

if [ "$fail" -eq 0 ]; then
    echo "check-rust-file-size: PASS (non-baseline hard cap $hard_cap; warning threshold $warning_at; warnings $warnings; grandfathered baseline entries held at their recorded size)"
else
    echo "check-rust-file-size: FAIL (non-baseline hard cap $hard_cap or baseline ratchet; warning threshold $warning_at; warnings $warnings; a grandfathered file grew past its recorded size)"
fi
exit "$fail"
