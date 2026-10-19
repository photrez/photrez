#!/bin/sh
# Guard: public artifacts (commit/PR messages, source files, public markdown) must
# NEVER leak internal plan/milestone terminology that lives in gitignored docs
# (docs/plans/, docs/AI_*.md, RESPONSE.md) - e.g. "Phase A-H", "Phase E pilot",
# "Option A/B", "flip-on". Use a product/technical description instead.
#
# SCOPE - what this guard is and is NOT:
#   IN scope: internal planning/milestone codenames and references to
#   untracked internal files. A public commit message or source comment must
#   not point a reader at a file that does not exist in a fresh clone, nor
#   name an unreleased internal milestone as if it were public vocabulary.
#   OUT of scope: authorship, tooling attribution, and provenance. This guard
#   does not police how code was produced; do not extend these patterns to
#   block tool or author names. Its job is keeping internal bookkeeping out of
#   public artifacts, nothing more.
#
# MARKDOWN SCOPE (2026-10-17): the --files check also covers the tracked public
# markdown set (README / CHANGELOG / CONTRIBUTING / SECURITY, docs/**, .github/**).
# Internal-facing markdown (docs/plans/**, docs/decisions/**) is never scanned:
# internal documents may say "Phase 2"; public ones may not.
#
# This is the SINGLE source of truth, called by BOTH the pre-commit hook and CI
# (GitHub Actions) so the rule cannot diverge. OSS convention: machine-enforced,
# not memory.
#
# Usage:
#   scripts/check-public-terminology.sh --msg "<commit / PR title>"
#   scripts/check-public-terminology.sh --files "<path> [path ...]"
#   scripts/check-public-terminology.sh --i18n --files "<path-or-dir> [path-or-dir ...]"
#   scripts/check-public-terminology.sh --staleness [--simulate-absent <stack>]
#
# "--i18n" flags a string literal used at a text-bearing call site in the
# Rust sources it is pointed at (`components/`, `views/`): user-visible text
# must come from the message catalog (`t("key")`), never a bare literal. It
# skips in-file `mod tests` regions (test fixtures may use literals) and
# accepts a line that carries `// i18n-exempt: <reason>` when a literal is
# deliberately not translated (a shortcut chord, a numeric value, a product
# name). The call-site list is intentionally narrow - the coarse net catches
# the direct forms; text that reaches the user through a variable is the
# reviewer's catch.
#
# "--staleness" checks that no tracked public surface still asserts a runtime
# stack the tree no longer contains. The token/rule list lives in
# scripts/public-stack-tokens.txt (single source of truth; referenced by path,
# never duplicated). "--simulate-absent <stack>" pretends the stack is gone and
# is how the check is proven red on a live tree.
#
# "Phase A:" / "Phase B:" used as in-method step labels (e.g. useBrushOverlay
# readback steps) are ALLOWED - the plan-phase reference form is what's blocked.
# Extended 2026-09-07: also blocks internal decision labels (SD-1..), "fork A/B/C",
# "prove-dead", "Keranjang", "North Star" - all absent from existing shipped code, so
# this adds no false positives. ADR references are NOT blocked (established convention,
# 39 existing uses).
set -u

PLAN_MSG='Phase [A-H]|phase [a-h]|flip-on|Option A[^A-Za-z]|Option B[^A-Za-z]|SD-[0-9]|fork [ABC]|prove-dead|Keranjang|North Star'
PLAN_SRC='Phase [A-H]([ -]?(pilot|slice|migration|increment|bundle|bridge|transfer|adapter))|SD-[0-9]|fork [ABC]|prove-dead|Keranjang|North Star|RESPONSE\.md|AI_HISTORY|AI_CURRENT_TASK|AI_CONTEXT|docs/plans|photrez-(counter|coder|reviewer|verifier|orchestrator)|pre-Wave|Wave [0-9]'
# Milestone vocabulary that is internal-only when it appears in PUBLIC markdown:
# workstream codenames, named-review labels and phase-numbered work labels are
# internal bookkeeping. Public docs describe what the product does instead.
PLAN_MD='pilot|FAANG|Technique [A-C]|slice|Phase [0-9]'
TOKEN_LIST='scripts/public-stack-tokens.txt'

# Text-bearing call sites in the Rust UI sources: a string literal in one of
# these positions is user-visible text. A const `&str` counts too, unless its
# value is key-shaped (all lowercase letters/digits/dots/dashes - a catalog
# key or an id, not UI text). A literal that reaches the user another way is
# review's catch, and the exemption marker is how review records its verdict.
I18N_CALL_PATTERN='(menu_row|text_run|caption|value_mono|dialog_button|resize_row|slider_row)\(r?"[^"]|\.child\(r?"[^"]|(text|label|title|name|heading|tooltip|hint|description|placeholder|caption): r?"[^"]|const [A-Z0-9_]+: &str = r?"[^"]'
I18N_KEY_SHAPED_CONST='const [A-Z0-9_]+: &str = r?"[a-z0-9_.-]+"'

STALENESS=0
SIMULATE_ABSENT=''
I18N=0
FILES=''

fail() { echo "ERROR: $1" >&2; exit 1; }

# --- staleness -----------------------------------------------------------------
# Token list format (TAB-separated records; '#' lines are comments):
#   !present <stack> <command>   exit 0 => the stack is still in the tree
#   <token> <globs> <mode>       globs = space-separated git pathspecs
#   mode: forbidden-when-absent:<stack>  hits red once <stack> is absent
#         allowlist                      dated/historical file, exempt + reason above
#         url-only                       allowed inside a URL (a citation needs its
#                                        address); red in prose or link labels
stack_present() { # $1 = stack
  [ "$SIMULATE_ABSENT" = "$1" ] && return 1
  cmd=$(awk -F'\t' -v s="$1" '$1=="!present" && $2==s {print $3; exit}' "$TOKEN_LIST")
  [ -n "$cmd" ] || fail "no !present record for stack: $1"
  sh -c "$cmd"
}

report() { # $1 = hits, $2 = error line
  printf '%s\n' "$1"
  printf 'ERROR: %s\n' "$2" >&2
  rc=1
}

staleness() {
  [ -f "$TOKEN_LIST" ] || fail "token list not found: $TOKEN_LIST"
  rc=0

  tokens=$(awk -F'\t' '!/^#/ && $1!="!present" && $3 ~ /^forbidden-when-absent:/ {print $1}' "$TOKEN_LIST" | sort -u)
  for token in $tokens; do
    stack=$(awk -F'\t' -v t="$token" '$1==t && $3 ~ /^forbidden-when-absent:/ {sub(/^forbidden-when-absent:/,"",$3); print $3; exit}' "$TOKEN_LIST")
    stack_present "$stack" && continue   # stack still shipped -> the claims are true
    globs=$(awk -F'\t' -v t="$token" '$1==t && $3 ~ /^forbidden-when-absent:/ {print $2}' "$TOKEN_LIST")
    for af in $(awk -F'\t' -v t="$token" '$1==t && $3=="allowlist" {print $2}' "$TOKEN_LIST"); do
      globs="$globs :(exclude)$af"
    done
    hits=$(git grep -n -I -iE "\\b$token" -- $globs || true)
    [ -n "$hits" ] && report "$hits" "public surface still asserts a removed stack (token \"$token\", stack \"$stack\")"
  done

  utokens=$(awk -F'\t' '!/^#/ && $1!="!present" && $3=="url-only" {print $1}' "$TOKEN_LIST" | sort -u)
  for token in $utokens; do
    globs=$(awk -F'\t' -v t="$token" '$1==t && $3=="url-only" {print $2}' "$TOKEN_LIST")
    hits=$(git grep -n -I -iE "\\b$token" -- $globs || true)
    [ -n "$hits" ] || continue
    bad=$(printf '%s\n' "$hits" | while IFS= read -r line; do
      printf '%s\n' "$line" | sed 's#https\?://[^ )">]*##g' | grep -qiE "\\b$token" && printf '%s\n' "$line"
    done)
    [ -n "$bad" ] && report "$bad" "brand token outside a citation URL (token \"$token\" is url-only)"
  done

  [ "$rc" -eq 0 ] || exit 1
}

# --- files (terminology scan) --------------------------------------------------
files_check() {
  for f in $FILES; do
    [ -f "$f" ] || continue
    case "$f" in
      *.ts|*.tsx|*.rs)
        grep -nE "$PLAN_SRC" "$f" && fail "internal plan terminology in $f"
        ;;
      *.md)
        case "$f" in
          README.md|CHANGELOG.md|CONTRIBUTING.md|SECURITY.md|docs/*|.github/*)
            grep -nE "$PLAN_SRC" "$f" && fail "internal plan terminology in $f"
            grep -niE "$PLAN_MD" "$f" && fail "internal milestone vocabulary in public doc $f"
            ;;
        esac
        ;;
    esac
  done
}

# --- i18n (hardcoded user-visible strings) -------------------------------------
# Prints file:line:<source line> for every hit. Lines inside an in-file
# `mod tests` block and lines carrying `i18n-exempt:` are skipped; a directory
# argument is walked for *.rs.
i18n_check() {
  found=''
  for f in $FILES; do
    if [ -d "$f" ]; then
      sources=$(find "$f" -name '*.rs' | sort)
    elif [ -f "$f" ]; then
      case "$f" in *.rs) sources="$f";; *) sources='';; esac
    else
      continue
    fi
    for src in $sources; do
      # awk skips in-file `mod tests` regions and stamps every surviving line
      # with its real line number; grep -E then matches the call shapes (its
      # ERE dialect treats \( and \. literally, which this awk's does not).
      hits=$(awk '
        /^mod tests/ { in_test = 1; next }
        in_test && /^}/ { in_test = 0; next }
        in_test { next }
        { printf "%d:%s\n", NR, $0 }
      ' "$src" | grep -E "$I18N_CALL_PATTERN" | grep -vE "$I18N_KEY_SHAPED_CONST" | grep -v 'i18n-exempt:' || true)
      [ -n "$hits" ] || continue
      printf '%s\n' "$hits" | while IFS= read -r hit; do
        printf '%s:%s\n' "$src" "$hit"
      done
      found=yes
    done
  done
  [ -z "$found" ] || fail 'hardcoded user-visible string at a text-bearing call site (wrap it in the message catalog: t("key")); a literal that is deliberately not translated carries // i18n-exempt: <reason>'
}

while [ $# -gt 0 ]; do
  case "$1" in
    --msg)
      shift
      [ $# -lt 1 ] && fail "--msg requires a value"
      printf '%s' "$1" | grep -qiE "$PLAN_MSG" && fail "internal plan terminology in message: $1"
      shift
      ;;
    --files)
      shift
      [ $# -lt 1 ] && fail "--files requires a value"
      # Consumes every following non-flag word, so both forms work:
      # `--files a b` and the hook's `--files "a b"`.
      while [ $# -gt 0 ]; do
        case "$1" in --*) break;; esac
        FILES="$FILES $1"
        shift
      done
      ;;
    --i18n)
      I18N=1
      shift
      ;;
    --staleness)
      STALENESS=1
      shift
      ;;
    --simulate-absent)
      shift
      [ $# -lt 1 ] && fail "--simulate-absent requires a value"
      SIMULATE_ABSENT="$1"
      shift
      ;;
    *)
      fail "unknown arg: $1"
      ;;
  esac
done

if [ "$STALENESS" -eq 1 ]; then
  staleness
  echo "check-public-terminology: staleness OK"
  exit 0
fi

if [ "$I18N" -eq 1 ]; then
  [ -n "$FILES" ] || fail "--i18n requires --files <path-or-dir> [path-or-dir ...]"
  i18n_check
  echo "check-public-terminology: i18n OK"
  exit 0
fi

[ -n "$FILES" ] && files_check

echo "check-public-terminology: OK"
exit 0
