// SPDX-License-Identifier: AGPL-3.0-or-later
import { createSignal, createEffect, onCleanup } from "solid-js";
import { useEditor } from "../shell/EditorContext";
import { hitTestLayers, type LayerInfo } from "@/viewport/layerHitTest";
import type { DocumentEngine } from "@/engine/document";
import type { CommandHistory } from "@/engine/history";
import {
  handlePointerDown,
  handlePointerMove,
  handlePointerUp,
  type ToolType,
  type ToolContext,
} from "@/viewport/input-handler";
import { getPaintToolBlockReason, type PaintToolSettings } from "../brushToolState";
import { PaintSmoother, smoothingToWindowSize } from "../paintSmoothing";
import { tryReleasePointerCapture, trySetPointerCapture } from "../tools/pointerCapture";
import { showToast } from "../Toast";
import { useDialog } from "../dialogs/DialogProvider";
import type { HudMode } from "../TransformHud";
import { rgbToHex, interpolateLinePoints } from "./pointerUtils";
import { computeEdgeScroll } from "./edgeScroll";
import { startSelectionRotation as startSelectionRotationFn } from "./selectionRotation";
import { prepareToolContext as prepareToolContextImpl } from "./pointerTools/prepareToolContext";
import { applyPaintBucketFill } from "./pointerTools/paintBucket";
import { startGradientDrag, trackGradientDrag, applyGradientFill } from "./pointerTools/gradientTool";
import { startShapeDrag, trackShapeDrag, applyShapeDrag } from "./pointerTools/shapeTool";
import { startTextPointer, trackTextPointer, applyTextPointer, cancelTextSession } from "./pointerTools/textTool";
import { startCropDrag, trackModernCropDrag, handleCropPointerUp } from "./pointerTools/modernCrop";
import type { PointerToolContext, ModernDragState, GradientDragState, ShapeDragState } from "./pointerTools/pointerToolContext";
import type { TextPointerState } from "./pointerTools/textTool";

const NOOP = () => {};

interface UseCanvasPointerToolsParams {
  getCanvasContainerRef: () => HTMLDivElement | undefined;
  getCanvasRef: () => HTMLCanvasElement | undefined;
  isSpacePressed: () => boolean;
  isPanning: () => boolean;
  isAltPressed: () => boolean;
  stopMomentum: () => void;
  fitToScreenAndRender: () => void;
  commitBrushStroke: (engine: DocumentEngine, history: CommandHistory, id: string, isEraser: boolean, anchor?: { x: number; y: number } | null) => void;
  onPaintStroke?: (
    points: { x: number; y: number }[],
    isEraser: boolean,
    settings: PaintToolSettings,
    isFinal?: boolean,
  ) => void;
  cropSnapTargets?: () => import("@/viewport/cropSnap").CropSnapTargets | undefined;
  moveSnapEnabled?: () => boolean;
}

type HudData = {
  mode: HudMode;
  clientX: number;
  clientY: number;
  deltaX: number;
  deltaY: number;
  width: number;
  height: number;
  scalePercent: number;
  angle: number;
  snapActive: boolean;
};

export function useCanvasPointerTools(params: UseCanvasPointerToolsParams) {
  const editor = useEditor();
  const dialogs = useDialog();
  const {
    workspace,
    renderer,
    scheduler,
    activeTool,
    setFgColor,
    setBgColor,
    colorPickerOpen,
    colorPickerTarget,
    zoom,
    pan,
    camera,
    setPan,
    selectedLayerId,
    setSelectedLayerId,
    moveAutoSelect,
    brushSize,
    setBrushSize,
    brushHardness,
    setBrushHardness,
    eraserSize,
    setEraserSize,
    eraserHardness,
    setEraserHardness,
    cropInteractionMode,
    selectionShape,
  } = editor;

  // ── Modern crop drag state (shared with pointerTools/modernCrop.ts) ──
  const modernDragState: ModernDragState = {
    start: null,
    exceededThreshold: false,
    end: null,
    snappedPreview: null,
    isPendingCropClick: false,
    reset: () => {
      modernDragState.start = null;
      modernDragState.exceededThreshold = false;
      modernDragState.end = null;
      modernDragState.snappedPreview = null;
    },
  };

  // ── Gradient drag state (shared with pointerTools/gradientTool.ts) ──
  const gradientDragState: GradientDragState = {
    start: null,
    end: null,
    isDragging: false,
    reset: () => {
      gradientDragState.start = null;
      gradientDragState.end = null;
      gradientDragState.isDragging = false;
    },
  };

  // ── Shape drag state (shared with pointerTools/shapeTool.ts) ──
  const shapeDragState: ShapeDragState = {
    start: null,
    tempLayerId: null,
    preSnapshot: null,
    isDragging: false,
    reset: () => {
      shapeDragState.start = null;
      shapeDragState.tempLayerId = null;
      shapeDragState.preSnapshot = null;
      shapeDragState.isDragging = false;
    },
  };

  // ── Text pointer state (session lives in editorState.textEditSession) ──
  const textPointerState: TextPointerState = {
    start: null,
    isDragging: false,
    reset: () => {
      textPointerState.start = null;
      textPointerState.isDragging = false;
    },
  };

  // ── On-canvas brush adjustment (Alt+RightButton+Drag) ──
  // Hold Alt + right mouse button and drag horizontally to adjust brush size,
  // vertically to adjust hardness. Shows a live HUD with current values.
  // Mirrors the Alt+RightClick drag gesture for quick brush tuning.
  let brushAdjustStart: {
    size: number;
    hardness: number;
    screenX: number;
    screenY: number;
  } | null = null;

  // ── Edge auto-scroll ──────────────────────────────────────────
  const EDGE_ZONE_PX = 40;
  // Speed is viewport-relative (see EDGE_SCROLL_SPEED_FACTOR in edgeScroll.ts),
  // so no absolute px/s const is needed here.

  let edgeRafId = 0;
  let edgeLastClientX = 0;
  let edgeLastClientY = 0;
  // ── Cached container rect ──────────────────────────────────────
  // Avoid getBoundingClientRect() on every pointermove. The rect is
  // lazily populated and invalidated when zoom or pan changes (via
  // createEffect below).
  let cachedContainerRect: DOMRect | null = null;

  function getCachedContainerRect(): DOMRect | null {
    if (cachedContainerRect) return cachedContainerRect;
    const container = params.getCanvasContainerRef();
    if (!container) return null;
    cachedContainerRect = container.getBoundingClientRect();
    return cachedContainerRect;
  }

  // Invalidate cached rect when camera state changes
  createEffect(() => {
    zoom();
    if (typeof pan === "function") pan();
    cachedContainerRect = null;
  });

  function applyEdgeScroll(dt: number) {
    return computeEdgeScroll(edgeLastClientX, edgeLastClientY, dt, {
      camera,
      setPan,
      scheduler,
      getContainerRect: getCachedContainerRect,
    }, EDGE_ZONE_PX);
  }

  function stopEdgeRaf() {
    if (edgeRafId) {
      cancelAnimationFrame(edgeRafId);
      edgeRafId = 0;
    }
  }

  function startEdgeRaf() {
    if (edgeRafId) return;
    let lastRafTime = performance.now();
    const tick = (time: number) => {
      const dt = lastRafTime > 0 ? (time - lastRafTime) / 1000 : 0;
      lastRafTime = time;
      if (!applyEdgeScroll(dt).scrolled) {
        edgeRafId = 0;
        return;
      }
      edgeRafId = requestAnimationFrame(tick);
    };
    edgeRafId = requestAnimationFrame(tick);
  }
  // ── end edge auto-scroll ──

  // Safety net: if the tab/window loses focus mid-drag, the RAF must not
  // keep panning on stale cursor coords. (pointerleave is unnecessary — during
  // an active drag the pointer is captured, so it won't fire.)
  const onWindowBlur = () => stopEdgeRaf();
  const onVisibilityChange = () => { if (document.hidden) stopEdgeRaf(); };
  if (typeof window !== "undefined") {
    window.addEventListener("blur", onWindowBlur);
    document.addEventListener("visibilitychange", onVisibilityChange);
  }

  onCleanup(() => {
    stopEdgeRaf();
    if (typeof window !== "undefined") {
      window.removeEventListener("blur", onWindowBlur);
      document.removeEventListener("visibilitychange", onVisibilityChange);
    }
  });

  const getLastPaintCoords = (): { x: number; y: number } | null => {
    const history = workspace.getActiveHistory();
    return history ? history.getLastPaintCoords() : null;
  };

  const setLastPaintCoords = (coords: { x: number; y: number } | null) => {
    const history = workspace.getActiveHistory();
    if (history) {
      history.setLastPaintCoords(coords);
    }
  };

  let axisLock: "horizontal" | "vertical" | null = null;

  createEffect(() => {
    const tool = activeTool();
    if (tool !== "brush" && tool !== "eraser") {
      setLastPaintCoords(null);
    }
  });

  const paintSmoother = new PaintSmoother();

  const [snapLines, setSnapLines] = createSignal<{ x1: number; y1: number; x2: number; y2: number }[]>([]);
  const [selectionBox, setSelectionBoxSignal] = createSignal<{
    x: number;
    y: number;
    w: number;
    h: number;
    angle: number;
    shape?: "rect" | "ellipse";
    inverted?: boolean;
  } | null>(null);

  const [hudInfo, setHudInfoInner] = createSignal<HudData | null>(null);

  const [cropDragPreview, setCropDragPreview] = createSignal<{
    x: number;
    y: number;
    w: number;
    h: number;
  } | null>(null);

  const interactiveState: ToolContext = {
    fgColor: "",
    bgColor: "",
    brushSize: 20,
    brushHardness: 0.8,
    brushOpacity: 1.0,
    paintSettings: { size: 20, hardness: 0.8, opacity: 1, flow: 1, smoothing: 0 },
    selectedLayerId: null,
    isAltPressed: false,
    isShiftPressed: false,
    isDragging: false,
    dragStart: { x: 0, y: 0 },
    dragCurrent: { x: 0, y: 0 },
    strokePoints: [],
    dragTool: null,
  };

  const setHudInfo = (hud: HudData | null) => {
    // HUD is rendered in a screen-space SVG overlay (inset: 0, width: 100%, height: 100%).
    // Coordinates must remain in screen/client space — do NOT convert to document space.
    // The previous conversion (screenToDocument) caused the HUD to appear far from the
    // cursor when the document is zoomed or panned.
    if (hud) {
      const rect = getCachedContainerRect();
      if (rect) {
        hud = {
          ...hud,
          clientX: hud.clientX - rect.left,
          clientY: hud.clientY - rect.top,
        };
      }
    }
    setHudInfoInner(hud);
  };

  /**
   * Convert pointer event coordinates to document space.
   *
   * Uses pan()/zoom() signals instead of engine.getViewport() because the
   * engine viewport goes stale during pan/scroll/momentum (usePanNavigation
   * intentionally skips engine.setViewport during panning to avoid triggering
   * layer re-selection). Using the always-up-to-date signals ensures all tools
   * (selection, brush, eraser, crop) get correct coordinates regardless of
   * viewport state.
   *
   * Bug 2026-07-05: tools broke after panning because engine.getViewport()
   * returned stale pan/zoom values while the camera + signals were already
   * updated via direct setZoom/setPan calls.
   */
  const getDocCoords = (e: PointerEvent) => {
    const rect = getCachedContainerRect();
    if (!rect) return { x: 0, y: 0 };
    const p = pan();
    const z = zoom();
    if (!Number.isFinite(z) || z <= 0) return { x: 0, y: 0 };
      return {
        x: (e.clientX - rect.left - p.x) / z,
        y: (e.clientY - rect.top - p.y) / z,
      };
  };

  const pointerCtx: PointerToolContext = {
    editor,
    getCanvasContainerRef: params.getCanvasContainerRef,
    getCanvasRef: params.getCanvasRef,
    isSpacePressed: params.isSpacePressed,
    isPanning: params.isPanning,
    isAltPressed: params.isAltPressed,
    stopMomentum: params.stopMomentum,
    onPaintStroke: params.onPaintStroke,
    cropSnapTargets: params.cropSnapTargets,
    moveSnapEnabled: params.moveSnapEnabled,
    getDocCoords,
    selectionBox: () => selectionBox(),
    setSelectionBoxSignal: (box) => setSelectionBoxSignal(box),
    setSnapLines: (lines) => setSnapLines(lines),
    setHudInfo,
    setCropDragPreview: (preview) => setCropDragPreview(preview),
  };

  // Sample the pixel under the cursor into the color-picker's active target
  // (foreground/background) while a non-modal color picker is open. A click
  // on the canvas outside the floating dialog commits the picked color.
  const sampleToColorPicker = (e: PointerEvent) => {
    const engine = workspace.getActiveEngine();
    if (!engine) return;
    const coords = getDocCoords(e);
    const color = engine.samplePixel(coords.x, coords.y);
    const hex = rgbToHex(color[0], color[1], color[2]);
    if (colorPickerTarget() === "foreground") setFgColor(hex);
    else setBgColor(hex);
    scheduler.requestRender();
  };

  const handleDoubleClick = (e: MouseEvent) => {
    if (activeTool() === "crop") return;
    // Don't snap to fit while panning (space/middle-drag) or while a paint
    // tool is active. Rapid repeated dabs during brushing read as a
    // double-click and would reset the user's zoom/pan to fit — the bug this
    // guards against. Other tools keep double-click-to-fit.
    if (params.isPanning() || params.isSpacePressed()) return;
    if (activeTool() === "brush" || activeTool() === "eraser") return;

    // Re-edit (R3): double-click a text layer with a non-paint tool → switch
    // to the text tool and open an edit session on that layer.
    const dblEngine = workspace.getActiveEngine();
    if (dblEngine) {
      const dblCoords = getDocCoords(e as unknown as PointerEvent);
      const allLayers = [...dblEngine.getLayers()];
      const hit = hitTestLayers(dblCoords, allLayers as LayerInfo[], (id, x, y) => dblEngine.sampleLayerAlpha(id, x, y));
      const layer = hit ? dblEngine.getLayer(hit.id) : null;
      if (layer && layer.type === "text" && layer.textData) {
        editor.setActiveTool("text");
        startTextPointer(pointerCtx, e as unknown as PointerEvent, textPointerState);
        return;
      }
    }

    const container = params.getCanvasContainerRef();
    const canvas = params.getCanvasRef();
    if (e.target === container || e.target === canvas) {
      params.fitToScreenAndRender();
    }
  };

  const onCanvasPointerDown = (e: PointerEvent) => {
    const _t0 = performance.now();

    // ── Clear stale brush adjustment state ──
    // If the user released the right button outside the window after a brush
    // adjust session, onCanvasPointerUp didn't fire and brushAdjustStart leaks.
    // This stale state would cause onCanvasPointerUp to return early for ANY
    // subsequent pointer (including left-click eraser), preventing commit.
    if (brushAdjustStart) {
      brushAdjustStart = null;
      setHudInfo(null);
    }

    // ── On-canvas brush adjustment (Alt+RightButton+Drag) ──
    // Before the generic right-click guard, check if this is a brush
    // adjustment gesture: right button + alt key while brush/eraser tool.
    // Shows a live HUD and updates size/hardness in real-time.
    if (e.button === 2 && e.altKey && (activeTool() === "brush" || activeTool() === "eraser")) {
      e.preventDefault();
      trySetPointerCapture(params.getCanvasRef(), e.pointerId);
      brushAdjustStart = {
        size: activeTool() === "eraser" ? eraserSize() : brushSize(),
        hardness: activeTool() === "eraser" ? eraserHardness() : brushHardness(),
        screenX: e.clientX,
        screenY: e.clientY,
      };
      return;
    }
    if (e.button === 2) return;
    if (params.isSpacePressed() || params.isPanning() || e.button === 1) return;

    params.stopMomentum();

    // Color picker open → any canvas click samples into the active target.
    if (colorPickerOpen()) {
      sampleToColorPicker(e);
      return;
    }

    const engine = workspace.getActiveEngine();
    const history = workspace.getActiveHistory();
    if (!engine || !history) return;

    // Eyedropper tool now routes through the shared dispatcher below. Sampling
    // is handled in viewport/input-handler (handlePointerDown L121 / handlePointerMove
    // L245). Previously this early-returned and the tool silently no-op'd on canvas
    // click — useCanvasPointerTools forgot to wire the new tool (AGENTS.md pattern).

    if ((activeTool() === "brush" || activeTool() === "eraser") && params.isAltPressed()) {
      const coords = getDocCoords(e);
      const color = engine.samplePixel(coords.x, coords.y);
      setFgColor(rgbToHex(color[0], color[1], color[2]));
      trySetPointerCapture(params.getCanvasRef(), e.pointerId);
      return;
    }

    if (startCropDrag(pointerCtx, e, modernDragState)) return;

    if (activeTool() === "move" && moveAutoSelect()) {
      const coords = getDocCoords(e);
      const allLayers = [...engine.getLayers()];
      // Alpha-aware hit-test — same sampler as handleMoveAutoSelect so the
      // transient canvas-handler selection never diverges from the panel
      // selection at transparent pixels (@bug 2026-08-03).
      const hit = hitTestLayers(coords, allLayers as LayerInfo[], (id, x, y) => engine.sampleLayerAlpha(id, x, y));
      if (hit && hit.id !== engine.getActiveLayerId()) {
        engine.setActiveLayer(hit.id);
        setSelectedLayerId(hit.id);
        scheduler.requestRender();
      } else if (!hit) {
        setSelectedLayerId(null);
      }
    }

    // Guard: prevent blocked paint strokes from starting an overlay command.
    // paintBucket/gradient also read+rewrite the active layer bitmap, so they
    // must hit the same shape-layer conversion guard as brush/eraser.
    if (activeTool() === "brush" || activeTool() === "eraser" || activeTool() === "paintBucket" || activeTool() === "gradient") {
      const layerId = engine.getActiveLayerId();
      let activePaintLayer: ReturnType<DocumentEngine["getLayer"]> | null = null;
      if (layerId) {
        activePaintLayer = engine.getLayer(layerId);
      }
      // Shape-layer pixel guard: painting on a shape layer would desync the
      // bitmap from its shapeParams (a later param edit would silently wipe
      // the painted pixels). Convert explicitly after the user confirms —
      // never silently. Lazy "confirm → convert → user re-draws".
      if (activePaintLayer && engine.isShapeLayer(activePaintLayer.id)) {
        const paintLayerId = activePaintLayer.id;
        void dialogs.confirm({
          title: "Convert shape to pixels?",
          message: "Painting on this shape layer will convert it to a plain pixel layer. Shape editing (fill, stroke, radius) will no longer be available.",
          confirmLabel: "Convert",
          tone: "default",
        }).then((ok) => {
          if (!ok) return;
          // Decision is async — re-check the engine/history are still alive.
          const liveEngine = workspace.getActiveEngine();
          const liveHistory = workspace.getActiveHistory();
          if (!liveEngine || !liveHistory) return;
          const pre = liveEngine.snapshot();
          liveEngine.shapeLayerToRaster(paintLayerId);
          liveHistory.commit(pre, "Convert Shape to Pixels");
          scheduler.requestRender();
        });
        return; // stroke waits for the decision
      }
      // Locked/hidden/lockTransparency toasts stay a brush/eraser concern;
      // paintBucket/gradient enforce their own guards downstream.
      if (activeTool() === "brush" || activeTool() === "eraser") {
        const blockReason = getPaintToolBlockReason(activePaintLayer, activeTool() === "eraser");
        if (blockReason) {
          showToast(blockReason, "warn");
          return;
        }
      }
    }

    // If modern crop mode →skip engine handlePointerDown (it would call
    // onCropCreated and leak state into the Classic crop rect). Modern
    // crop has its own drag-to-create handling via modernDragStart.
    if (activeTool() === "crop" && cropInteractionMode() === "modern") {
      trySetPointerCapture(params.getCanvasRef(), e.pointerId);
      setSnapLines([]);
      return;
    }

    // ── Paint Bucket: click-to-fill ──
    if (applyPaintBucketFill(pointerCtx, e)) return;

    // ── Gradient: start drag ──
    if (startGradientDrag(pointerCtx, e, gradientDragState)) return;

    // ── Shape: start drag ──
    if (startShapeDrag(pointerCtx, e, shapeDragState)) return;

    // ── Text: click-to-create / click-to-edit / drag-to-area ──
    if (startTextPointer(pointerCtx, e, textPointerState)) return;

    // Sync selectionBox from engine state before starting a drag.
    // SelectionOptionBar calls engine.createSelection(…) which updates the
    // engine but NOT the local signal — without this sync the drag would
    // use a stale angle (or stale position/size) from the signal.
    if (activeTool() === "selection") {
      const sel = engine.getSelection();
      if (sel) {
        setSelectionBoxSignal({ x: sel.x, y: sel.y, w: sel.width, h: sel.height, angle: sel.angle, shape: sel.shape });
      } else {
        setSelectionBoxSignal(null);
      }
    }

    prepareToolContext();
    interactiveState.isShiftPressed = e.shiftKey;
    interactiveState.screenPos = { x: e.clientX, y: e.clientY };
    setSnapLines([]);
    trySetPointerCapture(params.getCanvasRef(), e.pointerId);

    const coords = getDocCoords(e);
    
    if (activeTool() === "brush" || activeTool() === "eraser") {
      const lp = getLastPaintCoords();
      // Capture the pre-stroke anchor so the undo snapshot restores it
      // deterministically (independent of commitBrushStroke's async timing).
      interactiveState.brushStrokeAnchor = lp;
      if (e.shiftKey && lp) {
        interactiveState.strokePoints = interpolateLinePoints(lp, coords);
        interactiveState.dragStart = { ...coords };
      } else {
        interactiveState.strokePoints = [];
        setLastPaintCoords({ ...coords });
      }
    }

    if (activeTool() === "brush" || activeTool() === "eraser") {
      paintSmoother.setWindowSize(smoothingToWindowSize(interactiveState.paintSettings.smoothing));
      paintSmoother.reset();
    }
    const smoothed = activeTool() === "brush" || activeTool() === "eraser"
      ? paintSmoother.addPoint(coords.x, coords.y)
      : coords;
    const isPaintTool = activeTool() === "brush" || activeTool() === "eraser";      handlePointerDown(
      activeTool() as ToolType,
      smoothed.x,
      smoothed.y,
      engine,
      history,
      // Brush/eraser: suppress requestRender — overlay canvas handles preview,
      // layer data doesn't change until commit. Saves a full WebGL composite per event.
      isPaintTool ? NOOP : () => scheduler.requestRender(),
      interactiveState,
    );
    const _dt = performance.now() - _t0;
    if (_dt > 5) console.warn(`[perf] onCanvasPointerDown: ${_dt.toFixed(1)}ms (tool=${activeTool()})`);
  };

  const onCanvasPointerMove = (e: PointerEvent) => {
    const _t0 = performance.now();
    if (params.isPanning()) return;

    // Drag-sampling while the color picker is open (press + move across canvas).
    if (colorPickerOpen() && e.buttons === 1) {
      sampleToColorPicker(e);
      return;
    }

    // ── On-canvas brush adjustment ──
    if (brushAdjustStart) {
      const dx = e.clientX - brushAdjustStart.screenX;
      const dy = brushAdjustStart.screenY - e.clientY; // invert: up = more

      const isEraserTool = activeTool() === "eraser";
      const maxSize = 2000;
      const minSize = 1;

      // Size: proportional change based on percentage of start size
      const sizePct = 1 + dx * 0.005; // 200px drag = 2x size change
      const newSize = Math.round(Math.max(minSize, Math.min(maxSize, brushAdjustStart.size * sizePct)));

      // Hardness: linear change, 250px drag = 1.0 full range
      const newHardness = Math.max(0, Math.min(1, brushAdjustStart.hardness + dy * 0.004));

      if (isEraserTool) {
        setEraserSize(newSize);
        setEraserHardness(newHardness);
      } else {
        setBrushSize(newSize);
        setBrushHardness(newHardness);
      }

      // Show live HUD
      setHudInfo({
        mode: "brush",
        clientX: e.clientX,
        clientY: e.clientY,
        width: newSize,
        height: Math.round(newHardness * 100),
        deltaX: 0,
        deltaY: 0,
        scalePercent: 0,
        angle: 0,
        snapActive: false,
      });
      return;
    }

    edgeLastClientX = e.clientX;
    edgeLastClientY = e.clientY;

    // ── Gradient: track end point during drag (with Shift 45° angle lock) ──
    if (trackGradientDrag(pointerCtx, e, gradientDragState)) return;

    // ── Shape: track drag end for live preview ──
    if (trackShapeDrag(pointerCtx, e, shapeDragState)) return;

    // ── Text: grow area box during drag ──
    if (trackTextPointer(pointerCtx, e, textPointerState)) return;

    // Modern crop drag-to-create: show selection preview rect
    if (trackModernCropDrag(pointerCtx, e, modernDragState)) return;

    const engine = workspace.getActiveEngine();
    if (!engine) return;

    if ((activeTool() === "brush" || activeTool() === "eraser") && params.isAltPressed()) {
      if (e.buttons === 1) {
        const coords = getDocCoords(e);
        const color = engine.samplePixel(coords.x, coords.y);
        setFgColor(rgbToHex(color[0], color[1], color[2]));
        scheduler.requestRender();
      }
      return;
    }

    interactiveState.isAltPressed = params.isAltPressed();
    interactiveState.isShiftPressed = e.shiftKey;
    interactiveState.screenPos = { x: e.clientX, y: e.clientY };

    // Edge auto-scroll: pan camera when dragging near viewport edge
    if (interactiveState.isDragging) {
      const tool = activeTool();
      if (tool === "brush" || tool === "eraser" || tool === "move" || tool === "selection" || tool === "crop") {
        edgeLastClientX = e.clientX;
        edgeLastClientY = e.clientY;
        // Check zone with dt=0 — no actual scroll, only detection
        if (applyEdgeScroll(0).scrolled) {
          startEdgeRaf();
        } else {
          stopEdgeRaf();
        }
      }
    }

    let coords = getDocCoords(e);

    if ((activeTool() === "brush" || activeTool() === "eraser") && interactiveState.isDragging) {
      if (e.shiftKey) {
        const start = interactiveState.dragStart;
        const dx = coords.x - start.x;
        const dy = coords.y - start.y;
        
        if (!axisLock) {
          if (Math.abs(dx) >= 5 || Math.abs(dy) >= 5) {
            axisLock = Math.abs(dx) > Math.abs(dy) ? "horizontal" : "vertical";
          }
        }
        
        if (axisLock === "horizontal") {
          coords = { x: coords.x, y: start.y };
        } else if (axisLock === "vertical") {
          coords = { x: start.x, y: coords.y };
        }
      } else {
        axisLock = null;
      }
    }

    const smoothed = (activeTool() === "brush" || activeTool() === "eraser")
      ? paintSmoother.addPoint(coords.x, coords.y)
      : coords;
    const isPaintTool = activeTool() === "brush" || activeTool() === "eraser";
    handlePointerMove(
      activeTool() as ToolType,
      smoothed.x,
      smoothed.y,
      engine,
      // Brush/eraser: suppress requestRender — overlay canvas handles preview,
      // layer data doesn't change until commit. Saves a full WebGL composite per event.
      isPaintTool ? NOOP : () => scheduler.requestRender(),
      interactiveState,
    );
    const _dt = performance.now() - _t0;
    if (_dt > 5) console.warn(`[perf] onCanvasPointerMove: ${_dt.toFixed(1)}ms (tool=${activeTool()}, dragging=${interactiveState.isDragging})`);
  };

  const onCanvasPointerUp = (e: PointerEvent) => {
    if (params.isPanning()) return;

    // ── On-canvas brush adjustment ──
    if (brushAdjustStart) {
      tryReleasePointerCapture(params.getCanvasRef(), e.pointerId);
      brushAdjustStart = null;
      setHudInfo(null);
      return;
    }

    const engine = workspace.getActiveEngine();
    const history = workspace.getActiveHistory();
    if (!engine || !history) return;

    stopEdgeRaf();

    if ((activeTool() === "brush" || activeTool() === "eraser") && params.isAltPressed()) {
      tryReleasePointerCapture(params.getCanvasRef(), e.pointerId);
      return;
    }

    setSnapLines([]);
    let coords = getDocCoords(e);
    if ((activeTool() === "brush" || activeTool() === "eraser") && e.shiftKey) {
      const start = interactiveState.dragStart;
      const dx = coords.x - start.x;
      const dy = coords.y - start.y;
      if (!axisLock && (Math.abs(dx) >= 5 || Math.abs(dy) >= 5)) {
        axisLock = Math.abs(dx) > Math.abs(dy) ? "horizontal" : "vertical";
      }
      if (axisLock === "horizontal") coords = { x: coords.x, y: start.y };
      if (axisLock === "vertical") coords = { x: start.x, y: coords.y };
    }
    const tool = (interactiveState.dragTool ?? activeTool()) as ToolType;
    // Feed current marquee shape into the pointer-up handler so ellipse
    // selections are created with the right shape (not always rect).
    interactiveState.selectionShape = typeof selectionShape === "function" ? selectionShape() : "rect";
    const isPaintTool = tool === "brush" || tool === "eraser";
    const hasPoints = isPaintTool && interactiveState.strokePoints.length > 0;
    const smoothed = isPaintTool
      ? paintSmoother.addPoint(coords.x, coords.y)
      : coords;

    const isPaintToolForUp = tool === "brush" || tool === "eraser";
    handlePointerUp(
      activeTool() as ToolType,
      smoothed.x,
      smoothed.y,
      engine,
      history,
      // Brush/eraser: suppress requestRender — commitBrushStroke handles it after this call.
      isPaintToolForUp ? NOOP : () => scheduler.requestRender(),
      interactiveState,
    );

    tryReleasePointerCapture(params.getCanvasRef(), e.pointerId);

    if (hasPoints) {
      const layerId = engine.getActiveLayerId();
      if (layerId) {
        // anchor = pre-stroke `lastPaintCoords`; commitBrushStroke restores it
        // on undo and advances live `lastPaintCoords` to the stroke end itself.
        params.commitBrushStroke(engine, history, layerId, tool === "eraser", interactiveState.brushStrokeAnchor ?? null);
      }
    }

    interactiveState.dragTool = null;

    // ── Gradient: apply on pointer up ──
    if (applyGradientFill(pointerCtx, gradientDragState)) return;

    // ── Shape: apply/commit on pointer up (deletes temp under 3px) ──
    if (applyShapeDrag(pointerCtx, e, shapeDragState)) return;

    // ── Text: release capture; session stays open for typing ──
    if (applyTextPointer(pointerCtx, e, textPointerState)) return;

    // Modern crop: handle drag end or click fallback + classic pending-click
    handleCropPointerUp(pointerCtx, e, modernDragState, coords, interactiveState, tool);

    if (activeTool() === "selection") {
      const sel = engine.getSelection();
      if (sel) {
        setSelectionBoxSignal({ x: sel.x, y: sel.y, w: sel.width, h: sel.height, angle: sel.angle, shape: sel.shape });
      } else {
        setSelectionBoxSignal(null);
      }
      // Clear HUD after selection interaction completes
      setHudInfo(null);
    } else {
      setSelectionBoxSignal(null);
    }
  };

  const onCanvasPointerCancel = (e: PointerEvent) => {
    // ── Cleanup brush adjustment on cancel ──
    if (brushAdjustStart) {
      brushAdjustStart = null;
      setHudInfo(null);
      tryReleasePointerCapture(params.getCanvasRef(), e.pointerId);
    }

    stopEdgeRaf();
    paintSmoother.reset();
    const engine = workspace.getActiveEngine();
    const history = workspace.getActiveHistory();
    if (!engine) return;

    tryReleasePointerCapture(params.getCanvasRef(), e.pointerId);

    const tool = (interactiveState.dragTool ?? activeTool()) as ToolType;
    if (tool === "brush" || tool === "eraser") {
      const layerId = engine.getActiveLayerId();
      if (history && layerId && interactiveState.strokePoints.length > 0) {
        interactiveState.onPaintStroke?.(
          interactiveState.strokePoints,
          tool === "eraser",
          interactiveState.paintSettings,
          true,
        );
        params.commitBrushStroke(engine, history, layerId, tool === "eraser");
      }
    }

    if (tool === "selection") {
      // Sync from engine instead of blindly setting null — a pointerCancel
      // (e.g. context menu, touch gesture) should not destroy the visual
      // selection marquee when the engine still has an active selection.
      // If the engine has a selection, preserve the signal; only clear
      // when the engine truly cleared it (Bug #5).
      const sel = engine.getSelection();
      if (sel) {
        setSelectionBoxSignal({ x: sel.x, y: sel.y, w: sel.width, h: sel.height, angle: sel.angle, shape: sel.shape });
      } else {
        setSelectionBoxSignal(null);
      }
      setHudInfo(null);
    }
    interactiveState.strokePoints = [];
    interactiveState.isDragging = false;
    interactiveState.dragTool = null;
    setSnapLines([]);
    setCropDragPreview(null);
    modernDragState.reset();
    // ── Reset gradient drag state ──
    gradientDragState.reset();
    // ── Cancel shape drag: remove temp layer, no history entry ──
    if (shapeDragState.tempLayerId) engine.deleteLayer(shapeDragState.tempLayerId);
    shapeDragState.reset();
    // ── Cancel text session: remove temp layer, no history entry ──
    if (activeTool() === "text") {
      cancelTextSession(editor);
      textPointerState.reset();
    }
  };

  const onCanvasLostPointerCapture = (e: PointerEvent) => {
    // ── Cleanup brush adjustment on lost capture ──
    if (brushAdjustStart) {
      brushAdjustStart = null;
      setHudInfo(null);
    }

    stopEdgeRaf();
    paintSmoother.reset();
    const engine = workspace.getActiveEngine();
    const history = workspace.getActiveHistory();
    if (!engine) return;

    const tool = (interactiveState.dragTool ?? activeTool()) as ToolType;
    if (tool === "brush" || tool === "eraser") {
      const layerId = engine.getActiveLayerId();
      if (history && layerId && interactiveState.strokePoints.length > 0) {
        interactiveState.onPaintStroke?.(
          interactiveState.strokePoints,
          tool === "eraser",
          interactiveState.paintSettings,
          true,
        );
        params.commitBrushStroke(engine, history, layerId, tool === "eraser");
      }
    }

    interactiveState.strokePoints = [];
    interactiveState.isDragging = false;
    interactiveState.dragTool = null;
    setCropDragPreview(null);
    modernDragState.reset();
    // ── Reset gradient drag state ──
    gradientDragState.reset();
    // ── Cancel shape drag: remove temp layer, no history entry ──
    if (shapeDragState.tempLayerId) engine.deleteLayer(shapeDragState.tempLayerId);
    shapeDragState.reset();
    // ── Cancel text session: remove temp layer, no history entry ──
    if (activeTool() === "text") {
      cancelTextSession(editor);
      textPointerState.reset();
    }
  };

  const startSelectionRotation = () =>
    startSelectionRotationFn(
      () => selectionBox(),
      (box) => setSelectionBoxSignal(box),
      () => params.getCanvasContainerRef(),
      () => workspace.getActiveEngine(),
      () => ({ panX: pan().x, panY: pan().y, zoom: zoom() }),
    );

  const prepareToolContext = () => prepareToolContextImpl(pointerCtx, interactiveState);

  return {
    cropDragPreview,
    setCropDragPreview,
    snapLines,
    setSnapLines,
    selectionBox,
    setSelectionBoxSignal,
    startSelectionRotation,
    hudInfo,
    setHudInfo,
    getDocCoords,
    handleDoubleClick,
    onCanvasPointerDown,
    onCanvasPointerMove,
    onCanvasPointerUp,
    onCanvasPointerCancel,
    onCanvasLostPointerCapture,
    prepareToolContext,
  };
}
