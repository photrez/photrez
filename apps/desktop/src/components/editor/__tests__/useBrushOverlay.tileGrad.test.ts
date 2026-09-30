import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "../dialogs/DialogProvider";
import { useBrushOverlay, flushC4Commits } from "../useBrushOverlay";
import type { DocumentEngine } from "@/engine/document";
import type { CommandHistory } from "@/engine/history";

// T-BRUSH-TILECOMMIT-GRAD (2026-08-24): flag graduation + failure recovery +
// context-loss upload handling + stroke cancellation contracts from
// contracts drafted for the 2026-08-24 tile-commit graduation work.

const { showToast } = vi.hoisted(() => ({ showToast: vi.fn() }));
vi.mock("../Toast", () => ({ showToast }));

// jsdom polyfills mirroring useBrushOverlay.test.ts
function makeMockImageBitmap(w = 100, h = 80): HTMLCanvasElement {
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  (c as any).close = () => {};
  return c;
}
if (typeof globalThis.createImageBitmap === "undefined") {
  (globalThis as any).createImageBitmap = async (source: CanvasImageSource) => {
    const w = (source as any).width ?? 100;
    const h = (source as any).height ?? 80;
    return makeMockImageBitmap(w, h) as unknown as ImageBitmap;
  };
}
if (typeof globalThis.OffscreenCanvas === "undefined") {
  (globalThis as any).OffscreenCanvas = function OffscreenCanvas(w: number, h: number) {
    const c = document.createElement("canvas");
    c.width = w;
    c.height = h;
    (c as any).transferToImageBitmap = () => makeMockImageBitmap(w, h) as unknown as ImageBitmap;
    return c;
  };
}
// jsdom has no ImageData; the Rust commit path builds one per returned tile.
if (typeof (globalThis as any).ImageData === "undefined") {
  (globalThis as any).ImageData = class {
    data: Uint8ClampedArray;
    width: number;
    height: number;
    constructor(data: ArrayLike<number> | number, width?: number, height?: number) {
      if (typeof data === "number") {
        this.width = data;
        this.height = width!;
        this.data = new Uint8ClampedArray(data * width! * 4);
      } else {
        this.data = Uint8ClampedArray.from(data);
        this.width = width!;
        this.height = height!;
      }
    }
  };
}

// Every brush stroke is recorded by Rust on the default path, so the harness
// has to answer the Rust IPC surface the commit issues. Byte-faithful enough to
// prove the canonical buffer moved; not a pixel-parity oracle.
const rust = vi.hoisted(() => ({
  pixels: new Uint8ClampedArray(512 * 512 * 4),
  epoch: 0,
  writes: 0,
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string, args: any) => {
    if (cmd === "rust_pixels_get_epoch") return rust.epoch;
    if (cmd === "rust_pixels_init") {
      rust.pixels = new Uint8ClampedArray(args.bytes as ArrayLike<number>);
      rust.epoch = 0;
      return null;
    }
    if (cmd === "rust_pixels_snapshot_layer") {
      return [{ x: 0, y: 0, w: 512, h: 512, data: Array.from(rust.pixels) }];
    }
    if (cmd === "rust_pixels_write_region") {
      const before = Array.from(rust.pixels);
      const after = new Uint8ClampedArray(rust.pixels);
      for (let row = 0; row < args.h; row++) {
        after.set(
          args.rgba.subarray(row * args.w * 4, (row + 1) * args.w * 4),
          ((args.y + row) * 512 + args.x) * 4,
        );
      }
      rust.pixels = after;
      rust.epoch += 1;
      rust.writes += 1;
      const tile = { x: args.x, y: args.y, w: args.w, h: args.h };
      return {
        before: [{ ...tile, data: before }],
        after: [{ ...tile, data: Array.from(after) }],
        epoch: rust.epoch,
        version: rust.epoch,
      };
    }
    return null;
  }),
}));

const dialogConfirm = vi.fn();
beforeAll(() => {
  vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({
    confirm: dialogConfirm,
  } as unknown as ReturnType<typeof DialogProviderModule.useDialog>);
});
afterAll(() => {
  vi.restoreAllMocks();
});

function makeLayer() {
  return {
    id: "layer-1",
    name: "L",
    visible: true,
    locked: false,
    lockTransparency: false,
    isBackground: false,
    hasAdjustments: false,
    basicAdjustment: undefined,
    transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    width: 512,
    height: 512,
    imageBitmap: document.createElement("canvas") as unknown as ImageBitmap,
  };
}

/** Mock PaintTileSurface: snapshotTile/readRect/restoreTile are observable. */
function makeSurface() {
  const snapshotted: unknown[] = [];
  const restored: unknown[] = [];
  let readRectShouldThrow = false;
  const surface = {
    snapshotted,
    restored,
    context: {
      drawImage: vi.fn(),
      clearRect: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),
      // applyRustTilesToSurface paints the tiles Rust returned through these,
      // so the Rust commit path needs both to exist.
      putImageData: vi.fn(),
      getImageData: vi.fn((_x: number, _y: number, w: number, h: number) => ({
        width: w,
        height: h,
        data: new Uint8ClampedArray(w * h * 4),
      })),
      globalCompositeOperation: "source-over",
      globalAlpha: 1,
    },
    snapshotTile: vi.fn((t: { x: number; y: number; w: number; h: number }) => {
      const patch = { tx: t.x / 256, ty: t.y / 256, value: { width: t.w, height: t.h, data: new Uint8ClampedArray(t.w * t.h * 4).fill(7) } };
      snapshotted.push(patch);
      return patch;
    }),
    restoreTile: vi.fn((p: unknown) => {
      restored.push(p);
    }),
    readRect: vi.fn((_x: number, _y: number, w: number, h: number) => {
      if (readRectShouldThrow) throw new Error("readRect exploded");
      return { width: w, height: h, data: new Uint8ClampedArray(w * h * 4) };
    }),
    toImageBitmap: vi.fn(async () => makeMockImageBitmap()),
    failReadRect: () => { readRectShouldThrow = true; },
  };
  return surface;
}

function makeHarness(surface: ReturnType<typeof makeSurface> | null, uploadSurfaceTiles = vi.fn()) {
  const layer = makeLayer();
  const commit = vi.fn();
  let _lpc: { x: number; y: number } | null = null;
  const history = {
    commit,
    setLastPaintCoords: (c: { x: number; y: number } | null) => { _lpc = c; },
    getLastPaintCoords: () => _lpc,
  };
  let lastSetBitmap: unknown = null;
  const engine = {
    getActiveLayerId: () => layer.id,
    getLayer: () => layer,
    // Mirror the real engine: setLayerImageBitmap mutates the model bitmap, and
    // snapshot() reads it back. Lets a test assert the committed snapshot carries
    // the painted bitmap.
    snapshot: vi.fn(() => ({ layers: [{ id: "layer-1", imageBitmap: lastSetBitmap }] })),
    setLayerImageBitmap: vi.fn((_id: string, b: unknown) => { lastSetBitmap = b; }),
    getPaintSurface: surface ? () => surface : () => null,
  };
  const uploadImage = vi.fn();
  showToast.mockClear();
  mockUseEditor({
    // getActiveDocumentId is read by the Rust commit path, which now owns
    // every brush stroke by default (photrez.rustPixels gates bucket/fill/bake
    // only), so the harness has to answer it or the stroke dies on a TypeError.
    workspace: { getActiveEngine: () => engine, getActiveHistory: () => history, getActiveDocumentId: () => "doc-tilegrad" },
    renderer: { uploadImage, uploadSurfaceTiles },
    scheduler: { requestRender: vi.fn() },
    fgColor: () => "#ff0000",
    bgColor: () => "#ffffff",
    docWidth: () => 512,
    docHeight: () => 512,
    activeTool: () => "brush",
    brushSize: () => 20,
    brushHardness: () => 1,
    eraserSize: () => 20,
    eraserHardness: () => 1,
  });
  const canvas = document.createElement("canvas");
  canvas.width = 512;
  canvas.height = 512;
  const overlay = useBrushOverlay();
  overlay.setOverlayCanvasRef(canvas);
  return { overlay, layer, engine: engine as unknown as DocumentEngine, history: history as unknown as CommandHistory, commit, uploadSurfaceTiles, uploadImage };
}

const settings = { size: 20, hardness: 1, opacity: 1, flow: 1, smoothing: 0.5 };

// The brush stroke now always takes the deferred Rust commit path (nothing
// gates it on photrez.rustPixels), so the commit runs after commitBrushStroke
// returns. Every assertion below must drain that queue first, or it would read
// a half-finished stroke. The synchronous tile-commit arm is reached only when
// photrez.canonicalCommit is on (C3 mode) or the brush produced no scratch.
afterEach(async () => { await flushC4Commits(); });

describe("photrez.tileCommit graduation (default ON, opt-out 0)", () => {
  beforeEach(() => { localStorage.removeItem("photrez.tileCommit"); });
  afterEach(() => { localStorage.removeItem("photrez.tileCommit"); });

  it("default (no localStorage entry) uses the tile-commit path", async () => {
    const surface = makeSurface();
    const { overlay, engine, history, commit, uploadSurfaceTiles } = makeHarness(surface);
    const writesBefore = rust.writes;

    overlay.onPaintStroke([{ x: 30, y: 30 }], false, settings, false);
    await overlay.commitBrushStroke(engine, history, "layer-1", false);
    await flushC4Commits();

    // Non-vacuity: this commit came from the Rust path, not the recovery arm.
    expect(rust.writes - writesBefore, "the stroke recorded one region in Rust").toBe(1);
    expect(surface.snapshotTile).toHaveBeenCalled();
    expect(commit).toHaveBeenCalledTimes(1);
    const imperative = commit.mock.calls[0][2];
    expect(imperative.before.length).toBeGreaterThan(0);
    expect(imperative.after.length).toBeGreaterThan(0);
    expect(uploadSurfaceTiles).toHaveBeenCalled();
  });

  it("explicit opt-out ('0') falls back to the legacy path (no tile snapshots)", async () => {
    localStorage.setItem("photrez.tileCommit", "0");
    const surface = makeSurface();
    const { overlay, engine, history } = makeHarness(surface);

    overlay.onPaintStroke([{ x: 30, y: 30 }], false, settings, false);
    await overlay.commitBrushStroke(engine, history, "layer-1", false);

    expect(surface.snapshotTile).not.toHaveBeenCalled();
  });
});

describe("brush commit failure recovery (pre-H exception)", () => {
  beforeEach(() => { localStorage.removeItem("photrez.tileCommit"); });

  it("readRect failure restores every before-patch, writes NO history entry, shows toast", async () => {
    // Targets the SYNCHRONOUS tile-commit arm: photrez.canonicalCommit keeps the
    // Rust deferred arm out of this stroke (C3 mode owns it there instead), so
    // the sync readRect that this recovery guards is the one that runs. Under
    // the default Rust arm the same readRect failure is caught inside the
    // deferred commit, which reports it and records nothing - a different
    // contract, covered by the Rust-path tests.
    localStorage.setItem("photrez.canonicalCommit", "1");
    const surface = makeSurface();
    surface.failReadRect();
    const { overlay, engine, history, commit } = makeHarness(surface);

    overlay.onPaintStroke([{ x: 30, y: 30 }], false, settings, false);
    await overlay.commitBrushStroke(engine, history, "layer-1", false);
    localStorage.removeItem("photrez.canonicalCommit");

    expect(surface.snapshotTile).toHaveBeenCalled();
    expect(commit).not.toHaveBeenCalled();               // no entry
    expect(surface.restoreTile).toHaveBeenCalledTimes(surface.snapshotted.length); // pristine
    expect(showToast).toHaveBeenCalledWith(expect.stringContaining("Brush commit failed"), "error");
    expect(overlay.isStrokeActive()).toBe(false);        // session dropped
  });
});

describe("context-loss upload handling", () => {
  beforeEach(() => { localStorage.removeItem("photrez.tileCommit"); });

  it("holds uploads while GL is lost and flushes them on webglcontextrestored", async () => {
    const surface = makeSurface();
    const { overlay, engine, history, uploadSurfaceTiles } = makeHarness(surface);

    window.dispatchEvent(new Event("webglcontextlost"));
    overlay.onPaintStroke([{ x: 30, y: 30 }], false, settings, false);
    await overlay.commitBrushStroke(engine, history, "layer-1", false);
    await flushC4Commits();

    expect(uploadSurfaceTiles).not.toHaveBeenCalled();   // held (and the commit already ran)

    window.dispatchEvent(new Event("webglcontextrestored"));
    expect(uploadSurfaceTiles).toHaveBeenCalledTimes(1); // flushed
    expect(uploadSurfaceTiles.mock.calls[0][0]).toBe("layer-1");
  });
});

describe("stroke cancellation (pointercancel / Escape contract)", () => {
  beforeEach(() => { localStorage.removeItem("photrez.tileCommit"); });

  it("cancelActiveStroke during a live brush stroke discards without committing", async () => {
    const surface = makeSurface();
    const { overlay, engine, history, commit } = makeHarness(surface);

    overlay.onPaintStroke([{ x: 30, y: 30 }], false, settings, false);
    expect(overlay.isStrokeActive()).toBe(true);

    expect(overlay.cancelActiveStroke()).toBe(true);
    expect(overlay.isStrokeActive()).toBe(false);
    // Surface untouched: no snapshots taken pre-commit, nothing restored either.
    expect(surface.snapshotTile).not.toHaveBeenCalled();
    expect(surface.restoreTile).not.toHaveBeenCalled();

    await overlay.commitBrushStroke(engine, history, "layer-1", false);
    expect(commit).not.toHaveBeenCalled();               // cancelled stroke never commits
  });

  it("eraser cancel re-uploads the layer bitmap so the layer becomes visible again", () => {
    const surface = makeSurface();
    const { overlay, uploadImage } = makeHarness(surface);

    overlay.onPaintStroke([{ x: 30, y: 30 }], true, settings, false); // eraser seeds 1×1 transparent upload
    const seedCalls = uploadImage.mock.calls.length;
    expect(seedCalls).toBeGreaterThan(0);

    overlay.cancelActiveStroke();
    expect(uploadImage.mock.calls.length).toBeGreaterThan(seedCalls); // visibility restored
  });

  it("cancel with no active stroke is a harmless no-op returning false", () => {
    const surface = makeSurface();
    const { overlay } = makeHarness(surface);
    expect(overlay.isStrokeActive()).toBe(false);
    expect(overlay.cancelActiveStroke()).toBe(false);
  });
});

describe("tile-path model-sync invariant (regression f48f5b6)", () => {
  beforeEach(() => { localStorage.removeItem("photrez.tileCommit"); });
  afterEach(() => { localStorage.removeItem("photrez.tileCommit"); });

  // The tile-commit path paints the derived PaintTileSurface, not layer.imageBitmap.
  // It MUST write the painted bitmap back into the model BEFORE history.commit so
  // the committed snapshot (and any later delete/undo) sees the real pixels. A
  // reorder here is exactly the bug that made selection-delete undo restore a
  // stale background, so the ordering is pinned, not left to chance.
  it("calls setLayerImageBitmap BEFORE history.commit, and the committed snapshot carries the painted bitmap", async () => {
    const surface = makeSurface();
    const { overlay, engine, history, commit } = makeHarness(surface);

    overlay.onPaintStroke([{ x: 30, y: 30 }], false, settings, false);
    await overlay.commitBrushStroke(engine, history, "layer-1", false);
    await flushC4Commits();

    // Model sync happened with the painted bitmap.
    expect(engine.setLayerImageBitmap).toHaveBeenCalledTimes(1);
    const paintedBitmap = (engine.setLayerImageBitmap as ReturnType<typeof vi.fn>).mock.calls[0][1];
    expect(paintedBitmap).toBeTruthy();

    // Ordering: the model write precedes the history commit.
    expect((engine.setLayerImageBitmap as ReturnType<typeof vi.fn>).mock.invocationCallOrder[0]).toBeLessThan(
      (commit as ReturnType<typeof vi.fn>).mock.invocationCallOrder[0],
    );

    // The snapshot handed to history.commit reflects the freshly-set bitmap.
    const committedSnapshot = (commit as ReturnType<typeof vi.fn>).mock.calls[0][0];
    expect(committedSnapshot.layers[0].imageBitmap).toBe(paintedBitmap);
  });
});
