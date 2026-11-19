#!/bin/sh
# Guard: each package keeps the one function its name states, so a package must
# not depend on a workspace peer outside its allowed edge set. The graph is the
# contract: `photrez-document` is a leaf, `photrez-core` is the only package that
# may see every lower package, and the application imports the engine through
# `photrez-core` alone.
#
# Usage: sh scripts/check-package-boundaries.sh
# Exit 0 when every package's direct normal (non-dev) edges are allowed.
#
# `--prefix depth` is deliberate: the leading digit is the tree depth, and depth
# 1 is a direct dependency. The default prefix draws the tree with box-drawing
# characters, and `none` drops the depth information entirely, so neither tells
# a direct edge from a transitive one. The allowed sets below are direct edges.

set -u

# One line per package: "<package> <allowed direct dependencies>".
# `none` means the package may depend on no other package in this workspace.
ALLOWED='photrez-document none
photrez-raster none
photrez-format photrez-document
photrez-render photrez-document photrez-raster
photrez-core photrez-document photrez-format photrez-raster photrez-render
photrez photrez-core
xtask none'

allowed_for() {
  printf '%s\n' "$ALLOWED" | sed -n "s|^$1 ||p"
}

fail() {
  printf 'ERROR: %s\n' "$1" >&2
  exit 1
}

command -v cargo >/dev/null 2>&1 || fail "cargo is not on PATH"

status=0
printf '%s\n' "$ALLOWED" | while read -r package rest; do
  [ -n "${package:-}" ] || continue
  allowed="$rest"

  tree=$(cargo tree -p "$package" --edges normal --prefix depth 2>/dev/null) ||
    fail "cargo tree failed for $package (is the package name right?)"

  [ -n "$tree" ] || fail "cargo tree printed nothing for $package"

  # Depth-1 lines are this package's direct dependencies.
  internal=$(printf '%s\n' "$tree" |
    sed -n 's/^1\([A-Za-z][A-Za-z0-9_-]*\) .*/\1/p' |
    grep -E '^(photrez|photrez-[a-z0-9]+|xtask)$' |
    grep -v "^${package}$" |
    sort -u || true)

  for dep in $internal; do
    case " $allowed " in
      *" $dep "*) ;;
      *)
        printf 'ERROR: %s may depend directly on [%s], found %s\n' \
          "$package" "$allowed" "$dep" >&2
        exit 1
        ;;
    esac
  done

  printf 'ok  %-18s direct internal edges: %s\n' "$package" "${internal:-none}"
done || status=1

exit "$status"
