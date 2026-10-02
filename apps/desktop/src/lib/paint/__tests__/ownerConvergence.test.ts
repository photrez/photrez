// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * OWNER CONVERGENCE - the property that replaces the retired `photrez.rustPixels`
 * flag oracle.
 *
 * WHY THIS REPLACES A FLAG ORACLE. The dead test asked "does the flag turn the
 * Rust path on?". That question has no remaining subject: brush, eraser, bucket,
 * seeded fill, gradient and delete-pixels are UNCONDITIONALLY Rust, so five of the
 * six production reads the oracle compared are gone, and its pre-event baseline is
 * permanently unobtainable. This file asks the question that IS the project's
 * single-owner invariant and survives flag retirement forever: after any operation,
 * SEQUENCE, DO THE TWO LIVE OWNERS AGREE? There are exactly two - the TS projection
 * (`layer.imageBitmap`) and the Rust pixel store - so "one canonical owner" is
 * testable as "one canonical owner plus a projection that never diverges".
 *
 * WHAT IS REAL HERE, AND WHAT CANNOT BE.
 *   - REAL `ProtocolEngine` (the wasm pkg), reached the same way the production
 *     bridge reaches it, armed by the bridge-side discriminator rather than
 *     assumed. Never `bridge_emu`.
 *   - REAL `DocumentEngine`. Every operation below is a production entry point:
 *     `applyCrop`, `applyExternalRasterRestore`, `restore`.
 *   - The pixel store is a TAURI COMMAND (`paint_parity_cmds.rs`), not a wasm
 *     export, so it cannot be reached from the frontend suite at all. It is
 *     emulated by `rustStoreEmulator`, which round-trips every argument through
 *     the REAL Tauri serializer and REJECTS where Rust rejects - the fidelity the
 *     repo's three shipped green-suite defects make non-negotiable.
 *   - Store reads go through `rust_pixels_snapshot_layer` + the production
 *     `reconstructLayerBuffer`, so a comparison can never read the emulator's
 *     private buffer and call agreement proven.
 *
 * NON-VACUITY, THE TRAP THIS REPO HAS HIT: `null - null = 0` once read as a real
 * zero and an all-zero buffer looks like real pixels. So `readProjection` and
 * `readStore` each assert their own byte length against `width*height*4` and
 * refuse a zero-length read BEFORE any two values are compared, and
 * `converged` is proven able to see a disagreement before it is trusted to report
 * one.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import * as bridge from "@/lib/protocol/bridge";
import { __resetEmulatedForTests } from "@/lib/protocol/bridge";
import { CONTRACT_VERSION } from "@/lib/protocol/types";
import {
  __resetFacadeRegistryForTests,
  confirmExternalCursor,
  getExternalRecordSnapshot,
  parkExternalReplaySnapshot,
  recordExternalTransitionFor,
} from "@/lib/protocol/facadeRegistry";
import { reconstructLayerBuffer } from "@/lib/paint/regionProducer";
import { DocumentEngine } from "@/engine/document";
import { CommandHistory } from "@/engine/history";
import { DEFAULT_TEXT_DATA } from "@/engine/textTypes";
import {
  flattenAllLayers,
  mergeActiveLayerDown,
  mergeSelectedLayers,
  stampVisibleLayers,
} from "@/components/editor/layers/layerOperations";
import {
  createRustStoreEmulator,
  installCreateImageBitmapMock,
  settlePixelOps,
  type RustStoreEmulator,
} from "@/lib/paint/__tests__/rustStoreEmulator";

// MOCK FIDELITY. Tauri v2 `invoke` REJECTS with an error envelope when a Rust
// command returns Err and resolves `null` for Ok(None). A mock that resolved
// `{ok:false}` instead made every error-handling path look proven while never
// running. `createRustStoreEmulator` throws on the rejecting commands and
// returns undefined on the void ones, which is that contract.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const hoist = vi.hoisted(() => ({ invoke: null as null | ((c: string, a: unknown) => Promise<unknown>) }));

const DOC = "docConverge";
const SIZE = 128;
const CROP = 39;

type WasmModule = {
  protocol_apply_command: (json: string, docId: string) => string;
  protocol_reset: (docId: string) => void;
};
let wasmModule: WasmModule | null = null;
let store: RustStoreEmulator;

/**
 * A canvas that honours the 9-argument `drawImage(src, sx, sy, sw, sh, dx, dy,
 * dw, dh)` form with nearest-neighbour sampling.
 *
 * WHY NOT THE SHARED STUB: `faithfulOffscreenCanvas` (and the inline stub in
 * cropStoreCurrency.test.ts) copy a source raster from its ORIGIN and require the
 * byte lengths to match, so neither can perform a crop - and a convergence test
 * over a crop that never crops compares two copies of the same wrong image. This
 * stub makes the cropped bytes real, which is the only way the post-crop
 * comparison carries information.
 */
class CroppingCanvas {
  width: number;
  height: number;
  private buffer: Uint8ClampedArray;
  private contextObtained = false;

  constructor(w: number, h: number) {
    this.width = w;
    this.height = h;
    this.buffer = new Uint8ClampedArray(Math.max(0, w) * Math.max(0, h) * 4);
  }

  getContext(type: string): CanvasRenderingContext2D {
    if (type !== "2d") return null as unknown as CanvasRenderingContext2D;
    this.contextObtained = true;
    const self = this;
    let fill = "#000000";
    let alpha = 1;
    const ctx = {
      globalAlpha: 1,
      globalCompositeOperation: "source-over",
      imageSmoothingEnabled: true,
      get fillStyle() { return fill; },
      set fillStyle(v: string) { fill = v; },
      save() {},
      restore() {},
      translate() {},
      rotate() {},
      scale() {},
      beginPath() {},
      closePath() {},
      moveTo() {},
      lineTo() {},
      // `fill()` paints the whole surface, not the path. The parametric
      // rasterizers (shape, text) build a path this stub does not track, and
      // leaving `fill()` a no-op would hand them a fully TRANSPARENT raster -
      // which then compares equal to any other empty raster, and a convergence
      // verdict drawn from an empty buffer is exactly the false negative this
      // file exists to prevent.
      fill() { ctx.fillRect(0, 0, self.width, self.height); },
      stroke() {},
      rect() {},
      clip() {},
      setTransform() {},
      font: "",
      textBaseline: "alphabetic",
      letterSpacing: "0px",
      // Deterministic metrics: width from the string length and the font size, so
      // a measured text box is stable across runs and the rasterized layer has
      // real dimensions. Never zero, so the box cannot collapse to nothing.
      measureText(text: string) {
        const px = parseFloat(String(ctx.font).match(/(\d+(?:\.\d+)?)px/)?.[1] ?? "48");
        const width = Math.max(1, text.length * px * 0.5);
        return {
          width,
          actualBoundingBoxAscent: px * 0.8,
          actualBoundingBoxDescent: px * 0.25,
          fontBoundingBoxAscent: px * 0.8,
          fontBoundingBoxDescent: px * 0.25,
        };
      },
      fillText(text: string, x: number, y: number) {
        const m = ctx.measureText(text);
        ctx.fillRect(x, y, m.width, parseFloat(String(ctx.font).match(/(\d+(?:\.\d+)?)px/)?.[1] ?? "48"));
      },
      strokeText() {},
      clearRect(x: number, y: number, w: number, h: number) {
        for (let r = y; r < y + h; r++) {
          for (let c = x; c < x + w; c++) self.setPx(c, r, 0, 0, 0, 0);
        }
      },
      fillRect(x: number, y: number, w: number, h: number) {
        const hex = fill.replace("#", "");
        const r = parseInt(hex.slice(0, 2), 16) || 0;
        const g = parseInt(hex.slice(2, 4), 16) || 0;
        const b = parseInt(hex.slice(4, 6), 16) || 0;
        for (let row = y; row < y + h; row++) {
          for (let col = x; col < x + w; col++) self.setPx(col, row, r, g, b, Math.round(255 * alpha));
        }
      },
      drawImage(src: any, ...rest: number[]) {
        const sw = src.width as number;
        const sh = src.height as number;
        const srcData: ArrayLike<number> = typeof src.getImageData === "function"
          ? src.getImageData().data
          : (src.data as ArrayLike<number>);
        // 3-arg form = whole image at native size; 9-arg form = a crop/scale.
        const [sx, sy, cw, ch, dx, dy, dw, dh] = rest.length >= 6
          ? rest
          : [0, 0, sw, sh, 0, 0, sw, sh];
        for (let row = 0; row < dh; row++) {
          for (let col = 0; col < dw; col++) {
            const srcX = sx + Math.floor((col * cw) / dw);
            const srcY = sy + Math.floor((row * ch) / dh);
            if (srcX < 0 || srcY < 0 || srcX >= sw || srcY >= sh) continue;
            const si = (srcY * sw + srcX) * 4;
            self.setPx(
              dx + col,
              dy + row,
              srcData[si], srcData[si + 1], srcData[si + 2], srcData[si + 3],
            );
          }
        }
      },
      getImageData(x = 0, y = 0, w = self.width, h = self.height) {
        const out = new Uint8ClampedArray(w * h * 4);
        for (let row = 0; row < h; row++) {
          for (let col = 0; col < w; col++) {
            const sx = x + col;
            const sy = y + row;
            if (sx < 0 || sy < 0 || sx >= self.width || sy >= self.height) continue;
            const si = (sy * self.width + sx) * 4;
            const di = (row * w + col) * 4;
            out[di] = self.buffer[si];
            out[di + 1] = self.buffer[si + 1];
            out[di + 2] = self.buffer[si + 2];
            out[di + 3] = self.buffer[si + 3];
          }
        }
        return { data: out, width: w, height: h, colorSpace: "srgb" } as ImageData;
      },
      putImageData(img: any, dx = 0, dy = 0) {
        for (let row = 0; row < img.height; row++) {
          for (let col = 0; col < img.width; col++) {
            const si = (row * img.width + col) * 4;
            self.setPx(
              dx + col,
              dy + row,
              img.data[si], img.data[si + 1], img.data[si + 2], img.data[si + 3],
            );
          }
        }
      },
    };
    return ctx as unknown as CanvasRenderingContext2D;
  }

  private setPx(x: number, y: number, r: number, g: number, b: number, a: number): void {
    if (x < 0 || y < 0 || x >= this.width || y >= this.height) return;
    const i = (y * this.width + x) * 4;
    this.buffer[i] = r;
    this.buffer[i + 1] = g;
    this.buffer[i + 2] = b;
    this.buffer[i + 3] = a;
  }

  transferToImageBitmap(): ImageBitmap {
    if (!this.contextObtained) {
      const err = new Error("Cannot transfer an ImageBitmap from an OffscreenCanvas with no context");
      err.name = "InvalidStateError";
      throw err;
    }
    const buf = this.buffer;
    const width = this.width;
    const height = this.height;
    return {
      width,
      height,
      close: () => {},
      getImageData: (x = 0, y = 0, w = width, h = height) => {
        const out = new Uint8ClampedArray(w * h * 4);
        for (let row = 0; row < h; row++) {
          for (let col = 0; col < w; col++) {
            const si = ((y + row) * width + (x + col)) * 4;
            const di = (row * w + col) * 4;
            out[di] = buf[si];
            out[di + 1] = buf[si + 1];
            out[di + 2] = buf[si + 2];
            out[di + 3] = buf[si + 3];
          }
        }
        return { data: out, width: w, height: h, colorSpace: "srgb" };
      },
    } as unknown as ImageBitmap;
  }
}

/** An ImageBitmap carrying the given RGBA bytes, via the canvas under test. */
function rasterBitmap(w: number, h: number, pixels: Uint8ClampedArray): ImageBitmap {
  const canvas = new CroppingCanvas(w, h);
  const ctx = canvas.getContext("2d");
  ctx.drawImage({ width: w, height: h, data: pixels } as never, 0, 0, w, h);
  return canvas.transferToImageBitmap();
}

/**
 * The renderer surface the layer operations touch: `uploadImage`,
 * `uploadSurfaceTiles` and `destroyTexture`. GPU bookkeeping only - it cannot
 * affect either pixel owner, and stubbing it keeps the measurement about the two
 * owners rather than about WebGL.
 */
function makeRenderer(): never {
  return {
    uploadImage: vi.fn(),
    uploadSurfaceTiles: vi.fn(),
    destroyTexture: vi.fn(),
  } as never;
}

/**
 * A real `CommandHistory`. These five ops all call `history.commit(...)`, and the
 * measurement must include that call - a stub that dropped it would measure a
 * different operation than production runs.
 */
function makeHistory(): never {
  return new CommandHistory(8) as never;
}

/** A non-uniform raster, so any reindexing or mirroring shows as a mismatch. */
function gradientRaster(w: number, h: number): Uint8ClampedArray {
  const buf = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      buf[i] = (x * 2) & 0xff;
      buf[i + 1] = (y * 2) & 0xff;
      buf[i + 2] = ((x + y) * 3) & 0xff;
      buf[i + 3] = 255;
    }
  }
  return buf;
}

interface Owner {
  width: number;
  height: number;
  bytes: Uint8ClampedArray;
  source: string;
}

/**
 * The TypeScript projection's bytes, read back through the PRODUCTION helper
 * (`readbackBitmap` is inlined here because it is not exported from
 * storeCurrency; the OffscreenCanvas path is identical).
 */
function readProjection(engine: DocumentEngine, layerId: string): Owner {
  const layer = engine.getLayer(layerId);
  if (!layer) throw new Error(`no layer ${layerId}`);
  if (!layer.imageBitmap) throw new Error(`premise: layer ${layerId} carries no raster`);
  const canvas = new CroppingCanvas(layer.width, layer.height);
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("premise: the projection raster needs a 2d context");
  ctx.drawImage(layer.imageBitmap, 0, 0, layer.width, layer.height);
  const data = ctx.getImageData(0, 0, layer.width, layer.height).data;
  const expected = layer.width * layer.height * 4;
  if (data.length !== expected) {
    throw new Error(`projection read is ${data.length} bytes, expected ${expected}`);
  }
  if (expected === 0) throw new Error("projection read is empty; refusing to compare");
  return { width: layer.width, height: layer.height, bytes: data, source: "TS projection" };
}

/** The Rust store's bytes, through the store's OWN read command. */
async function readStore(layerId: string): Promise<Owner> {
  const tiles = (await hoist.invoke!("rust_pixels_snapshot_layer", {
    docId: DOC,
    layerId,
  })) as { x: number; y: number; w: number; h: number; data: number[] }[];
  if (!Array.isArray(tiles) || tiles.length === 0) {
    throw new Error(`premise: the Rust store must have a layer to read (${layerId})`);
  }
  // Tile coverage defines the store's dimensions; derived from the tiles rather
  // than from the TS layer, so the comparison cannot borrow the answer.
  const width = Math.max(...tiles.map((t) => t.x + t.w));
  const height = Math.max(...tiles.map((t) => t.y + t.h));
  const bytes = reconstructLayerBuffer(tiles, width, height);
  const expected = width * height * 4;
  if (bytes.length !== expected) {
    throw new Error(`store read is ${bytes.length} bytes, expected ${expected}`);
  }
  if (expected === 0) throw new Error("store read is empty; refusing to compare");
  return { width, height, bytes, source: "Rust pixel store" };
}

/** First differing byte index, or -1 when the two owners are identical. */
function firstDifference(a: Owner, b: Owner): number {
  const n = Math.min(a.bytes.length, b.bytes.length);
  for (let i = 0; i < n; i++) if (a.bytes[i] !== b.bytes[i]) return i;
  return a.bytes.length === b.bytes.length ? -1 : n;
}

/**
 * The Rust store's read, as a THREE-WAY result rather than a value.
 *
 * "Rust has no entry for this layer" and "Rust's entry matches" are DIFFERENT
 * facts and only one of them is a pass, so they cannot share a return type that
 * collapses a missing layer into an empty buffer: `null - null = 0` once fooled
 * this repo. `NO-ENTRY` is therefore returned as itself and never as bytes.
 */
type StoreRead =
  | { verdict: "HAS-ENTRY"; owner: Owner }
  | { verdict: "NO-ENTRY"; reason: string };

async function readStoreVerdict(layerId: string): Promise<StoreRead> {
  try {
    return { verdict: "HAS-ENTRY", owner: await readStore(layerId) };
  } catch (err) {
    return { verdict: "NO-ENTRY", reason: String(err) };
  }
}

/**
 * The measured outcome for one operation, as a single comparable string. The
 * per-op cases assert the EXACT string, so a change in behaviour cannot pass
 * silently - it fails and someone has to look at why.
 */
type ConvergenceVerdict =
  | "CONVERGES"
  | "DIVERGES-dimensions"
  | "DIVERGES-bytes"
  | "RUST-HAS-NO-ENTRY"
  // A layer with no raster cannot be compared at all. Deliberately its OWN
  // verdict: folding it into NO-ENTRY would let a harness that failed to
  // rasterise anything report "Rust has no entry" and read as a measurement.
  | "PROJECTION-UNREADABLE";

async function measureConvergence(
  engine: DocumentEngine,
  layerId: string,
): Promise<{ verdict: ConvergenceVerdict; detail: string }> {
  let projection: Owner;
  try {
    projection = readProjection(engine, layerId);
  } catch (err) {
    return { verdict: "PROJECTION-UNREADABLE", detail: String(err) };
  }
  const storeRead = await readStoreVerdict(layerId);
  if (storeRead.verdict === "NO-ENTRY") {
    return {
      verdict: "RUST-HAS-NO-ENTRY",
      detail: `projection holds ${projection.width}x${projection.height} (${projection.bytes.length} bytes); ${storeRead.reason}`,
    };
  }
  const rust = storeRead.owner;
  if (projection.width !== rust.width || projection.height !== rust.height) {
    return {
      verdict: "DIVERGES-dimensions",
      detail: `projection ${projection.width}x${projection.height} vs store ${rust.width}x${rust.height}`,
    };
  }
  const at = firstDifference(projection, rust);
  if (at !== -1) {
    const px = Math.floor(at / 4);
    return {
      verdict: "DIVERGES-bytes",
      detail:
        `first difference at byte ${at} (pixel ${Math.floor(px / rust.width)},${px % rust.width}, ` +
        `channel ${at % 4}): projection=${projection.bytes[at]} store=${rust.bytes[at]}`,
    };
  }
  return {
    verdict: "CONVERGES",
    detail: `${rust.width}x${rust.height}, ${rust.bytes.length} bytes identical`,
  };
}

/** The assertion under test, in one place so every case below shares it. */
async function expectConverged(engine: DocumentEngine, layerId: string, label: string): Promise<void> {
  const projection = readProjection(engine, layerId);
  const storeOwner = await readStore(layerId);
  expect(
    { width: projection.width, height: projection.height },
    `${label}: dimensions must agree between the two owners`,
  ).toEqual({ width: storeOwner.width, height: storeOwner.height });
  // GEOMETRY NON-VACUITY, kept separate from the byte comparison because a
  // right-sized-but-stale store is a DIFFERENT defect from a
  // right-content-wrong-size one. Asserting the compared region is a real
  // rectangle means a later edit cannot shrink the document until both owners
  // agree on an empty region.
  expect(projection.width, `${label}: the compared region must have real width`).toBeGreaterThan(0);
  expect(projection.height, `${label}: the compared region must have real height`).toBeGreaterThan(0);
  const at = firstDifference(projection, storeOwner);
  if (at !== -1) {
    const px = Math.floor(at / 4);
    throw new Error(
      `${label}: the ${projection.source} and the ${storeOwner.source} DISAGREE at byte ${at} ` +
        `(pixel ${Math.floor(px / storeOwner.width)},${px % storeOwner.width}, channel ${at % 4}): ` +
        `projection=${projection.bytes[at]} store=${storeOwner.bytes[at]}`,
    );
  }
}

let engine: DocumentEngine;
let layerId: string;

async function seed(): Promise<void> {
  await bridge.applyCommand({
    contractVersion: CONTRACT_VERSION,
    expectedVersion: undefined,
    docId: DOC,
    command: { type: "addLayer", id: "layer-converge", name: "Paint", width: SIZE, height: SIZE, index: 0 },
  } as never);
  engine = new DocumentEngine(DOC, "Converge", SIZE, SIZE);
  layerId = engine.addLayer("Paint").id;
  const raster = gradientRaster(SIZE, SIZE);
  const canvas = new CroppingCanvas(SIZE, SIZE);
  canvas.getContext("2d");
  canvas.getContext("2d")!.drawImage({ width: SIZE, height: SIZE, data: raster } as never, 0, 0, SIZE, SIZE);
  engine.setLayerImageBitmap(layerId, canvas.transferToImageBitmap());
  store.seed(layerId, SIZE, SIZE, raster);
}

beforeEach(async () => {
  const g = globalThis as Record<string, unknown>;
  g.OffscreenCanvas = CroppingCanvas as unknown as typeof OffscreenCanvas;
  installCreateImageBitmapMock();
  vi.mocked(invoke).mockImplementation(((c: string, a: unknown) => hoist.invoke!(c, a)) as never);
  localStorage.clear();
  // Explicit: an UNSET facade key means ENABLED (isFacadeEnabled is `!== "0"`).
  localStorage.setItem("photrez.facade", "1");
  localStorage.setItem("photrez.facadeAuthority", "wasm");
  __resetEmulatedForTests();
  __resetFacadeRegistryForTests();
  store = createRustStoreEmulator();
  hoist.invoke = store.invoke;
  if (!wasmModule) {
    const m = await getWasmExportModule();
    expect(m, "the REAL wasm pkg must load; bridge_emu cannot prove convergence").not.toBeNull();
    wasmModule = m as unknown as WasmModule;
  }
});

afterEach(() => {
  store.dispose();
  __resetFacadeRegistryForTests();
  wasmModule?.protocol_reset(DOC);
  localStorage.clear();
  vi.restoreAllMocks();
});

describe("the two live pixel owners converge after every operation sequence", () => {
  it("the comparator can SEE a disagreement, before it is trusted to report agreement", async () => {
    await seed();

    // Premise: agreement is real, not two empty reads comparing equal.
    await expectConverged(engine, layerId, "premise");

    // A one-byte divergence in ONE owner must be detected. Without this the
    // whole file could pass on a comparator that always returns -1.
    const storeOwner = await readStore(layerId);
    const drifted = new Uint8ClampedArray(storeOwner.bytes);
    const at = 4 * 17 + 2;
    drifted[at] = (drifted[at] + 1) & 0xff;
    expect(
      firstDifference(readProjection(engine, layerId), {
        ...storeOwner,
        bytes: drifted,
      }),
      "a one-byte drift in the store must be visible to the comparator",
    ).toBe(at);

    // And the assertion helper itself must refuse that pair.
    store.layers.get(layerId)!.pixels[at] = (drifted[at] + 1) & 0xff;
    await expect(expectConverged(engine, layerId, "induced drift")).rejects.toThrow(/DISAGREE/);
  });

  it("a DIMENSION mismatch alone is reported, not just a byte mismatch", async () => {
    await seed();

    // Right-sized comparison region, wrong-sized STORE: the defect the crop
    // undo/redo work was about. Resizing the store alone must be caught even
    // though every byte the comparator can line up still matches.
    const seeded = store.layers.get(layerId)!;
    store.layers.set(layerId, {
      ...seeded,
      width: CROP,
      height: CROP,
      pixels: new Uint8ClampedArray(CROP * CROP * 4),
    });
    await expect(expectConverged(engine, layerId, "dims drift")).rejects.toThrow(
      /dimensions must agree between the two owners/,
    );
  });

  it("converges on a plain document, after a crop, and after both history directions", async () => {
    await seed();
    await expectConverged(engine, layerId, "fresh document");

    // The crop, through the real engine: the raster is REPLACED at CROP x CROP
    // and the store is reseeded from the exact cropped bytes.
    //
    // The crop is mirrored into the canonical stream as an External transition
    // first, exactly as cropToolActions does: a document-resize is a host
    // transition, and without the record the walker has nothing to step.
    const preOp = engine.snapshot();
    await recordExternalTransitionFor(
      DOC,
      {
        label: "Crop Canvas",
        affectedLayerIds: [layerId],
        snapshot: preOp,
        docSizeChange: { before: { width: SIZE, height: SIZE }, after: { width: CROP, height: CROP } },
      },
      engine,
    );
    engine.applyCrop(16, 16, CROP, CROP, { deleteCroppedPixels: true, targetSize: { w: CROP, h: CROP } });
    await settlePixelOps();
    expect(engine.getLayer(layerId)!.width, "premise: the crop really resized the layer").toBe(CROP);
    await expectConverged(engine, layerId, "after crop");

    // The crop's undo and redo, driven exactly as runFacadeExternalHandoff
    // drives them: park what the step leaves behind, then restore the half for
    // this direction.
    const preCropSnap = await bridge.applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: undefined,
      docId: DOC,
      command: { type: "undo" },
    } as never) as unknown as { status?: string; externalSeq?: number; externalToken?: string };
    expect(preCropSnap.status).toBe("external");
    // Park the LIVE model, not `preOp`: the replay half is the state this step
    // is LEAVING behind, which is the post-crop raster.
    parkExternalReplaySnapshot(preCropSnap.externalToken!, engine.snapshot());
    engine.applyExternalRasterRestore(getExternalRecordSnapshot(preCropSnap.externalToken!)!);
    // The External step arms the pending-external barrier and the redo cannot
    // run until production clears it, which is what runFacadeExternalHandoff does.
    expect((await confirmExternalCursor(DOC, preCropSnap.externalSeq!, "undo")).ok).toBe(true);
    await settlePixelOps();
    expect(engine.getLayer(layerId)!.width, "premise: the undo really resized back").toBe(SIZE);
    await expectConverged(engine, layerId, "after crop undo");

    const redoRes = await bridge.applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: undefined,
      docId: DOC,
      command: { type: "redo" },
    } as never) as unknown as { status?: string; externalToken?: string };
    expect(redoRes.status).toBe("external");
    engine.applyExternalRasterRestore(getExternalRecordSnapshot(redoRes.externalToken, "redo")!);
    await settlePixelOps();
    expect(engine.getLayer(layerId)!.width, "premise: the redo really resized to cropped").toBe(CROP);
    await expectConverged(engine, layerId, "after crop redo");
  });

  it("converges after a full-model restore, at the restored dimensions", async () => {
    await seed();
    const beforeCrop = engine.snapshot();
    engine.applyCrop(16, 16, CROP, CROP, { deleteCroppedPixels: true, targetSize: { w: CROP, h: CROP } });
    await settlePixelOps();
    await expectConverged(engine, layerId, "cropped");

    engine.restore(beforeCrop);
    await settlePixelOps();
    expect(engine.getLayer(layerId)!.width).toBe(SIZE);
    await expectConverged(engine, layerId, "after restore");
  });

  it("converges after TWO consecutive crops, at the true intermediate size", async () => {
    await seed();
    engine.applyCrop(8, 8, 96, 96, { deleteCroppedPixels: true, targetSize: { w: 96, h: 96 } });
    await settlePixelOps();
    await expectConverged(engine, layerId, "first crop");

    engine.applyCrop(4, 4, 64, 64, { deleteCroppedPixels: true, targetSize: { w: 64, h: 64 } });
    await settlePixelOps();
    expect(engine.getLayer(layerId)!.width).toBe(64);
    await expectConverged(engine, layerId, "second crop");
  });

  it("the MEASUREMENT oracle sees every verdict class, not just agreement", async () => {
    await seed();

    // `measureConvergence` is what the five ops below are asserted through, so it
    // needs its own proof: a verdict function that could only ever return
    // CONVERGES would make all five measurements report the same thing without
    // measuring anything. Each class is induced here.
    //
    // 1. CONVERGES.
    expect(
      (await measureConvergence(engine, layerId)).verdict,
      "the seeded layer's two owners agree",
    ).toBe("CONVERGES");

    // 2. RUST-HAS-NO-ENTRY - a layer Rust never learned about, which is exactly
    //    the state the five measured ops leave behind. Distinct from agreement:
    //    the projection here holds real bytes and the store holds nothing.
    // A shape layer is used because it is proven to install a raster on creation,
    // with no store write - which is the exact state under test.
    const stranger = engine.addShapeLayer("Stranger", {
      kind: "rect", width: 40, height: 24, radius: 4,
      fill: { kind: "solid", color: "#3366cc" },
      stroke: { enabled: false, color: "#000000", width: 0 },
      arrowHead: false,
    } as never);
    expect(
      engine.getLayer(stranger.id)?.imageBitmap,
      "premise: the stranger layer really carries a raster",
    ).toBeTruthy();
    const noEntry = await measureConvergence(engine, stranger.id);
    expect(noEntry.verdict, "an unseeded layer reads as NO-ENTRY, not as agreement").toBe(
      "RUST-HAS-NO-ENTRY",
    );
    expect(
      noEntry.detail,
      "the detail names the layer Rust has no entry for, so the verdict is not a bare label",
    ).toContain("layer not initialized");

    // 3. DIVERGES-bytes - move the store's content only.
    const driftAt = 4 * 5 + 1;
    const stored = store.layers.get(layerId)!;
    stored.pixels[driftAt] = (stored.pixels[driftAt] + 7) & 0xff;
    const byteDiverged = await measureConvergence(engine, layerId);
    expect(byteDiverged.verdict).toBe("DIVERGES-bytes");
    expect(byteDiverged.detail, "the byte signature names the exact offset").toContain(
      `byte ${driftAt}`,
    );
    stored.pixels[driftAt] = (stored.pixels[driftAt] - 7) & 0xff;

    // 4. DIVERGES-dimensions - right content, wrong geometry on the store side.
    store.layers.set(layerId, {
      ...stored,
      width: CROP,
      height: CROP,
      pixels: new Uint8ClampedArray(CROP * CROP * 4),
    });
    const dimDiverged = await measureConvergence(engine, layerId);
    expect(dimDiverged.verdict).toBe("DIVERGES-dimensions");
    expect(dimDiverged.detail, "the dimension signature names both sizes").toContain(
      `${CROP}x${CROP}`,
    );

    // 5. Back to agreement, so the three classes above were reached by MOVING the
    //    owners rather than by a harness that simply always says one thing.
    store.layers.set(layerId, stored);
    expect((await measureConvergence(engine, layerId)).verdict).toBe("CONVERGES");
  });

  it("a REAL cross-owner disagreement is caught: a store write the projection never saw", async () => {
    await seed();
    await expectConverged(engine, layerId, "premise");

    // The disagreement the single-owner invariant forbids, induced the only way
    // a differential test can prove itself: move ONE owner, leave the other.
    await hoist.invoke!("rust_pixels_write_region", {
      docId: DOC,
      layerId,
      x: 8,
      y: 8,
      w: 4,
      h: 4,
      rgba: new Uint8Array(4 * 4 * 4).fill(255),
    });

    await expect(expectConverged(engine, layerId, "store-only write")).rejects.toThrow(/DISAGREE/);
    expect(
      firstDifference(readProjection(engine, layerId), await readStore(layerId)),
      "the write really did move only one owner",
    ).toBeGreaterThanOrEqual(0);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// MEASUREMENT: five pixel-producing operations that call NO pixel-store command.
//
// A scoping review established that these five rasterise on a CPU OffscreenCanvas
// and reach Rust only through the MODEL - never through `rust_pixels_write_region`,
// `rust_pixels_init` or `rust_pixels_resize_layer`. Each case below drives the op
// exactly as it exists today (no production change) and records the measured
// verdict.
//
// The verdict is asserted as an EXACT string, so these are measurements, not
// aspirations: if an op starts converging, or starts diverging, the case fails
// and someone has to look at why rather than let the number drift.
// ─────────────────────────────────────────────────────────────────────────────

describe("MEASURED: convergence for ops that write pixels without a pixel-store command", () => {
  it("ADD SHAPE: renderShapeToBitmap replaces the layer raster with no store write", async () => {
    await seed();
    const shape = engine.addShapeLayer("Rect", {
      kind: "rect",
      width: 40,
      height: 24,
      radius: 4,
      fill: { kind: "solid", color: "#3366cc" },
      stroke: { enabled: false, color: "#000000", width: 0 },
      arrowHead: false,
    } as never);

    const measured = await measureConvergence(engine, shape.id);
    expect(
      shape.imageBitmap,
      "premise: addShapeLayer really rasterised a layer",
    ).not.toBeNull();
    expect(
      measured.verdict,
      `ADD SHAPE measured: ${measured.detail}`,
    ).toBe("RUST-HAS-NO-ENTRY");
  });

  // THE FALSIFIABILITY PROOF FOR THE MEASUREMENT ITSELF.
//
// The five verdicts below all read RUST-HAS-NO-ENTRY. That is only a
// measurement if NO-ENTRY is caused by the missing store entry and not by a
// comparator that reports NO-ENTRY for everything. So: seed the store for the
// same shape layer, from the SAME bytes the projection holds, and the verdict
// must move to CONVERGES. If it does not, the five measurements are an artefact
// of the harness and must not be believed.
//
// This was run as the defeat for this task: with the seed added the verdict
// changed, so the measurement was reading the store and not the harness.
it("the NO-ENTRY verdict is caused by the missing store entry, not by the harness", async () => {
    await seed();
    const shape = engine.addShapeLayer("Rect", {
      kind: "rect", width: 40, height: 24, radius: 4,
      fill: { kind: "solid", color: "#3366cc" },
      stroke: { enabled: false, color: "#000000", width: 0 },
      arrowHead: false,
    } as never);

    // Before: exactly what the ADD SHAPE case measures.
    const before = await measureConvergence(engine, shape.id);
    expect(before.verdict, `before seeding: ${before.detail}`).toBe("RUST-HAS-NO-ENTRY");

    // Seed the store from the projection's OWN bytes - no independent source, so
    // the only thing that changed is the store's existence.
    const projection = readProjection(engine, shape.id);
    store.seed(shape.id, projection.width, projection.height, projection.bytes);

    // After: identical pixels, and now there IS a store entry.
    const after = await measureConvergence(engine, shape.id);
    expect(
      after.verdict,
      `seeding the store moved the verdict to ${after.detail}`,
    ).toBe("CONVERGES");
    expect(
      after.detail,
      "the convergence is reported in bytes, not as a bare label",
    ).toContain(`${projection.bytes.length} bytes identical`);

    // And the store is genuinely readable now, which is the difference.
    expect(store.layers.has(shape.id)).toBe(true);
    expect((await readStoreVerdict(shape.id)).verdict).toBe("HAS-ENTRY");
  });

  it("EDIT SHAPE: updateShapeParams re-rasterises in place with no store write", async () => {
    await seed();
    const shape = engine.addShapeLayer("Rect", {
      kind: "rect",
      width: 40,
      height: 24,
      radius: 4,
      fill: { kind: "solid", color: "#3366cc" },
      stroke: { enabled: false, color: "#000000", width: 0 },
      arrowHead: false,
    } as never);
    const afterAdd = await measureConvergence(engine, shape.id);

    // The edit is what a user does second: a different fill AND a different box,
    // so both the bytes and the dims move.
    engine.updateShapeParams(shape.id, {
      kind: "rect",
      width: 56,
      height: 33,
      radius: 12,
      fill: { kind: "solid", color: "#cc3366" },
      stroke: { enabled: false, color: "#000000", width: 0 },
      arrowHead: false,
    } as never);

    const measured = await measureConvergence(engine, shape.id);
    expect(shape.imageBitmap, "premise: the shape layer carries a raster").not.toBeNull();
    expect(
      { add: afterAdd.verdict, edit: measured.verdict },
      `EDIT SHAPE measured: add=${afterAdd.detail} | edit=${measured.detail}`,
    ).toEqual({ add: "RUST-HAS-NO-ENTRY", edit: "RUST-HAS-NO-ENTRY" });
  });

  it("ADD TEXT: rasterizeText installs a raster with no store write", async () => {
    await seed();
    const text = engine.addTextLayer("Label", {
      ...DEFAULT_TEXT_DATA,
      content: "measure me",
      fontSize: 24,
      color: "#112233",
    });

    const measured = await measureConvergence(engine, text.id);
    expect(text.imageBitmap, "premise: addTextLayer really rasterised a layer").not.toBeNull();
    expect(
      measured.verdict,
      `ADD TEXT measured: ${measured.detail}`,
    ).toBe("RUST-HAS-NO-ENTRY");
  });

  it("EDIT TEXT: updateTextData re-rasterises in place with no store write", async () => {
    await seed();
    const text = engine.addTextLayer("Label", {
      ...DEFAULT_TEXT_DATA,
      content: "before",
      fontSize: 24,
      color: "#112233",
    });
    const afterAdd = await measureConvergence(engine, text.id);

    engine.updateTextData(text.id, {
      ...DEFAULT_TEXT_DATA,
      content: "after edit, longer",
      fontSize: 40,
      color: "#332211",
    });

    const measured = await measureConvergence(engine, text.id);
    expect(
      { add: afterAdd.verdict, edit: measured.verdict },
      `EDIT TEXT measured: add=${afterAdd.detail} | edit=${measured.detail}`,
    ).toEqual({ add: "RUST-HAS-NO-ENTRY", edit: "RUST-HAS-NO-ENTRY" });
  });

  it("STAMP VISIBLE: compositeAllLayers into a new layer, no store write", async () => {
    await seed();
    const stamp = engine.addLayer("Under");
    engine.setLayerImageBitmap(stamp.id, rasterBitmap(SIZE, SIZE, gradientRaster(SIZE, SIZE)));

    // The production function, with the renderer surface it touches stubbed: the
    // measurement is about the two pixel owners, and `uploadImage`/`destroyTexture`
    // are GPU bookkeeping that cannot affect either one.
    const ok = stampVisibleLayers(engine, makeHistory(), makeRenderer());
    expect(ok, "premise: stampVisibleLayers composited").toBe(true);

    const stamped = engine.getLayers().find((l) => l.name === "Stamp Visible")!;
    const measured = await measureConvergence(engine, stamped.id);
    expect(
      { width: stamped.width, height: stamped.height },
      "premise: the stamped layer is document-sized",
    ).toEqual({ width: SIZE, height: SIZE });
    expect(
      measured.verdict,
      `STAMP VISIBLE measured: ${measured.detail}`,
    ).toBe("RUST-HAS-NO-ENTRY");
  });

  it("MERGE DOWN: compositeTwoLayers into a new layer, no store write", async () => {
    await seed();
    const lower = engine.addLayer("Lower");
    engine.setLayerImageBitmap(lower.id, rasterBitmap(SIZE, SIZE, gradientRaster(SIZE, SIZE)));
    // Stack order is top-first: `addLayer` inserts ABOVE the active layer, so the
    // seeded Paint layer now sits at index 1. `mergeDown` takes the partner at
    // `idx + 1`, so the layer being merged must be the one at index 0.
    expect(
      engine.getLayers().map((l) => l.name),
      "premise: stack order is top-first",
    ).toEqual(["Lower", "Paint"]);
    engine.setActiveLayer(lower.id);

    const ok = mergeActiveLayerDown(engine, makeHistory(), makeRenderer(), lower.id);
    expect(ok, "premise: mergeActiveLayerDown merged").toBe(true);

    const merged = engine.getLayer(engine.getActiveLayerId() || "")!;
    expect(merged.name, "premise: the merged layer really is a new one").toContain("+");
    const measured = await measureConvergence(engine, merged.id);
    expect(
      measured.verdict,
      `MERGE DOWN measured: ${measured.detail}`,
    ).toBe("RUST-HAS-NO-ENTRY");
  });

  it("MERGE SELECTED: compositeAllLayers into a new layer, no store write", async () => {
    await seed();
    const second = engine.addLayer("Second");
    engine.setLayerImageBitmap(second.id, rasterBitmap(SIZE, SIZE, gradientRaster(SIZE, SIZE)));

    const ok = mergeSelectedLayers(engine, makeHistory(), makeRenderer(), [layerId, second.id]);
    expect(ok, "premise: mergeSelectedLayers merged").toBe(true);

    const merged = engine.getLayer(engine.getActiveLayerId() || "")!;
    expect(merged.name, "premise: the merged layer really is a new one").toContain("+");
    const measured = await measureConvergence(engine, merged.id);
    expect(
      measured.verdict,
      `MERGE SELECTED measured: ${measured.detail}`,
    ).toBe("RUST-HAS-NO-ENTRY");
  });

  it("FLATTEN IMAGE: compositeAllLayers into a new Background, no store write", async () => {
    await seed();
    const second = engine.addLayer("Second");
    engine.setLayerImageBitmap(second.id, rasterBitmap(SIZE, SIZE, gradientRaster(SIZE, SIZE)));

    const ok = flattenAllLayers(engine, makeHistory(), makeRenderer());
    expect(ok, "premise: flattenAllLayers flattened").toBe(true);

    const flattened = engine.getLayers()[0];
    expect(
      engine.getLayers().length,
      "premise: flatten collapsed to one layer",
    ).toBe(1);
    const measured = await measureConvergence(engine, flattened.id);
    expect(
      measured.verdict,
      `FLATTEN IMAGE measured: ${measured.detail}`,
    ).toBe("RUST-HAS-NO-ENTRY");
  });

  it("the five ops emit ZERO pixel-store commands, which is why Rust has no entry", async () => {
    // THE CAUSE, not the symptom. If an op ever starts calling a store command,
    // the NO-ENTRY verdicts above become stale, so this pins the census-visible
    // fact directly: the ops write pixels and reach Rust only through the model.
    await seed();
    store.calls.length = 0;

    const shape = engine.addShapeLayer("Rect", {
      kind: "rect", width: 40, height: 24, radius: 4,
      fill: { kind: "solid", color: "#3366cc" },
      stroke: { enabled: false, color: "#000000", width: 0 },
      arrowHead: false,
    } as never);
    engine.updateShapeParams(shape.id, {
      kind: "rect", width: 56, height: 33, radius: 12,
      fill: { kind: "solid", color: "#cc3366" },
      stroke: { enabled: false, color: "#000000", width: 0 },
      arrowHead: false,
    } as never);
    const text = engine.addTextLayer("Label", { ...DEFAULT_TEXT_DATA, content: "x", fontSize: 24 });
    engine.updateTextData(text.id, { ...DEFAULT_TEXT_DATA, content: "yy", fontSize: 40 });
    await settlePixelOps();

    const storeCommands = store.calls.filter((c) => c.cmd.startsWith("rust_pixels_"));
    expect(
      storeCommands.map((c) => c.cmd),
      "no pixel-store command is emitted by shape or text add/edit",
    ).toEqual([]);
  });
});