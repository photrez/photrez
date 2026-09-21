#!/usr/bin/env bun
/*
 * perf-audit-live.mjs - live in-app latency battery runner (TOOLING ONLY, dev-only).
 *
 * Why this exists:
 *   Unit tests mock the IPC/render boundary, so reported latency splits can
 *   drift from the real app. This script boots the REAL desktop app, calls the
 *   dev-only in-page battery (window.__photrezPerfAudit), saves the returned
 *   rows to a JSON file, and prints the same aligned table to stdout.
 *
 * Launch/CDP/teardown conventions mirror scripts/live-verify.mjs (spawn
 * `bun run tauri dev` with a remote-debugging port, poll /json, raw CDP
 * WebSocket over bun fetch+WebSocket, kill only the spawned tree).
 * Default CDP port is 9223 so a live-verify run (default 9222) is not disturbed.
 *
 * Run (from repo root):
 *   bun scripts/perf-audit-live.mjs --help
 *   PHOTREZ_FLAGS="facade=1" bun scripts/perf-audit-live.mjs
 *
 * Env knobs:
 *   PHOTREZ_CDP_PORT         CDP debug port (default 9223).
 *   PHOTREZ_LAUNCH_TIMEOUT_MS  launch + CDP-ready budget (default 240000).
 *   PHOTREZ_FLAGS            space/comma separated key=value (default "facade=1");
 *                            mapped to localStorage["photrez.<key>"] at runtime.
 *   PHOTREZ_AUDIT_DIMS       optional "WxH" (e.g. "1536x2304"); default = in-app default.
 *   PHOTREZ_AUDIT_OUT        optional output path; default <tmpdir>/photrez-perf-audit-<timestamp>.json.
 *   PHOTREZ_AUDIT_TIMEOUT_MS budget for the battery call (default 600000).
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

const CDP_PORT = Number(process.env.PHOTREZ_CDP_PORT || 9223);
const LAUNCH_TIMEOUT_MS = Number(process.env.PHOTREZ_LAUNCH_TIMEOUT_MS || 240000);
const AUDIT_TIMEOUT_MS = Number(process.env.PHOTREZ_AUDIT_TIMEOUT_MS || 600000);
const REPO_ROOT = process.cwd();

if (process.argv.includes("--help") || process.argv.includes("-h")) {
  console.log("Usage: bun scripts/perf-audit-live.mjs [options]");
  console.log("");
  console.log("Boots the real desktop app, runs the in-app latency battery");
  console.log("(window.__photrezPerfAudit), writes rows to a JSON file, prints a table.");
  console.log("");
  console.log("Options:");
  console.log("  --help, -h     Print this usage and exit 0 (spawns nothing).");
  console.log("");
  console.log("Env:");
  console.log("  PHOTREZ_CDP_PORT          CDP debug port (default 9223).");
  console.log("  PHOTREZ_LAUNCH_TIMEOUT_MS launch + CDP-ready budget ms (default 240000).");
  console.log("  PHOTREZ_FLAGS             key=value list, default \"facade=1\".");
  console.log("  PHOTREZ_AUDIT_DIMS        optional WxH, e.g. \"1536x2304\".");
  console.log("  PHOTREZ_AUDIT_OUT         optional output path.");
  console.log("  PHOTREZ_AUDIT_TIMEOUT_MS  battery call budget ms (default 600000).");
  console.log("");
  console.log("Exit codes: 0 captured, 1 failure, 2 blocked by env.");
  process.exit(0);
}

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

function parseDims(raw) {
  if (!raw) return null;
  const m = String(raw).trim().match(/^(\d+)\s*x\s*(\d+)$/i);
  if (!m) return null;
  const w = Math.max(1, Math.min(8192, Number(m[1])));
  const h = Math.max(1, Math.min(8192, Number(m[2])));
  if (!Number.isFinite(w) || !Number.isFinite(h)) return null;
  return { w, h };
}

const DIMS = parseDims(process.env.PHOTREZ_AUDIT_DIMS || "");
if (process.env.PHOTREZ_AUDIT_DIMS && !DIMS) {
  console.error(`FAIL [dims]: PHOTREZ_AUDIT_DIMS must look like "1536x2304", got ${JSON.stringify(process.env.PHOTREZ_AUDIT_DIMS)}`);
  process.exit(1);
}

const OUT_PATH =
  process.env.PHOTREZ_AUDIT_OUT ||
  path.join(os.tmpdir(), `photrez-perf-audit-${Date.now()}.json`);

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
    this.opened = null;
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

async function waitAuditReady(cdp, timeoutMs) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const ok = await cdp.evaluate("typeof window.__photrezPerfAudit === 'function'");
      if (ok) return true;
    } catch {}
    await sleep(500);
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
const tauriErrors = [];

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

function cellText(cell) {
  if (cell === "ERROR") return "ERROR";
  if (typeof cell !== "number" || !Number.isFinite(cell) || cell < 0) return "-";
  return cell.toFixed(1);
}

function formatTable(rows, dims) {
  const head = ["op", "total ms", "invoke ms", "raster ms", "upload ms", "snap/hist ms", "notes"];
  const body = rows.map((r) => [
    String(r.op ?? "?"),
    cellText(r.totalMs),
    cellText(r.invokeMs),
    cellText(r.rasterMs),
    cellText(r.uploadMs),
    cellText(r.snapHistMs),
    String(r.notes ?? ""),
  ]);
  const widths = head.map((h, i) => Math.max(h.length, ...body.map((b) => b[i].length)));
  const line = (cells) =>
    cells.map((c, i) => (i === 0 || i === cells.length - 1 ? c.padEnd(widths[i]) : c.padStart(widths[i]))).join(" | ");
  const bar = widths.map((w) => "-".repeat(w)).join("-+-");
  const label = DIMS ? `${DIMS.w}x${DIMS.h}` : (dims ? `${dims.w}x${dims.h}` : "default dims");
  return clean(
    [`photrez perf audit (live, ${label}, ${rows.length} rows, ms, - = not part of row)`, line(head), bar, ...body.map(line)].join("\n"),
  );
}

function fail(stage, message) {
  err(`FAIL [${stage}]: ${message}`);
}

async function main() {
  log("=== Photrez live perf audit ===");
  log(`flags: ${FLAG_PAIRS.map((f) => f.key + "=" + f.value).join(", ") || "(none)"}  cdpPort=${CDP_PORT}  dims=${DIMS ? DIMS.w + "x" + DIMS.h : "(in-app default)"}`);

  if (await netProbe(CDP_PORT)) {
    err("");
    err(`BLOCKED: CDP endpoint 127.0.0.1:${CDP_PORT} is already open.`);
    err("Close the running instance on this port, then re-run this script.");
    err("");
    process.exit(2);
  }

  let exitCode = 0;
  let cdp = null;
  const launchStart = Date.now();
  try {
    log("launching: bun run tauri dev (with --remote-debugging-port=" + CDP_PORT + ")");
    spawnApp();

    let cdpReady = false;
    while (Date.now() - launchStart < LAUNCH_TIMEOUT_MS) {
      if (childExitedBeforeRunning || buildFailed) break;
      if (sawRunningExe && (await cdpVersionUp())) { cdpReady = true; break; }
      await sleep(1000);
    }

    if (!cdpReady) {
      if (childExitedBeforeRunning) {
        fail("launch", `tauri dev process exited (code ${childExitCode ?? "?"}) before the app binary launched.`);
      } else if (buildFailed) {
        fail("launch", "tauri dev build failed: " + buildErrorLine);
      } else if (!sawRunningExe) {
        fail("launch", "tauri dev launched but the 'Running ...exe' binary line never appeared.");
      } else {
        fail("launch", `CDP /json/version never reachable on 127.0.0.1:${CDP_PORT} within ${LAUNCH_TIMEOUT_MS}ms.`);
      }
      exitCode = 1;
    } else {
      log("CDP endpoint ready; attaching...");
      cdp = await connectCdp();
      if (!cdp) {
        fail("attach", "no page target with webSocketDebuggerUrl.");
        exitCode = 1;
      } else if (!(await waitDocReady(cdp, 60000))) {
        fail("ready", "app document never reached readyState complete before flag seeding.");
        exitCode = 1;
      } else {
        await cdp.evaluate(
          FLAG_PAIRS.map((f) => `localStorage.setItem(${JSON.stringify(f.key)}, ${JSON.stringify(f.value)});`).join(""),
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
        if (!target) {
          fail("reload", "lost CDP target after reload.");
          exitCode = 1;
        } else {
          cdp = await connectCdp();
          if (!(await waitDocReady(cdp, 60000))) {
            fail("ready", "app document not ready after reload/flag application.");
            exitCode = 1;
          } else if (!(await waitAuditReady(cdp, 120000))) {
            fail("ready", "window.__photrezPerfAudit never became a function (dev battery not loaded).");
            exitCode = 1;
          } else {
            log("battery ready; running...");
            const expr = DIMS
              ? `window.__photrezPerfAudit({ w: ${DIMS.w}, h: ${DIMS.h} })`
              : "window.__photrezPerfAudit()";
            let rows;
            try {
              rows = await cdp.evaluate(expr, AUDIT_TIMEOUT_MS);
            } catch (e) {
              fail("battery", clean(e?.message || String(e)));
              exitCode = 1;
            }
            if (exitCode === 0) {
              if (!Array.isArray(rows) || rows.length === 0) {
                fail("battery", "battery returned no rows.");
                exitCode = 1;
              } else {
                const payload = {
                  dims: DIMS,
                  rows,
                  capturedAt: new Date().toISOString(),
                  flags: FLAG_PAIRS.map((f) => `${f.key}=${f.value}`),
                };
                fs.mkdirSync(path.dirname(OUT_PATH), { recursive: true });
                fs.writeFileSync(OUT_PATH, JSON.stringify(payload, null, 2) + "\n", "utf8");
                log("");
                log(formatTable(rows, DIMS));
                log("");
                log(`saved: ${OUT_PATH}`);
              }
            }
          }
        }
      }
    }
  } catch (e) {
    fail("harness", clean(e?.stack || e?.message || String(e)));
    exitCode = 1;
  } finally {
    try { if (cdp) cdp.close(); } catch {}
    await cleanup();
  }

  process.exit(exitCode);
}

main();
