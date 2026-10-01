import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from "vitest";
import {
  clearCropPreview,
  resetCropPreviewToCanvas,
  applyCropPreview,
  hideCropPreview,
  restoreHiddenCropPreview,
  discardCropSession,
  type CropPreviewControls,
} from "../cropToolActions";
import { DocumentEngine } from "@/engine/document";
import * as bridge from "@/lib/protocol/bridge";
import {
  getFacade,
  seedFacadeFromEngine,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";
import { installCanvasRouteEmulator, type CanvasRouteEmulator } from "@/__tests__/canvasRouteEmulator";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as Mock<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>;

function controls(overrides: Partial<CropPreviewControls> = {}) {
  return {
    cropRect: () => ({ x: 10, y: 20, w: 100, h: 80 }),
    cropRotation: () => 12,
    hiddenCropPreview: () => null,
    setCropRect: vi.fn(),
    setCropRotation: vi.fn(),
    setHiddenCropPreview: vi.fn(),
    ...overrides,
  } satisfies CropPreviewControls;
}

describe("cropToolActions", () => {
  beforeEach(() => {
    localStorage.setItem("photrez.facade", "0");
    localStorage.setItem("photrez.facadeAuthority", "wasm");
  });
  afterEach(() => {
    localStorage.removeItem("photrez.facade");
    localStorage.removeItem("photrez.facadeAuthority");
  });
  it("clearCropPreview clears rect and rotation", () => {
    const setCropRect = vi.fn();
    const setCropRotation = vi.fn();
    clearCropPreview({ setCropRect, setCropRotation });
    expect(setCropRect).toHaveBeenCalledWith(null);
    expect(setCropRotation).toHaveBeenCalledWith(0);
  });

  it("resetCropPreviewToCanvas resets rect to engine bounds and rotation to 0", () => {
    const setCropRect = vi.fn();
    const setCropRotation = vi.fn();
    const setHiddenCropPreview = vi.fn();
    const engine = {
      getWidth: () => 1200,
      getHeight: () => 800,
    };
    resetCropPreviewToCanvas({ engine, setCropRect, setCropRotation, setHiddenCropPreview });
    expect(setCropRect).toHaveBeenCalledWith({ x: 0, y: 0, w: 1200, h: 800 });
    expect(setCropRotation).toHaveBeenCalledWith(0);
    expect(setHiddenCropPreview).toHaveBeenCalledWith(null);
  });

  it("applyCropPreview commits history, applies crop, schedules render, clears, and sets tool to move", () => {
    const setCropRect = vi.fn();
    const setCropRotation = vi.fn();
    const setHiddenCropPreview = vi.fn();
    const setActiveTool = vi.fn();
    const setSelectedLayerId = vi.fn();
    const recenterViewport = vi.fn();
    const snapshot = { dummy: "snapshot" };
    
    const engine = {
      snapshot: () => snapshot,
      applyCrop: vi.fn(),
      setActiveLayer: vi.fn(),
      getWidth: () => 300,
      getHeight: () => 200,
      getViewport: () => ({ zoom: 1 }),
      getLayers: () => [
        { id: "layer-with-bitmap", imageBitmap: { width: 300, height: 400 } },
        { id: "layer-without-bitmap", imageBitmap: null },
      ],
    };
    
    const history = {
      commit: vi.fn(),
    };
    
    const workspace = {
      getActiveEngine: () => engine,
      getActiveHistory: () => history,
    };
    
    const scheduler = {
      requestRender: vi.fn(),
    };

    const renderer = {
      uploadImage: vi.fn(),
      resize: vi.fn(),
      resizeToViewport: vi.fn(),
    };

    applyCropPreview({
      workspace: workspace as any,
      renderer: renderer as any,
      viewport: { width: 1048, height: 594 },
      cropRect: { x: 10, y: 20, w: 100, h: 200 },
      cropMode: "size",
      cropSizeTarget: { w: 300, h: 400 },
      cropDeletePixels: true,
      cropRotation: 45,
      scheduler: scheduler as any,
      setCropRect,
      setCropRotation,
      setHiddenCropPreview,
      setActiveTool,
      setSelectedLayerId,
      recenterViewport,
    });

    // The commit carries the host-owned document-size pair (see the docSizeChange
    // parameter): the pre-crop model size and the crop's own output size.
    expect(history.commit).toHaveBeenCalledWith(snapshot, "Crop Canvas", undefined, false, null, {
      before: { width: 300, height: 200 },
      after: { width: 300, height: 400 },
    });
    expect(engine.applyCrop).toHaveBeenCalledWith(10, 20, 100, 200, {
      deleteCroppedPixels: true,
      targetSize: { w: 300, h: 400 },
      rotation: 45,
    });
    expect(renderer.resizeToViewport).toHaveBeenCalledWith(1048, 594, 1);
    expect(renderer.uploadImage).toHaveBeenCalledWith("layer-with-bitmap", { width: 300, height: 400 });
    expect(renderer.uploadImage).toHaveBeenCalledTimes(1);
    expect(recenterViewport).toHaveBeenCalledOnce();
    expect(scheduler.requestRender).toHaveBeenCalled();
    expect(setCropRect).toHaveBeenCalledWith(null);
    expect(setCropRotation).toHaveBeenCalledWith(0);
    expect(setHiddenCropPreview).toHaveBeenCalledWith(null);
    expect(setActiveTool).toHaveBeenCalledWith("move");
    expect(setSelectedLayerId).toHaveBeenCalledWith(null);
    expect(engine.setActiveLayer).toHaveBeenCalledWith(null);
  });

  it("supplies the host-owned size pair from the crop rect when there is no target size", () => {
    // No cropSizeTarget -> the crop's output size IS the (rounded) crop rect, which
    // is what DocumentEngine.applyCrop derives as finalW/finalH. Both halves must
    // be right, or the undo resizes the document to a size the user was never in.
    const history = { commit: vi.fn() };
    const engine = {
      snapshot: () => ({}),
      applyCrop: vi.fn(),
      setActiveLayer: vi.fn(),
      getWidth: () => 512,
      getHeight: () => 480,
      getViewport: () => ({ zoom: 1 }),
      getLayers: () => [],
    };
    const workspace = {
      getActiveEngine: () => engine,
      getActiveHistory: () => history,
    };
    const scheduler = { requestRender: vi.fn() };
    const renderer = { uploadImage: vi.fn(), resize: vi.fn(), resizeToViewport: vi.fn() };

    applyCropPreview({
      workspace: workspace as any,
      renderer: renderer as any,
      viewport: { width: 800, height: 600 },
      cropRect: { x: 10, y: 20, w: 100.4, h: 60.6 },
      cropMode: "free",
      cropSizeTarget: null,
      cropDeletePixels: true,
      cropRotation: 0,
      scheduler: scheduler as any,
      setCropRect: vi.fn(),
      setCropRotation: vi.fn(),
      setHiddenCropPreview: vi.fn(),
      setActiveTool: vi.fn(),
      setSelectedLayerId: vi.fn(),
      recenterViewport: vi.fn(),
    });

    expect(history.commit).toHaveBeenCalledWith({}, "Crop Canvas", undefined, false, null, {
      before: { width: 512, height: 480 },
      // Rounded from 100.4 x 60.6, matching the rounded rect handed to applyCrop.
      after: { width: 100, height: 61 },
    });
  });

  // The counter's concrete scenario, pinned. The crop commit happens BEFORE
  // applyCrop runs, and applyCrop silently no-ops on a target size past the
  // device/app ceiling. If the commit recorded that size anyway, undo would be a
  // no-op and the following REDO would write 20000 into model.width - past
  // MAX_CANVAS_DIM. cropDeletePixels defaults to true, and the routed path
  // returns "legacy" BEFORE its own size validation, so this is the DEFAULT path.
  function cropWithTarget(target: { w: number; h: number }) {
    const history = { commit: vi.fn() };
    const engine = {
      snapshot: () => ({}),
      applyCrop: vi.fn(),
      setActiveLayer: vi.fn(),
      getWidth: () => 128,
      getHeight: () => 128,
      getViewport: () => ({ zoom: 1 }),
      getLayers: () => [],
    };
    applyCropPreview({
      workspace: {
        getActiveEngine: () => engine,
        getActiveHistory: () => history,
      } as any,
      renderer: { uploadImage: vi.fn(), resize: vi.fn(), resizeToViewport: vi.fn() } as any,
      viewport: { width: 800, height: 600 },
      cropRect: { x: 10, y: 20, w: 100, h: 100 },
      cropMode: "size",
      cropSizeTarget: target,
      cropDeletePixels: true,
      cropRotation: 0,
      scheduler: { requestRender: vi.fn() } as any,
      setCropRect: vi.fn(),
      setCropRotation: vi.fn(),
      setHiddenCropPreview: vi.fn(),
      setActiveTool: vi.fn(),
      setSelectedLayerId: vi.fn(),
      recenterViewport: vi.fn(),
    });
    return history.commit.mock.calls[0];
  }

  it("records NO size when the target size exceeds the canvas ceiling", () => {
    const call = cropWithTarget({ w: 20000, h: 20000 });
    // undefined, not a pair: the entry then carries no size, so neither undo nor
    // redo can write a size the document never took.
    expect(call[5]).toBeUndefined();
    expect(JSON.stringify(call)).not.toContain("20000");
  });

  it("records NO size when the crop rect rounds to zero", () => {
    const history = { commit: vi.fn() };
    const engine = {
      snapshot: () => ({}),
      applyCrop: vi.fn(),
      setActiveLayer: vi.fn(),
      getWidth: () => 128,
      getHeight: () => 128,
      getViewport: () => ({ zoom: 1 }),
      getLayers: () => [],
    };
    applyCropPreview({
      workspace: { getActiveEngine: () => engine, getActiveHistory: () => history } as any,
      renderer: { uploadImage: vi.fn(), resize: vi.fn(), resizeToViewport: vi.fn() } as any,
      viewport: { width: 800, height: 600 },
      // 0.4 x 0.4 rounds to 0 x 0 - applyCrop rejects it.
      cropRect: { x: 10, y: 20, w: 0.4, h: 0.4 },
      cropMode: "free",
      cropSizeTarget: null,
      cropDeletePixels: true,
      cropRotation: 0,
      scheduler: { requestRender: vi.fn() } as any,
      setCropRect: vi.fn(),
      setCropRotation: vi.fn(),
      setHiddenCropPreview: vi.fn(),
      setActiveTool: vi.fn(),
      setSelectedLayerId: vi.fn(),
      recenterViewport: vi.fn(),
    });
    expect(history.commit.mock.calls[0][5]).toBeUndefined();
  });

  it("rounds fractional crop rect to integers before committing", () => {
    const engine = {
      snapshot: () => ({}),
      applyCrop: vi.fn(),
      setActiveLayer: vi.fn(),
      getWidth: () => 300,
      getHeight: () => 200,
      getViewport: () => ({ zoom: 1 }),
      getLayers: () => [],
    };
    const workspace = {
      getActiveEngine: () => engine,
      getActiveHistory: () => ({ commit: vi.fn() }),
    };
    const scheduler = { requestRender: vi.fn() };
    const renderer = { uploadImage: vi.fn(), resize: vi.fn(), resizeToViewport: vi.fn() };

    applyCropPreview({
      workspace: workspace as any,
      renderer: renderer as any,
      viewport: { width: 1048, height: 594 },
      cropRect: { x: 10.3, y: 20.7, w: 100.9, h: 200.1 },
      cropMode: "free",
      cropSizeTarget: null,
      cropDeletePixels: false,
      cropRotation: 0,
      scheduler: scheduler as any,
      setCropRect: vi.fn(),
      setCropRotation: vi.fn(),
      setHiddenCropPreview: vi.fn(),
      setActiveTool: vi.fn(),
      setSelectedLayerId: vi.fn(),
    });

    expect(engine.applyCrop).toHaveBeenCalledWith(10, 21, 101, 200, expect.any(Object));
  });

  it("regression 2026-06-19: post-crop buffer resize uses VIEWPORT dimensions, not doc×zoom (prevents stretched checkerboard)", () => {
    // Bug: renderer.resize(docW, docH, zoom, dpr) sized the buffer to doc×zoom×dpr
    // while the canvas CSS is 100%×100% of the viewport. When doc aspect ≠ viewport
    // aspect, browser non-uniformly scaled the buffer → cells became non-square
    // ("melar/stretched"). Fix: resizeToViewport(viewportW, viewportH, dpr).
    const engine = {
      snapshot: () => ({}),
      applyCrop: vi.fn(),
      setActiveLayer: vi.fn(),
      getWidth: () => 300,
      getHeight: () => 200,
      getViewport: () => ({ zoom: 0.8 }),
      getLayers: () => [],
    };
    const workspace = {
      getActiveEngine: () => engine,
      getActiveHistory: () => ({ commit: vi.fn() }),
    };
    const scheduler = { requestRender: vi.fn() };
    const renderer = { uploadImage: vi.fn(), resize: vi.fn(), resizeToViewport: vi.fn() };

    applyCropPreview({
      workspace: workspace as any,
      renderer: renderer as any,
      viewport: { width: 1100, height: 760 },
      cropRect: { x: 0, y: 0, w: 200, h: 200 },
      cropMode: "free",
      cropSizeTarget: null,
      cropDeletePixels: false,
      cropRotation: 0,
      scheduler: scheduler as any,
      setCropRect: vi.fn(),
      setCropRotation: vi.fn(),
      setHiddenCropPreview: vi.fn(),
      setActiveTool: vi.fn(),
      setSelectedLayerId: vi.fn(),
    });

    // After fix: buffer = viewport × dpr (not docW × zoom × dpr).
    expect(renderer.resizeToViewport).toHaveBeenCalledWith(1100, 760, 1);
    // Old buggy path must not be called.
    expect(renderer.resize).not.toHaveBeenCalled();
  });
});

describe("cropToolActions hidden preview", () => {
  it("hides the visible crop preview without discarding it", () => {
    const c = controls();

    hideCropPreview(c);

    expect(c.setHiddenCropPreview).toHaveBeenCalledWith({
      rect: { x: 10, y: 20, w: 100, h: 80 },
      rotation: 12,
    });
    expect(c.setCropRect).toHaveBeenCalledWith(null);
    expect(c.setCropRotation).toHaveBeenCalledWith(0);
  });

  it("restores hidden crop preview exactly", () => {
    const c = controls({
      cropRect: () => null,
      cropRotation: () => 0,
      hiddenCropPreview: () => ({
        rect: { x: 30, y: 40, w: 120, h: 90 },
        rotation: -8,
      }),
    });

    const restored = restoreHiddenCropPreview(c);

    expect(restored).toBe(true);
    expect(c.setCropRect).toHaveBeenCalledWith({ x: 30, y: 40, w: 120, h: 90 });
    expect(c.setCropRotation).toHaveBeenCalledWith(-8);
    expect(c.setHiddenCropPreview).toHaveBeenCalledWith(null);
  });

  it("reports false when there is no hidden preview to restore", () => {
    const c = controls({
      cropRect: () => null,
      hiddenCropPreview: () => null,
    });

    const restored = restoreHiddenCropPreview(c);

    expect(restored).toBe(false);
    expect(c.setCropRect).not.toHaveBeenCalled();
    expect(c.setCropRotation).not.toHaveBeenCalled();
  });

  it("discards visible and hidden crop preview", () => {
    const c = controls({
      hiddenCropPreview: () => ({
        rect: { x: 30, y: 40, w: 120, h: 90 },
        rotation: -8,
      }),
    });

    discardCropSession(c);

    expect(c.setCropRect).toHaveBeenCalledWith(null);
    expect(c.setCropRotation).toHaveBeenCalledWith(0);
    expect(c.setHiddenCropPreview).toHaveBeenCalledWith(null);
  });
});

describe("applyCropPreview routed path (flag ON + native)", () => {
  let emulator: CanvasRouteEmulator;

  afterEach(() => {
    vi.restoreAllMocks();
    localStorage.removeItem("photrez.facade");
    localStorage.removeItem("photrez.facadeAuthority");
    bridge.__resetNativeAuthorityForTests();
    __resetFacadeRegistryForTests();
    invokeMock.mockReset();
    emulator?.reset();
  });

  function controls() {
    return {
      setCropRect: vi.fn(),
      setCropRotation: vi.fn(),
      setHiddenCropPreview: vi.fn(),
      setActiveTool: vi.fn(),
      setSelectedLayerId: vi.fn(),
      recenterViewport: vi.fn(),
      scheduler: { requestRender: vi.fn() },
      renderer: { uploadImage: vi.fn(), resizeToViewport: vi.fn() },
    };
  }

  async function nativeDoc() {
    localStorage.setItem("photrez.facade", "1");
    localStorage.setItem("photrez.facadeAuthority", "native");
    bridge.__resetNativeAuthorityForTests();
    invokeMock.mockReset();
    emulator = installCanvasRouteEmulator(invokeMock);
    const engine = new DocumentEngine("crop1", "crop1", 800, 600);
    const layer = engine.addLayer("A", 100, 100);
    layer.imageBitmap = { width: 4, height: 4 } as unknown as ImageBitmap;
    const facade = getFacade("crop1");
    await seedFacadeFromEngine(engine as never, facade);
    // Native baseline size so undo has a prior size to restore.
    await facade.resizeCanvas(800, 600);
    engine.applyFacadeSnapshot(facade.snapshot as never, { dimsAuthoritative: true });
    const historyCommit = vi.fn();
    const workspace = {
      getActiveEngine: () => engine,
      getActiveHistory: () => ({ commit: historyCommit }),
    };
    return { engine, facade, historyCommit, workspace };
  }

  it("non-destructive crop: dims applied via the native arm, no TS history commit, facade undo restores dims", async () => {
    const { engine, facade, historyCommit, workspace } = await nativeDoc();
    const c = controls();
    const applyCropSpy = vi.spyOn(engine, "applyCrop");

    applyCropPreview({
      workspace: workspace as any,
      renderer: c.renderer as any,
      viewport: { width: 1048, height: 594 },
      cropRect: { x: 10, y: 20, w: 100, h: 100 },
      cropMode: "free",
      cropSizeTarget: null,
      cropDeletePixels: false,
      cropRotation: 0,
      scheduler: c.scheduler as any,
      setCropRect: c.setCropRect,
      setCropRotation: c.setCropRotation,
      setHiddenCropPreview: c.setHiddenCropPreview,
      setActiveTool: c.setActiveTool,
      setSelectedLayerId: c.setSelectedLayerId,
      recenterViewport: c.recenterViewport,
    });

    await vi.waitFor(() => {
      expect(engine.getWidth()).toBe(100);
    });
    expect(engine.getHeight()).toBe(100);
    expect(historyCommit).not.toHaveBeenCalled();
    expect(applyCropSpy).not.toHaveBeenCalled();
    expect(c.setActiveTool).toHaveBeenCalledWith("move");
    expect(c.renderer.resizeToViewport).toHaveBeenCalled();

    await facade.undo();
    expect(facade.lastHistoryDeltaWasEmpty).toBe(false);
    engine.applyFacadeSnapshot(facade.snapshot as never, { dimsAuthoritative: true });
    expect([engine.getWidth(), engine.getHeight()]).toEqual([800, 600]);
  });

  it("destructive crop defers to the legacy path (TS history commit + engine.applyCrop)", async () => {
    const { engine, historyCommit, workspace } = await nativeDoc();
    const c = controls();
    const applyCropSpy = vi.spyOn(engine, "applyCrop");

    applyCropPreview({
      workspace: workspace as any,
      renderer: c.renderer as any,
      viewport: { width: 1048, height: 594 },
      cropRect: { x: 10, y: 20, w: 100, h: 100 },
      cropMode: "free",
      cropSizeTarget: null,
      cropDeletePixels: true,
      cropRotation: 0,
      scheduler: c.scheduler as any,
      setCropRect: c.setCropRect,
      setCropRotation: c.setCropRotation,
      setHiddenCropPreview: c.setHiddenCropPreview,
      setActiveTool: c.setActiveTool,
      setSelectedLayerId: c.setSelectedLayerId,
    });

    await vi.waitFor(() => {
      expect(applyCropSpy).toHaveBeenCalled();
    });
    // History sees the PRE-crop snapshot (800x600), committed before the mutation,
    // plus the host-owned size pair for the crop that is about to run.
    expect(historyCommit).toHaveBeenCalledWith(
      expect.objectContaining({ width: 800, height: 600 }),
      "Crop Canvas",
      undefined,
      false,
      null,
      {
        before: { width: 800, height: 600 },
        after: { width: 100, height: 100 },
      },
    );
    expect(applyCropSpy).toHaveBeenCalledWith(
      10,
      20,
      100,
      100,
      expect.objectContaining({ deleteCroppedPixels: true }),
    );
  });
});
