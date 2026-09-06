#!/usr/bin/env bun
/*
 * live-verify.mjs — Photrez live verification harness (TOOLING ONLY, dev-only).
 *
 * Why this exists:
 *   Headless unit tests mock the IPC/wasm boundary and miss real runtime behavior
 *   (this repo has been burned by the mock-vs-real gap). This harness boots the
 *   REAL desktop app and drives its real facade/editor code paths through the
 *   Chrome DevTools Protocol (CDP) so runtime errors, Rust panics, and
 *   state/version divergence are caught against the actual running build.
 *
 * How it works (reused from prior CDP live-drive work in this repo):
 *   1. Launches `bun run tauri dev` with WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
 *      set so the WebView2 is CDP-attachable on a debug port (default 9222).
 *   2. Polls the CDP HTTP endpoint (/json) until a page target appears.
 *   3. Connects a raw CDP WebSocket (bun's built-in fetch + WebSocket; no deps)
 *      and drives a scripted facade sequence through Runtime.evaluate:
 *        open doc A -> addLayer -> setOpacity -> transform -> deleteLayer ->
 *        undo -> redo -> open doc B (per-doc isolation check).
 *   4. Captures console errors / unhandled exceptions / tauri stderr, plus a
 *      state digest (layer count, active layer id, facade renderedVersion,
 *      facade layer count, best-effort canvas pixel hash) after every step.
 *   5. Prints PASS/FAIL: PASS = no captured errors AND the layer-count sequence
 *      matches the expected facade path; FAIL lists the first error + the step
 *      where a digest diverged.
 *   6. Kills only the process tree it spawned (never a pre-existing instance).
 *
 * Run (from repo root):
 *   PHOTREZ_FLAGS="facade=1" bun scripts/live-verify.mjs
 *   PHOTREZ_FLAGS="facadeAuthority=native" bun scripts/live-verify.mjs   # future reroute
 *
 * Env knobs:
 *   PHOTREZ_FLAGS        space/comma separated key=value (e.g. "facade=1");
 *                        mapped to localStorage["photrez.<key>"] at runtime.
 *   PHOTREZ_CDP_PORT     CDP debug port (default 9222).
 *   PHOTREZ_LAUNCH_TIMEOUT_MS  launch + CDP-ready budget (default 240000).
 *
 * Exit codes: 0 = PASS, 1 = FAIL (error or divergence), 2 = blocked by env.
 *
 * Public-clean: ASCII only, no internal plan codenames.
 */

import { spawn } from "node:child_process";
import net from "node:net";

// ── config ────────────────────────────────────────────────────────────────
const CDP_PORT = Number(process.env.PHOTREZ_CDP_PORT || 9222);
const LAUNCH_TIMEOUT_MS = Number(process.env.PHOTREZ_LAUNCH_TIMEOUT_MS || 240000);
const REPO_ROOT = process.cwd();

// Parse PHOTREZ_FLAGS -> localStorage entries (only the "photrez.<key>" shape
// the app already reads). Known keys: facade, facadeAuthority.
const FLAG_PAIRS = (process.env.PHOTREZ_FLAGS || "facade=1")
  .split(/[\s,]+/)
  .map((s) => s.trim())
  .filter(Boolean)
  .map((kv) => {
    const i = kv.indexOf("=");
    if (i < 0) return null;
    const k = kv.slice(0, i).trim();
    const v = kv.slice(i + 1).trim();
    if (!k) return null;
    return { key: `photrez.${k}`, value: v };
  })
  .filter(Boolean);

// ── tiny helpers ───────────────────────────────────────────────────────────
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const log = (...a) => console.log(...a);
const err = (...a) => console.error(...a);

function netProbe(port, timeoutMs = 1500) {
  return new Promise((resolve) => {
    const c = net.createConnection({ host: "127.0.0.1", port });
    const to = setTimeout(() => {
      try { c.destroy(); } catch {}
      resolve(false);
    }, timeoutMs);
    c.once("connect", () => {
      clearTimeout(to);
      try { c.destroy(); } catch {}
      resolve(true);
    });
    c.once("error", () => {
      clearTimeout(to);
      resolve(false);
    });
  });
}

// ── CDP client (raw WebSocket over bun globals) ─────────────────────────────
class Cdp {
  constructor(url) {
    this.url = url;
    this.ws = null;
    this.nextId = 1;
    this.pending = new Map();
    this.listeners = new Map(); // method -> Set<fn>
    this.opened = null;
  }
  on(method, fn) {
    if (!this.listeners.has(method)) this.listeners.set(method, new Set());
    this.listeners.get(method).add(fn);
  }
  connect() {
    return new Promise((resolve, reject) => {
      const ws = new WebSocket(this.url);
      this.ws = ws;
      ws.addEventListener("open", () => resolve());
      ws.addEventListener("error", (e) => reject(new Error("ws error: " + (e?.message || "unknown"))));
      ws.addEventListener("message", (ev) => {
        let msg;
        try { msg = JSON.parse(ev.data); } catch { return; }
        if (msg.id !== undefined && this.pending.has(msg.id)) {
          this.pending.get(msg.id)(msg);
          this.pending.delete(msg.id);
          return;
        }
        if (msg.method && this.listeners.has(msg.method)) {
          for (const fn of this.listeners.get(msg.method)) fn(msg.params);
        }
      });
      ws.addEventListener("close", () => {});
    });
  }
  send(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = this.nextId++;
      const t = setTimeout(() => {
        if (this.pending.has(id)) { this.pending.delete(id); reject(new Error(`CDP timeout: ${method}`)); }
      }, 30000);
      this.pending.set(id, (msg) => {
        clearTimeout(t);
        resolve(msg);
      });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }
  async evaluate(expression) {
    const r = await this.send("Runtime.evaluate", {
      expression,
      returnByValue: true,
      awaitPromise: true,
    });
    if (r.error) throw new Error("CDP error: " + JSON.stringify(r.error));
    if (r.result?.exceptionDetails) {
      const d = r.result.exceptionDetails;
      throw new Error("eval exception: " + (d.exception?.description || d.text || JSON.stringify(d)));
    }
    return r.result?.result?.value;
  }
  async enableRuntime() { await this.send("Runtime.enable"); }
  close() { try { this.ws.close(); } catch {} }
}

async function fetchCdpTargets() {
  const res = await fetch(`http://127.0.0.1:${CDP_PORT}/json`);
  if (!res.ok) throw new Error(`/json ${res.status}`);
  return await res.json();
}
async function findPageTarget() {
  const targets = await fetchCdpTargets();
  return (targets || []).find((t) => t.type === "page" && t.webSocketDebuggerUrl) || null;
}

// ── CDP connection lifecycle ────────────────────────────────────────────────
async function connectCdp() {
  const target = await findPageTarget();
  if (!target) return null;
  const cdp = new Cdp(target.webSocketDebuggerUrl);
  await cdp.connect();
  await cdp.enableRuntime();
  return cdp;
}

async function waitReady(cdp, timeoutMs) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const ok = await cdp.evaluate(
        "!!(window.__photrezEditor && window.__photrezEditor.workspace && window.__photrezEditor.workspace.constructor && window.__photrezEditor.workspace.constructor.createBlankDocument)",
      );
      if (ok) return true;
    } catch {}
    await sleep(500);
  }
  return false;
}

// The facade must be Rust-backed before any command runs (photrez.facade=1 arms
// the wasm engine via ensureFacadeReady at EditorShell boot, fire-and-forget).
// That resolves asynchronously, so wait for it explicitly rather than racing it.
async function waitFacadeReady(cdp, timeoutMs) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const ok = await cdp.evaluate(
        "(async () => { try { const m = await import(location.origin + '/src/lib/protocol/bridge.ts'); if (m.isFacadeArmed && m.isFacadeArmed()) return true; try { await m.ensureFacadeReady(); } catch (e) {} return !!(m.isFacadeArmed && m.isFacadeArmed()); } catch (e) { return false; } })()",
      );
      if (ok) return true;
    } catch {}
    await sleep(300);
  }
  return false;
}

// ── app lifecycle ───────────────────────────────────────────────────────────
let child = null;
let childPid = null;
const tauriErrors = [];

// Launch-state signals used by the startup-failure detection (BUG 1 fix).
let sawRunningExe = false;             // "Running `...photrez-desktop.exe`" line seen
let buildFailed = false;               // a REAL build error appeared (never a warning)
let buildErrorLine = "";
let childExitedBeforeRunning = false;  // exited non-zero before the binary launched
let childExitCode = null;

// Strip ANSI escape codes before any substring matching in the log.
const ANSI_RE = /\x1b\[[0-9;?]*[A-Za-z]/g;
function stripAnsi(s) { return String(s).replace(ANSI_RE, ""); }

// Real build-failure patterns ONLY — must NOT match "warning:" (which contains
// the substring "error"). warning: lines are explicitly ignored below.
const BUILD_ERROR_RE = /error\[E\d|error: could not compile|error: failed/i;

async function cdpVersionUp() {
  try {
    const res = await fetch(`http://127.0.0.1:${CDP_PORT}/json/version`);
    return res.ok;
  } catch { return false; }
}

function spawnApp() {
  const env = { ...process.env };
  env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = `--remote-debugging-port=${CDP_PORT}`;
  child = spawn("bun", ["run", "tauri", "dev"], {
    cwd: REPO_ROOT,
    env,
    stdio: ["ignore", "pipe", "pipe"],
  });
  childPid = child.pid;
  const onChunk = (raw) => {
    const s = stripAnsi(raw.toString());
    if (/warning:/i.test(s)) return; // warnings are not failures
    if (/Running\s+[^\n]*\.exe/i.test(s)) sawRunningExe = true;
    if (/panic/i.test(s)) { tauriErrors.push(s.trim().split("\n").pop()); return; }
    if (BUILD_ERROR_RE.test(s)) {
      buildFailed = true;
      buildErrorLine = s.trim().split("\n").pop();
      tauriErrors.push(buildErrorLine);
    }
  };
  child.stdout.on("data", (b) => { process.stdout.write(b); onChunk(b); });
  child.stderr.on("data", (b) => { process.stderr.write(b); onChunk(b); });
  child.on("exit", (code) => {
    childExitCode = code;
    if (code !== null && code !== 0) {
      if (!sawRunningExe) childExitedBeforeRunning = true;
      tauriErrors.push(`tauri dev exited (code ${code})`);
    }
  });
}

// Kill ONLY the process tree we spawned (by PID, /T /F) — never by image name,
// so a pre-existing user instance is never touched. Awaited so taskkill completes
// BEFORE the harness process exits (BUG 2 fix).
//
// IMPORTANT ordering: run taskkill /T /F FIRST, while the parent bun process is
// still alive. Killing the parent first would detach its children (cargo,
// photrez-desktop.exe, vite) and reparent them to init, so a later taskkill on
// the dead parent PID traverses nothing and the app/webview orphans survive.
async function cleanup() {
  if (!childPid) return;
  await new Promise((resolve) => {
    try {
      const tk = spawn("taskkill", ["/pid", String(childPid), "/T", "/F"], { stdio: "ignore" });
      tk.on("exit", () => resolve());
      tk.on("error", () => resolve());
    } catch { resolve(); }
  });
  try { child.kill("SIGKILL"); } catch {}
}

// ── scripted facade sequence (runs in-page; mirrors production wiring) ───────
const SETUP_FN = `
(async () => {
  const mod = await import(location.origin + '/src/lib/protocol/facadeRegistry.ts');
  const ed = window.__photrezEditor;
  const ws = ed.workspace;
  // open doc A
  const idA = 'lv-docA-' + Date.now();
  const session = ws.constructor.createBlankDocument(idA, 'LV Doc A', 200, 200, { backgroundColor: 'white' });
  ws.addDocument(session);
  const engine = ws.getActiveEngine();
  const facade = mod.getFacade(idA);
  await mod.seedFacadeFromEngine(engine, facade);
  window.__lv = { mod, engine, facade, ws, docAId: idA };
  return { docAId: idA, layerCount: engine.getLayers().length, activeLayerId: engine.getActiveLayerId() };
})()
`;

function stepFn(body) {
  return `(async () => { const { engine, facade } = window.__lv; ${body} return window.__digest(); })()`;
}

const DIGEST_HELPER = `
window.__digest = function () {
  const { engine, facade } = window.__lv;
  let canvasHash = null;
  try {
    const cs = Array.from(document.querySelectorAll('canvas'));
    const c = cs.find((x) => x.width > 100 && x.height > 100);
    if (c) {
      const ctx = c.getContext('2d');
      if (ctx && ctx.getImageData) {
        const w = Math.min(c.width, 48), h = Math.min(c.height, 48);
        const d = ctx.getImageData(0, 0, w, h).data;
        let hsh = 5381;
        for (let i = 0; i < d.length; i += 4) hsh = (((hsh * 33) ^ d[i]) ^ (d[i + 1] << 8) ^ (d[i + 2] << 16)) >>> 0;
        canvasHash = hsh.toString(16);
      }
    }
  } catch (e) { canvasHash = 'err'; }
  const layers = engine.getLayers();
  return {
    layerCount: layers.length,
    activeLayerId: engine.getActiveLayerId(),
    renderedVersion: facade.renderedVersion,
    facadeLayerCount: facade.snapshot.layers.length,
    layerIds: layers.map((l) => l.id),
    canvasHash,
  };
};
`;

const STEP_ADD = stepFn(`
  const snap = await facade.addLayer('LV Layer');
  engine.applyFacadeSnapshot(snap);
  window.__lv.addedId = snap.layers[snap.layers.length - 1].id;
`);

// Pixel-commit -> facade bridge. Drives the REAL production paint-bucket fill
// (applyPaintBucketFill) on an existing layer with the canonical pixel flag on,
// so it routes through rust_pixels_write_region and then
// syncFacadeVersionFromPixel(docId, res.version) — the wiring under test. Then
// issues a FOLLOWING facade command (setOpacity): under native authority this
// would throw E_VERSION_MISMATCH if the pixel-commit version had NOT been carried
// into facade.renderedVersion. Layer count is unchanged by a pixel commit.
const STEP_PIXELCOMMIT = stepFn(`
  const addedId = window.__lv.addedId;
  const targetId = addedId;
  const ed = window.__photrezEditor;
  const layer = engine.getLayer(targetId);
  if (!layer) throw new Error('pixelCommit: target layer missing');
  // A paint surface requires a layer imageBitmap; give the added layer one.
  if (!layer.imageBitmap) {
    const off = new OffscreenCanvas(layer.width || 200, layer.height || 200);
    const cx = off.getContext('2d');
    if (cx) cx.clearRect(0, 0, off.width, off.height);
    engine.setLayerImageBitmap(targetId, off.transferToImageBitmap());
  }
  // The fill operates on the active layer; make it the added layer.
  engine.setActiveLayer(targetId);
  // Enable the canonical pixel path so the fill routes through rust_pixels_write_region.
  localStorage.setItem('photrez.rustPixels', '1');
  if (typeof ed.setActiveTool === 'function') ed.setActiveTool('paintBucket');
  const { applyPaintBucketFill } = await import(location.origin + '/src/components/editor/canvas/pointerTools/paintBucket.ts');
  const ctx = {
    editor: ed,
    getDocCoords: () => ({ x: 0, y: 0 }),
    getCanvasRef: () => ({ current: null }),
  };
  const rvBefore = facade.renderedVersion;
  const surfBefore = engine.getPaintSurface(targetId);
  const pvBefore = surfBefore ? surfBefore.pixelVersion : 0;
  applyPaintBucketFill(ctx, { pointerId: 1, clientX: 0, clientY: 0 });
  let committed = false;
  const deadline = Date.now() + 15000;
  while (Date.now() < deadline) {
    const s = engine.getPaintSurface(targetId);
    if (s && s.pixelVersion > pvBefore) { committed = true; break; }
    await new Promise((r) => setTimeout(r, 50));
  }
  const rvAfterFill = facade.renderedVersion;
  // Following facade command: the interleave that fails under native if the
  // pixel-commit version was not propagated into the facade.
  let followError = null;
  try {
    const snap = await facade.setOpacity(targetId, 0.5);
    if (snap) engine.applyFacadeSnapshot(snap);
  } catch (e) { followError = String((e && e.message) || e); }
  const dig = window.__digest();
  dig.rvBefore = rvBefore;
  dig.rvAfterFill = rvAfterFill;
  dig.followOk = !followError;
  dig.followError = followError;
  dig.committed = committed;
  dig.note = 'rvBefore=' + rvBefore + ' rvAfterFill=' + rvAfterFill + ' followOk=' + (!followError) + (followError ? (' err=' + followError) : '') + ' committed=' + committed;
  return dig;
`);

const STEP_OPACITY = stepFn(`
  const layers = engine.getLayers();
  const target = layers.find((l) => !l.isBackground) || layers[layers.length - 1];
  const snap = await facade.setOpacity(target.id, 0.5);
  engine.applyFacadeSnapshot(snap);
`);

const STEP_TRANSFORM = stepFn(`
  const layers = engine.getLayers();
  const target = layers.find((l) => !l.isBackground) || layers[layers.length - 1];
  const t = engine.getLayer(target.id).transform;
  facade.beginTransform(target.id, t);
  facade.updateTransform({ ...t, x: t.x + 12, y: t.y + 8 });
  const snap = await facade.commitTransform();
  if (snap) engine.applyFacadeSnapshot(snap);
`);

const STEP_DELETE = stepFn(`
  const id = window.__lv.addedId;
  const snap = await facade.deleteLayer(id);
  if (snap) engine.applyFacadeSnapshot(snap);
`);

const STEP_UNDO = stepFn(`
  const snap = await facade.undo();
  if (!facade.lastHistoryDeltaWasEmpty && snap) engine.applyFacadeSnapshot(snap);
`);

const STEP_REDO = stepFn(`
  const snap = await facade.redo();
  if (!facade.lastHistoryDeltaWasEmpty && snap) engine.applyFacadeSnapshot(snap);
`);

// Module-scoped so both runSequence() and the main() divergence check share it.
const FACADE_STEPS = [
  ["addLayer", STEP_ADD],
  ["pixelCommit", STEP_PIXELCOMMIT],
  ["setOpacity", STEP_OPACITY],
  ["transform", STEP_TRANSFORM],
  ["deleteLayer", STEP_DELETE],
  ["undo", STEP_UNDO],
  ["redo", STEP_REDO],
];

const STEP_DOCB = `
(async () => {
  const { mod, ws } = window.__lv;
  const idB = 'lv-docB-' + Date.now();
  const session = ws.constructor.createBlankDocument(idB, 'LV Doc B', 200, 200, { backgroundColor: 'white' });
  ws.addDocument(session);
  const engineB = ws.getActiveEngine();
  const facadeB = mod.getFacade(idB);
  await mod.seedFacadeFromEngine(engineB, facadeB);
  const snap = await facadeB.addLayer('LV B Layer');
  engineB.applyFacadeSnapshot(snap);
  window.__lv.docBId = idB;
  window.__lv.engineB = engineB;
  // isolation: doc A layer count must be unchanged
  const aEngine = window.__lv.engine;
  return {
    docBLayerCount: engineB.getLayers().length,
    docALayerCountAfterB: aEngine.getLayers().length,
  };
})()
`;

// Expected layer-count sequence for doc A across the scripted steps.
// [docA initial=1, +addLayer=2, pixelCommit=2 (no count change), setOpacity=2,
//  transform=2, delete=1, undo=2, redo=1]
// pixelCommit exercises the PIXEL-COMMIT -> syncFacadeVersionFromPixel -> facade
// bridge: a real rust_pixels_write_region bumps the native engine documentVersion,
// which must be carried into the facade's renderedVersion for the FOLLOWING facade
// command (setOpacity) to succeed instead of throwing E_VERSION_MISMATCH.
const EXPECTED_DOCA_LAYERS = [1, 2, 2, 2, 2, 1, 2, 1];

async function runSequence(cdp) {
  const captured = []; // { step, digest }
  const consoleErrors = [];
  cdp.on("Runtime.consoleAPICalled", (p) => {
    if (p.type === "error") {
      const text = (p.args || []).map((a) => a.value ?? a.description ?? "").join(" ");
      consoleErrors.push(text);
    }
  });
  cdp.on("Runtime.exceptionThrown", (p) => {
    const d = p.exceptionDetails;
    consoleErrors.push("uncaught: " + (d?.exception?.description || d?.text || "exception"));
  });

  // install digest helper
  await cdp.evaluate(DIGEST_HELPER);

  const setup = await cdp.evaluate(SETUP_FN);
  captured.push({ step: "setup(docA)", digest: setup });

  const steps = FACADE_STEPS;
  for (const [name, fn] of steps) {
    const digest = await cdp.evaluate(fn);
    captured.push({ step: name, digest });
  }

  const docB = await cdp.evaluate(STEP_DOCB);
  captured.push({ step: "docB(isolation)", digest: docB });

  return { captured, consoleErrors };
}

// ── main ────────────────────────────────────────────────────────────────────
async function main() {
  log("=== Photrez live verification harness ===");
  log(`flags: ${FLAG_PAIRS.map((f) => f.key + "=" + f.value).join(", ") || "(none)"}  cdpPort=${CDP_PORT}`);

  // Pre-flight: never attach to / kill a pre-existing user instance.
  if (await netProbe(CDP_PORT)) {
    err("");
    err("BLOCKED: CDP endpoint 127.0.0.1:" + CDP_PORT + " is already open.");
    err("A Photrez/WebView2 session is already running on this machine. live-verify must");
    err("own its own app instance and will NOT attach to or kill a running user session.");
    err("To run: close the running Photrez instance, then re-run this harness.");
    err("");
    process.exit(2);
  }

  let exitCode = 0;
  let cdp = null;
  const launchStart = Date.now();
  try {
    // Launch.
    log("launching: bun run tauri dev (with --remote-debugging-port=" + CDP_PORT + ")");
    spawnApp();

    // SUCCESS signal = "Running ...exe" line seen AND CDP /json/version responds.
    // FAILURE is detected by distinct causes only (BUG 1 fix):
    //   (a) child exited non-zero BEFORE the binary launched, OR
    //   (b) a real build error appeared, OR
    //   (c) after the binary launched, CDP never became reachable within budget.
    let cdpReady = false;
    while (Date.now() - launchStart < LAUNCH_TIMEOUT_MS) {
      if (childExitedBeforeRunning || buildFailed) break;
      if (sawRunningExe && (await cdpVersionUp())) { cdpReady = true; break; }
      await sleep(1000);
    }

    if (!cdpReady) {
      if (childExitedBeforeRunning) {
        err(`FAIL: tauri dev process exited (code ${childExitCode ?? "?"}) before the app binary launched.`);
      } else if (buildFailed) {
        err("FAIL: tauri dev build failed: " + buildErrorLine);
      } else if (!sawRunningExe) {
        err("FAIL: tauri dev launched but the 'Running ...exe' binary line never appeared (build/launch error).");
      } else {
        err(`FAIL: app binary launched (saw 'Running ...exe') but CDP /json/version never became reachable on 127.0.0.1:${CDP_PORT} within ${LAUNCH_TIMEOUT_MS}ms.`);
      }
      exitCode = 1;
    } else {
      log("CDP endpoint ready (app launched + /json/version up); attaching...");
      cdp = await connectCdp();
      if (!(await waitReady(cdp, 60000))) {
        err("FAIL: app loaded but window.__photrezEditor never became ready.");
        exitCode = 1;
      } else {
        // Apply flags via localStorage, then reload so the app boots with them.
        await cdp.evaluate(
          FLAG_PAIRS.map((f) => `localStorage.setItem(${JSON.stringify(f.key)}, ${JSON.stringify(f.value)});`).join(""),
        );
        await cdp.evaluate("location.reload()");
        await sleep(1500);
        // Reconnect (target/ws may have changed after reload).
        cdp.close();
        cdp = null;
        const t2 = await (async () => {
          const s = Date.now();
          while (Date.now() - s < 60000) {
            try { const t = await findPageTarget(); if (t) return t; } catch {}
            await sleep(1000);
          }
          return null;
        })();
        if (!t2) {
          err("FAIL: lost CDP target after reload.");
          exitCode = 1;
        } else {
          cdp = await connectCdp();
          if (!(await waitReady(cdp, 60000))) {
            err("FAIL: app not ready after reload/flag application.");
            exitCode = 1;
          } else if (!(await waitFacadeReady(cdp, 90000))) {
            // The wasm engine never armed: a facade command would throw
            // E_FACADE_NOT_READY. Surface it instead of a misleading command error.
            err("FAIL: facade/wasm engine never armed after reload (photrez.facade=1).");
            exitCode = 1;
          } else {
            log("app ready (flags applied). driving facade sequence...");
            const { captured, consoleErrors } = await runSequence(cdp);

            // Verify layer-count sequence for doc A. EXPECTED_DOCA_LAYERS includes
            // the initial setup count (captured[0]) as element 0, so compare from 0.
            const docALayers = captured.slice(0, EXPECTED_DOCA_LAYERS.length).map((c) => c.digest.layerCount);
            let divergence = null;
            for (let i = 0; i < EXPECTED_DOCA_LAYERS.length; i++) {
              if (docALayers[i] !== EXPECTED_DOCA_LAYERS[i]) {
                divergence = { step: FACADE_STEPS[i][0], expected: EXPECTED_DOCA_LAYERS[i], actual: docALayers[i] };
                break;
              }
            }
            const docB = captured[captured.length - 1].digest;
            const isolationOk = docB.docBLayerCount === 2 && docB.docALayerCountAfterB === EXPECTED_DOCA_LAYERS[EXPECTED_DOCA_LAYERS.length - 1];

            // Report.
            log("");
            log("--- digests ---");
            for (const c of captured) {
              const d = c.digest;
              const s = typeof d === "object" && "layerCount" in d
                ? `layers=${d.layerCount} active=${String(d.activeLayerId).slice(0, 8)} rv=${d.renderedVersion} facadeLayers=${d.facadeLayerCount} canvas=${d.canvasHash}`
                : JSON.stringify(d);
              const note = (typeof d === "object" && d.note) ? `  ${d.note}` : "";
              log(`  ${c.step.padEnd(16)} ${s}${note}`);
            }

            const pixelCommit = captured.find((c) => c.step === "pixelCommit");
            const pixelCommitOk = !pixelCommit || (pixelCommit.digest && pixelCommit.digest.committed !== false);
            const pass = consoleErrors.length === 0 && !divergence && isolationOk && tauriErrors.length === 0 && pixelCommitOk;
            log("");
            if (pass) {
              log("PASS: no console errors/panics, layer-count sequence matches facade path, per-doc isolation OK.");
            } else {
              log("FAIL:");
              if (consoleErrors.length) log("  console errors: " + consoleErrors.slice(0, 3).join(" | "));
              if (tauriErrors.length) log("  tauri stderr: " + tauriErrors.slice(0, 3).join(" | "));
              if (divergence) log(`  digest divergence at '${divergence.step}': expected layers=${divergence.expected}, got ${divergence.actual}`);
              if (!isolationOk) log(`  per-doc isolation FAILED: docB=${docB.docBLayerCount}, docA-after-B=${docB.docALayerCountAfterB}`);
              if (!pixelCommitOk) log(`  pixelCommit did not run a real pixel commit (committed=${pixelCommit?.digest?.committed ?? "n/a"}): bridge NOT exercised.`);
            }
            exitCode = pass ? 0 : 1;
          }
        }
      }
    }
  } catch (e) {
    err("FAIL: harness error: " + (e?.stack || e?.message || String(e)));
    exitCode = 1;
  } finally {
    // BUG 2 fix: cleanup runs on EVERY exit path (success, fail, timeout, throw).
    await cleanup();
  }

  // Post-run sanity: confirm the launched tree is gone and ports are free.
  // Retry briefly — a freshly killed listening socket can take a moment to release.
  let port1420 = true, portCdp = true;
  for (let i = 0; i < 10 && (port1420 || portCdp); i++) {
    await sleep(1000);
    port1420 = await netProbe(1420, 1000);
    portCdp = await netProbe(CDP_PORT, 1000);
  }
  if (port1420 || portCdp) {
    err(`WARN: port 1420=${port1420} port ${CDP_PORT}=${portCdp} still open after cleanup (orphan possible).`);
  } else {
    log("cleanup OK: ports 1420 and " + CDP_PORT + " are free after the run.");
  }

  process.exit(exitCode);
}

main();
