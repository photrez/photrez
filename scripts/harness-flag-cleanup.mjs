/*
 * Removes the harness-written pixel flag from the dev origin's localStorage.
 *
 * The live verification harness sets localStorage["photrez.rustPixels"]="1"
 * in-page so its fill step routes through the rust pixel path. WebView2 persists
 * that origin's localStorage on disk, so a run that never removes the key leaves
 * it set for the next run and for any later read of the flag.
 *
 * Call it from the harness finally block BEFORE the app process is killed: the
 * page must still be alive. Every failure is reported on stderr and never
 * re-thrown, so a cleanup problem can never replace the run result that is
 * already in flight. Only this one key is touched - no other key, no other
 * origin.
 */

export const RUST_PIXELS_FLAG = "photrez.rustPixels";

// Evaluated in the page over CDP. Returns null while the key is absent, the
// leftover value if the removal did not take, or "remove-threw" when the page
// rejected removeItem itself.
const CLEAR_EXPR = `(() => {
  try { localStorage.removeItem(${JSON.stringify(RUST_PIXELS_FLAG)}); }
  catch (e) { return "remove-threw"; }
  return localStorage.getItem(${JSON.stringify(RUST_PIXELS_FLAG)});
})()`;

export async function clearHarnessFlag(cdp) {
  const warn = (msg) => console.error(`WARN: ${msg}`);
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
