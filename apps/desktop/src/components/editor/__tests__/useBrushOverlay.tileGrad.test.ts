import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "../dialogs/DialogProvider";
import { useBrushOverlay } from "../useBrushOverlay";
import type { DocumentEngine } from "@/engine/document";
import type { CommandHistory } from "@/engine/history";

// T-BRUSH-TILECOMMIT-GRAD (2026-08-24): flag graduation + failure recovery +
// context-loss upload handling + stroke cancellation contracts from
// docs/plans/2026-08-24-brush-ux-production-path-design.md.

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
  const engine = {
    getActiveLayerId: () => layer.id,
    getLayer: () => layer,
    snapshot: vi.fn(() => ({ model: true })),
    setLayerImageBitmap: vi.fn(),
    getPaintSurface: surface ? () => surface : () => null,
  };
  const uploadImage = vi.fn();
  showToast.mockClear();
  mockUseEditor({
    workspace: { getActiveEngine: () => engine, getActiveHistory: () => history },
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

describe("photrez.tileCommit graduation (default ON, opt-out 0)", () => {
  beforeEach(() => { localStorage.removeItem("photrez.tileCommit"); });
  afterEach(() => { localStorage.removeItem("photrez.tileCommit"); });

  it("default (no localStorage entry) uses the tile-commit path", async () => {
    const surface = makeSurface();
    const { overlay, engine, history, commit, uploadSurfaceTiles } = makeHarness(surface);

    overlay.onPaintStroke([{ x: 30, y: 30 }], false, settings, false);
    await overlay.commitBrushStroke(engine, history, "layer-1", false);

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
    const surface = makeSurface();
    surface.failReadRect();
    const { overlay, engine, history, commit } = makeHarness(surface);

    overlay.onPaintStroke([{ x: 30, y: 30 }], false, settings, false);
    await overlay.commitBrushStroke(engine, history, "layer-1", false);

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

    expect(uploadSurfaceTiles).not.toHaveBeenCalled();   // held

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
