#!/bin/sh
# Guard: public artifacts (commit/PR messages, source files) must NEVER leak
# internal plan/milestone terminology that lives in gitignored docs
# (docs/plans/, docs/AI_*.md, RESPONSE.md) — e.g. "Phase A–H", "Phase E pilot",
# "Option A/B", "flip-on". Use a product/technical description instead.
#
# This is the SINGLE source of truth, called by BOTH the pre-commit hook and CI
# (GitHub Actions) so the rule cannot diverge. OSS convention: machine-enforced,
# not memory.
#
# Usage:
#   scripts/check-public-terminology.sh --msg "<commit / PR title>"
#   scripts/check-public-terminology.sh --files "<path> [path ...]"
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

fail() { echo "ERROR: $1" >&2; exit 1; }

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
      for f in $1; do
        [ -f "$f" ] || continue
        grep -nE "$PLAN_SRC" "$f" && fail "internal plan terminology in $f"
      done
      shift
      ;;
    *)
      fail "unknown arg: $1"
      ;;
  esac
done

echo "check-public-terminology: OK"
exit 0
