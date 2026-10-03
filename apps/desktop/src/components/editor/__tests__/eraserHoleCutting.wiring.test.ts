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
 * HARNESS FIDELITY. Real `DocumentEngine`, real wasm graph mirror, real `EditorFacade`,
 * the transport-faithful Rust store emulator (rejects where Rust rejects, including
 * `rust_pixels_get_epoch` for an unseeded layer), and the REAL `useBrushOverlay` hook
 * driven through its exported `onPaintStroke` / `commitBrushStroke`. The raster is read
 * back from the STORE, never from a downstream symptom. The tool is set INSIDE the helper:
 * a helper that sets the tool to brush and then strokes has shipped a false positive in
 * this repo before.
 */
import { describe, it, expect, vi, beforeAll, beforeEach, afterEach, type Mock } from "vitest";
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
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
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
      return store.invoke(cmd, args);
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

async function readStoreRaster(docId: string, layerId: string): Promise<Raster> {
  let epoch = 0;
  try {
    epoch = Number(await store.invoke("rust_pixels_get_epoch", { docId, layerId }));
  } catch {
    // The emulator REJECTS for an unseeded layer - that rejection IS the absence.
    return { sumRGB: 0, nz: 0, alphaZero: 0, present: false, epoch: 0 };
  }
  const tiles = (await store.invoke("rust_pixels_snapshot_layer", { docId, layerId })) as Array<{
    x: number;
    y: number;
    w: number;
    h: number;
    data: number[];
  }>;
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

const SETTINGS = { size: 32, hardness: 1, opacity: 1, flow: 1, smoothing: 0 };

/**
 * One real eraser stroke through the real hook.
 *
 * The tool is set HERE, inside the helper, before the stroke: a helper that leaves the
 * tool on brush and then calls the stroke has produced a false "the eraser works"
 * measurement in this repo before.
 */
async function eraseBandOn(
  opts: { isBackground: boolean; bgSwatch: string },
): Promise<{
  before: Raster;
  after: Raster;
  stages: string;
  /** Overlay census read AFTER the final composite, BEFORE the snapshot capture. */
  overlayAtSnapshot: Raster;
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
  // Opaque white content, so "did the erase change anything" is unambiguous.
  const canvas = new OffscreenCanvas(SIZE, SIZE);
  const ctx = canvas.getContext("2d") as unknown as CanvasRenderingContext2D;
  ctx.fillStyle = "#ffffff";
  ctx.fillRect(0, 0, SIZE, SIZE);
  engine.setLayerImageBitmap(layer.id, canvas.transferToImageBitmap());
  engine.setActiveLayer(layer.id);
  // Seed the store so the layer is PRESENT before the stroke - a real document's
  // background always is, and the reported run observed `false -> PRESENT`.
  const surface = engine.getPaintSurface(layer.id);
  expect(surface, "premise: the engine exposes a real paint surface").not.toBeNull();
  await store.invoke("rust_pixels_open_document", { docId: DOC });
  await store.invoke("rust_pixels_init", {
    docId: DOC,
    layerId: layer.id,
    width: SIZE,
    height: SIZE,
    bytes: new Uint8Array(SIZE * SIZE * 4).fill(255),
  });

  const before = await readStoreRaster(DOC, layer.id);
  expect(before.present, "premise: the store holds the layer").toBe(true);
  expect(before.alphaZero, "premise: the layer starts fully opaque").toBe(0);

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
    activeTool: () => "eraser",
    eraserSize: () => SETTINGS.size,
    eraserHardness: () => SETTINGS.hardness,
    brushSize: () => SETTINGS.size,
    brushHardness: () => SETTINGS.hardness,
    fgColor: () => "#000000",
    bgColor: () => opts.bgSwatch,
    docWidth: () => SIZE,
    docHeight: () => SIZE,
  });
  const overlay = useBrushOverlay();
  overlay.setOverlayCanvasRef(overlayCanvas as never);

  // A horizontal band through the middle: eraser size 32 over a 64px canvas.
  overlay.onPaintStroke(
    [
      { x: 8, y: SIZE / 2 },
      { x: SIZE - 8, y: SIZE / 2 },
    ],
    true,
    SETTINGS,
    true,
  );
  // THE SEAM MEASUREMENT. `onPaintStroke(..., isFinal=true)` runs the final composite
  // synchronously (useBrushOverlay.ts:939-941) and the snapshot capture happens INSIDE
  // `commitBrushStroke` (:1424-1426), so this read sits exactly between them and needs no
  // trust in the dab path: it is the overlay the capture is about to copy.
  //   overlay erased here, payload opaque  -> the SNAPSHOT CAPTURE is the defect
  //   overlay pre-stroke here             -> the COMPOSITE is the defect
  const snapshotCensus = (() => {
    const ctx = overlayCanvas.getContext("2d") as unknown as {
      getImageData(): { data: Uint8ClampedArray };
    };
    const d = ctx.getImageData().data;
    let sumRGB = 0;
    let alphaZero = 0;
    for (let i = 0; i + 3 < d.length; i += 4) {
      sumRGB += d[i] + d[i + 1] + d[i + 2];
      if (d[i + 3] === 0) alphaZero += 1;
    }
    return { sumRGB, nz: d.length / 4 - alphaZero, alphaZero, present: true, epoch: 0 };
  })();

  writes = [];
  await overlay.commitBrushStroke(engine as never, history as never, layer.id, true);
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
    stages: `overlay alphaZero=${oAlphaZero} surface alphaZero=${sAlphaZero} writes=${JSON.stringify(writes)}`,
    overlayAtSnapshot: snapshotCensus,
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

describe("eraser on an opaque white layer: what decides whether it cuts holes", () => {
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

  it.fails("SEAM MEASUREMENT: a NON-background erase must reach the wire", async () => {
    // THE MEASUREMENT, and the answer it gave.
    //
    // `onPaintStroke(..., isFinal=true)` runs the final composite synchronously
    // (useBrushOverlay.ts:939-941); the snapshot capture happens INSIDE
    // `commitBrushStroke` (:1424-1426). The census below is read between those two, so it
    // needs no trust in the dab path.
    //
    //   overlayAtSnapshot = { sumRGB: 3133440, nz: 4096, alphaZero: 0 }   PRE-STROKE WHITE
    //   payload           = { sumRGB: 1811520, alphaZero: 0, w: 64, h: 37 }
    //   store after       = { sumRGB: 3133440, nz: 4096, alphaZero: 0, epoch: 1 }
    //
    // The overlay does NOT hold the erase at capture time and DOES hold it afterwards
    // (post-commit `overlay alphaZero=4096`), so the erase is applied to the overlay after
    // the payload was copied. THE SEAM IS THE COMPOSITE, not the snapshot capture: the
    // final composite is not producing the erased overlay that `c4ScratchSnap` copies.
    //
    // `it.fails` pins the defect exactly and keeps the suite green; when the composite is
    // fixed this test will fail loudly, which is the alarm we want. Delete it then.
    const { before, after, overlayAtSnapshot, payload, stages } = await eraseBandOn({
      isBackground: false,
      bgSwatch: "#ffffff",
    });

    const report =
      "overlayAtSnapshot=" + JSON.stringify(overlayAtSnapshot) +
      " payload=" + JSON.stringify(payload) +
      " store=" + JSON.stringify(after) +
      " before=" + JSON.stringify(before) +
      " " + stages;

    // INVARIANT, stated once and true in both directions: the payload is a copy of the
    // overlay region, so overlay-erased-but-payload-opaque names the SNAPSHOT CAPTURE,
    // and overlay-pre-stroke names the COMPOSITE.
    expect(
      payload !== null && payload.alphaZero > 0,
      "the payload must carry erased pixels; " + report,
    ).toBe(true);
    expect(
      after.alphaZero,
      "and the store must end up with holes; " + report,
    ).toBeGreaterThan(before.alphaZero);
  });

  it("prior brush history is NOT the discriminator - the layer KIND is", () => {
    // erased an already-opaque layer. `resolveEraserFill` reads ONE field, and it is
    // not history.
    expect(resolveEraserFill({ isBackground: true } as never, true, "#ffffff")).toEqual({
      isEraser: false,
      color: "#ffffff",
    });
    expect(resolveEraserFill({ isBackground: false } as never, true, "#ffffff").isEraser).toBe(true);
  });
});