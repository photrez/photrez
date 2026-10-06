// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * The eraser on an opaque white layer: does it cut holes, and what makes the
 * difference?
 *
 * REPORTED (real app, tool `eraser`, size 32, opacity 1, hardness 1): on an opaque
 * white BACKGROUND, `sumRGB` stayed 892296000 and the non-zero pixel count stayed
 * 1166400 - identical before, after, and after undo - while the Rust store went
 * `false -> PRESENT`, `epoch 1 -> epoch 2`. A real write reached Rust, the history
 * entry moved, and the pixels did not change.
 *
 * A PRIOR ROUND MEASURED THE ERASER AS WORKING: alpha-zero rose 0 -> 3006, the whole
 * band was erased, the off-band was untouched, payload byte-exact. The only recorded
 * difference was that that round painted with the BRUSH first.
 *
 * `resolveEraserFill` (brushToolState.ts:198) is the discriminator, and it has nothing
 * to do with history:
 *
 *     if (isEraser && layer?.isBackground) return { isEraser: false, color: bgColor };
 *
 * On the BACKGROUND layer the eraser stops being an eraser and becomes a source-over
 * paint of the background swatch - a background layer cannot be transparent. On an
 * opaque white background with a white swatch that is white over white: the raster is
 * legitimately unchanged, and the store write and epoch bump are the record of a paint
 * that happened to be a no-op. Every other layer keeps `isEraser: true` and cuts real
 * holes with destination-out.
 *
 * The brush stroke in the working measurement was incidental - what mattered was that it
 * created a SEPARATE, non-background layer. These cases prove that: the hole-cutting case
 * below has NO prior brush stroke at all, and still cuts holes. Prior history is not what
 * enables the erase.
 *
 // WHAT THE PIXEL ASSERTIONS HERE ARE WORTH. The raster is read back from the store,
// and each claim is pinned in a form that a wrong result CANNOT satisfy: the eraser
// is measured with LOCALITY (holes inside the stroke's band, the off-band provably
// untouched) rather than a whole-layer count that a one-pixel hole and a full wipe
// share, and the brush is measured by an EXACT-COLOUR census rather than a colour
// sum that a white dab moves just as much as the intended swatch. The shim itself is
// pinned in __tests__/faithfulOffscreenCanvas.test.ts; if the shim draws nothing,
// every assertion above is vacuous, so that file is not optional.
//
// HARNESS FIDELITY. Real `DocumentEngine`, real wasm graph mirror, real `EditorFacade`,
// the transport-faithful Rust store emulator (rejects where Rust rejects, including
// `rust_pixels_get_epoch` for an unseeded layer), and the REAL `useBrushOverlay` hook
// driven through its exported `onPaintStroke` / `commitBrushStroke`. The raster is read
// back from the STORE, never from a downstream symptom. The tool is set INSIDE the helper:
// a helper that sets the tool to brush and then strokes has shipped a false positive in
// this repo before.
 */
import { describe, it, expect, vi, beforeAll, beforeEach, afterEach, type Mock } from "vitest";
import { pixelSeedDispatch } from "@/lib/protocol/pixelSeedCall";
import { invoke } from "@tauri-apps/api/core";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { WorkspaceManager } from "@/engine/workspace";
import { CommandHistory } from "@/engine/history";
import { useBrushOverlay, flushC4Commits } from "../useBrushOverlay";
import * as DialogProviderModule from "../dialogs/DialogProvider";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import {
  __resetFacadeRegistryForTests,
  seedFacadeFromEngine,
  installFacadeCommitShim,
  getFacade,
} from "@/lib/protocol/facadeRegistry";
import { __resetNativeAuthorityForTests, awaitNativeSeed } from "@/lib/protocol/bridge";
import { __resetCanonicalRepushForTests } from "@/lib/protocol/canonicalSeed";
import {
  createRustStoreEmulator,
  type RustStoreEmulator,
} from "@/lib/paint/__tests__/rustStoreEmulator";
import { installFaithfulCanvas } from "@/__tests__/faithfulOffscreenCanvas";
import { getBrushTip } from "../brushTipMask";
import { resolveEraserFill } from "../brushToolState";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as Mock<
  (cmd: string, args: Record<string, unknown>) => Promise<unknown>
>;

type WasmModule = {
  protocol_reset: (docId: string) => void;
  protocol_apply_command: (json: string, docId: string) => string;
  protocol_snapshot_json: (docId: string) => string;
  protocol_version: (docId: string) => bigint;
  protocol_history_query_json: (docId: string) => string;
  protocol_history_cursor_commit: (json: string, docId: string) => string;
  protocol_register_payload_adapter: (adapterId: string, docId: string) => void;
  protocol_seed_canonical: (json: string, docId: string) => string;
};
let writes: Array<{ layerId: string; x: number; y: number; w: number; h: number; sumRGB: number; alphaZero: number; len: number }> = [];
let wasmMod: WasmModule;
let store: RustStoreEmulator;
let restoreCanvas: (() => void) | undefined;
const TAURI_KEY = "__TAURI_INTERNALS__";
const NS = "native::";
let DOC = "eraserHoles-0";
const SIZE = 64;

function nativeRejection(e: unknown): string {
  const raw = e instanceof Error ? e.message : String(e);
  try {
    const parsed = JSON.parse(raw) as { code?: unknown; message?: unknown };
    if (typeof parsed?.code === "string" && typeof parsed?.message === "string") {
      return `${parsed.code}: ${parsed.message}`;
    }
  } catch {
    /* not a JSON envelope */
  }
  return raw;
}

function installTransport(): void {
  const opened = new Set<string>();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}, options?: any) => {
    if (cmd.startsWith("rust_pixels_")) {
      if (cmd === "rust_pixels_write_region") {
        // Record what actually went on the wire: the erase can be correct in the
        // overlay and the surface and still be lost in the payload.
        const rgba = (args as { rgba?: ArrayLike<number> }).rgba;
        let alphaZero = 0;
        let sumRGB = 0;
        if (rgba) {
          for (let i = 0; i + 3 < rgba.length; i += 4) {
            if (rgba[i + 3] === 0) alphaZero += 1;
            sumRGB += rgba[i] + rgba[i + 1] + rgba[i + 2];
          }
        }
        writes.push({
          layerId: String((args as { layerId?: string }).layerId),
          x: Number((args as { x?: number }).x),
          y: Number((args as { y?: number }).y),
          w: Number((args as { w?: number }).w),
          h: Number((args as { h?: number }).h),
          sumRGB,
          alphaZero,
          len: rgba?.length ?? -1,
        });
      }
      return store.invoke(cmd, args, options);
    }
    const rawDocId = (args.docId as string) ?? "";
    const key = `${NS}${rawDocId === "" ? "default" : rawDocId}`;
    switch (cmd) {
      case "rust_pixels_open_document":
        if (!opened.has(key)) {
          opened.add(key);
          wasmMod.protocol_reset(key);
        }
        return undefined;
      case "protocol_seed_native":
        return JSON.stringify({ version: 0, layers: [] });
      case "protocol_seed_canonical_native":
        return wasmMod.protocol_seed_canonical(args.payloadJson as string, key);
      case "protocol_apply_command_native":
        try {
          return wasmMod.protocol_apply_command(args.envelopeJson as string, key);
        } catch (e) {
          throw nativeRejection(e);
        }
      case "protocol_snapshot_native":
        return wasmMod.protocol_snapshot_json(key);
      case "protocol_version_native":
        return Number(wasmMod.protocol_version(key));
      case "protocol_history_query_native":
        return wasmMod.protocol_history_query_json(key);
      case "protocol_history_cursor_commit_native":
        try {
          return wasmMod.protocol_history_cursor_commit(
            JSON.stringify({ seq: args.seq, direction: args.direction }),
            key,
          );
        } catch (e) {
          throw nativeRejection(e);
        }
      case "protocol_register_adapter_native":
        wasmMod.protocol_register_payload_adapter(args.adapterId as string, key);
        return null;
      default:
        throw `E_UNKNOWN_COMMAND: ${cmd}`;
    }
  });
}

type Engine = ReturnType<WorkspaceManager["getActiveEngine"]> & object;

/** Dims + content digest + alpha census, read back from the STORE. */
type Raster = { sumRGB: number; nz: number; alphaZero: number; present: boolean; epoch: number };

type StoreTiles = Array<{ x: number; y: number; w: number; h: number; data: number[] }>;

async function storeTiles(docId: string, layerId: string): Promise<StoreTiles> {
  return (await store.invoke("rust_pixels_snapshot_layer", { docId, layerId })) as StoreTiles;
}

async function readStoreRaster(docId: string, layerId: string): Promise<Raster> {
  let epoch = 0;
  try {
    epoch = Number(await store.invoke("rust_pixels_get_epoch", { docId, layerId }));
  } catch {
    // The emulator REJECTS for an unseeded layer - that rejection IS the absence.
    return { sumRGB: 0, nz: 0, alphaZero: 0, present: false, epoch: 0 };
  }
  const tiles = await storeTiles(docId, layerId);
  let sumRGB = 0;
  let nz = 0;
  let alphaZero = 0;
  for (const t of tiles) {
    for (let i = 0; i + 3 < t.data.length; i += 4) {
      const r = t.data[i];
      const g = t.data[i + 1];
      const b = t.data[i + 2];
      const a = t.data[i + 3];
      sumRGB += r + g + b;
      if (r !== 0 || g !== 0 || b !== 0 || a !== 0) nz += 1;
      if (a === 0) alphaZero += 1;
    }
  }
  return { sumRGB, nz, alphaZero, present: true, epoch };
}

/**
 * The same census restricted to a rect. Locality is the whole point of an eraser
 * measurement: "holes in the band" and "the entire layer destroyed" are both
 * `alphaZero > 0`, so the band and the off-band have to be read separately or the
 * assertion cannot tell them apart.
 */
async function readStoreRect(
  docId: string,
  layerId: string,
  x0: number,
  y0: number,
  rw: number,
  rh: number,
): Promise<Raster & { pixels: number }> {
  const tiles = await storeTiles(docId, layerId);
  let sumRGB = 0;
  let nz = 0;
  let alphaZero = 0;
  let pixels = 0;
  for (const t of tiles) {
    for (let row = 0; row < t.h; row++) {
      const py = t.y + row;
      if (py < y0 || py >= y0 + rh) continue;
      for (let col = 0; col < t.w; col++) {
        const px = t.x + col;
        if (px < x0 || px >= x0 + rw) continue;
        const i = (row * t.w + col) * 4;
        const r = t.data[i];
        const g = t.data[i + 1];
        const b = t.data[i + 2];
        const a = t.data[i + 3];
        pixels += 1;
        sumRGB += r + g + b;
        if (r !== 0 || g !== 0 || b !== 0 || a !== 0) nz += 1;
        if (a === 0) alphaZero += 1;
      }
    }
  }
  return { sumRGB, nz, alphaZero, present: true, epoch: 0, pixels };
}

/**
 * How many store pixels match an exact predicate. This is what a colour claim has
 * to be made of: a change in a colour SUM cannot tell #cc3300 from white, but an
 * exact pixel match can, because a wrong-colour dab contributes to one count and
 * not the other.
 */
async function readStoreMatching(
  docId: string,
  layerId: string,
  match: (r: number, g: number, b: number, a: number) => boolean,
): Promise<number> {
  const tiles = await storeTiles(docId, layerId);
  let n = 0;
  for (const t of tiles) {
    for (let i = 0; i + 3 < t.data.length; i += 4) {
      if (match(t.data[i], t.data[i + 1], t.data[i + 2], t.data[i + 3])) n += 1;
    }
  }
  return n;
}

const SETTINGS = { size: 32, hardness: 1, opacity: 1, flow: 1, smoothing: 0 };

/**
 * One real eraser stroke through the real hook.
 *
 * The tool is set HERE, inside the helper, before the stroke: a helper that leaves the
 * tool on brush and then calls the stroke has produced a false "the eraser works"
 * measurement in this repo before.
 */
async function eraseBandOn(
  opts: { isBackground: boolean; bgSwatch: string; tool?: "brush" | "eraser"; fill?: string },
): Promise<{
  before: Raster;
  after: Raster;
  /** The document and layer the store census must be re-read for. */
  docId: string;
  layerId: string;
  stages: string;
  /** What actually went on the wire. */
  payload: { sumRGB: number; alphaZero: number; w: number; h: number } | null;
}> {
  const wm = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument(DOC, "Eraser", SIZE, SIZE);
  wm.addDocument(session);
  const engine = session.engine as never as Engine;
  liveEngine = engine as never;
  await seedFacadeFromEngine(engine as never, getFacade(DOC));
  await awaitNativeSeed(DOC);
  await engine.applyFacadeSnapshot(getFacade(DOC).snapshot as never);

  const layer = engine.getLayers()[0];
  layer.isBackground = opts.isBackground;
  // An eraser needs known OPAQUE content so "did anything change" is unambiguous. A
  // brush needs the opposite: a genuinely BLANK layer, so deposited pixels are a rise
  // in the opaque count rather than a change hidden inside an already-white field.
  const blank = opts.tool === "brush";
  const canvas = new OffscreenCanvas(SIZE, SIZE);
  const ctx = canvas.getContext("2d") as unknown as CanvasRenderingContext2D;
  if (!blank) {
    ctx.fillStyle = "#ffffff";
    ctx.fillRect(0, 0, SIZE, SIZE);
  }
  engine.setLayerImageBitmap(layer.id, canvas.transferToImageBitmap());
  engine.setActiveLayer(layer.id);
  // Seed the store so the layer is PRESENT before the stroke - a real document's
  // background always is, and the reported run observed `false -> PRESENT`.
  const surface = engine.getPaintSurface(layer.id);
  expect(surface, "premise: the engine exposes a real paint surface").not.toBeNull();
  await store.invoke("rust_pixels_open_document", { docId: DOC });
  await store.invoke(
    "rust_pixels_init",
    pixelSeedDispatch(DOC, layer.id, SIZE, SIZE, blank ? new Uint8Array(SIZE * SIZE * 4) : new Uint8Array(SIZE * SIZE * 4).fill(255)),
  );

  const before = await readStoreRaster(DOC, layer.id);
  expect(before.present, "premise: the store holds the layer").toBe(true);
  expect(
    before.alphaZero,
    blank ? "premise: the brush layer starts blank" : "premise: the erase layer starts opaque",
  ).toBe(blank ? SIZE * SIZE : 0);

  const history = new CommandHistory();
  // A real OffscreenCanvas, so `installFaithfulCanvas` gives the overlay a genuine 2d
  // context: a jsdom <canvas> element has no createImageData, and the eraser needs one.
  const overlayCanvas = new OffscreenCanvas(SIZE, SIZE);
  mockUseEditor({
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => history,
      getActiveDocumentId: () => DOC,
      notifyVisualChange: vi.fn(),
    },
    renderer: { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn() },
    scheduler: { requestRender: vi.fn() },
    // TOOL SET INSIDE THE HELPER.
    activeTool: () => opts.tool ?? "eraser",
    eraserSize: () => SETTINGS.size,
    eraserHardness: () => SETTINGS.hardness,
    brushSize: () => SETTINGS.size,
    brushHardness: () => SETTINGS.hardness,
    fgColor: () => opts.fill ?? "#000000",
    bgColor: () => opts.bgSwatch,
    docWidth: () => SIZE,
    docHeight: () => SIZE,
  });
  const overlay = useBrushOverlay();
  overlay.setOverlayCanvasRef(overlayCanvas as never);

  overlay.onPaintStroke(
    [
      { x: 8, y: SIZE / 2 },
      { x: SIZE - 8, y: SIZE / 2 },
    ],
    opts.tool === "brush" ? false : true,
    SETTINGS,
    true,
  );

  writes = [];
  await overlay.commitBrushStroke(
    engine as never,
    history as never,
    layer.id,
    opts.tool === "brush" ? false : true,
  );
  await flushC4Commits();

  // Where are the holes lost? The overlay is the eraser's carrier (the commit reads
  // its erased region), and the surface is what the store is written from. This helper
  // holds both, so one reading each localises the stage.
  const census = (r: Raster) =>
    `sumRGB=${r.sumRGB} nz=${r.nz} alphaZero=${r.alphaZero} epoch=${r.epoch}`;
  void census;
  const octx = overlayCanvas.getContext("2d") as unknown as {
    getImageData(x?: number, y?: number, w?: number, h?: number): { data: Uint8ClampedArray };
  };
  const od = octx.getImageData().data;
  let oAlphaZero = 0;
  for (let i = 3; i < od.length; i += 4) if (od[i] === 0) oAlphaZero += 1;
  const surf = engine.getPaintSurface(layer.id);
  const sd = (
    surf as unknown as { readRect(x: number, y: number, w: number, h: number): { data: Uint8ClampedArray } }
  ).readRect(0, 0, SIZE, SIZE).data;
  let sAlphaZero = 0;
  for (let i = 3; i < sd.length; i += 4) if (sd[i] === 0) sAlphaZero += 1;

  return {
    before,
    after: await readStoreRaster(DOC, layer.id),
    docId: DOC,
    layerId: layer.id,
    stages: `overlay alphaZero=${oAlphaZero} surface alphaZero=${sAlphaZero} writes=${JSON.stringify(writes)}`,
    payload: writes[0]
      ? { sumRGB: writes[0].sumRGB, alphaZero: writes[0].alphaZero, w: writes[0].w, h: writes[0].h }
      : null,
  };
}

let liveEngine: { getId(): string; getLayers(): Array<{ id: string }> } | null = null;

beforeAll(async () => {
  const mod = (await getWasmExportModule()) as WasmModule | null;
  expect(mod, "the REAL wasm pkg must load").not.toBeNull();
  wasmMod = mod!;
  (globalThis as Record<string, unknown>)[TAURI_KEY] = { invoke: invokeMock };
  installFacadeCommitShim({
    getEngine: () => liveEngine,
    getDocId: () => liveEngine?.getId() ?? "default",
  });
});

let docSeq = 0;

beforeEach(() => {
  localStorage.clear();
  localStorage.setItem("photrez.facade", "1");
  localStorage.setItem("photrez.facadeAuthority", "native");
  DOC = `eraserHoles-${(docSeq += 1)}`;
  restoreCanvas = installFaithfulCanvas();
  invokeMock.mockReset();
  store = createRustStoreEmulator();
  __resetNativeAuthorityForTests();
  __resetCanonicalRepushForTests();
  __resetFacadeRegistryForTests();
  installTransport();
  writes = [];
  wasmMod.protocol_reset(`${NS}${DOC}`);
  // Re-applied per test: `afterEach` restores all mocks, which would otherwise drop
  // the dialog stub and the hook would throw "must be used within DialogProvider".
  vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({
    confirm: vi.fn(),
    toast: vi.fn(),
  } as unknown as ReturnType<typeof DialogProviderModule.useDialog>);
});

afterEach(() => {
  store.dispose();
  localStorage.clear();
  restoreCanvas?.();
  restoreCanvas = undefined;
  __resetNativeAuthorityForTests();
  __resetCanonicalRepushForTests();
  __resetFacadeRegistryForTests();
  vi.restoreAllMocks();
});

describe("eraser and brush on the store raster: what decides whether pixels change", () => {
  /**
   * THE REPORTED CASE. The real-app run erased the BACKGROUND layer, where the
   * eraser is a source-over paint of the background swatch, not a destination-out
   * cut. On an opaque white background with a white swatch that is white over white:
   * the raster is legitimately unchanged, while a real write still happened - which
   * is exactly the reported "store entry written, pixels unchanged, epoch bumped".
   */
  it("leaves an opaque white BACKGROUND layer unchanged, and records the write", async () => {
    const { before, after } = await eraseBandOn({ isBackground: true, bgSwatch: "#ffffff" });

    expect(after.sumRGB, "white over white leaves the colour sum unchanged").toBe(before.sumRGB);
    expect(after.nz, "and the opaque pixel count unchanged").toBe(before.nz);
    expect(after.alphaZero, "and nothing becomes transparent").toBe(before.alphaZero);
    expect(
      after.epoch,
      "the store write is still recorded - the report saw PRESENT and an epoch bump",
    ).toBeGreaterThan(before.epoch);
  });
  it("the tip mask rasterises to a real alpha ramp (not an empty mask)", () => {
    // `rasterizeBrushTipWithCurve` is pure Float32Array math and never touches a
    // canvas, so the mask was never one of the shim's unfaithfulnesses. Pinned so a
    // future "the mask must be empty" theory dies here. Read off the real object, no
    // hash channel over a possibly-undefined field.
    const tip = getBrushTip({ size: 32, hardness: 1, curve: "soft" });
    let max = 0;
    let nonZero = 0;
    for (let i = 0; i < tip.data.length; i += 1) {
      if (tip.data[i] > max) max = tip.data[i];
      if (tip.data[i] > 0) nonZero += 1;
    }
    expect(max, "the tip must have a solid core").toBe(1);
    expect(nonZero, "and a non-empty footprint").toBeGreaterThan(0);
  });

  it("the shim's own destination-out cuts a hole in a known buffer", () => {
    // Decides shim-vs-hook with no product code involved at all: paint an opaque white
    // canvas, cut a hole in it with destination-out using a solid tip, and count
    // alpha-zero. A shim that cannot do this cannot falsify any eraser claim.
    const canvas = new OffscreenCanvas(32, 32);
    const ctx = canvas.getContext("2d") as unknown as {
      fillStyle: string;
      globalAlpha: number;
      globalCompositeOperation: string;
      fillRect(x: number, y: number, w: number, h: number): void;
      drawImage(src: unknown, ...rest: number[]): void;
      getImageData(): { data: Uint8ClampedArray };
    };
    ctx.fillStyle = "#ffffff";
    ctx.fillRect(0, 0, 32, 32);

    const tip = new OffscreenCanvas(16, 16);
    const tctx = tip.getContext("2d") as unknown as {
      fillStyle: string;
      fillRect(x: number, y: number, w: number, h: number): void;
    };
    tctx.fillStyle = "#000000";
    tctx.fillRect(0, 0, 16, 16);

    ctx.globalCompositeOperation = "destination-out";
    ctx.globalAlpha = 1;
    ctx.drawImage(tip, 0, 0, 16, 16, 8, 8, 16, 16);

    const d = ctx.getImageData().data;
    let alphaZero = 0;
    for (let i = 3; i < d.length; i += 4) if (d[i] === 0) alphaZero += 1;

    expect(alphaZero, "destination-out must punch a hole").toBeGreaterThan(0);
  });

  it("a NON-background erase punches real holes in the Rust store", async () => {
    // THE PIXEL-LEVEL ERASER ASSERTION. RED before the shim's `drawImage` could read a
    // canvas source (every dab silently drew nothing, so `destination-out` was inert and
    // the store came back byte-identical), GREEN after.
    //
    // The discriminating measurement is LOCALITY, and a whole-layer alphaZero rise
    // cannot express it: `alphaZero > 0` is equally true for a one-pixel hole and for
    // all 4096 pixels erased, so the band the stroke crossed and the band it did not
    // are read separately and both are pinned.
    const { before, after, docId, layerId } = await eraseBandOn({
      isBackground: false,
      bgSwatch: "#ffffff",
    });

    // The stroke runs along y = SIZE / 2 with a 32px tip, so rows [16,48) are the band
    // and rows [0,8) are nine rows clear of the nearest dab.
    const band = await readStoreRect(docId, layerId, 0, 16, SIZE, 32);
    const offBand = await readStoreRect(docId, layerId, 0, 0, SIZE, 8);

    expect(band.alphaZero, "the erased band must become transparent IN THE STORE").toBeGreaterThan(
      0,
    );
    expect(after.alphaZero, "so the layer-wide transparent count must rise").toBeGreaterThan(
      before.alphaZero,
    );
    // One 32px tip has ~750 pixels in its opaque core, so a single dab already erases
    // far more than 512. A one-pixel hole cannot satisfy this.
    expect(
      after.alphaZero,
      "a 32px tip erasing a 48px-long stroke must remove hundreds of pixels, not one",
    ).toBeGreaterThanOrEqual(512);
    expect(
      after.alphaZero,
      "and it must NOT be the whole layer: locality is what separates an erase from a wipe",
    ).toBeLessThan(SIZE * SIZE);
    expect(after.nz, "and the opaque pixel count must fall").toBeLessThan(before.nz);
    expect(after.sumRGB, "so the colour sum must fall").toBeLessThan(before.sumRGB);

    // The off-band is the part that makes the measurement mean something.
    expect(offBand.pixels, "premise: the off-band rect is the size it claims to be").toBe(SIZE * 8);
    expect(
      offBand.alphaZero,
      "the off-band must stay fully opaque: an erase that reached it was a wipe",
    ).toBe(0);
    expect(offBand.nz, "and every off-band pixel must still be non-zero").toBe(SIZE * 8);
    expect(
      offBand.sumRGB,
      "and still white - the seeded store value, so nothing repainted or erased it",
    ).toBe(SIZE * 8 * 3 * 255);
  });

  it("a BRUSH stroke deposits pixels in the store and leaves the rest untouched", async () => {
    // THE PIXEL-LEVEL BRUSH ASSERTION, and the one that was missing for the whole
    // project: every pre-existing paint test counts writes and call shapes, so none of
    // them could see that the shim was drawing nothing. RED before the shim's
    // `drawImage` learned to read a canvas source, GREEN after.
    const { before, after, docId, layerId } = await eraseBandOn({
      isBackground: false,
      bgSwatch: "#ffffff",
      tool: "brush",
      fill: "#cc3300",
    });

    expect(after.nz, "the store must gain opaque pixels under the stroke").toBeGreaterThan(
      before.nz,
    );
    expect(
      before.alphaZero - after.alphaZero,
      "and the blank count must FALL by the deposited pixels",
    ).toBe(after.nz - before.nz);

    // Not "a write happened" and not "a colour sum moved": the deposited pixels must
    // BE #cc3300. A colour sum rises just as happily for white, and on a blank layer
    // any non-zero sum satisfies a sum-differs check, so the exact-colour census is
    // the only form of this assertion that can fail on a wrong-colour dab.
    const isFg = (r: number, g: number, b: number, a: number): boolean =>
      r === 204 && g === 51 && b === 0 && a > 0;
    const painted = await readStoreMatching(docId, layerId, isFg);
    const deposited = after.nz - before.nz;

    expect(deposited, "the stroke must deposit pixels at all").toBeGreaterThan(0);
    expect(
      painted,
      "EVERY deposited pixel must be exactly the fg swatch #cc3300 - one white pixel would " +
        "satisfy a colour-sum change while contributing nothing to this count",
    ).toBe(deposited);
    expect(
      await readStoreMatching(docId, layerId, (r, g, b, a) => r !== 0 || g !== 0 || b !== 0 || a !== 0),
      "and the store must hold no other coloured pixel at all: a stroke that deposited the " +
        "right colour AND something else fails here",
    ).toBe(deposited);
  });

  it("prior brush history is NOT the discriminator - the layer KIND is", () => {
    expect(resolveEraserFill({ isBackground: true } as never, true, "#ffffff")).toEqual({
      isEraser: false,
      color: "#ffffff",
    });
    expect(resolveEraserFill({ isBackground: false } as never, true, "#ffffff").isEraser).toBe(true);
  });
});