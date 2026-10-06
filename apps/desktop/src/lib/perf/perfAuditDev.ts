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

import { pixelSeedDispatch, encodeRustTiles } from "@/lib/protocol/pixelSeedCall";
import type { DocumentEngine } from "@/engine/document";
import type { LayerNode } from "@/engine/types";

export interface PerfAuditDims {
  w: number;
  h: number;
}

export type PerfCell = number | "ERROR";

/** One read of the byte probe against one document at one stage of its commit run. */
export interface PerfByteSample {
  doc_id: string;
  stage:
    | "seeded"
    | "after-subtile-commits"
    | "after-full-layer-commit"
    | "control-full-layer-only";
  /** Commits recorded in the store when this read happened. */
  commits: number;
  layer_count: number;
  row_major_bytes: number;
  tile_total_bytes: number;
  shared_bytes: number;
  private_bytes: number;
  tile_count: number;
  state_count: number;
  tile_reference_count: number;
  /**
   * Tile-graph bytes the document owes but has not allocated, because a packed
   * canon is built on a layer's first commit. The `seeded` row is the one that
   * reads non-zero, and it is the row that would otherwise look like a cheap
   * document: the tile graph there is genuinely empty, but it is empty because
   * it has not been built, not because it is free.
   */
  owed_anchor_bytes: number;
  owed_anchor_layer_count: number;
  /** Wall time of this one probe read (IPC round trip + the Rust walk). */
  probe_ms: number;
}

export interface PerfAuditRow {
  op: string;
  totalMs: PerfCell;
  invokeMs: PerfCell;
  rasterMs: PerfCell;
  uploadMs: PerfCell;
  snapHistMs: PerfCell;
  notes: string;
  /** Present only on the byte-accounting row: the probe reads behind the numbers. */
  bytes?: PerfByteSample[];
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
  /** Square document sides (px) the byte-accounting row measures. */
  byteSizes: number[];
  /** Sub-tile commits per document before the sharing read. */
  byteCommits: number;
}

export interface PerfAuditOptions {
  dims?: PerfAuditDims;
  runners?: Record<string, RowRunner>;
  buildScratch?: (w: number, h: number) => Promise<PerfScratch>;
  closeScratch?: (scratch: PerfScratch) => Promise<void>;
  makeGl?: PerfGlFactory;
  byteSizes?: number[];
  byteCommits?: number;
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
  "text-raster-preview",
  "shape-raster",
  "ipc-probe",
  "byte-accounting",
];

const DEFAULT_W = 3072;
const DEFAULT_H = 4608;
const NOT_PART = -1;
/** Square sides the byte row measures by default: one mid-size, one large. */
const DEFAULT_BYTE_SIZES = [2048, 4096];
/**
 * Sub-tile commits per document before the sharing read. This is the store's own
 * stream cap (`max_depth`), NOT an arbitrary sample: the memory requirement is
 * about a history at its cap, so the harness has to reach the same depth the
 * Rust measurement pins or it reproduces a shallower document than the one being
 * claimed about.
 */
const DEFAULT_BYTE_COMMITS = 50;
/** Side of the sub-tile commit that leaves the other tiles shared. */
const BYTE_SUBTILE_PX = 4;

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
    // `undoThroughModel` / `redoThroughModel`, not `undo(snap, false)`: this scratch
    // history records nothing (it has no doc-id getter and is thrown away), so a
    // step would consume whatever entry Rust does hold for the audited document - a
    // brush stroke's Pixel entry. The NAMED methods also keep this opt-out
    // greppable, which an anonymous boolean would not be.
    const prev = hist.undoThroughModel(s.engine.snapshot());
    const undoMs = performance.now() - t0;
    let restoreMs = 0;
    if (prev) {
      const t = performance.now();
      s.engine.restore(prev);
      restoreMs = performance.now() - t;
    }
    const t2 = performance.now();
    const next = hist.redoThroughModel(s.engine.snapshot());
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

  "text-raster-preview": async (s) => {
    const { rasterizeText } = await import("@/engine/textRasterizer");
    const { DEFAULT_TEXT_DATA } = await import("@/engine/textTypes");
    const n = 3;
    let wall = 0;
    for (let i = 0; i < n; i++) {
      const t = performance.now();
      const res = rasterizeText(
        {
          ...DEFAULT_TEXT_DATA,
          content: `Perf audit line ${i}`,
          boxMode: "area",
          boxWidth: s.width,
          boxHeight: s.height,
        },
        undefined,
        { preview: true },
      );
      wall += performance.now() - t;
      closeUnknown(res);
    }
    return {
      totalMs: round1(wall),
      invokeMs: NOT_PART,
      rasterMs: round1(wall),
      uploadMs: NOT_PART,
      snapHistMs: NOT_PART,
      notes: `${n}x area-mode full-doc text raster; a keystroke also pays upload on top (preview flag)`,
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

  /**
   * Byte accounting: what the canonical pixel store actually holds, read through
   * the read-only `rust_pixels_store_bytes` probe.
   *
   * Four reads per document size, and the ORDER is the measurement:
   *
   * 1. seeded - there are NO tiles yet. The packed canon is built on a layer's
   *    first commit, so this read reports the mirror alone with an empty tile
   *    graph plus `owed_anchor_bytes`, the anchor that is not built yet. Reading
   *    its empty graph as "this document is cheap" is the misread the owed field
   *    exists to stop.
   * 2. after sub-tile commits - the case sharing exists for. A 4x4 commit lands
   *    inside one 256px tile, so every other tile keeps its identity and is
   *    counted once however many states reach it.
   * 3. after one FULL-layer commit - which re-tiles every tile, so the newly
   *    written state owns all of them. Watch `shared_bytes`, not `share`: the
   *    shared BYTES do not move (the older states still reach the untouched
   *    tiles) while the denominator grows, so the RATIO falls. That is dilution.
   * 4. the CONTROL: a separate document whose only commit is full-layer, so it
   *    retains nothing. That is the one case where `shared_bytes` is exactly 0.
   *    Without it in the printed block, stage 3 reads as collapse - which is the
   *    misread this row exists to prevent, on the surface a human actually reads.
   *
   * One number alone would say nothing about whether sharing works.
   *
   * It is NOT a frame-pacing measurement and the timing column here is only the
   * probe's own wall time: the renderer callback is all that is timeable
   * headlessly, and jsdom's requestAnimationFrame is timer-based.
   */
  "byte-accounting": async (_s, ctx) => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { getPixelStoreBytes } = await import("@/lib/protocol/pixelStoreBytes");
    const sizes = ctx?.byteSizes?.length ? ctx.byteSizes : DEFAULT_BYTE_SIZES;
    const commits = ctx?.byteCommits ?? DEFAULT_BYTE_COMMITS;
    const samples: PerfByteSample[] = [];
    let probeMs = 0;
    const rowStart = performance.now();

    const read = async (
      docId: string,
      stage: PerfByteSample["stage"],
      done: number,
    ): Promise<void> => {
      const t0 = performance.now();
      const b = await getPixelStoreBytes(docId);
      const ms = performance.now() - t0;
      probeMs += ms;
      samples.push({
        doc_id: docId,
        stage,
        commits: done,
        layer_count: b.layer_count,
        row_major_bytes: b.row_major_bytes,
        tile_total_bytes: b.tile_graph.total_bytes,
        shared_bytes: b.tile_graph.shared_bytes,
        private_bytes: b.tile_graph.private_bytes,
        tile_count: b.tile_graph.tile_count,
        state_count: b.tile_graph.state_count,
        tile_reference_count: b.tile_graph.tile_reference_count,
        owed_anchor_bytes: b.owed_anchor_bytes,
        owed_anchor_layer_count: b.owed_anchor_layer_count,
        probe_ms: round1(ms),
      });
    };

    for (const size of sizes) {
      const docId = `perf-audit-bytes-${size}`;
      const layerId = `perf-audit-bytes-layer-${size}`;
      const bytes = size * size * 4;
      // A flat seed is enough: the probe reports LENGTHS, never pixel values.
      const rgba = new Uint8Array(bytes).fill(7);
      await invoke("rust_pixels_open_document", { docId });
      try {
        await invoke("rust_pixels_init", pixelSeedDispatch(docId, layerId, size, size, rgba));
        await read(docId, "seeded", 0);
        for (let i = 0; i < commits; i++) {
          // Sub-tile: inside the first tile, so every other tile keeps its
          // identity across the whole run and can be reported as shared.
          await invoke("apply_tile_patch", {
            docId,
            layerId,
            before: [],
            after: encodeRustTiles([{ x: 0, y: 0, width: BYTE_SUBTILE_PX, height: BYTE_SUBTILE_PX, data: rgba.subarray(0, BYTE_SUBTILE_PX * BYTE_SUBTILE_PX * 4) }]),
          });
        }
        await read(docId, "after-subtile-commits", commits);
        await invoke("apply_tile_patch", {
          docId,
          layerId,
          before: [],
          after: encodeRustTiles([{ x: 0, y: 0, width: size, height: size, data: rgba }]),
        });
        await read(docId, "after-full-layer-commit", commits + 1);
      } finally {
        // Never leave a seeded document behind: the next run re-opens the same
        // ids, and a leaked one would make the numbers describe two documents.
        await invoke("rust_pixels_close_document", { docId });
      }
    }

    // The CONTROL: a separate document whose only commit is full-layer, so it
    // retains nothing and `shared_bytes` is exactly 0. Without this in the
    // printed block, stage 3's falling ratio reads as collapse rather than
    // dilution, and a human running the audit in-app is the one who misreads it.
    for (const size of sizes) {
      const docId = `perf-audit-bytes-control-${size}`;
      const layerId = `perf-audit-bytes-control-layer-${size}`;
      const rgba = new Uint8Array(size * size * 4).fill(7);
      await invoke("rust_pixels_open_document", { docId });
      try {
        await invoke("rust_pixels_init", pixelSeedDispatch(docId, layerId, size, size, rgba));
        await invoke("apply_tile_patch", {
          docId,
          layerId,
          before: [],
          after: encodeRustTiles([{ x: 0, y: 0, width: size, height: size, data: rgba }]),
        });
        await read(docId, "control-full-layer-only", 1);
      } finally {
        await invoke("rust_pixels_close_document", { docId });
      }
    }

    return {
      // The WHOLE row, so `total ms` means what the column says. The probe reads
      // are only a fraction of it: seeding allocates and ships a whole document's
      // RGBA per size, and the full-layer patches are another whole document each,
      // which at 4096 is 67 MB apiece. `invoke ms` stays the probe-only figure so
      // the measurement's own cost is still readable on its own.
      totalMs: round1(performance.now() - rowStart),
      invokeMs: round1(samples.length ? probeMs / samples.length : NOT_PART),
      rasterMs: NOT_PART,
      uploadMs: NOT_PART,
      snapHistMs: NOT_PART,
      notes: cleanNote(
        `${samples.length} byte reads over ${sizes.join("/")}px docs at ${commits} sub-tile commits; total ms covers seeding + commits + reads, invoke ms is the probe alone; share falls after a full-layer commit because the denominator grows, not because sharing collapsed - see the control row for the one case that reads 0; no frame-pacing claim`,
      ),
      bytes: samples,
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
    ...formatByteBlock(rows),
    `scratch: ${closed ? "closed" : "LEAK (close failed)"}`,
  ];
  return cleanTable(out.join("\n"));
}

/**
 * Byte-accounting reads, printed under the timing table because they are lengths
 * and a sharing ratio, not milliseconds. `share` is shared_bytes / tile_total,
 * the only ratio that says whether structural sharing is in effect; the row-major
 * mirror is listed apart because it is a SECOND population, not part of the tile
 * graph.
 */
function formatByteBlock(rows: PerfAuditRow[]): string[] {
  const samples = rows.flatMap((r) => r.bytes ?? []);
  if (samples.length === 0) return [];
  const lines = samples.map((s) => {
    const share =
      s.tile_total_bytes > 0
        ? `${((s.shared_bytes / s.tile_total_bytes) * 100).toFixed(1)}%`
        : "n/a";
    return (
      `  ${s.doc_id} n=${s.commits} ${s.stage}` +
      ` mirror=${s.row_major_bytes}` +
      ` tile_total=${s.tile_total_bytes}` +
      ` shared=${s.shared_bytes} private=${s.private_bytes}` +
      ` share=${share}` +
      ` tiles=${s.tile_count} states=${s.state_count} refs=${s.tile_reference_count}` +
      ` layers=${s.layer_count} probe=${s.probe_ms.toFixed(1)}ms` +
      // Printed even when zero, and marked, so a row whose tile graph is empty
      // reads as "not built yet" rather than "free".
      (s.owed_anchor_layer_count > 0
        ? ` owes_tile_graph=${s.owed_anchor_bytes} over ${s.owed_anchor_layer_count} layer(s) NOT YET BUILT`
        : " owes_tile_graph=0")
    );
  });
  return [
    "",
    "byte accounting (rust_pixels_store_bytes, read-only; lengths in bytes, share = shared/tile_total)",
    ...lines,
    // Printed next to the numbers, not in a comment a reader has to find: a
    // falling `share` after a full-layer commit is DILUTION, because the shared
    // BYTES held steady while the denominator grew. The control row is the only
    // case that reads 0, and having both side by side is what stops the falling
    // ratio from being read as collapse.
    "  share falls after a full-layer commit because tile_total grows; shared_bytes holds",
    "  steady while older states still reach untouched tiles = dilution, not collapse.",
    "  the control row (its only commit is full-layer, nothing retained) is the zero case.",
  ];
}

export function printPerfAudit(rows: PerfAuditRow[], closed: boolean, dims: PerfAuditDims): void {
  console.log(formatPerfAudit(rows, closed, dims));
}

export async function runPerfAudit(options?: PerfAuditOptions): Promise<PerfAuditRow[]> {
  const dims = normalizeDims(options?.dims);
  const build = options?.buildScratch ?? defaultBuildScratch;
  const close = options?.closeScratch ?? defaultCloseScratch;
  const runners = { ...defaultRunners, ...options?.runners };
  const ctx: PerfRunCtx = {
    makeGl: options?.makeGl ?? makeGlBackend,
    byteSizes: options?.byteSizes ?? DEFAULT_BYTE_SIZES,
    byteCommits: options?.byteCommits ?? DEFAULT_BYTE_COMMITS,
  };
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
