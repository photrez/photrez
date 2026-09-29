/*
 * Removes the harness-written pixel flag from the dev origin's localStorage.
 *
 * Who writes localStorage["photrez.rustPixels"]="1" today:
 *   - tests/validation/bitmap-sync.mjs and tests/validation/restore-sync.mjs,
 *     because every assertion in them is about what the rust pixel path does.
 *     They set it at the start of the run and now call this helper from finish()
 *     on every exit path, including a thrown error.
 *   - PHOTREZ_FLAGS, the harness env var the three live runners (live-verify,
 *     perf-audit-live, ipc-frequency-live) map to localStorage entries at
 *     start-up. It could write this key ("rustPixels=1" in it), and two of the
 *     three runners never cleared it, so one leaked env var armed every later
 *     run. parseHarnessFlags below now refuses the key and reports the refusal
 *     on stderr: the enablement is set by a test that also removes it, never by
 *     a run that can leave the dev profile armed.
 *
 * WebView2 persists that origin's localStorage on disk, so a harness that sets
 * the key and does not remove it leaves the dev profile armed for the next run
 * and for any later read of the flag.
 *
 * Call it from the harness finally block BEFORE the app process is killed: the
 * page must still be alive. Every failure is reported on stderr and never
 * re-thrown, so a cleanup problem can never replace the run result that is
 * already in flight. Only this one key is touched - no other key, no other
 * origin.
 */

export const RUST_PIXELS_FLAG = "photrez.rustPixels";

const warn = (msg) => console.error(`WARN: ${msg}`);

// PHOTREZ_FLAGS -> the localStorage entries a live runner seeds before boot.
// Every runner shares this one parser, so the reserved key is refused in a
// single place instead of at three call sites that could drift apart. The
// refusal is reported, never silent: an operator who asked for rustPixels=1
// must see that the request was dropped, or the run silently measures the
// non-Rust pixel path and the numbers look plausible.
export function parseHarnessFlags(raw) {
  return String(raw || "")
    .split(/[\s,]+/)
    .map((s) => s.trim())
    .filter(Boolean)
    .flatMap((kv) => {
      const i = kv.indexOf("=");
      if (i < 0) return [];
      const k = kv.slice(0, i).trim();
      const v = kv.slice(i + 1).trim();
      if (!k) return [];
      const key = `photrez.${k}`;
      if (key === RUST_PIXELS_FLAG) {
        warn(
          `refused reserved harness flag "${key}=${v}" from PHOTREZ_FLAGS; ` +
            `that key is enablement-only and is not settable from a harness run. Ignored.`,
        );
        return [];
      }
      return [{ key, value: v }];
    });
}

// Evaluated in the page over CDP. Returns null while the key is absent, the
// leftover value if the removal did not take, or "remove-threw" when the page
// rejected removeItem itself.
const CLEAR_EXPR = `(() => {
  try { localStorage.removeItem(${JSON.stringify(RUST_PIXELS_FLAG)}); }
  catch (e) { return "remove-threw"; }
  return localStorage.getItem(${JSON.stringify(RUST_PIXELS_FLAG)});
})()`;

export async function clearHarnessFlag(cdp) {
  if (!cdp) {
    warn(`no CDP session; localStorage["${RUST_PIXELS_FLAG}"] may still be set on the dev origin.`);
    return false;
  }
  let left;
  try {
    left = await cdp.evaluate(CLEAR_EXPR);
  } catch (e) {
    warn(`could not clear localStorage["${RUST_PIXELS_FLAG}"] (reported, not raised): ${e?.message || e}`);
    return false;
  }
  if (left === "remove-threw") {
    warn(`removeItem threw in the page; localStorage["${RUST_PIXELS_FLAG}"] may still be set.`);
    return false;
  }
  if (left !== null && left !== undefined) {
    warn(`localStorage["${RUST_PIXELS_FLAG}"] still set after cleanup (value ${JSON.stringify(String(left))}).`);
    return false;
  }
  return true;
}
