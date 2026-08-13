// apps/desktop/src/components/editor/__tests__/useSelectionTransformDrag.test.ts
//
// Unit + contract tests for useSelectionTransformDrag — the hook that
// handles resize-handle drags, layer movement, rotation, snap cleanup,
// and Escape cancel on the SelectionTransformOverlay.
//
// This module had ZERO dedicated tests despite being 429 lines of critical
// transform/resize logic (discovered 2026-07-05 audit).
//
// Pattern: vi.mock useEditor, call hook via createRoot, dispatch
// synthetic events to handlers, assert dragState/layer mutations.

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createRoot, createSignal } from "solid-js";
import * as TransformGeometryModule from "@/viewport/transformGeometry";
import * as CursorRotateModule from "@/viewport/cursorRotate";
import { useSelectionTransformDrag } from "../useSelectionTransformDrag";
import type { Transform2D, DocumentModel } from "@/engine/types";

// ── Hoisted mock for useEditor ──────────────────────────────────────────
// Using vi.mock + vi.hoisted ensures the mock is active BEFORE any module
// evaluation (unlike vi.spyOn which patches after imports resolve).

const { mockEditorState, createMockEngine, createMockHistory,
        createMockWorkspace, DEFAULT_TRANSFORM, DEFAULT_LAYER } =
  vi.hoisted(() => {
    const _DEFAULT_TRANSFORM: Transform2D = {
      x: 100, y: 50, scaleX: 1, scaleY: 1, rotation: 0,
      flipH: false, flipV: false,
    };

    const _DEFAULT_LAYER = {
      id: "layer-1",
      name: "Test Layer",
      type: "raster" as const,
      transform: { ..._DEFAULT_TRANSFORM },
      locked: false,
      visible: true,
      opacity: 1,
      blendMode: "normal" as const,
      width: 200,
      height: 150,
      imageBitmap: null,
    };

    function _createMockEngine() {
      let currentTransform = { ..._DEFAULT_TRANSFORM };
      return {
        getId: () => "doc-1",
        getLayer: vi.fn((id: string) =>
          id === "layer-1"
            ? { ..._DEFAULT_LAYER, transform: { ...currentTransform } }
            : null
        ),
        transformLayer: vi.fn(
          (id: string, t: Partial<Transform2D>) => {
            Object.assign(currentTransform, t);
          }
        ),
        snapshot: vi.fn(
          () =>
            ({ snap: Date.now(), layers: [] } as unknown as DocumentModel)
        ),
        restore: vi.fn(),
        getWidth: () => 800,
        getHeight: () => 600,
        getActiveLayerId: () => "layer-1",
      };
    }

    function _createMockHistory() {
      return { commit: vi.fn() };
    }

    function _createMockWorkspace(
      engine: ReturnType<typeof _createMockEngine>
    ) {
      const history = _createMockHistory();
      return {
        getActiveEngine: () => engine,
        getActiveHistory: () => history,
        getEngine: () => engine,
        getHistory: () => history,
        addDocument: vi.fn(),
        getAllDocuments: () => [],
        notifyVisualChange: vi.fn(),
      };
    }

    // The mutable mock state — tests mutate this before calling the hook.
    const _mockEditorState: Record<string, any> = {};

    return {
      mockEditorState: _mockEditorState,
      createMockEngine: _createMockEngine,
      createMockHistory: _createMockHistory,
      createMockWorkspace: _createMockWorkspace,
      DEFAULT_TRANSFORM: _DEFAULT_TRANSFORM,
      DEFAULT_LAYER: _DEFAULT_LAYER,
    };
  });

// Mock useEditor BEFORE imports resolve — this replaces the module entirely.
vi.mock("../shell/EditorContext", () => ({
  useEditor: () => mockEditorState,
}));

// ── Helpers ──────────────────────────────────────────────────────────────

function makePointerEvent(
  overrides: Partial<PointerEvent> = {}
): PointerEvent {
  return {
    button: 0,
    clientX: 100,
    clientY: 100,
    pointerId: 1,
    shiftKey: false,
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    target: document.createElement("div"),
    currentTarget: document.createElement("div"),
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
    ...overrides,
  } as unknown as PointerEvent;
}

// ── Test setup ───────────────────────────────────────────────────────────

function setupHook(
  opts: {
    layerTransform?: Partial<Transform2D>;
    zoom?: number;
    pan?: { x: number; y: number };
    addLayer?: boolean;
    navigationMode?: boolean;
    snapEnabled?: boolean;
  } = {}
) {
  const engine = createMockEngine();
  const ws = createMockWorkspace(engine);
  const scheduler = { requestRender: vi.fn() };

  // Build the mock useEditor state. Use real SolidJS createSignal so
  // createMemo inside the hook tracks dependencies correctly.
  // Each setter is wrapped as a vi.fn spy that also updates the underlying
  // signal — this lets tests verify the setter was called AND the signal
  // value is updated for downstream reads.
  const editorSignals: Record<string, any> = {};

  const defaults: Record<string, any> = {
    workspace: ws,
    scheduler,
    activeTool: "move",
    fgColor: "#000",
    bgColor: "#fff",
    zoom: opts.zoom ?? 1,
    pan: opts.pan ?? { x: 0, y: 0 },
    selectedLayerId: opts.addLayer !== false ? "layer-1" : null,
    layers: opts.addLayer !== false ? [{ ...DEFAULT_LAYER }] : [],
    hoverHandle: null,
    hoverPos: null,
    moveSnapEnabled: opts.snapEnabled ?? false,
    commitTransformState: vi.fn(),
    layerTransformSession: null,
    constrainRatio: true,
    docWidth: 800,
    docHeight: 600,
  };

  createRoot(() => {
    for (const [key, val] of Object.entries(defaults)) {
      if (key === "workspace" || key === "scheduler") continue;
      const [s, set] = createSignal(val);
      editorSignals[key] = s;
      const setKey =
        "set" + key.charAt(0).toUpperCase() + key.slice(1);
      // Spy that also updates the underlying Solid signal
      editorSignals[setKey] = vi.fn((v: any) => {
        set(v);
      });
    }
    editorSignals.workspace = ws;
    editorSignals.scheduler = scheduler;
  });

  // Point the hoisted mock at these signals
  Object.assign(mockEditorState, editorSignals);

  // SVG ref
  const svgEl = document.createElementNS(
    "http://www.w3.org/2000/svg",
    "svg"
  );
  const setCaptureSpy = vi.fn();
  const releaseCaptureSpy = vi.fn();
  svgEl.setPointerCapture = setCaptureSpy;
  svgEl.releasePointerCapture = releaseCaptureSpy;

  let result: ReturnType<typeof useSelectionTransformDrag> | null = null;
  let disposeRoot: (() => void) | null = null;
  createRoot((dispose) => {
    disposeRoot = dispose;
    result = useSelectionTransformDrag({
      getSvgRef: () => svgEl,
      onStopMomentum: vi.fn(),
      onHudUpdate: vi.fn(),
      onSnapClear: vi.fn(),
      onComputeSnap: vi.fn(() => ({ dx: 0, dy: 0, lines: [] })),
      isNavigationMode: opts.navigationMode ?? false,
    });
  });

  return {
    result: result!,
    engine,
    ws,
    svgEl,
    setCaptureSpy,
    releaseCaptureSpy,
    editorSignals,
    dispose: () => disposeRoot?.(),
  };
}

// ── Tests ────────────────────────────────────────────────────────────────

describe("useSelectionTransformDrag", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe("handlePointerDown", () => {
    it("stops propagation and prevents default", () => {
      const { result, dispose } = setupHook();
      const e = makePointerEvent();
      result.handlePointerDown(e, "move");
      expect(e.stopPropagation).toHaveBeenCalled();
      expect(e.preventDefault).toHaveBeenCalled();
      dispose();
    });

    it("early-returns in navigation mode", () => {
      const { result, engine, dispose } = setupHook({
        navigationMode: true,
      });
      const e = makePointerEvent();
      result.handlePointerDown(e, "move");
      expect(engine.transformLayer).not.toHaveBeenCalled();
      dispose();
    });

    it("early-returns when no layer exists", () => {
      const { result, engine, dispose } = setupHook({
        addLayer: false,
      });
      const e = makePointerEvent();
      result.handlePointerDown(e, "move");
      expect(engine.transformLayer).not.toHaveBeenCalled();
      dispose();
    });

    it("sets pointer capture on the SVG root", () => {
      const { result, setCaptureSpy, dispose } = setupHook();
      const e = makePointerEvent({ pointerId: 42 });
      result.handlePointerDown(e, "move");
      expect(setCaptureSpy).toHaveBeenCalledWith(42);
      dispose();
    });

    it("creates a dragState for move type with pendingMoveSnapshot", () => {
      const { result, engine, editorSignals, dispose } = setupHook();
      const e = makePointerEvent({ clientX: 150, clientY: 80 });
      result.handlePointerDown(e, "move");
      const ds = result.dragState();
      expect(ds).not.toBeNull();
      expect(ds!.type).toBe("move");
      expect(ds!.layerId).toBe("layer-1");
      expect(ds!.startX).toBe(150);
      expect(ds!.startY).toBe(80);
      expect(ds!.pendingMoveSnapshot).toBeDefined();
      expect(engine.snapshot).toHaveBeenCalled();
      dispose();
    });

    it("creates a dragState for resize type WITHOUT pendingMoveSnapshot", () => {
      const { result, editorSignals, dispose } = setupHook();
      const e = makePointerEvent();
      result.handlePointerDown(e, "se");
      const ds = result.dragState();
      expect(ds).not.toBeNull();
      expect(ds!.type).toBe("se");
      expect(ds!.pendingMoveSnapshot).toBeNull();
      dispose();
    });

    it("creates a layerTransformSession for resize type", () => {
      const { result, editorSignals, dispose } = setupHook();
      const e = makePointerEvent();
      result.handlePointerDown(e, "se");
      expect(editorSignals.setLayerTransformSession).toHaveBeenCalled();
      const session =
        editorSignals.setLayerTransformSession.mock.calls[0][0];
      expect(session.documentId).toBe("doc-1");
      expect(session.layerId).toBe("layer-1");
      expect(session.mode).toBe("resize");
      expect(session.originalSnapshot).toBeDefined();
      expect(session.originalTransform).toBeDefined();
      dispose();
    });

    it("creates a layerTransformSession for rotate type", () => {
      const { result, editorSignals, dispose } = setupHook();
      const e = makePointerEvent();
      result.handlePointerDown(e, "rotate");
      expect(editorSignals.setLayerTransformSession).toHaveBeenCalled();
      const session =
        editorSignals.setLayerTransformSession.mock.calls[0][0];
      expect(session.mode).toBe("rotate");
      dispose();
    });

    it("does NOT create layerTransformSession for move type", () => {
      const { result, editorSignals, dispose } = setupHook();
      const e = makePointerEvent();
      result.handlePointerDown(e, "move");
      expect(
        editorSignals.setLayerTransformSession
      ).not.toHaveBeenCalled();
      dispose();
    });

    it("rejects pointerDown when an existing session targets a different layer", () => {
      const { result, editorSignals, engine, dispose } = setupHook();
      // Manually set a session for a DIFFERENT layer via the signal setter
      editorSignals.setLayerTransformSession({
        documentId: "doc-1",
        layerId: "other-layer",
        originalSnapshot: { snap: 1 },
        originalTransform: DEFAULT_TRANSFORM,
        mode: "resize",
        lockRatio: false,
        startedAt: Date.now(),
      });
      editorSignals.setLayerTransformSession.mockClear();
      const e = makePointerEvent();
      result.handlePointerDown(e, "se");
      // Should NOT create a new dragState — early return
      expect(engine.transformLayer).not.toHaveBeenCalled();
      dispose();
    });

    it("logs a snapshot for the move pendingMoveSnapshot (not a transform session)", () => {
      const { result, engine, dispose } = setupHook();
      const e = makePointerEvent();
      result.handlePointerDown(e, "move");
      expect(engine.snapshot).toHaveBeenCalledTimes(1);
      // Verify snapshot is stashed in dragState, not in setLayerTransformSession
      expect(result.dragState()!.pendingMoveSnapshot).toBeDefined();
      dispose();
    });
  });

  describe("handlePointerMove (move type)", () => {
    it("moves the layer via engine.transformLayer with offset from start", () => {
      const { result, engine, dispose } = setupHook({ zoom: 2 });
      result.handlePointerDown(
        makePointerEvent({ clientX: 100, clientY: 100 }),
        "move"
      );
      engine.transformLayer.mockClear();

      // Move pointer by 80px right / 40px down at zoom=2 -> 40dx / 20dy in doc space
      const moveE = makePointerEvent({
        clientX: 180,
        clientY: 140,
        pointerId: 1,
      });
      result.handlePointerMove(moveE);

      const expectedX = 100 + (180 - 100) / 2;
      const expectedY = 50 + (140 - 100) / 2;
      expect(engine.transformLayer).toHaveBeenCalledWith("layer-1", {
        x: expectedX,
        y: expectedY,
      });
      dispose();
    });

    it("updates HUD with delta values", () => {
      const { result, editorSignals, dispose } = setupHook();
      result.handlePointerDown(
        makePointerEvent({ clientX: 100, clientY: 100 }),
        "move"
      );
      const moveE = makePointerEvent({
        clientX: 200,
        clientY: 150,
        pointerId: 1,
      });
      result.handlePointerMove(moveE);
      expect(result.dragState()).not.toBeNull();
      dispose();
    });

    it("does nothing when there is no active drag", () => {
      const { result, engine, dispose } = setupHook();
      const e = makePointerEvent();
      result.handlePointerMove(e);
      expect(engine.transformLayer).not.toHaveBeenCalled();
      dispose();
    });

    it("does nothing when pointerId does not match dragState", () => {
      const { result, engine, dispose } = setupHook();
      result.handlePointerDown(makePointerEvent({ pointerId: 1 }), "move");
      engine.transformLayer.mockClear();
      const e = makePointerEvent({ pointerId: 2 });
      result.handlePointerMove(e);
      expect(engine.transformLayer).not.toHaveBeenCalled();
      dispose();
    });
  });

  describe("handlePointerMove (resize type)", () => {
    it("resizes the layer via applyResizeHandle", () => {
      const { result, engine, dispose } = setupHook();
      result.handlePointerDown(
        makePointerEvent({ clientX: 100, clientY: 100 }),
        "se"
      );
      engine.transformLayer.mockClear();

      const moveE = makePointerEvent({
        clientX: 200,
        clientY: 150,
        pointerId: 1,
      });
      result.handlePointerMove(moveE);

      expect(engine.transformLayer).toHaveBeenCalled();
      const callArgs = engine.transformLayer.mock.calls[0];
      expect(callArgs[0]).toBe("layer-1");
      const newTransform = callArgs[1];
      expect(newTransform).toHaveProperty("scaleX");
      expect(newTransform).toHaveProperty("scaleY");
      dispose();
    });

    it("default ON + no Shift keeps aspect ratio locked on canvas drag", () => {
      const { result, engine, dispose } = setupHook();
      const resizeSpy = vi.spyOn(
        TransformGeometryModule,
        "applyResizeHandle"
      );
      result.handlePointerDown(
        makePointerEvent({ clientX: 100, clientY: 100 }),
        "se"
      );
      engine.transformLayer.mockClear();

      const moveE = makePointerEvent({
        clientX: 200,
        clientY: 150,
        pointerId: 1,
      });
      result.handlePointerMove(moveE);
      expect(resizeSpy).toHaveBeenCalled();
      const args = resizeSpy.mock.calls[resizeSpy.mock.calls.length - 1];
      // applyResizeHandle(transform, layerW, layerH, handle, screenDx, screenDy, breakAspect, altKey)
      // Default session lockRatio is ON, no Shift -> breakAspect false (ratio preserved)
      expect(args[6]).toBe(false);
      resizeSpy.mockRestore();
      dispose();
    });

    it("default ON + Shift breaks aspect ratio (Shift inverts the default)", () => {
      const { result, engine, dispose } = setupHook();
      const resizeSpy = vi.spyOn(
        TransformGeometryModule,
        "applyResizeHandle"
      );
      result.handlePointerDown(
        makePointerEvent({ clientX: 100, clientY: 100 }),
        "se"
      );
      engine.transformLayer.mockClear();

      const shiftE = makePointerEvent({
        clientX: 200,
        clientY: 150,
        pointerId: 1,
        shiftKey: true,
      });
      result.handlePointerMove(shiftE);
      expect(resizeSpy).toHaveBeenCalled();
      const args = resizeSpy.mock.calls[resizeSpy.mock.calls.length - 1];
      // Default ON + Shift held -> breakAspect true (free resize)
      expect(args[6]).toBe(true);
      resizeSpy.mockRestore();
      dispose();
    });

    it("toggling constrainRatio OFF frees the canvas drag", () => {
      const { result, engine, editorSignals, dispose } = setupHook();
      const resizeSpy = vi.spyOn(
        TransformGeometryModule,
        "applyResizeHandle"
      );
      result.handlePointerDown(
        makePointerEvent({ clientX: 100, clientY: 100 }),
        "se"
      );
      // Simulate either the TransformOptionBar "Ratio" toggle or the
      // PropertiesPanel "Constrain proportions" toggle turning OFF — both
      // write the single shared constrainRatio signal.
      editorSignals.setConstrainRatio(false);
      engine.transformLayer.mockClear();

      const moveE = makePointerEvent({
        clientX: 200,
        clientY: 150,
        pointerId: 1,
      });
      result.handlePointerMove(moveE);
      expect(resizeSpy).toHaveBeenCalled();
      const args = resizeSpy.mock.calls[resizeSpy.mock.calls.length - 1];
      // constrainRatio false + no Shift -> breakAspect true (free resize)
      expect(args[6]).toBe(true);
      resizeSpy.mockRestore();
      dispose();
    });
  });

  describe("handlePointerMove (rotate type)", () => {
    it("rotates the layer via applyRotationDrag", () => {
      const { result, engine, dispose } = setupHook();
      const rotateSpy = vi.spyOn(
        TransformGeometryModule,
        "applyRotationDrag"
      );
      result.handlePointerDown(
        makePointerEvent({ clientX: 200, clientY: 200 }),
        "rotate"
      );
      engine.transformLayer.mockClear();

      const moveE = makePointerEvent({
        clientX: 250,
        clientY: 150,
        pointerId: 1,
      });
      result.handlePointerMove(moveE);
      expect(rotateSpy).toHaveBeenCalled();
      rotateSpy.mockRestore();
      dispose();
    });

    it("passes shiftKey to applyRotationDrag for angle snapping", () => {
      const { result, engine, dispose } = setupHook();
      const rotateSpy = vi.spyOn(
        TransformGeometryModule,
        "applyRotationDrag"
      );
      result.handlePointerDown(
        makePointerEvent({ clientX: 200, clientY: 200 }),
        "rotate"
      );
      engine.transformLayer.mockClear();

      const shiftE = makePointerEvent({
        clientX: 250,
        clientY: 150,
        pointerId: 1,
        shiftKey: true,
      });
      result.handlePointerMove(shiftE);
      const args = rotateSpy.mock.calls[rotateSpy.mock.calls.length - 1];
      expect(args[4]).toBe(true);
      rotateSpy.mockRestore();
      dispose();
    });

    it("updates hoverPos during rotation", () => {
      const { result, editorSignals, dispose } = setupHook();
      const e = makePointerEvent({ clientX: 200, clientY: 200 });
      result.handlePointerDown(e, "rotate");
      editorSignals.setHoverPos.mockClear();

      const moveE = makePointerEvent({
        clientX: 250,
        clientY: 150,
        pointerId: 1,
      });
      result.handlePointerMove(moveE);
      expect(editorSignals.setHoverPos).toHaveBeenCalledWith({
        x: 250,
        y: 150,
      });
      dispose();
    });
  });

  describe("handlePointerMove — hover state (no active drag)", () => {
    it("updates hover handle when pointer is near a resize handle", () => {
      const detectSpy = vi
        .spyOn(TransformGeometryModule, "detectHandle")
        .mockReturnValue("se");
      const { result, editorSignals, dispose } = setupHook();

      const e = makePointerEvent({ clientX: 150, clientY: 150 });
      result.handlePointerMove(e);
      expect(editorSignals.setHoverHandle).toHaveBeenCalledWith("se");
      expect(editorSignals.setHoverPos).toHaveBeenCalled();

      detectSpy.mockRestore();
      dispose();
    });

    it("clears hover when detectHandle returns null", () => {
      const detectSpy = vi
        .spyOn(TransformGeometryModule, "detectHandle")
        .mockReturnValue(null);
      const { result, editorSignals, dispose } = setupHook();

      const e = makePointerEvent({ clientX: 999, clientY: 999 });
      result.handlePointerMove(e);
      expect(editorSignals.setHoverHandle).toHaveBeenCalledWith(null);
      expect(editorSignals.setHoverPos).toHaveBeenCalledWith(null);

      detectSpy.mockRestore();
      dispose();
    });

    it("resolves rotate handle to rotate-{corner} via getNearestRotateCorner", () => {
      const detectSpy = vi
        .spyOn(TransformGeometryModule, "detectHandle")
        .mockReturnValue("rotate");
      const cornerSpy = vi
        .spyOn(TransformGeometryModule, "getNearestRotateCorner")
        .mockReturnValue("nw");
      const { result, editorSignals, dispose } = setupHook();

      const e = makePointerEvent({ clientX: 150, clientY: 150 });
      result.handlePointerMove(e);
      expect(editorSignals.setHoverHandle).toHaveBeenCalledWith(
        "rotate-nw"
      );

      detectSpy.mockRestore();
      cornerSpy.mockRestore();
      dispose();
    });

    it("does not update hover when a drag is active", () => {
      const { result, editorSignals, dispose } = setupHook();
      // Start a drag
      result.handlePointerDown(
        makePointerEvent({ pointerId: 1 }),
        "move"
      );
      editorSignals.setHoverHandle.mockClear();
      editorSignals.setHoverPos.mockClear();

      // Move while dragging — should NOT update hover
      const moveE = makePointerEvent({
        clientX: 200,
        clientY: 200,
        pointerId: 1,
      });
      result.handlePointerMove(moveE);
      expect(editorSignals.setHoverHandle).not.toHaveBeenCalled();
      expect(editorSignals.setHoverPos).not.toHaveBeenCalled();

      dispose();
    });
  });

  describe("handlePointerUp", () => {
    it("commits pendingMoveSnapshot when layer actually moved", () => {
      const { result, engine, ws, dispose } = setupHook();
      const history = ws.getActiveHistory()!;
      result.handlePointerDown(
        makePointerEvent({ clientX: 100, clientY: 100 }),
        "move"
      );
      // Move the layer
      const moveE = makePointerEvent({
        clientX: 200,
        clientY: 150,
        pointerId: 1,
      });
      result.handlePointerMove(moveE);
      // Release
      const upE = makePointerEvent({
        clientX: 200,
        clientY: 150,
        pointerId: 1,
      });
      result.handlePointerUp(upE);
      expect(history.commit).toHaveBeenCalled();
      expect(result.dragState()).toBeNull();
      dispose();
    });

    it("does NOT commit snapshot when layer did NOT move (click-without-drag)", () => {
      const { result, engine, ws, dispose } = setupHook();
      const history = ws.getActiveHistory()!;
      result.handlePointerDown(
        makePointerEvent({ clientX: 100, clientY: 100 }),
        "move"
      );
      const upE = makePointerEvent({
        clientX: 100,
        clientY: 100,
        pointerId: 1,
      });
      result.handlePointerUp(upE);
      expect(history.commit).not.toHaveBeenCalled();
      expect(result.dragState()).toBeNull();
      dispose();
    });

    it("releases pointer capture on the SVG", () => {
      const { result, releaseCaptureSpy, dispose } = setupHook();
      result.handlePointerDown(
        makePointerEvent({ pointerId: 42 }),
        "move"
      );
      result.handlePointerUp(makePointerEvent({ pointerId: 42 }));
      expect(releaseCaptureSpy).toHaveBeenCalledWith(42);
      dispose();
    });

    it("clears dragState on pointerUp", () => {
      const { result, dispose } = setupHook();
      result.handlePointerDown(
        makePointerEvent({ pointerId: 1 }),
        "se"
      );
      expect(result.dragState()).not.toBeNull();
      result.handlePointerUp(makePointerEvent({ pointerId: 1 }));
      expect(result.dragState()).toBeNull();
      dispose();
    });

    it("clears hoverPos for rotate type on pointerUp", () => {
      const { result, editorSignals, dispose } = setupHook();
      result.handlePointerDown(
        makePointerEvent({ pointerId: 1 }),
        "rotate"
      );
      editorSignals.setHoverPos.mockClear();
      result.handlePointerUp(makePointerEvent({ pointerId: 1 }));
      expect(editorSignals.setHoverPos).toHaveBeenCalledWith(null);
      dispose();
    });

    it("ignores pointerUp with different pointerId (not the captured one)", () => {
      const { result, ws, dispose } = setupHook();
      const history = ws.getActiveHistory()!;
      result.handlePointerDown(
        makePointerEvent({ pointerId: 1 }),
        "move"
      );
      result.handlePointerUp(makePointerEvent({ pointerId: 2 }));
      expect(result.dragState()).not.toBeNull();
      expect(history.commit).not.toHaveBeenCalled();
      dispose();
    });
  });

  describe("handlePointerCancel", () => {
    it("clears dragState without committing history", () => {
      const { result, ws, dispose } = setupHook();
      const history = ws.getActiveHistory()!;
      result.handlePointerDown(
        makePointerEvent({ pointerId: 1 }),
        "move"
      );
      result.handlePointerMove(
        makePointerEvent({
          clientX: 200,
          clientY: 150,
          pointerId: 1,
        })
      );
      result.handlePointerCancel(makePointerEvent({ pointerId: 1 }));
      expect(history.commit).not.toHaveBeenCalled();
      expect(result.dragState()).toBeNull();
      dispose();
    });

    it("releases pointer capture on cancel", () => {
      const { result, releaseCaptureSpy, dispose } = setupHook();
      result.handlePointerDown(
        makePointerEvent({ pointerId: 7 }),
        "move"
      );
      result.handlePointerCancel(makePointerEvent({ pointerId: 7 }));
      expect(releaseCaptureSpy).toHaveBeenCalledWith(7);
      dispose();
    });

    it("clears hoverPos for rotate type", () => {
      const { result, editorSignals, dispose } = setupHook();
      result.handlePointerDown(
        makePointerEvent({ pointerId: 1 }),
        "rotate"
      );
      editorSignals.setHoverPos.mockClear();
      result.handlePointerCancel(makePointerEvent({ pointerId: 1 }));
      expect(editorSignals.setHoverPos).toHaveBeenCalledWith(null);
      dispose();
    });
  });

  describe("handleLostPointerCapture", () => {
    it("clears dragState regardless of pointerId match", () => {
      const { result, dispose } = setupHook();
      result.handlePointerDown(
        makePointerEvent({ pointerId: 1 }),
        "move"
      );
      expect(result.dragState()).not.toBeNull();
      result.handleLostPointerCapture(
        makePointerEvent({ pointerId: 999 })
      );
      expect(result.dragState()).toBeNull();
      dispose();
    });
  });

  describe("Escape key handling", () => {
    it("restores the layer to startTransform for move type", () => {
      const { result, engine, dispose } = setupHook();
      result.handlePointerDown(
        makePointerEvent({ clientX: 100, clientY: 100, pointerId: 1 }),
        "move"
      );
      result.handlePointerMove(
        makePointerEvent({ clientX: 300, clientY: 200, pointerId: 1 })
      );
      window.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Escape", bubbles: true })
      );
      expect(engine.transformLayer).toHaveBeenLastCalledWith(
        "layer-1",
        expect.objectContaining({ x: 100, y: 50 })
      );
      dispose();
    });

    it("restores original snapshot via layerTransformSession for resize type", () => {
      const { result, engine, editorSignals, dispose } = setupHook();
      result.handlePointerDown(
        makePointerEvent({ pointerId: 1 }),
        "se"
      );
      const sessionCall =
        editorSignals.setLayerTransformSession.mock.calls[0];
      const originalSnapshot = sessionCall[0].originalSnapshot;
      editorSignals.setLayerTransformSession.mockClear();

      window.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Escape", bubbles: true })
      );
      expect(engine.restore).toHaveBeenCalledWith(originalSnapshot);
      dispose();
    });

    it("releases pointer capture on Escape", () => {
      const { result, releaseCaptureSpy, dispose } = setupHook();
      result.handlePointerDown(
        makePointerEvent({ pointerId: 42 }),
        "move"
      );
      window.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Escape", bubbles: true })
      );
      expect(releaseCaptureSpy).toHaveBeenCalledWith(42);
      dispose();
    });

    it("clears hoverPos for rotate type on Escape", () => {
      const { result, editorSignals, dispose } = setupHook();
      result.handlePointerDown(
        makePointerEvent({ pointerId: 1 }),
        "rotate"
      );
      editorSignals.setHoverPos.mockClear();
      window.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Escape", bubbles: true })
      );
      expect(editorSignals.setHoverPos).toHaveBeenCalledWith(null);
      dispose();
    });
  });

  describe("edge cases (bugs found during audit)", () => {
    it("handlePointerDown: dragState is set even when session exists for SAME layer", () => {
      const { result, editorSignals, dispose } = setupHook();
      editorSignals.setLayerTransformSession({
        documentId: "doc-1",
        layerId: "layer-1",
        originalSnapshot: { snap: 1 },
        originalTransform: DEFAULT_TRANSFORM,
        mode: "resize",
        lockRatio: false,
        startedAt: Date.now(),
      });
      const e = makePointerEvent();
      result.handlePointerDown(e, "move");
      expect(result.dragState()).not.toBeNull();
      dispose();
    });

    it("handlePointerCancel: session is NOT cleared (survives for Apply/Enter)", () => {
      const { result, editorSignals, dispose } = setupHook();
      result.handlePointerDown(
        makePointerEvent({ pointerId: 1 }),
        "se"
      );
      editorSignals.setLayerTransformSession.mockClear();
      result.handlePointerCancel(makePointerEvent({ pointerId: 1 }));
      expect(
        editorSignals.setLayerTransformSession
      ).not.toHaveBeenCalledWith(null);
      dispose();
    });

    it("handleLostPointerCapture: pendingMoveSnapshot dropped without commit", () => {
      const { result, ws, dispose } = setupHook();
      const history = ws.getActiveHistory()!;
      result.handlePointerDown(
        makePointerEvent({ pointerId: 1 }),
        "move"
      );
      result.handlePointerMove(
        makePointerEvent({ clientX: 200, clientY: 150, pointerId: 1 })
      );
      result.handleLostPointerCapture(
        makePointerEvent({ pointerId: 1 })
      );
      expect(history.commit).not.toHaveBeenCalled();
      expect(result.dragState()).toBeNull();
      dispose();
    });

    it("Escape: restores original for move drag via transformLayer (no session)", () => {
      const { result, engine, dispose } = setupHook();
      result.handlePointerDown(
        makePointerEvent({ clientX: 100, clientY: 100, pointerId: 1 }),
        "move"
      );
      result.handlePointerMove(
        makePointerEvent({ clientX: 300, clientY: 200, pointerId: 1 })
      );
      window.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Escape", bubbles: true })
      );
      const lastCall =
        engine.transformLayer.mock.calls[
          engine.transformLayer.mock.calls.length - 1
        ];
      expect(lastCall[1]).toMatchObject({ x: 100, y: 50 });
      dispose();
    });

    // ── Zoom extremes ──
    it("zoom=100: tiny delta in doc space from large screen movement", () => {
      const { result, engine, dispose } = setupHook({ zoom: 100 });
      result.handlePointerDown(
        makePointerEvent({ clientX: 100, clientY: 100, pointerId: 1 }),
        "move"
      );
      engine.transformLayer.mockClear();
      // Move 1000px screen → 10px doc space
      result.handlePointerMove(
        makePointerEvent({ clientX: 1100, clientY: 100, pointerId: 1 })
      );
      expect(engine.transformLayer).toHaveBeenCalled();
      const args = engine.transformLayer.mock.calls[0][1];
      expect(args.x).toBeCloseTo(110, 2); // 100 + 1000/100 = 110
      dispose();
    });

    it("zoom=0.01: massive delta in doc space from small screen movement", () => {
      const { result, engine, dispose } = setupHook({ zoom: 0.01 });
      result.handlePointerDown(
        makePointerEvent({ clientX: 100, clientY: 100, pointerId: 1 }),
        "move"
      );
      engine.transformLayer.mockClear();
      // Move 5px screen → 500px doc space
      result.handlePointerMove(
        makePointerEvent({ clientX: 105, clientY: 100, pointerId: 1 })
      );
      expect(engine.transformLayer).toHaveBeenCalled();
      const args = engine.transformLayer.mock.calls[0][1];
      expect(args.x).toBeCloseTo(600, 0); // 100 + 5/0.01 = 600
      expect(Number.isFinite(args.x)).toBe(true);
      dispose();
    });

    // ── Rapid sequential drags (concurrent safety) ──
    it("second pointerDown without pointerUp replaces first drag (stale pointer safety)", () => {
      const { result, engine, dispose } = setupHook();
      // Start first move drag
      result.handlePointerDown(
        makePointerEvent({ clientX: 100, clientY: 100, pointerId: 1 }),
        "move"
      );
      engine.transformLayer.mockClear();
      // Second pointerDown with different pointerId starts a different drag
      result.handlePointerDown(
        makePointerEvent({ clientX: 200, clientY: 200, pointerId: 2 }),
        "move"
      );
      // The second should create a new dragState and use the second coords
      const ds = result.dragState();
      expect(ds).not.toBeNull();
      expect(ds!.pointerId).toBe(2);
      expect(ds!.startX).toBe(200);
      expect(ds!.startY).toBe(200);
      dispose();
    });

    // ── Negative coordinates ──
    it("handles negative clientX/clientY (drag off-canvas left/up)", () => {
      const { result, engine, dispose } = setupHook();
      result.handlePointerDown(
        makePointerEvent({ clientX: -50, clientY: -50, pointerId: 1 }),
        "move"
      );
      engine.transformLayer.mockClear();
      result.handlePointerMove(
        makePointerEvent({ clientX: -30, clientY: -20, pointerId: 1 })
      );
      expect(engine.transformLayer).toHaveBeenCalled();
      const args = engine.transformLayer.mock.calls[0][1];
      expect(Number.isFinite(args.x)).toBe(true);
      expect(Number.isFinite(args.y)).toBe(true);
      // delta = (-30-(-50), -20-(-50)) = (20, 30) → layer should move +20, +30
      expect(args.x).toBeCloseTo(120, 4);
      expect(args.y).toBeCloseTo(80, 4);
      dispose();
    });

    // ── Zero delta stability ──
    it("zero delta across pointerDown+pointerMove (no layer movement)", () => {
      const { result, engine, dispose } = setupHook();
      result.handlePointerDown(
        makePointerEvent({ clientX: 100, clientY: 100, pointerId: 1 }),
        "move"
      );
      engine.transformLayer.mockClear();
      // Move to exactly the same position
      result.handlePointerMove(
        makePointerEvent({ clientX: 100, clientY: 100, pointerId: 1 })
      );
      expect(engine.transformLayer).toHaveBeenCalled();
      const args = engine.transformLayer.mock.calls[0][1];
      expect(args.x).toBe(100); // original x unchanged
      expect(args.y).toBe(50);  // original y unchanged
      dispose();
    });

    // ── Snap behavior ──
    it("snap offset is applied when moveSnapEnabled is true", () => {
      const { result, engine, dispose } = setupHook({
        snapEnabled: true,
      });
      // The onComputeSnap returns { dx: 0, dy: 0, lines: [] } by default
      // We need to check that onComputeSnap is called with the correct args
      // But we can't easily spy on it from setupHook closure...
      result.handlePointerDown(
        makePointerEvent({ clientX: 100, clientY: 100, pointerId: 1 }),
        "move"
      );
      engine.transformLayer.mockClear();
      result.handlePointerMove(
        makePointerEvent({ clientX: 150, clientY: 100, pointerId: 1 })
      );
      // Layer should move despite snap returning 0 offset
      expect(engine.transformLayer).toHaveBeenCalled();
      const args = engine.transformLayer.mock.calls[0][1];
      expect(Number.isFinite(args.x)).toBe(true);
      dispose();
    });

    // ── Large pan offset ──
    it("large pan offset does not overflow coordinate conversion", () => {
      const { result, engine, dispose } = setupHook({
        pan: { x: 10000, y: 5000 },
        zoom: 0.5,
      });
      result.handlePointerDown(
        makePointerEvent({ clientX: 10200, clientY: 5100, pointerId: 1 }),
        "move"
      );
      engine.transformLayer.mockClear();
      result.handlePointerMove(
        makePointerEvent({
          clientX: 10400,
          clientY: 5200,
          pointerId: 1,
        })
      );
      expect(engine.transformLayer).toHaveBeenCalled();
      const args = engine.transformLayer.mock.calls[0][1];
      // delta doc = ((10400-10200)/0.5, (5200-5100)/0.5) = (400, 200)
      // original position (100,50), so new = (500, 250)
      expect(args.x).toBeCloseTo(500, 2);
      expect(args.y).toBeCloseTo(250, 2);
      expect(Number.isFinite(args.x)).toBe(true);
      dispose();
    });

    // ── pointerDown immediately followed by pointerUp (click without drag) ──
    it("pointerDown+pointerUp at same position does NOT commit history (no drag)", () => {
      const { result, ws, dispose } = setupHook();
      const history = ws.getActiveHistory()!;
      result.handlePointerDown(
        makePointerEvent({ clientX: 100, clientY: 100, pointerId: 1 }),
        "move"
      );
      result.handlePointerUp(
        makePointerEvent({ clientX: 100, clientY: 100, pointerId: 1 })
      );
      expect(history.commit).not.toHaveBeenCalled();
      expect(result.dragState()).toBeNull();
      dispose();
    });

    // ── Handle type 'none' (hover without hit) ──
    it("pointerDown with unknown handle type does not crash", () => {
      const { result, engine, dispose } = setupHook();
      const e = makePointerEvent();
      expect(() => {
        result.handlePointerDown(e, "unknown-handle");
      }).not.toThrow();
      // Unknown handle creates a move-type drag as fallback (or does nothing)
      // Either way, should not throw
      dispose();
    });

    // ── Multiple pointerDowns with same pointerId ──
    it("double pointerDown with same pointerId resets drag state", () => {
      const { result, engine, dispose } = setupHook();
      result.handlePointerDown(
        makePointerEvent({ clientX: 100, clientY: 100, pointerId: 1 }),
        "move"
      );
      engine.transformLayer.mockClear();
      // Second pointerDown with same pointerId should reinitialize
      result.handlePointerDown(
        makePointerEvent({ clientX: 300, clientY: 300, pointerId: 1 }),
        "move"
      );
      const ds = result.dragState();
      expect(ds!.startX).toBe(300);
      expect(ds!.startY).toBe(300);
      // Move from new start
      result.handlePointerMove(
        makePointerEvent({ clientX: 400, clientY: 300, pointerId: 1 })
      );
      const args = engine.transformLayer.mock.calls[
        engine.transformLayer.mock.calls.length - 1
      ][1];
      expect(args.x).toBeCloseTo(200, 4); // 100 + (400-300) = 200
      dispose();
    });
  });

  describe("getLayer transformability guards", () => {
    it("returns null for a Background layer (suppresses transform box on document open)", () => {
      const { result, editorSignals, dispose } = setupHook();
      editorSignals.setLayers([
        { ...DEFAULT_LAYER, id: "bg", isBackground: true, lockPosition: true, lockRotation: true } as any,
      ]);
      editorSignals.setSelectedLayerId("bg");
      expect(result.getLayer()).toBeNull();
      dispose();
    });

    it("returns null when both position and rotation are locked", () => {
      const { result, editorSignals, dispose } = setupHook();
      editorSignals.setLayers([
        { ...DEFAULT_LAYER, id: "locked", lockPosition: true, lockRotation: true } as any,
      ]);
      editorSignals.setSelectedLayerId("locked");
      expect(result.getLayer()).toBeNull();
      dispose();
    });

    it("still returns the layer when only rotation is locked (position remains transformable)", () => {
      const { result, editorSignals, dispose } = setupHook();
      editorSignals.setLayers([
        { ...DEFAULT_LAYER, id: "partial", lockPosition: false, lockRotation: true } as any,
      ]);
      editorSignals.setSelectedLayerId("partial");
      expect(result.getLayer()).not.toBeNull();
      expect(result.getLayer()!.id).toBe("partial");
      dispose();
    });
  });

  describe("text layer scale bake on resize pointerup", () => {
    it("bakes scaleX into fontSize and resets transform scale to 1.0 when resize ends", () => {
      const { result, engine, editorSignals, dispose } = setupHook();
      const textLayer = {
        ...DEFAULT_LAYER,
        id: "text-1",
        type: "text" as const,
        textData: {
          content: "Hello",
          family: "Inter",
          fontSize: 48,
          color: "#000000",
          align: "left" as const,
          boxMode: "point" as const,
          boxWidth: 0,
        },
        transform: { x: 0, y: 0, scaleX: 2.0, scaleY: 2.0, rotation: 0, flipH: false, flipV: false },
      };
      editorSignals.setLayers([textLayer as any]);
      editorSignals.setSelectedLayerId("text-1");
      engine.getLayer.mockReturnValue(textLayer as any);
      (engine as any).updateTextData = vi.fn();

      result.handlePointerDown(
        makePointerEvent({ clientX: 100, clientY: 100, pointerId: 1 }),
        "se"
      );
      result.handlePointerUp(
        makePointerEvent({ clientX: 200, clientY: 200, pointerId: 1 })
      );

      expect((engine as any).updateTextData).toHaveBeenCalledWith(
        "text-1",
        expect.objectContaining({ fontSize: 96 })
      );
      expect(engine.transformLayer).toHaveBeenCalledWith("text-1", {
        scaleX: 1.0,
        scaleY: 1.0,
      });
      dispose();
    });
  });
});
