// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Dev-only latency self-test. Runs a scripted battery on a generated scratch
// document and prints ONE timing table with per-op invoke, raster, upload,
// and snapshot/history costs, so one device run reports the full split.
//
// Run in the devtools console under `bun run tauri dev`:
//   await window.__photrezPerfAudit()
//   await window.__photrezPerfAudit({ w: 1536, h: 2304 })
// Paste the printed table back. Production builds never load this module: the
// only import site is gated by import.meta.env.DEV (benchRealEngineDev precedent).

import type { DocumentEngine } from "@/engine/document";
import type { LayerNode } from "@/engine/types";

export interface PerfAuditDims {
  w: number;
  h: number;
}

export type PerfCell = number | "ERROR";

export interface PerfAuditRow {
  op: string;
  totalMs: PerfCell;
  invokeMs: PerfCell;
  rasterMs: PerfCell;
  uploadMs: PerfCell;
  snapHistMs: PerfCell;
  notes: string;
}

export type PerfRowCells = Omit<PerfAuditRow, "op">;

export interface PerfScratch {
  engine: DocumentEngine;
  layerId: LayerNode["id"];
  width: number;
  height: number;
  closed: boolean;
  bitmaps: ImageBitmap[];
}

export type RowRunner = (scratch: PerfScratch, ctx?: PerfRunCtx) => Promise<PerfRowCells>;

export interface PerfGlBackend {
  uploadFull: (id: string, bmp: ImageBitmap) => void;
  uploadPatch: (id: string, bmp: ImageBitmap) => void;
  dispose: () => void;
}

export type PerfGlFactory = (w: number, h: number) => Promise<PerfGlBackend>;

export interface PerfRunCtx {
  makeGl: PerfGlFactory;
}

export interface PerfAuditOptions {
  dims?: PerfAuditDims;
  runners?: Record<string, RowRunner>;
  buildScratch?: (w: number, h: number) => Promise<PerfScratch>;
  closeScratch?: (scratch: PerfScratch) => Promise<void>;
  makeGl?: PerfGlFactory;
}

export const PERF_AUDIT_OPS: readonly string[] = [
  "meta-routed",
  "brush-commit",
  "undo-legacy",
  "undo-facade",
  "save-serialize",
  "export-bake",
  "upload-full-patch",
  "selection-draw",
  "drag-per-move",
  "gpu-composite",
  "text-raster",
  "shape-raster",
  "ipc-probe",
];

const DEFAULT_W = 3072;
const DEFAULT_H = 4608;
const NOT_PART = -1;

function round1(ms: number): number {
  if (ms < 0 || !Number.isFinite(ms)) return NOT_PART;
  return Math.round(ms * 10) / 10;
}

function cleanNote(text: string): string {
  return text.replace(/[^\x20-\x7E]/g, "?").slice(0, 160);
}

function cleanTable(text: string): string {
  return text.replace(/[^\x20-\x7E\n]/g, "?");
}

function messageOf(err: unknown): string {
  if (err instanceof Error) return err.message;
  return String(err);
}

function errorRow(op: string, err: unknown): PerfAuditRow {
  return {
    op,
    totalMs: "ERROR",
    invokeMs: "ERROR",
    rasterMs: "ERROR",
    uploadMs: "ERROR",
    snapHistMs: "ERROR",
    notes: cleanNote(messageOf(err)),
  };
}

function normalizeDims(dims: PerfAuditDims | undefined): PerfAuditDims {
  const w = Math.max(1, Math.min(8192, Math.floor(dims?.w ?? DEFAULT_W)));
  const h = Math.max(1, Math.min(8192, Math.floor(dims?.h ?? DEFAULT_H)));
  return { w, h };
}

function closeUnknown(value: unknown): void {
  if (typeof value !== "object" || value === null) return;
  const bitmap = (value as { bitmap?: unknown }).bitmap;
  if (typeof bitmap !== "object" || bitmap === null) return;
  const close = (bitmap as { close?: unknown }).close;
  if (typeof close === "function") (bitmap as { close: () => void }).close();
}

function scratchBitmap(scratch: PerfScratch): ImageBitmap {
  const bitmap = scratch.engine.getLayer(scratch.layerId)?.imageBitmap;
  if (!bitmap) throw new Error("scratch layer has no bitmap");
  return bitmap;
}

async function makeGradientBitmap(w: number, h: number): Promise<ImageBitmap> {
  const canvas = new OffscreenCanvas(w, h);
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("2d context unavailable for scratch bitmap");
  const grad = ctx.createLinearGradient(0, 0, w, h);
  grad.addColorStop(0, "#203040");
  grad.addColorStop(0.5, "#8090a0");
  grad.addColorStop(1, "#101820");
  ctx.fillStyle = grad;
  ctx.fillRect(0, 0, w, h);
  ctx.fillStyle = "#ffffff";
  for (let i = 0; i < 8; i++) ctx.fillRect((i * w) / 8, 0, 2, h);
  return canvas.transferToImageBitmap();
}

// Read-only IPC round-trip probe. Uses a boolean getter command so the probe
// never writes anything, even against a live native registry.
async function probeInvokeMs(): Promise<number> {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const t0 = performance.now();
    await invoke("paint_parity_autorun_enabled");
    return performance.now() - t0;
  } catch {
    return NOT_PART;
  }
}

async function facadeState(): Promise<string> {
  try {
    const bridge = await import("@/lib/protocol/bridge");
    return bridge.isFacadeEnabled() ? "on" : "off";
  } catch {
    return "unreadable";
  }
}

export async function makeGlBackend(w: number, h: number): Promise<PerfGlBackend> {
  const mod = await import("@/renderer/webgl2");
  const canvas = document.createElement("canvas");
  canvas.width = w;
  canvas.height = h;
  const backend = new mod.WebGL2Backend();
  backend.initialize(canvas);
  const dispose = (): void => {
    try {
      backend.dispose();
    } catch {
      // Log-only harness: a failed dispose must not fail the measured row.
    }
    // dispose() frees GL objects but keeps the context slot, and browsers
    // cap live contexts (around 16). Lose it so repeated runs cannot exhaust
    // the budget while the canvas awaits garbage collection.
    try {
      canvas.getContext("webgl2")?.getExtension("WEBGL_lose_context")?.loseContext();
    } catch {
      // Context already gone.
    }
  };
  return {
    uploadFull: (id, bmp) => {
      backend.uploadImage(id, bmp);
    },
    uploadPatch: (id, bmp) => {
      backend.uploadImage(id, bmp, {
        x: 0,
        y: 0,
        width: Math.min(512, w),
        height: Math.min(512, h),
      });
    },
    dispose,
  };
}

export const defaultRunners: Record<string, RowRunner> = {
  "meta-routed": async (s) => {
    const t0 = performance.now();
    for (let i = 0; i < 20; i++) s.engine.setLayerOpacity(s.layerId, i % 2 === 0 ? 0.5 : 1);
    const batch = performance.now() - t0;
    return {
      totalMs: round1(batch / 20),
      invokeMs: round1(await probeInvokeMs()),
      rasterMs: NOT_PART,
      uploadMs: NOT_PART,
      snapHistMs: NOT_PART,
      notes: cleanNote(`20x setLayerOpacity avg, facade ${await facadeState()}; invoke = one read-only probe`),
    };
  },

  "brush-commit": async (s) => {
    const { CommandHistory } = await import("@/engine/history");
    const { compositeAllLayers } = await import("@/engine/layerComposite");
    const t0 = performance.now();
    const snap = s.engine.snapshot();
    const snapMs = performance.now() - t0;
    const hist = new CommandHistory();
    const t1 = performance.now();
    hist.commit(snap, "perf-audit-brush");
    const histMs = performance.now() - t1;
    const t2 = performance.now();
    const bmp = compositeAllLayers(s.engine.getLayers(), s.width, s.height);
    const rasterMs = performance.now() - t2;
    if (bmp) bmp.close();
    return {
      totalMs: round1(snapMs + histMs + rasterMs),
      invokeMs: NOT_PART,
      rasterMs: round1(rasterMs),
      uploadMs: NOT_PART,
      snapHistMs: round1(snapMs + histMs),
      notes: "snapshot+history+composite floor; live surface/upload split in [paint-commit] lines",
    };
  },

  "undo-legacy": async (s) => {
    const { CommandHistory } = await import("@/engine/history");
    const hist = new CommandHistory();
    hist.commit(s.engine.snapshot(), "perf-audit-undo");
    const t0 = performance.now();
    const prev = hist.undo(s.engine.snapshot());
    const undoMs = performance.now() - t0;
    let restoreMs = 0;
    if (prev) {
      const t = performance.now();
      s.engine.restore(prev);
      restoreMs = performance.now() - t;
    }
    const t2 = performance.now();
    const next = hist.redo(s.engine.snapshot());
    const redoMs = performance.now() - t2;
    if (next) s.engine.restore(next);
    return {
      totalMs: round1(undoMs + restoreMs + redoMs),
      invokeMs: NOT_PART,
      rasterMs: NOT_PART,
      uploadMs: NOT_PART,
      snapHistMs: round1(undoMs + restoreMs + redoMs),
      notes: "legacy TS undo + restore + redo on the scratch doc",
    };
  },

  "undo-facade": async (s) => {
    const t0 = performance.now();
    const snap = s.engine.snapshot();
    const snapMs = performance.now() - t0;
    const t1 = performance.now();
    s.engine.restore(snap);
    const restoreMs = performance.now() - t1;
    const invokeMs = await probeInvokeMs();
    return {
      totalMs: round1(snapMs + restoreMs + Math.max(0, invokeMs)),
      invokeMs: round1(invokeMs),
      rasterMs: NOT_PART,
      uploadMs: NOT_PART,
      snapHistMs: round1(snapMs + restoreMs),
      notes: cleanNote(`facade ${await facadeState()}; TS half + IPC floor, applied facade undo needs an open native doc`),
    };
  },

  "save-serialize": async (s) => {
    const { compositeAllLayers } = await import("@/engine/layerComposite");
    const t0 = performance.now();
    s.engine.snapshot();
    const snapMs = performance.now() - t0;
    const t1 = performance.now();
    const bmp = compositeAllLayers(s.engine.getLayers(), s.width, s.height);
    const compMs = performance.now() - t1;
    let encMs = NOT_PART;
    if (bmp) {
      try {
        const canvas = new OffscreenCanvas(s.width, s.height);
        const ctx = canvas.getContext("2d");
        if (!ctx) throw new Error("2d context unavailable for encode");
        ctx.drawImage(bmp, 0, 0);
        const t2 = performance.now();
        await canvas.convertToBlob({ type: "image/png" });
        encMs = performance.now() - t2;
      } finally {
        bmp.close();
      }
    }
    return {
      totalMs: round1(snapMs + compMs + Math.max(0, encMs)),
      invokeMs: NOT_PART,
      rasterMs: round1(compMs),
      uploadMs: NOT_PART,
      snapHistMs: round1(snapMs),
      notes: "snapshot + composite + PNG encode; disk write skipped; cache-hit path not exercised",
    };
  },

  "export-bake": async (s) => {
    const { encodeComposite } = await import("@/components/editor/exportDocument");
    const t0 = performance.now();
    const bytes = await encodeComposite(s.engine, "png", 90);
    const totalMs = performance.now() - t0;
    return {
      totalMs: round1(totalMs),
      invokeMs: NOT_PART,
      rasterMs: NOT_PART,
      uploadMs: NOT_PART,
      snapHistMs: NOT_PART,
      notes: `${(bytes.length / 1048576).toFixed(1)}MB png via the real export path incl. ensure+bake`,
    };
  },

  "upload-full-patch": async (s, ctx) => {
    const gl = await (ctx?.makeGl ?? makeGlBackend)(s.width, s.height);
    try {
      const bmp = scratchBitmap(s);
      const t0 = performance.now();
      gl.uploadFull("perf-audit-full", bmp);
      const full = performance.now() - t0;
      const t1 = performance.now();
      gl.uploadPatch("perf-audit-full", bmp);
      const patch = performance.now() - t1;
      return {
        totalMs: round1(full),
        invokeMs: NOT_PART,
        rasterMs: NOT_PART,
        uploadMs: round1(patch),
        snapHistMs: NOT_PART,
        notes: `FULL vs 512px PATCH delta=${round1(full - patch)}ms; mipmap share in [perf] uploadImage lines`,
      };
    } finally {
      try {
        gl.dispose();
      } catch {
        // Log-only harness: a failed dispose must not fail the measured row.
      }
    }
  },

  "selection-draw": async (s) => {
    const n = 120;
    const t0 = performance.now();
    for (let i = 0; i < n; i++) s.engine.createSelection(10 + (i % 50), 10, 200, 150);
    const batch = performance.now() - t0;
    return {
      totalMs: round1(batch),
      invokeMs: NOT_PART,
      rasterMs: NOT_PART,
      uploadMs: NOT_PART,
      snapHistMs: NOT_PART,
      notes: `${n}x createSelection wall, avg=${round1(batch / n)}ms; UI path is transient + 1 commit`,
    };
  },

  "drag-per-move": async (s) => {
    const n = 10;
    const t0 = performance.now();
    for (let i = 0; i < n; i++) s.engine.moveLayerSilent(s.layerId, i, i);
    const moves = performance.now() - t0;
    const t1 = performance.now();
    s.engine.flushChangeNotification();
    const flush = performance.now() - t1;
    return {
      totalMs: round1(moves + flush),
      invokeMs: NOT_PART,
      rasterMs: NOT_PART,
      uploadMs: NOT_PART,
      snapHistMs: NOT_PART,
      notes: `${n}x silent move + 1 flush; composite cadence in scheduler [perf] lines during a live drag`,
    };
  },

  "gpu-composite": async (s, ctx) => {
    const tInit0 = performance.now();
    const gl = await (ctx?.makeGl ?? makeGlBackend)(s.width, s.height);
    try {
      const initMs = performance.now() - tInit0;
      const t0 = performance.now();
      gl.uploadFull("perf-audit-gpu", scratchBitmap(s));
      const uploadMs = performance.now() - t0;
      return {
        totalMs: round1(initMs + uploadMs),
        invokeMs: NOT_PART,
        rasterMs: NOT_PART,
        uploadMs: round1(uploadMs),
        snapHistMs: NOT_PART,
        notes: "texture upload share; frame composite from the dev frame stats during a live drag",
      };
    } finally {
      try {
        gl.dispose();
      } catch {
        // Log-only harness: a failed dispose must not fail the measured row.
      }
    }
  },

  "text-raster": async (s) => {
    const { rasterizeText } = await import("@/engine/textRasterizer");
    const { DEFAULT_TEXT_DATA } = await import("@/engine/textTypes");
    const n = 3;
    let wall = 0;
    for (let i = 0; i < n; i++) {
      const t = performance.now();
      const res = rasterizeText({
        ...DEFAULT_TEXT_DATA,
        content: `Perf audit line ${i}`,
        boxMode: "area",
        boxWidth: s.width,
        boxHeight: s.height,
      });
      wall += performance.now() - t;
      closeUnknown(res);
    }
    return {
      totalMs: round1(wall),
      invokeMs: NOT_PART,
      rasterMs: round1(wall),
      uploadMs: NOT_PART,
      snapHistMs: NOT_PART,
      notes: `${n}x area-mode full-doc text raster; a keystroke also pays upload on top`,
    };
  },

  "shape-raster": async (s) => {
    const { renderShapeToBitmap } = await import("@/engine/shapeRaster");
    const n = 3;
    let wall = 0;
    for (let i = 0; i < n; i++) {
      const t = performance.now();
      const bmp = renderShapeToBitmap({
        kind: "rect",
        width: s.width,
        height: s.height,
        radius: 0,
        fill: { kind: "solid", color: "#808080" },
        stroke: { enabled: false, color: "#000000", width: 1 },
        arrowHead: false,
      });
      wall += performance.now() - t;
      bmp.close();
    }
    return {
      totalMs: round1(wall),
      invokeMs: NOT_PART,
      rasterMs: round1(wall),
      uploadMs: NOT_PART,
      snapHistMs: NOT_PART,
      notes: `${n}x full-doc shape raster; a drag move also pays upload on top`,
    };
  },

  "ipc-probe": async () => {
    let wall = 0;
    let ok = 0;
    for (let i = 0; i < 5; i++) {
      const ms = await probeInvokeMs();
      if (ms >= 0) {
        wall += ms;
        ok++;
      }
    }
    if (ok === 0) throw new Error("invoke unavailable (not a Tauri webview)");
    return {
      totalMs: round1(wall / ok),
      invokeMs: round1(wall / ok),
      rasterMs: NOT_PART,
      uploadMs: NOT_PART,
      snapHistMs: NOT_PART,
      notes: `${ok}x read-only probe avg, debug build; release needs a device run`,
    };
  },
};

export async function defaultBuildScratch(w: number, h: number): Promise<PerfScratch> {
  const { DocumentEngine } = await import("@/engine/document");
  const engine = new DocumentEngine("perf-audit-scratch", "Perf Audit Scratch", w, h);
  const layer = engine.addLayer("photo", w, h);
  const bmp = await makeGradientBitmap(w, h);
  engine.setLayerImageBitmap(layer.id, bmp);
  return { engine, layerId: layer.id, width: w, height: h, closed: false, bitmaps: [bmp] };
}

export async function defaultCloseScratch(scratch: PerfScratch): Promise<void> {
  try {
    for (const bmp of scratch.bitmaps) bmp.close();
    // The scratch engine never routes through the facade and the projection
    // refresh only peeks, so no namespaced facade entry should exist for it.
    // Evict by id anyway so a future runner that touches the facade cannot
    // leak one entry per run.
    const docId = scratch.engine?.getId?.();
    if (typeof docId === "string") {
      try {
        const mod = await import("@/lib/protocol/selectionMirror");
        mod.removeFacade(docId);
      } catch {
        // Cleanup only; never fail the close.
      }
    }
    // The engine holds a per-instance native mirror while the native module
    // is loaded, and it exposes no public releaser. Call the first releaser
    // present so repeated runs do not pile up one mirror per scratch doc.
    try {
      const mirror = (
        scratch.engine as unknown as {
          rustEngine?: { free?: () => void; dispose?: () => void; close?: () => void };
        }
      ).rustEngine;
      if (typeof mirror?.free === "function") mirror.free();
      else if (typeof mirror?.dispose === "function") mirror.dispose();
      else if (typeof mirror?.close === "function") mirror.close();
    } catch {
      // Best effort; never fail the close.
    }
  } finally {
    scratch.bitmaps = [];
    scratch.closed = true;
  }
}

function cellText(cell: PerfCell): string {
  if (cell === "ERROR") return "ERROR";
  if (cell < 0) return "-";
  return cell.toFixed(1);
}

export function formatPerfAudit(rows: PerfAuditRow[], closed: boolean, dims: PerfAuditDims): string {
  const head = ["op", "total ms", "invoke ms", "raster ms", "upload ms", "snap/hist ms", "notes"];
  const body = rows.map((r) => [
    r.op,
    cellText(r.totalMs),
    cellText(r.invokeMs),
    cellText(r.rasterMs),
    cellText(r.uploadMs),
    cellText(r.snapHistMs),
    r.notes,
  ]);
  const widths = head.map((h, i) => Math.max(h.length, ...body.map((b) => b[i].length)));
  const line = (cells: string[]): string =>
    cells.map((c, i) => (i === 0 || i === cells.length - 1 ? c.padEnd(widths[i]) : c.padStart(widths[i]))).join(" | ");
  const bar = widths.map((w) => "-".repeat(w)).join("-+-");
  const out = [
    `photrez perf audit (${dims.w}x${dims.h}, ${rows.length} rows, ms, - = not part of row)`,
    line(head),
    bar,
    ...body.map(line),
    `scratch: ${closed ? "closed" : "LEAK (close failed)"}`,
  ];
  return cleanTable(out.join("\n"));
}

export function printPerfAudit(rows: PerfAuditRow[], closed: boolean, dims: PerfAuditDims): void {
  console.log(formatPerfAudit(rows, closed, dims));
}

export async function runPerfAudit(options?: PerfAuditOptions): Promise<PerfAuditRow[]> {
  const dims = normalizeDims(options?.dims);
  const build = options?.buildScratch ?? defaultBuildScratch;
  const close = options?.closeScratch ?? defaultCloseScratch;
  const runners = { ...defaultRunners, ...options?.runners };
  const ctx: PerfRunCtx = { makeGl: options?.makeGl ?? makeGlBackend };
  const rows: PerfAuditRow[] = [];
  let scratch: PerfScratch | null = null;
  try {
    scratch = await build(dims.w, dims.h);
  } catch (err) {
    for (const op of PERF_AUDIT_OPS) rows.push(errorRow(op, err));
    printPerfAudit(rows, false, dims);
    return rows;
  }
  try {
    for (const op of PERF_AUDIT_OPS) {
      const run = runners[op];
      if (!run) {
        rows.push(errorRow(op, "no runner for row"));
        continue;
      }
      try {
        rows.push({ op, ...(await run(scratch, ctx)) });
      } catch (err) {
        rows.push(errorRow(op, err));
      }
    }
  } finally {
    try {
      await close(scratch);
    } catch {
      if (scratch) scratch.closed = false;
    }
  }
  printPerfAudit(rows, scratch.closed, dims);
  return rows;
}

export function installPerfAuditWindow(
  target: { __photrezPerfAudit?: (dims?: PerfAuditDims) => Promise<PerfAuditRow[]> },
  isDev: boolean,
): void {
  if (!isDev) return;
  target.__photrezPerfAudit = (dims) => runPerfAudit(dims ? { dims } : undefined);
}

if (typeof window !== "undefined" && import.meta.env.DEV) {
  installPerfAuditWindow(
    window as unknown as {
      __photrezPerfAudit?: (dims?: PerfAuditDims) => Promise<PerfAuditRow[]>;
    },
    true,
  );
  console.log("[bench] window.__photrezPerfAudit() ready - e.g. await window.__photrezPerfAudit()");
}
