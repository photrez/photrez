#!/usr/bin/env bun
/*
 * frame-pacing-live.mjs - frame-to-frame pacing measurement in the REAL app.
 *
 * WHY THIS EXISTS
 * ---------------
 * "Large-document interaction free of 33ms-class jank" is a frame-PACING
 * claim: it is about the interval between presented frames, not about how long
 * a render callback takes. The app's own instrument times only the synchronous
 * render callback (renderer/scheduler.ts:28-32), which excludes GPU present,
 * texture upload, the Tauri IPC hop, and vsync. That metric cannot see a
 * dropped frame. It is also gated behind `import.meta.env.DEV`
 * (components/editor/shell/BottomStatusBar.tsx:51), so it does not exist in a
 * release build at all.
 *
 * A jsdom or vitest substitute is NOT valid here: jsdom has no compositor and
 * no GPU, and its requestAnimationFrame is timer-driven, so a jsdom "frame
 * time" is a fiction. This script therefore drives the real WebView2 window.
 *
 * HOW IT WORKS
 * ------------
 *   1. Attach to an ALREADY RUNNING app over the Chrome DevTools Protocol.
 *   2. Create a document of a given size through the real New Document dialog
 *      (real clicks, real typed keystrokes).
 *   3. Select the brush through the real tool rail.
 *   4. Drive real brush strokes with CDP `Input.dispatchMouseEvent`, which goes
 *      through the browser input pipeline and produces genuine pointer events
 *      for the production dispatcher. No direct function calls.
 *   5. Sample real requestAnimationFrame timestamps across the strokes and
 *      report the frame-to-frame delta distribution.
 *
 * LAUNCHING THE APP (this script does not launch it)
 * --------------------------------------------------
 * The app must already be running with a CDP debug port:
 *
 *   # release profile (optimised Rust + built frontend)
 *   bun run build
 *   bun run tauri build --no-bundle
 *   $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9225'
 *   Start-Process target/release/photrez-desktop.exe
 *
 *   # dev profile (vite dev server + unoptimised Rust)
 *   bun run --filter photrez-desktop dev        # serves :1420
 *   $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9224'
 *   Start-Process target/debug/photrez-desktop.exe
 *
 * DO NOT use `cargo build --release -p photrez-desktop` for this. That produces
 * an exe which runs but has NO EMBEDDED FRONTEND: the webview falls back to the
 * devUrl (http://localhost:1420), so the window comes up with no editor chrome.
 * Such a binary is not measurable, and a harness that does not check will
 * happily "measure" a blank page. This script refuses to run unless the app is
 * serving http://tauri.localhost/ and its chrome is present.
 *
 * This workspace builds to the REPO-ROOT `target/` directory, not to
 * `apps/desktop/src-tauri/target/`.
 *
 * TWO GUARDS THAT COST REAL TIME TO LEARN - DO NOT REINTRODUCE THE MISTAKES
 * ------------------------------------------------------------------------
 * 1. A bare `/json` poll CANNOT tell "the app is still loading" from "the app
 *    is broken". An earlier run of scripts/perf-audit-live.mjs reported
 *    `FAIL [ready]: app document never reached readyState complete` against a
 *    perfectly healthy app, because that harness hardcodes a 60s ready budget
 *    (scripts/perf-audit-live.mjs:369) and a cold vite load on a loaded
 *    machine took ~100s. THIS SCRIPT DOES NOT LAUNCH THE APP, so it sidesteps
 *    the whole class of problem: it waits for a real document and reports
 *    "not ready yet, still loading" as a distinct outcome rather than a
 *    failure. If you ever extend this to launch the app, the ready budget must
 *    be derived from observation and must distinguish timeout from breakage.
 *
 *    Related, and the reason .tmp-harness/cdp-probe.sh is NOT a substitute:
 *    that script defaults to `apps/desktop/src-tauri/target/debug/...`, which
 *    does not exist in this workspace, and its 20s CDP wait is far under the
 *    real cold-load time. Both make it report a false BLOCKED. (That file is
 *    untracked debris owned elsewhere; it was read, never modified.)
 *
 * 2. `history.getUndoCount()` and `history.getHistoryStack()` are NOT commit
 *    oracles. History is owned by the Rust side, so the JS CommandHistory
 *    stays at 0 / length 1 while real brush commits land. A run that trusted
 *    it reported "0 commits" against strokes that were visibly painting.
 *    The Rust-owned probe `rust_pixels_history_depth(doc_id)` IS the real
 *    oracle, but it needs a doc id, which is only reachable through the dev
 *    handle `window.__photrezEditor` - absent in release builds, because
 *    shouldExposeEditorDebugHandle() returns false for MODE=production
 *    (components/editor/shell/EditorContext.tsx:82).
 *    => Under `--profile release`, commit count is reported as UNKNOWN.
 *       Paint is proven instead by a composited-pixel diff (see PAINT PROOF).
 *
 * THE REQUIREMENT IS NOT FALSIFIABLE AS WRITTEN
 * ---------------------------------------------
 * The clause "large-document interaction free of 33ms-class jank" specifies no
 * document size, no interaction, no percentile, no run count and no build
 * profile. Two runs of the same code differed (0 vs 8 dropped frames) purely on
 * whether the document had just been opened. It also presumes a 60Hz panel,
 * where every frame over 16.7ms is already a dropped frame - so "33ms-class"
 * really means "dropped at least one frame", a coarse binary.
 * Every one of those knobs is therefore an explicit, defaulted parameter below,
 * so that a run is reproducible and its scope is self-describing.
 *
 * NOT A HARD GATE
 * ---------------
 * Machine-dependent frame timing MUST NOT fail CI on a loaded machine; this box
 * alone showed a 1.7-2.1x run-to-run spread on identical settled code. The
 * default action is to REPORT. Pass --max-dropped to opt into a threshold.
 *
 * Env:
 *   FP_CDP_PORT    CDP port (default 9225)
 *   FP_PROFILE     label recorded in the output, e.g. "release" or "dev"
 *
 * Exit codes: 0 = captured, 1 = guard failed (no number reported), 2 = no target.
 */

const PORT = Number(process.env.FP_CDP_PORT || 9225);
const PROFILE = process.env.FP_PROFILE || "unknown";

function arg(name, dflt) {
  const i = process.argv.indexOf("--" + name);
  if (i === -1) return dflt;
  const v = process.argv[i + 1];
  return v === undefined || v.startsWith("--") ? true : v;
}
const DOC_W = Number(arg("doc-w", 4096));
const DOC_H = Number(arg("doc-h", 4096));
const STROKES = Number(arg("strokes", 12));
const MOVES = Number(arg("moves", 40));
const WARMUP = Number(arg("warmup", 3));
const COLD = arg("cold", false) === true;
const MAX_DROPPED = arg("max-dropped", 0);
const LABEL = String(arg("label", `${PROFILE}-${DOC_W}x${DOC_H}`));

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const log = (...a) => console.log(...a);

/* ----------------------------- CDP client ------------------------------ */

class Cdp {
  constructor(url) {
    this.url = url;
    this.nextId = 1;
    this.pending = new Map();
  }
  connect() {
    return new Promise((resolve, reject) => {
      const ws = new WebSocket(this.url);
      this.ws = ws;
      ws.addEventListener("open", () => resolve());
      ws.addEventListener("error", () => reject(new Error("cdp websocket error")));
      ws.addEventListener("message", (ev) => {
        const m = JSON.parse(ev.data);
        const p = this.pending.get(m.id);
        if (!p) return;
        this.pending.delete(m.id);
        if (m.error) p.rej(new Error(JSON.stringify(m.error)));
        else p.res(m.result);
      });
    });
  }
  send(method, params = {}, timeoutMs = 180000) {
    const id = this.nextId++;
    return new Promise((res, rej) => {
      this.pending.set(id, { res, rej });
      this.ws.send(JSON.stringify({ id, method, params }));
      setTimeout(() => {
        if (this.pending.has(id)) {
          this.pending.delete(id);
          rej(new Error("cdp timeout: " + method));
        }
      }, timeoutMs);
    });
  }
  async evaluate(expression, timeoutMs = 60000) {
    const r = await this.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true }, timeoutMs);
    if (r.exceptionDetails) {
      throw new Error(String(r.exceptionDetails.exception?.description || r.exceptionDetails.text).slice(0, 400));
    }
    return r.result?.value;
  }
  /** Real composited pixels. Works for WebGL canvases, unlike toDataURL. */
  async screenshot(clip) {
    const r = await this.send("Page.captureScreenshot", { format: "png", clip });
    return r.data;
  }
  close() {
    try {
      this.ws.close();
    } catch {}
  }
}

/* ------------------------------- helpers -------------------------------- */

let cdp = null;
function guard(stage, msg) {
  log(`GUARD FAILED [${stage}]: ${msg}`);
  log("No frame number is reported. Fix the precondition and re-run.");
  if (cdp) cdp.close();
  process.exit(1);
}

async function buttons() {
  return JSON.parse(
    await cdp.evaluate(`JSON.stringify([...document.querySelectorAll('button')]
      .map(b => { const r = b.getBoundingClientRect();
        return { label: b.getAttribute('aria-label') || b.getAttribute('title') || (b.textContent || '').trim().slice(0, 24),
                 disabled: !!b.disabled, x: Math.round(r.x + r.width / 2), y: Math.round(r.y + r.height / 2) }; })
      .filter(b => b.x > 0))`),
  );
}

async function click(x, y) {
  for (const type of ["mousePressed", "mouseReleased"]) {
    await cdp.send("Input.dispatchMouseEvent", {
      type, x, y, button: "left", clickCount: 1, buttons: type === "mousePressed" ? 1 : 0,
    });
  }
  await sleep(60);
}

/** Real click, select-all, then real per-character keystrokes. */
async function typeInto(x, y, text) {
  await click(x, y);
  await sleep(110);
  await cdp.send("Input.dispatchKeyEvent", { type: "keyDown", key: "a", code: "KeyA", modifiers: 2, windowsVirtualKeyCode: 65 });
  await cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key: "a", code: "KeyA", modifiers: 2, windowsVirtualKeyCode: 65 });
  await sleep(90);
  for (const ch of text) {
    await cdp.send("Input.dispatchKeyEvent", { type: "keyDown", text: ch, key: ch });
    await cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key: ch });
    await sleep(25);
  }
  await sleep(130);
}

/**
 * Document size WITHOUT the dev handle (release builds have none).
 *
 * MUST be scoped to the status bar <footer>. An unscoped document.body search
 * matches the New Document PRESET labels ("Instagram Post 1080 x 1080 px") and
 * reports a size for a document that is not even open - a false pass. The real
 * size lives in a span that only renders while a document is active
 * (BottomStatusBar.tsx:127-130), so a footer-scoped match is proof that a
 * document is open as well as proof of its size.
 */
const readStatusBarSize = () =>
  cdp
    .evaluate(`(() => {
      for (const f of document.querySelectorAll('footer')) {
        const m = (f.innerText || '').match(/(\\d[\\d.,]*)\\s*[\u00d7x]\\s*(\\d[\\d.,]*)\\s*px/i);
        if (m) return JSON.stringify({ w: parseInt(m[1].replace(/[.,]/g, ''), 10), h: parseInt(m[2].replace(/[.,]/g, ''), 10) });
      }
      return JSON.stringify({ w: 0, h: 0 });
    })()`)
    .then(JSON.parse);

/** Real signals when the dev handle exists; footer-scoped read otherwise. */
const docSize = async () => {
  if (hasHandle) {
    return JSON.parse(
      await cdp.evaluate(`(() => { const e = window.__photrezEditor;
        return JSON.stringify({ w: e.docWidth(), h: e.docHeight(), brush: e.brushSize(), tool: e.activeTool() }); })()`),
    );
  }
  return readStatusBarSize();
};

/* --------------------------- in-page instrumentation -------------------- */

const START_SAMPLER = `(() => {
  window.__fp = { ts: [], on: true };
  const step = (t) => { if (!window.__fp.on) return; window.__fp.ts.push(t); requestAnimationFrame(step); };
  requestAnimationFrame(step);
  return true;
})()`;

const READ_STATS = `(() => {
  window.__fp.on = false;
  const ts = window.__fp.ts;
  const d = [];
  for (let i = 1; i < ts.length; i++) d.push(ts[i] - ts[i - 1]);
  const s = d.slice().sort((a, b) => a - b);
  const q = (p) => (s.length ? s[Math.min(s.length - 1, Math.floor(p * s.length))] : -1);
  return JSON.stringify({
    frames: s.length,
    wallMs: +(ts.length ? ts[ts.length - 1] - ts[0] : 0).toFixed(1),
    min: +s[0].toFixed(2),
    median: +q(0.5).toFixed(2),
    p90: +q(0.9).toFixed(2),
    p95: +q(0.95).toFixed(2),
    p99: +q(0.99).toFixed(2),
    max: +s[s.length - 1].toFixed(2),
    dropped33: s.filter((x) => x >= 33).length,
    over50: s.filter((x) => x >= 50).length,
    over100: s.filter((x) => x >= 100).length,
    hidden: document.visibilityState,
  });
})()`;

/* --------------------------------- main --------------------------------- */

let targets = null;
try {
  targets = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
} catch (e) {
  log(`NO CDP ENDPOINT on 127.0.0.1:${PORT} (${e.message}).`);
  log("Start the app with WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=" + PORT + " first.");
  process.exit(2);
}
const page = (targets || []).find((t) => t.type === "page" && t.webSocketDebuggerUrl);
if (!page) {
  log("No page target with a debugger URL. The webview may still be starting, or the window never navigated.");
  process.exit(2);
}
cdp = new Cdp(page.webSocketDebuggerUrl);
await cdp.connect();
await cdp.send("Runtime.enable");
await cdp.send("Page.enable");

/* ---- PRECONDITIONS: refuse to measure a binary that cannot be measured ----
 *
 * A bare `cargo build --release -p photrez-desktop` produces an exe that RUNS
 * and looks fine, but with NO EMBEDDED FRONTEND: the webview falls back to the
 * devUrl and loads http://localhost:1420, which is normally not serving. The
 * window therefore comes up with no editor chrome at all. An earlier version of
 * this script then reported a clean-looking failure 9 times over ("New Document
 * button not found") without ever making it obvious that the BINARY, not the
 * pacing, was at fault - the exact green-but-false shape this harness exists to
 * prevent.
 *
 * A release binary is only measurable when it serves its own embedded assets,
 * i.e. it was produced through the Tauri CLI (`bun run tauri build`, or
 * `--no-bundle` to skip installers). Check both that, and that the editor
 * chrome is actually present, BEFORE reporting any number.
 */
if (!/tauri\.localhost/.test(String(page.url || ""))) {
  guard(
    "precondition",
    `app is not serving embedded assets (target=${page.url}). A bare \`cargo build --release\` yields an exe with no embedded frontend that falls back to the dev server; such a binary is not measurable. Build with \`bun run tauri build --no-bundle\`.`,
  );
}
const chromeButtons = Number(
  await cdp.evaluate(`document.querySelectorAll('button').length`),
);
if (chromeButtons < 20) {
  guard("precondition", `editor chrome is not rendered (only ${chromeButtons} buttons); the app did not load its UI.`);
}

const hasHandle = await cdp.evaluate(`typeof window.__photrezEditor === "object" && !!window.__photrezEditor`);
log(`profile=${PROFILE} target=${page.url} devHandle=${hasHandle}`);

let doc = await docSize();
log(`document at start: ${doc.w}x${doc.h}`);
if (COLD && (doc.w > 0 || doc.h > 0)) {
  guard("cold", `--cold requires no open document, but one is open (${doc.w}x${doc.h}). Relaunch the app.`);
}

/* ---- ensure a document of the requested size exists, via the real UI ---- */

const DOC_NAME = `FP${DOC_W}x${DOC_H}`;
let created = false;
if (!(doc.w === DOC_W && doc.h === DOC_H)) {
  const btns1 = await buttons();
  const newDoc = btns1.find((b) => /dokumen baru|new document|new canvas/i.test(b.label));
  if (!newDoc) guard("ui", "New Document button not found in the tool rail");
  await click(newDoc.x, newDoc.y);
  await sleep(900);

  // Scope to the dialog: an unscoped input[type=number] query also matches the
  // brush-size and opacity fields elsewhere in the app.
  const DLG = `[role=dialog] input[type=number]`;
  const fields = JSON.parse(
    await cdp.evaluate(`JSON.stringify([...document.querySelectorAll(${JSON.stringify(DLG)})]
      .map(el => { const r = el.getBoundingClientRect();
        return { val: el.value, x: Math.round(r.x + r.width / 2), y: Math.round(r.y + r.height / 2) }; })
      .filter(f => f.x > 0))`),
  );
  if (fields.length < 2) guard("ui", `expected 2 number inputs in the dialog, got ${JSON.stringify(fields)}`);

  // Name the document too, so the created tab can be positively identified.
  const nameField = JSON.parse(
    await cdp.evaluate(`JSON.stringify([...document.querySelectorAll('[role=dialog] input[type=text]')]
      .map(el => { const r = el.getBoundingClientRect();
        return { x: Math.round(r.x + r.width / 2), y: Math.round(r.y + r.height / 2) }; })
      .filter(f => f.x > 0))`),
  )[0];
  if (nameField) await typeInto(nameField.x, nameField.y, DOC_NAME);

  await typeInto(fields[0].x, fields[0].y, String(DOC_W));
  await typeInto(fields[1].x, fields[1].y, String(DOC_H));

  const typed = JSON.parse(
    await cdp.evaluate(`JSON.stringify([...document.querySelectorAll(${JSON.stringify(DLG)})].map((e) => e.value))`),
  );
  if (Number(typed[0]) !== DOC_W || Number(typed[1]) !== DOC_H) {
    guard("ui", `the dialog fields did not take the requested size: got ${JSON.stringify(typed)}`);
  }

  const btns2 = await buttons();
  const create = btns2.filter((b) => !b.disabled).find((b) => /^(buat|create|ok)$/i.test(b.label));
  if (!create) guard("ui", "Create button not found: " + btns2.map((b) => b.label).join("|"));
  await click(create.x, create.y);
  await sleep(1800);
  created = true;
}

doc = await docSize();
if (doc.w !== DOC_W || doc.h !== DOC_H) {
  guard("doc", `document is ${doc.w}x${doc.h}, wanted ${DOC_W}x${DOC_H} (created=${created})`);
}
/* Anti-stale guard: the document we just created must be the visible one. */
const nameVisible = await cdp.evaluate(
  `[...document.querySelectorAll('*')].some((e) => e.children.length === 0 && (e.textContent || '').trim() === ${JSON.stringify(DOC_NAME)})`,
);
if (created && !nameVisible) {
  guard("doc", `document ${DOC_NAME} is not present in the DOM; the size read may belong to another document`);
}

/* ---- brush tool -------------------------------------------------------- */

if (hasHandle) {
  if (doc.tool !== "brush") {
    const b = (await buttons()).find((x) => /kuas|brush/i.test(x.label));
    if (!b) guard("ui", "Brush tool button not found");
    await click(b.x, b.y);
    await sleep(600);
    doc = await docSize();
    if (doc.tool !== "brush") guard("ui", `brush did not activate (activeTool=${doc.tool})`);
  }
} else {
  const b = (await buttons()).find((x) => /kuas|brush/i.test(x.label));
  if (!b) guard("ui", "Brush tool button not found");
  await click(b.x, b.y);
  await sleep(600);
}

/* ---- geometry: the canvas that actually receives pointer input ---------- */

const geom = JSON.parse(
  await cdp.evaluate(`JSON.stringify((() => {
    const cs = [...document.querySelectorAll('canvas')]
      .map(c => { const r = c.getBoundingClientRect();
        return { bw: c.width, bh: c.height, cw: Math.round(r.width), ch: Math.round(r.height),
                 x: Math.round(r.x), y: Math.round(r.y) }; })
      .filter(c => c.cw > 50 && c.ch > 50);
    cs.sort((a, b) => (b.cw * b.ch) - (a.cw * a.ch));
    const c = cs[0];
    if (!c) return {};
    const hit = document.elementFromPoint(c.x + c.cw / 2, c.y + c.ch / 2);
    return Object.assign({}, c, { scale: 1, hit: hit ? hit.tagName : null });
  })())`),
);
if (!geom.bw) guard("canvas", "no canvas with a visible rect");
if (geom.hit !== "CANVAS") guard("canvas", `canvas centre hit-tests to ${geom.hit}, not CANVAS`);
log(`canvas backing=${geom.bw}x${geom.bh} css=${geom.cw}x${geom.ch} hit=${geom.hit}`);

const cx = geom.x + geom.cw / 2;
const cy = geom.y + geom.ch / 2;
const spanX = Math.max(30, geom.cw * 0.26);
const spanY = Math.max(30, geom.ch * 0.26);

async function stroke(rowFrac) {
  const y0 = cy - spanY + 2 * spanY * rowFrac;
  const x0 = cx - spanX;
  await cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", x: x0, y: y0, button: "left", clickCount: 1, buttons: 1 });
  for (let m = 1; m <= MOVES; m++) {
    await cdp.send("Input.dispatchMouseEvent", {
      type: "mouseMoved",
      x: x0 + (2 * spanX * m) / MOVES,
      y: y0 + Math.sin((m / MOVES) * Math.PI) * spanY * 0.15,
      button: "left", buttons: 1,
    });
    await sleep(8);
  }
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: x0 + 2 * spanX, y: y0, button: "left", clickCount: 1, buttons: 0 });
  await sleep(110);
}

/* ---- commit oracle (dev handle only; Rust owns real history) ------------ */

async function rustHistoryDepth() {
  if (!hasHandle) return null;
  const out = await cdp.evaluate(`(async () => {
    try {
      const e = window.__photrezEditor;
      const docId = e.workspace.getActiveDocumentId();
      if (!docId) return 'ERR no active document';
      const internals = window.__TAURI_INTERNALS__;
      if (!internals || typeof internals.invoke !== 'function') return 'ERR no tauri invoke bridge';
      const r = await internals.invoke('rust_pixels_history_depth', { docId });
      return JSON.stringify(r);
    } catch (err) { return 'ERR ' + err.message; }
  })()`);
  try {
    return JSON.parse(out);
  } catch {
    return out;
  }
}

/* ---- PAINT PROOF + measurement ----------------------------------------- */

for (let i = 0; i < WARMUP; i++) await stroke(i / (WARMUP || 1));

const clip = { x: geom.x, y: geom.y, width: geom.cw, height: geom.ch, scale: 1 };
const depthBefore = await rustHistoryDepth();
const shotBefore = await cdp.screenshot(clip);

/* Nothing may run inside the sampled window except the strokes themselves.
   A CDP evaluate or a screenshot here blocks the main thread and manufactures
   the very frame drop this script exists to measure. */
await cdp.evaluate(START_SAMPLER);
for (let i = 0; i < STROKES; i++) await stroke(i / (STROKES - 1 || 1));
const stats = JSON.parse(await cdp.evaluate(READ_STATS));
const shotAfter = await cdp.screenshot(clip);

const depthAfter = await rustHistoryDepth();
const painted = shotBefore !== shotAfter;

const commitInfo =
  depthBefore && depthAfter && typeof depthBefore.total_depth === "number"
    ? { before: depthBefore.total_depth, after: depthAfter.total_depth, delta: depthAfter.total_depth - depthBefore.total_depth }
    : { note: "UNKNOWN - rust_pixels_history_depth needs a doc id, reachable only via the dev handle" };

const report = {
  label: LABEL,
  profile: PROFILE,
  docW: doc.w,
  docH: doc.h,
  docName: DOC_NAME,
  docCreated: created,
  brushSize: doc.brush ?? null,
  canvasBacking: [geom.bw, geom.bh],
  strokes: STROKES,
  movesPerStroke: MOVES,
  warmup: WARMUP,
  cold: COLD,
  paintProven: painted,
  commits: commitInfo,
  ...stats,
};
log(JSON.stringify(report, null, 1));

if (!painted) {
  log("INVALID: the composited pixels did not change across the measured window - the strokes did not paint.");
}
const dropped = stats.dropped33;
const overBudget = MAX_DROPPED !== 0 && dropped > MAX_DROPPED;
cdp.close();
process.exit(overBudget ? 1 : 0);