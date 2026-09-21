#!/usr/bin/env node
/*
 * ipc-frequency-live.mjs - live Tauri invoke-frequency probe (TOOLING ONLY, dev-only).
 *
 * Why this exists:
 *   The latency battery measures per-op COST (ms per invoke), but nobody has
 *   measured per-gesture FREQUENCY: how many Tauri invokes fire for one real
 *   user gesture (toggle, type, drag, undo). Without frequency, per-op cost
 *   cannot be turned into per-gesture lag. This script boots the REAL desktop
 *   app, reads a DEV-only in-client counter (no production behavior change),
 *   drives scripted gestures through the same Runtime.evaluate seam
 *   as live-verify, and prints per-gesture invoke counts by command.
 *
 * Launch/CDP/teardown conventions mirror scripts/perf-audit-live.mjs (spawn
 * `bun run tauri dev` with a remote-debugging port, poll /json, raw CDP
 * WebSocket, kill only the spawned tree). Default CDP port is 9224 so neither
 * live-verify (9222) nor perf-audit-live (9223) is disturbed.
 *
 * Run (from repo root):
 *   node scripts/ipc-frequency-live.mjs --help
 *   PHOTREZ_FREQ_MODE=native node scripts/ipc-frequency-live.mjs
 *
 * Env knobs:
 *   PHOTREZ_CDP_PORT         CDP debug port (default 9224).
 *   PHOTREZ_LAUNCH_TIMEOUT_MS  launch + CDP-ready budget (default 240000).
 *   PHOTREZ_FREQ_N           repetitions per gesture (default 10).
 *   PHOTREZ_FREQ_MODE        native | legacy | both (default native; see below).
 *   PHOTREZ_FLAGS            overridden by MODE unless explicitly set.
 *   PHOTREZ_FREQ_OUT         optional output path; default
 *                            <tmpdir>/photrez-ipc-freq-<timestamp>.json.
 *   PHOTREZ_FREQ_TIMEOUT_MS  budget per gesture evaluate (default 60000).
 *
 * Modes (default native): native arms the Rust-backed facade
 * (flags "facade=1 facadeAuthority=native", pinned so a stale persisted
 * authority from an earlier session cannot reroute the run) - the path
 * where the 3-invoke metadata cost lives.
 * legacy uses flags "facade=0 facadeAuthority=wasm". both runs native then
 * legacy sequentially with separate tables. Default is native (not both)
 * because each mode is a full app launch (~minutes); run both explicitly
 * when you can afford two launches.
 *
 * Exit codes: 0 = captured, 1 = failure (stage named on stderr), 2 = blocked by env.
 *
 * Public-clean: ASCII only, no internal plan codenames.
 */

import { spawn } from "node:child_process";
import net from "node:net";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const DEFAULT_PORT = 9224;
const CDP_PORT = Number(process.env.PHOTREZ_CDP_PORT || DEFAULT_PORT);
const LAUNCH_TIMEOUT_MS = Number(process.env.PHOTREZ_LAUNCH_TIMEOUT_MS || 240000);
const EVAL_TIMEOUT_MS = Number(process.env.PHOTREZ_FREQ_TIMEOUT_MS || 60000);
const REPO_ROOT = process.cwd();
const N = Math.max(1, Math.min(1000, Number(process.env.PHOTREZ_FREQ_N || 10)));

// Snapshot-scale probe (no new env knobs): layer counts x reps for the
// snapshot-cost isolation. Reuses the fq-doc seed path (blank doc + facade
// seed); small bitmaps only so layer-count scaling dominates pixel scaling.
const SCALE_SIZES = [1, 8, 32];
const SCALE_REPS = 5;

const MODE_RAW = String(process.env.PHOTREZ_FREQ_MODE || "native").trim().toLowerCase();
const MODE = ["native", "legacy", "both"].includes(MODE_RAW) ? MODE_RAW : "native";

const MODE_FLAGS = {
  // Native pins facadeAuthority explicitly: the dev webview profile persists
  // localStorage across launches, so a stale facadeAuthority=wasm from an
  // earlier session would otherwise reroute every gesture to the wasm path
  // (real walls, zero native invokes, passing canary).
  native: "facade=1 facadeAuthority=native",
  legacy: "facade=0 facadeAuthority=wasm",
};

if (process.argv.includes("--help") || process.argv.includes("-h")) {
  console.log("Usage: node scripts/ipc-frequency-live.mjs [options]");
  console.log("");
  console.log("Boots the real desktop app, counts Tauri invokes per scripted");
  console.log("gesture via a DEV-only in-client counter, prints");
  console.log("a per-gesture table and saves JSON.");
  console.log("");
  console.log("Options:");
  console.log("  --help, -h     Print this usage and exit 0 (spawns nothing).");
  console.log("");
  console.log("Env:");
  console.log("  PHOTREZ_CDP_PORT          CDP debug port (default 9224).");
  console.log("  PHOTREZ_LAUNCH_TIMEOUT_MS launch + CDP-ready budget ms (default 240000).");
  console.log("  PHOTREZ_FREQ_N            repetitions per gesture (default 10).");
  console.log("  PHOTREZ_FREQ_MODE         native | legacy | both (default native).");
  console.log("  PHOTREZ_FLAGS             overrides the mode flag preset.");
  console.log("  PHOTREZ_FREQ_OUT          optional output path.");
  console.log("  PHOTREZ_FREQ_TIMEOUT_MS   per-gesture evaluate budget ms (default 60000).");
  console.log("");
  console.log("Exit codes: 0 captured, 1 failure, 2 blocked by env.");
  process.exit(0);
}

function flagPairsFor(raw) {
  if (process.env.PHOTREZ_FLAGS) raw = process.env.PHOTREZ_FLAGS;
  return String(raw || "")
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
}

const OUT_PATH =
  process.env.PHOTREZ_FREQ_OUT ||
  path.join(os.tmpdir(), `photrez-ipc-freq-${Date.now()}.json`);

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const log = (...a) => console.log(...a);
const err = (...a) => console.error(...a);

function clean(s) {
  return String(s).replace(/[^\x20-\x7E\n]/g, "?");
}

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

class Cdp {
  constructor(url) {
    this.url = url;
    this.ws = null;
    this.nextId = 1;
    this.pending = new Map();
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
        }
      });
      ws.addEventListener("close", () => {});
    });
  }
  send(method, params = {}, timeoutMs = 30000) {
    return new Promise((resolve, reject) => {
      const id = this.nextId++;
      const t = setTimeout(() => {
        if (this.pending.has(id)) { this.pending.delete(id); reject(new Error(`CDP timeout: ${method}`)); }
      }, timeoutMs);
      this.pending.set(id, (msg) => {
        clearTimeout(t);
        resolve(msg);
      });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }
  async evaluate(expression, timeoutMs = 30000) {
    const r = await this.send("Runtime.evaluate", {
      expression,
      returnByValue: true,
      awaitPromise: true,
    }, timeoutMs);
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

function isAppTarget(t) {
  if (!t || t.type !== "page" || !t.webSocketDebuggerUrl) return false;
  const u = String(t.url || "");
  if (!u) return false;
  if (u.startsWith("about:") || u.startsWith("devtools://") || u.startsWith("chrome://") || u.startsWith("chrome-extension://")) return false;
  return true;
}

async function findPageTarget() {
  const targets = await fetchCdpTargets();
  const pages = (targets || []).filter(isAppTarget);
  if (pages.length === 0) return null;
  return pages.find((t) => /tauri|localhost|127\.0\.0\.1|^https?:/.test(String(t.url || ""))) || pages[0];
}

async function connectCdp() {
  const target = await findPageTarget();
  if (!target) return null;
  const cdp = new Cdp(target.webSocketDebuggerUrl);
  await cdp.connect();
  await cdp.enableRuntime();
  return cdp;
}

async function waitDocReady(cdp, timeoutMs) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const ok = await cdp.evaluate("document.readyState === 'complete' && !/^about:/.test(location.href)");
      if (ok) return true;
    } catch {}
    await sleep(500);
  }
  return false;
}

async function waitEditorReady(cdp, timeoutMs) {
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

let child = null;
let childPid = null;

const ANSI_RE = /\x1b\[[0-9;?]*[A-Za-z]/g;
function stripAnsi(s) { return String(s).replace(ANSI_RE, ""); }
const BUILD_ERROR_RE = /error\[E\d|error: could not compile|error: failed/i;

let sawRunningExe = false;
let buildFailed = false;
let buildErrorLine = "";
let childExitedBeforeRunning = false;
let childExitCode = null;

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
    if (/warning:/i.test(s)) return;
    if (/Running\s+[^\n]*\.exe/i.test(s)) sawRunningExe = true;
    if (BUILD_ERROR_RE.test(s)) {
      buildFailed = true;
      buildErrorLine = s.trim().split("\n").pop();
    }
  };
  child.stdout.on("data", (b) => { process.stdout.write(b); onChunk(b); });
  child.stderr.on("data", (b) => { process.stderr.write(b); onChunk(b); });
  child.on("exit", (code) => {
    childExitCode = code;
    if (code !== null && code !== 0 && !sawRunningExe) childExitedBeforeRunning = true;
  });
}

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

// In-client DEV counter protocol. The app records every nativeProtocol
// invoke into window.__ipcFreqNative (see nativeClient.ts: DEV-gated, capped
// at 50000 with oldest-drop). This script only resets and reads that array
// through Runtime.evaluate - it never wraps any transport seam, so there is
// nothing to install before page modules load. Reset guards a missing array;
// read caps the returned slice and reports truncation explicitly.
const FREQ_CAP = 50000;
const FREQ_RESET_EXPR =
  "(function(){try{if(!Array.isArray(window.__ipcFreqNative)){window.__ipcFreqNative=[];}else{window.__ipcFreqNative.length=0;}}catch(e){}return true;})()";
const FREQ_READ_EXPR =
  "(function(){try{var a=window.__ipcFreqNative;if(!Array.isArray(a)){return{events:[],dropped:0,truncated:false,count:0};}var n=a.length;if(n>50000){return{events:a.slice(n-50000),dropped:0,truncated:true,count:n};}return{events:a.slice(),dropped:0,truncated:false,count:n};}catch(e){return{events:[],dropped:0,truncated:false,count:0};}})()";
// Canary import mirrors the gesture setup below: it dynamically imports the
// app module by absolute path (location.origin + '/src/lib/protocol/...')
// exactly like FREQ_SETUP_FN imports facadeRegistry, then drives one real
// nativeProtocol call and asserts the counter observed it.

// Doc + facade setup. Mirrors the live-verify SETUP_FN mechanism: import the
// facade registry in page context, open a blank doc, seed the facade.
const FREQ_SETUP_FN = `
(async () => {
  const mod = await import(location.origin + '/src/lib/protocol/facadeRegistry.ts');
  const ed = window.__photrezEditor;
  const ws = ed.workspace;
  const id = 'fq-doc-' + Date.now();
  const session = ws.constructor.createBlankDocument(id, 'Freq Doc', 200, 200, { backgroundColor: 'white' });
  ws.addDocument(session);
  const engine = ws.getActiveEngine();
  const facade = mod.getFacade(id);
  await mod.seedFacadeFromEngine(engine, facade);
  const layers = engine.getLayers();
  const target = layers.find((l) => !l.isBackground) || layers[layers.length - 1];
  window.__fq = { mod, engine, facade, ws, docId: id, targetId: target.id };
  return { docId: id, targetId: target.id, layerCount: layers.length };
})()
`;

// One gesture repetition in page context. Applies the facade snapshot to the
// engine exactly like the live-verify STEP_* fns so the production wiring runs.
function gestureBody(kind, i) {
  switch (kind) {
    case "visibility":
      return `const v = (${i} % 2 === 0); const snap = await facade.setLayerVisibility(window.__fq.targetId, v); engine.applyFacadeSnapshot(snap);`;
    case "opacity":
      return `const o = (${i} % 2 === 0) ? 0.5 : 0.8; const snap = await facade.setOpacity(window.__fq.targetId, o); engine.applyFacadeSnapshot(snap);`;
    case "rename":
      return `const snap = await facade.setLayerName(window.__fq.targetId, 'fq-name-${i}'); engine.applyFacadeSnapshot(snap);`;
    case "addDelete":
      return `const snap = await facade.addLayer('fq-tmp-${i}'); engine.applyFacadeSnapshot(snap); const id = snap.layers[snap.layers.length - 1].id; const snap2 = await facade.deleteLayer(id); if (snap2) engine.applyFacadeSnapshot(snap2);`;
    case "undo":
      return `const snap = await facade.undo(); if (!facade.lastHistoryDeltaWasEmpty && snap) engine.applyFacadeSnapshot(snap);`;
    case "redo":
      return `const snap = await facade.redo(); if (!facade.lastHistoryDeltaWasEmpty && snap) engine.applyFacadeSnapshot(snap);`;
    default:
      throw new Error("unknown gesture " + kind);
  }
}

const GESTURES = ["visibility", "opacity", "rename", "addDelete", "undo", "redo"];

function aggregate(events) {
  const byCommand = {};
  let bytes = 0;
  for (const e of events) {
    byCommand[e.cmd] = (byCommand[e.cmd] || 0) + 1;
    if (typeof e.bytes === "number" && e.bytes > 0) bytes += e.bytes;
  }
  return { total: events.length, byCommand, argsBytes: bytes };
}

// First-visible gestures: the felt-lag proxy only applies here (toggle,
// opacity, undo). Other gestures keep wall timing only.
const FV_GESTURES = ["visibility", "opacity", "undo"];

// Gesture timing is page-side (performance.now inside the evaluated fn),
// NOT a CDP round-trip measure. Wall excludes the composited frame;
// firstVisible-approx awaits two consecutive requestAnimationFrame
// callbacks after the gesture body, then stops the clock: an rAF-bound
// proxy for the composited frame, not a GPU timestamp; dev-build vsync
// behavior may vary.
async function runGesture(cdp, kind) {
  const perRep = [];
  const wallMs = [];
  const fvMsArr = [];
  const wantFv = FV_GESTURES.includes(kind);
  let dropped = 0;
  for (let i = 0; i < N; i++) {
    await cdp.evaluate(FREQ_RESET_EXPR, EVAL_TIMEOUT_MS);
    const expr = wantFv
      ? `(async () => { const { engine, facade } = window.__fq; const t0 = performance.now(); ${gestureBody(kind, i)} const wall = performance.now() - t0; await new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))); const fv = performance.now() - t0; return { ms: wall, fvMs: fv }; })()`
      : `(async () => { const { engine, facade } = window.__fq; const t0 = performance.now(); ${gestureBody(kind, i)} const dt = performance.now() - t0; return { ms: dt }; })()`;
    const timed = await cdp.evaluate(expr, EVAL_TIMEOUT_MS);
    const read = await cdp.evaluate(FREQ_READ_EXPR, EVAL_TIMEOUT_MS);
    const agg = aggregate(read.events || []);
    dropped += read.dropped || 0;
    perRep.push(agg);
    if (timed && typeof timed.ms === "number") wallMs.push(timed.ms);
    if (wantFv && timed && typeof timed.fvMs === "number") fvMsArr.push(timed.fvMs);
  }
  const totals = { total: 0, argsBytes: 0, byCommand: {} };
  for (const r of perRep) {
    totals.total += r.total;
    totals.argsBytes += r.argsBytes;
    for (const [cmd, c] of Object.entries(r.byCommand)) {
      totals.byCommand[cmd] = (totals.byCommand[cmd] || 0) + c;
    }
  }
  const meanWallMs = wallMs.length
    ? Number((wallMs.reduce((s, v) => s + v, 0) / wallMs.length).toFixed(2))
    : null;
  const fvMs = wantFv && fvMsArr.length
    ? Number((fvMsArr.reduce((s, v) => s + v, 0) / fvMsArr.length).toFixed(2))
    : null;
  return {
    gesture: kind,
    reps: N,
    totalInvokes: totals.total,
    meanPerRep: totals.total / N,
    meanWallMs,
    fvMs,
    argsBytesTotal: totals.argsBytes,
    byCommand: totals.byCommand,
    dropped,
    note: wantFv
      ? "wall mean + firstVisible-approx mean (2x rAF proxy for composited frame, not GPU timestamp) over " + N + " reps"
      : "page-side gesture wall mean over " + N + " reps; excludes the composited frame",
  };
}

// Stretch gestures: best-effort only, no new harness. A text keystroke via
// CDP Input.dispatchKeyEvent and a brush pointer chain via
// Input.dispatchMouseEvent at canvas center. If no canvas is found the gesture
// reports SKIP with the reason instead of failing the run.
async function runStretch(cdp) {
  const out = [];
  const canvasInfo = await cdp.evaluate(
    "(() => { const cs = Array.from(document.querySelectorAll('canvas')); const c = cs.find((x) => x.width > 100 && x.height > 100) || cs[0]; if (!c) return null; const r = c.getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; })()",
    EVAL_TIMEOUT_MS,
  ).catch(() => null);
  if (!canvasInfo) {
    out.push({ gesture: "keystroke", skipped: true, reason: "no canvas element found" });
    out.push({ gesture: "brushStroke", skipped: true, reason: "no canvas element found" });
    return out;
  }
  try {
    await cdp.evaluate(FREQ_RESET_EXPR, EVAL_TIMEOUT_MS);
    await cdp.send("Input.dispatchKeyEvent", { type: "keyDown", key: "a", code: "KeyA", text: "a" });
    await cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key: "a", code: "KeyA" });
    await sleep(300);
    const read = await cdp.evaluate(FREQ_READ_EXPR, EVAL_TIMEOUT_MS);
    const agg = aggregate(read.events || []);
    out.push({ gesture: "keystroke", reps: 1, totalInvokes: agg.total, meanPerRep: agg.total, argsBytesTotal: agg.argsBytes, byCommand: agg.byCommand, dropped: read.dropped || 0, note: "single 'a' keypress; count may be 0 if no binding invokes IPC" });
  } catch (e) {
    out.push({ gesture: "keystroke", skipped: true, reason: clean(e?.message || String(e)).slice(0, 120) });
  }
  try {
    const { x, y } = canvasInfo;
    await cdp.evaluate(FREQ_RESET_EXPR, EVAL_TIMEOUT_MS);
    await cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1 });
    for (let i = 1; i <= 5; i++) {
      await cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: x + i * 8, y });
    }
    await cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: x + 40, y, button: "left", clickCount: 1 });
    await sleep(500);
    const read = await cdp.evaluate(FREQ_READ_EXPR, EVAL_TIMEOUT_MS);
    const agg = aggregate(read.events || []);
    out.push({ gesture: "brushStroke", reps: 1, totalInvokes: agg.total, meanPerRep: agg.total, argsBytesTotal: agg.argsBytes, byCommand: agg.byCommand, dropped: read.dropped || 0, note: "pointer chain at canvas center; count depends on active tool" });
  } catch (e) {
    out.push({ gesture: "brushStroke", skipped: true, reason: clean(e?.message || String(e)).slice(0, 120) });
  }
  return out;
}

// Snapshot-scale section: isolates snapshot-cost scaling (the suspected
// undo-lag amplifier). Seeds docs with 1/8/32 layers via the same blank-doc
// + facade-seed path as FREQ_SETUP_FN (small bitmaps), then times
// protocol_snapshot_native vs protocol_layer_ids_native per size, 5 reps
// each, reporting mean ms + mean response bytes. Read-only probes: no model
// mutation, no new env knobs, ASCII only.
async function runSnapshotScale(cdp) {
  const rows = [];
  for (const size of SCALE_SIZES) {
    const setupExpr = `(async () => { const mod = await import(location.origin + '/src/lib/protocol/facadeRegistry.ts'); const ed = window.__photrezEditor; const ws = ed.workspace; const id = 'fq-scale-' + ${size} + '-' + Date.now(); const session = ws.constructor.createBlankDocument(id, 'Scale Doc', 64, 64, { backgroundColor: 'white' }); ws.addDocument(session); const engine = ws.getActiveEngine(); for (let k = 1; k < ${size}; k++) { try { engine.addLayer('scale-' + k, 64, 64); } catch (e) {} } const facade = mod.getFacade(id); await mod.seedFacadeFromEngine(engine, facade); window.__fqScale = { docId: id }; return { docId: id, layerCount: engine.getLayers().length }; })()`;
    let setup = null;
    try {
      setup = await cdp.evaluate(setupExpr, EVAL_TIMEOUT_MS);
    } catch (e) {
      rows.push({ gesture: "snapshot-scale-" + size, skipped: true, reason: clean(e?.message || String(e)).slice(0, 120) });
      rows.push({ gesture: "layerids-scale-" + size, skipped: true, reason: clean(e?.message || String(e)).slice(0, 120) });
      continue;
    }
    for (const probe of ["protocol_snapshot_native", "protocol_layer_ids_native"]) {
      const kind = (probe === "protocol_snapshot_native" ? "snapshot-scale-" : "layerids-scale-") + size;
      const times = [];
      const bytes = [];
      let failed = null;
      for (let r = 0; r < SCALE_REPS; r++) {
        try {
          const m = await cdp.evaluate(
            `(async () => { const nc = await import(location.origin + '/src/lib/protocol/nativeClient.ts'); const docId = window.__fqScale.docId; const t0 = performance.now(); const res = await nc.nativeProtocol.${probe}(docId); const dt = performance.now() - t0; const n = (typeof res === 'string') ? res.length : JSON.stringify(res).length; return { ms: dt, bytes: n }; })()`,
            EVAL_TIMEOUT_MS,
          );
          times.push(m.ms);
          bytes.push(m.bytes);
        } catch (e) {
          failed = clean(e?.message || String(e)).slice(0, 120);
          break;
        }
      }
      if (failed !== null) {
        rows.push({ gesture: kind, skipped: true, reason: failed });
      } else {
        const mean = times.reduce((s, v) => s + v, 0) / times.length;
        const meanBytes = Math.round(bytes.reduce((s, v) => s + v, 0) / bytes.length);
        rows.push({ gesture: kind, reps: SCALE_REPS, totalInvokes: SCALE_REPS, meanPerRep: 1, meanMs: Number(mean.toFixed(2)), meanBytes, argsBytesTotal: 0, byCommand: { [probe]: SCALE_REPS }, layers: (setup && setup.layerCount) || size, note: "scale probe: mean ms + mean response bytes over " + SCALE_REPS + " reps" });
      }
    }
  }
  return rows;
}

// Scaled-gesture section: does gesture wall grow with layer count (native
// only; legacy wall delta already known at 1 layer). Reuses the scale-doc
// seeding path (blank doc + engine.addLayer fill + facade seed) and the
// page-side wall + DEV-counter read from runGesture; skips firstVisible
// (proven environmental) and reports one row per gesture per size.
async function runScaledGestures(cdp) {
  const rows = [];
  for (const size of SCALE_SIZES) {
    const setupExpr = `(async () => { const mod = await import(location.origin + '/src/lib/protocol/facadeRegistry.ts'); const ed = window.__photrezEditor; const ws = ed.workspace; const id = 'fq-gesture-scale-' + ${size} + '-' + Date.now(); const session = ws.constructor.createBlankDocument(id, 'Gesture Scale Doc', 64, 64, { backgroundColor: 'white' }); ws.addDocument(session); const engine = ws.getActiveEngine(); for (let k = 1; k < ${size}; k++) { try { engine.addLayer('gscale-' + k, 64, 64); } catch (e) {} } const facade = mod.getFacade(id); await mod.seedFacadeFromEngine(engine, facade); const layers = engine.getLayers(); const target = layers.find((l) => !l.isBackground) || layers[layers.length - 1]; window.__fq = { mod, engine, facade, ws, docId: id, targetId: target.id }; window.__fqScale = { docId: id }; return { docId: id, targetId: target.id, layerCount: layers.length }; })()`;
    let setup = null;
    try {
      setup = await cdp.evaluate(setupExpr, EVAL_TIMEOUT_MS);
    } catch (e) {
      const reason = clean(e?.message || String(e)).slice(0, 120);
      rows.push({ gesture: "visibility-scale-" + size, skipped: true, reason });
      rows.push({ gesture: "undo-scale-" + size, skipped: true, reason });
      continue;
    }
    // Undo needs real history: a fresh scale doc has none, so undo reps
    // would restore nothing (vacuous walls, zero invokes). Seed exactly
    // SCALE_REPS value-changing visibility toggles first, alternating from
    // the live state, so each undo rep pops a real entry. Single
    // round-trip; measured reps reset the counter per rep, so seeding
    // never pollutes the table.
    let seedError = null;
    try {
      await cdp.evaluate(
        `(async () => { const { engine, facade } = window.__fq; let cur = true; try { const found = facade.snapshot.layers.find((l) => l.id === window.__fq.targetId); if (found && typeof found.visible === "boolean") cur = found.visible; } catch (e) {} for (let s = 0; s < ${SCALE_REPS}; s++) { cur = !cur; const snap = await facade.setLayerVisibility(window.__fq.targetId, cur); engine.applyFacadeSnapshot(snap); } return true; })()`,
        EVAL_TIMEOUT_MS,
      );
    } catch (e) {
      seedError = clean(e?.message || String(e)).slice(0, 120);
    }
    for (const kind of ["visibility", "undo"]) {
      if (kind === "undo" && seedError !== null) {
        rows.push({ gesture: kind + "-scale-" + size, skipped: true, reason: "history seed failed: " + seedError });
        continue;
      }
      const perRep = [];
      const wallMs = [];
      let dropped = 0;
      let failed = null;
      for (let i = 0; i < SCALE_REPS; i++) {
        try {
          await cdp.evaluate(FREQ_RESET_EXPR, EVAL_TIMEOUT_MS);
          const timed = await cdp.evaluate(
            `(async () => { const { engine, facade } = window.__fq; const t0 = performance.now(); ${gestureBody(kind, i)} const dt = performance.now() - t0; return { ms: dt }; })()`,
            EVAL_TIMEOUT_MS,
          );
          const read = await cdp.evaluate(FREQ_READ_EXPR, EVAL_TIMEOUT_MS);
          const agg = aggregate(read.events || []);
          dropped += read.dropped || 0;
          perRep.push(agg);
          if (timed && typeof timed.ms === "number") wallMs.push(timed.ms);
        } catch (e) {
          failed = clean(e?.message || String(e)).slice(0, 120);
          break;
        }
      }
      if (failed !== null) {
        rows.push({ gesture: kind + "-scale-" + size, skipped: true, reason: failed });
        continue;
      }
      const totals = { total: 0, argsBytes: 0, byCommand: {} };
      for (const r of perRep) {
        totals.total += r.total;
        totals.argsBytes += r.argsBytes;
        for (const [cmd, c] of Object.entries(r.byCommand)) {
          totals.byCommand[cmd] = (totals.byCommand[cmd] || 0) + c;
        }
      }
      rows.push({
        gesture: kind + "-scale-" + size,
        reps: SCALE_REPS,
        totalInvokes: totals.total,
        meanPerRep: totals.total / SCALE_REPS,
        meanWallMs: wallMs.length
          ? Number((wallMs.reduce((s, v) => s + v, 0) / wallMs.length).toFixed(2))
          : null,
        fvMs: null,
        argsBytesTotal: totals.argsBytes,
        byCommand: totals.byCommand,
        dropped,
        layers: (setup && setup.layerCount) || size,
        note: "scale gesture: page-side wall mean over " + SCALE_REPS + " reps; excludes the composited frame",
      });
    }
  }
  return rows;
}

// Table columns carry mean ms where a row measured it page-side
// (scale probes and gesture walls); "-" where no page-side timer ran.
// The notes column repeats each row's caveat (gesture walls exclude the
// composited frame); skipped rows keep their reason in the same column.
function formatTable(rows, modeLabel) {
  const head = ["gesture", "reps", "total", "mean/rep", "mean ms", "fv ms", "args bytes", "by command", "notes"];
  const body = rows.map((r) => {
    if (r.skipped) return [r.gesture, "SKIP", "-", "-", "-", "-", "-", r.reason || "", ""];
    const cmds = Object.entries(r.byCommand || {}).map(([c, n]) => `${c}=${n}`).join(" ");
    const meanMs = (typeof r.meanMs === "number") ? r.meanMs.toFixed(2) : (typeof r.meanWallMs === "number" ? r.meanWallMs.toFixed(2) : "-");
    const fv = (typeof r.fvMs === "number") ? r.fvMs.toFixed(2) : "-";
    return [
      String(r.gesture),
      String(r.reps),
      String(r.totalInvokes),
      Number(r.meanPerRep).toFixed(2),
      meanMs,
      fv,
      String(r.argsBytesTotal),
      cmds.slice(0, 90),
      String(r.note || "").slice(0, 80),
    ];
  });
  const widths = head.map((h, i) => Math.max(h.length, ...body.map((b) => String(b[i]).length)));
  const line = (cells) => cells.map((c, i) => String(c).padEnd(widths[i])).join(" | ");
  const bar = widths.map((w) => "-".repeat(w)).join("-+-");
  return clean(
    [`photrez ipc frequency (live, mode=${modeLabel}, N=${N})`, "fv ms = firstVisible-approx (2x rAF proxy for the composited frame, not a GPU timestamp; dev-build vsync may vary)", line(head), bar, ...body.map(line)].join("\n"),
  );
}

function fail(stage, message) {
  err(`FAIL [${stage}]: ${message}`);
}

async function captureForMode(modeName, flagRaw) {
  const flagPairs = flagPairsFor(flagRaw);
  log(`--- mode=${modeName} flags: ${flagPairs.map((f) => f.key + "=" + f.value).join(", ")} ---`);
  sawRunningExe = false;
  buildFailed = false;
  buildErrorLine = "";
  childExitedBeforeRunning = false;
  childExitCode = null;
  let cdp = null;
  try {
    log("launching: bun run tauri dev (with --remote-debugging-port=" + CDP_PORT + ")");
    spawnApp();
    const launchStart = Date.now();
    let cdpReady = false;
    while (Date.now() - launchStart < LAUNCH_TIMEOUT_MS) {
      if (childExitedBeforeRunning || buildFailed) break;
      if (sawRunningExe && (await cdpVersionUp())) { cdpReady = true; break; }
      await sleep(1000);
    }
    if (!cdpReady) {
      if (childExitedBeforeRunning) fail("launch", `tauri dev exited (code ${childExitCode ?? "?"}) before the binary launched.`);
      else if (buildFailed) fail("launch", "tauri dev build failed: " + buildErrorLine);
      else if (!sawRunningExe) fail("launch", "tauri dev launched but the 'Running ...exe' line never appeared.");
      else fail("launch", `CDP /json/version never reachable on 127.0.0.1:${CDP_PORT} within ${LAUNCH_TIMEOUT_MS}ms.`);
      return { ok: false };
    }
    log("CDP endpoint ready; attaching...");
    let attachTarget = null;
    const attachT0 = Date.now();
    while (Date.now() - attachT0 < 90000) {
      try { attachTarget = await findPageTarget(); if (attachTarget) break; } catch {}
      await sleep(1000);
    }
    cdp = attachTarget ? await connectCdp() : null;
    if (!cdp) { fail("attach", "no page target with webSocketDebuggerUrl."); return { ok: false }; }
    if (!(await waitDocReady(cdp, 60000))) { fail("ready", "document never ready before flag seeding."); return { ok: false }; }
    // Single reload seeds the mode flags into localStorage before the app
    // boots. It exists for flags, not for the counter: the DEV recorder
    // lives in the app module itself, so no pre-load installation is needed.
    await cdp.evaluate(
      flagPairs.map((f) => `localStorage.setItem(${JSON.stringify(f.key)}, ${JSON.stringify(f.value)});`).join(""),
    );
    await cdp.evaluate("location.reload()");
    await sleep(1500);
    cdp.close();
    cdp = null;
    let target = null;
    const t0 = Date.now();
    while (Date.now() - t0 < 60000) {
      try { target = await findPageTarget(); if (target) break; } catch {}
      await sleep(1000);
    }
    if (!target) { fail("reload", "lost CDP target after reload."); return { ok: false }; }
    cdp = await connectCdp();
    if (!(await waitDocReady(cdp, 60000))) { fail("ready", "document not ready after reload."); return { ok: false }; }
    if (!(await waitEditorReady(cdp, 60000))) { fail("ready", "window.__photrezEditor never ready."); return { ok: false }; }
    if (modeName === "native" && !(await waitFacadeReady(cdp, 90000))) {
      fail("ready", "facade/wasm engine never armed (native mode needs photrez.facade=1).");
      return { ok: false };
    }
    // Counter canary: dynamically import the app's nativeClient module exactly
    // like FREQ_SETUP_FN imports facadeRegistry, drive one real
    // protocol_version_native call, and assert the DEV counter observed it.
    // A missing or silent counter fails loudly at the counter stage.
    const seam = { source: "nativeClient-dev-counter" };
    const canary = await cdp.evaluate("(async () => { var before = Array.isArray(window.__ipcFreqNative) ? window.__ipcFreqNative.length : 0; try { var m = await import(location.origin + '/src/lib/protocol/nativeClient.ts'); try { await m.nativeProtocol.protocol_version_native('default'); } catch (e) {} } catch (e) { return { threw: String((e && e.message) || e), before: before, after: before }; } var after = Array.isArray(window.__ipcFreqNative) ? window.__ipcFreqNative.length : 0; return { threw: null, before: before, after: after }; })()", EVAL_TIMEOUT_MS);
    log(`counter active (in-client DEV recorder): before=${canary && canary.before} after=${canary && canary.after}`);
    if (canary && canary.threw !== null && canary.threw !== undefined) {
      fail("counter", "canary threw before invoking: " + clean(canary.threw).slice(0, 160));
      return { ok: false };
    }
    if (!canary || !(canary.after >= canary.before + 1)) {
      fail("counter", "counter bypassed: nativeClient invoke not observed (before=" + ((canary && canary.before) ?? "?") + " after=" + ((canary && canary.after) ?? "?") + ")");
      return { ok: false };
    }
    await cdp.evaluate(FREQ_RESET_EXPR, EVAL_TIMEOUT_MS);
    const setup = await cdp.evaluate(FREQ_SETUP_FN, EVAL_TIMEOUT_MS);
    log(`doc ready: docId=${setup.docId} target=${String(setup.targetId).slice(0, 8)} layers=${setup.layerCount}`);
    // Authority assertion: fail loud when the page did not land on the
    // dispatch path the mode requires. Without this a stale persisted
    // facadeAuthority silently yields real walls with zero native invokes.
    const authority = await cdp.evaluate("(async () => { try { const m = await import(location.origin + '/src/lib/protocol/bridge.ts'); return { facade: m.isFacadeEnabled(), native: m.isNativeAuthority() }; } catch (e) { return { threw: String((e && e.message) || e) }; } })()", EVAL_TIMEOUT_MS);
    const wantNative = modeName === "native";
    if (!authority || authority.threw || authority.native !== wantNative) {
      fail("authority", "mode=" + modeName + " needs nativeAuthority=" + wantNative + " but observed " + clean(JSON.stringify(authority)).slice(0, 160));
      return { ok: false };
    }
    log(`authority ok (mode=${modeName} native=${authority.native} facade=${authority.facade})`);
    const rows = [];
    for (const g of GESTURES) {
      log(`gesture: ${g} x${N} ...`);
      rows.push(await runGesture(cdp, g));
    }
    for (const s of await runStretch(cdp)) rows.push(s);
    for (const s of await runSnapshotScale(cdp)) rows.push(s);
    for (const s of await runScaledGestures(cdp)) rows.push(s);
    if (modeName === "native") {
      const total = rows.filter((r) => !r.skipped).reduce((s, r) => s + (r.totalInvokes || 0), 0);
      if (total === 0) err("WARN [counter]: native-mode gesture table totals zero across all reps; counts may indicate a bypassed seam.");
    }
    return { ok: true, rows, seam };
  } catch (e) {
    fail("harness", clean(e?.stack || e?.message || String(e)).split("\n").slice(0, 3).join(" | "));
    return { ok: false };
  } finally {
    try { if (cdp) cdp.close(); } catch {}
    await cleanup();
    await sleep(2000);
  }
}

async function main() {
  log("=== Photrez live IPC frequency probe ===");
  log(`mode=${MODE} N=${N} cdpPort=${CDP_PORT}`);

  if (await netProbe(CDP_PORT)) {
    err("");
    err(`BLOCKED: CDP endpoint 127.0.0.1:${CDP_PORT} is already open.`);
    err("Close the running instance on this port, then re-run this script.");
    err("");
    process.exit(2);
  }

  const modes = MODE === "both" ? ["native", "legacy"] : [MODE];
  const payload = { capturedAt: new Date().toISOString(), reps: N, modes: {} };
  let failed = false;
  for (const m of modes) {
    const res = await captureForMode(m, MODE_FLAGS[m]);
    if (!res.ok) { failed = true; break; }
    payload.modes[m] = { flags: MODE_FLAGS[m], seam: res.seam, gestures: res.rows };
    log("");
    log(formatTable(res.rows, m));
    log("");
  }

  if (!failed) {
    fs.mkdirSync(path.dirname(OUT_PATH), { recursive: true });
    fs.writeFileSync(OUT_PATH, JSON.stringify(payload, null, 2) + "\n", "utf8");
    log(`saved: ${OUT_PATH}`);
  }
  process.exit(failed ? 1 : 0);
}

main();
