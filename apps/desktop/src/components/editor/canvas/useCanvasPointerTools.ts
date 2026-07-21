import { createSignal, createEffect, onCleanup } from "solid-js";
import { useEditor } from "../shell/EditorContext";
import { screenToDocument, documentToScreen } from "@/viewport/coords";
import { snapCropRect, type CropSnapTargets } from "@/viewport/cropSnap";
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
import { getActivePaintToolSettings, getPaintToolBlockReason, type PaintToolSettings } from "../brushToolState";
import { PaintSmoother, smoothingToWindowSize } from "../paintSmoothing";
import { tryReleasePointerCapture, trySetPointerCapture } from "../tools/pointerCapture";
import { getLayerAabb, documentToLayerLocal } from "@/viewport/transformGeometry";
import { computeSnapAdjustment, type SnapRect } from "@/viewport/smartGuides";
import { showToast } from "../Toast";
import type { HudMode } from "../TransformHud";
import { getDefaultModernCropFrame, getProjectedCanvasSize, clampFrameToProjectedBounds } from "@/viewport/modernCropGeometry";
import { resetCropPreviewToCanvas, restoreHiddenCropPreview, createCropRectFromDocumentPoints } from "../cropToolActions";
import { rgbToHex, interpolateLinePoints } from "./pointerUtils";
import { floodFill, gradientFill, type FillMask, type ColorStop } from "@/features/fill/fillOperations";
import { SelectionOperations } from "@/features/selection/SelectionOperations";
import { startSelectionRotation as startSelectionRotationFn } from "./selectionRotation";
import { computeEdgeScroll } from "./edgeScroll";

const DRAG_CREATE_THRESHOLD = 5;
const MIN_CROP_SIZE = 100;
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
  cropSnapTargets?: () => CropSnapTargets | undefined;
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
  const {
    workspace,
    renderer,
    scheduler,
    activeTool,
    fgColor,
    bgColor,
    setFgColor,
    setBgColor,
    colorPickerOpen,
    colorPickerTarget,
    zoom,
    pan,
    camera,
    setViewportState,
    setPan,
    cropRect,
    cropMode,
    cropAspect,
    cropSizeTarget,
    setCropRect,
    cropRotation,
    setCropRotation,
    hiddenCropPreview,
    setHiddenCropPreview,
    selectedLayerId,
    setSelectedLayerId,
    setHoverHandle,
    selectionConstraintMode,
    selectionShape,
    selectionRatioW,
    selectionRatioH,
    selectionSizeW,
    selectionSizeH,
    cropInteractionMode,
    modernCropFrame,
    setModernCropFrame,
    modernCropImageTransform,
    setModernCropImageTransform,
    viewportWidth,
    viewportHeight,
    docWidth,
    docHeight,
    moveAutoSelect,
    moveSnapEnabled,
    setHoverPos,
    brushSize,
    setBrushSize,
    brushHardness,
    setBrushHardness,
    brushOpacity,
    eraserSize,
    setEraserSize,
    eraserHardness,
    setEraserHardness,
    eraserOpacity,
    brushFlow,
    brushSmoothing,
    eraserFlow,
    eraserSmoothing,
    fillTolerance,
    gradientType,
    gradientPreset,
  } = useEditor();

  let isPendingCropClick = false;
  let modernDragStart: { x: number; y: number } | null = null;
  let modernDragExceededThreshold = false;
  let modernDragEnd: { x: number; y: number } | null = null;
  let modernDragSnappedPreview: { x: number; y: number; w: number; h: number } | null = null;

  // ── Gradient drag state ──
  let gradientDragStart: { x: number; y: number } | null = null;
  let isGradientDragging = false;
  let gradientDragEnd: { x: number; y: number } | null = null;

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

  function resetModernDragState() {
    modernDragStart = null;
    modernDragExceededThreshold = false;
    modernDragEnd = null;
    modernDragSnappedPreview = null;
  }

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

  function prepareToolContext() {
    const engine = workspace.getActiveEngine();
    interactiveState.fgColor = fgColor();
    interactiveState.bgColor = bgColor();
    // For the move tool only: if the UI signal says no layer is selected
    // (pasteboard/canvas deselect), don't operate on a stale engine layer.
    // Other tools (brush, eraser, selection, crop) should use the engine's
    // active layer as-is since they don't depend on the UI selection state.
    const engineLayerId = engine ? engine.getActiveLayerId() : null;
    if (activeTool() === "move" && selectedLayerId() === null) {
      interactiveState.selectedLayerId = null;
    } else {
      interactiveState.selectedLayerId = engineLayerId;
    }
    interactiveState.isAltPressed = params.isAltPressed();
    interactiveState.setFgColor = setFgColor;
    interactiveState.setBgColor = setBgColor;
    interactiveState.selectionConstraintMode = typeof selectionConstraintMode === "function" ? selectionConstraintMode() : "normal";
    interactiveState.selectionRatioW = typeof selectionRatioW === "function" ? selectionRatioW() : 1;
    interactiveState.selectionRatioH = typeof selectionRatioH === "function" ? selectionRatioH() : 1;
    interactiveState.selectionSizeW = typeof selectionSizeW === "function" ? selectionSizeW() : 100;
    interactiveState.selectionSizeH = typeof selectionSizeH === "function" ? selectionSizeH() : 100;
    interactiveState.onSelectionCreated = (x, y, w, h) => {
      const currentShape = typeof selectionShape === "function" ? selectionShape() : undefined;
      setSelectionBoxSignal({ x, y, w, h, angle: 0, shape: currentShape });
      // Show W×H HUD during selection draw drag
      const sp = interactiveState.screenPos;
      if (sp) {
        setHudInfo({
          mode: "resize",
          clientX: sp.x,
          clientY: sp.y,
          width: w,
          height: h,
          deltaX: 0,
          deltaY: 0,
          scalePercent: 0,
          angle: 0,
          snapActive: false,
        });
      }
    };
    interactiveState.selectionBounds = selectionBox() ? {
      x: selectionBox()!.x,
      y: selectionBox()!.y,
      width: selectionBox()!.w,
      height: selectionBox()!.h,
      angle: selectionBox()!.angle ?? 0,
    } : null;
    interactiveState.onSelectionMoved = (x, y) => {
      const box = selectionBox();
      const eng = workspace.getActiveEngine();
      if (box && eng) {
        // Clamp to document bounds so selection can't be moved completely off-canvas
        const docW = eng.getWidth();
        const docH = eng.getHeight();
        const clampedX = Math.max(-box.w + 1, Math.min(docW - 1, x));
        const clampedY = Math.max(-box.h + 1, Math.min(docH - 1, y));
        setSelectionBoxSignal({ ...box, x: clampedX, y: clampedY });
        eng.createSelection(clampedX, clampedY, box.w, box.h, box.angle, box.shape);
      }
      // Show ΔX ΔY HUD during selection move
      const sp = interactiveState.screenPos;
      const orig = interactiveState.pendingOriginalSelectionPos;
      if (sp && orig) {
        setHudInfo({
          mode: "move",
          clientX: sp.x,
          clientY: sp.y,
          deltaX: x - orig.x,
          deltaY: y - orig.y,
          width: 0,
          height: 0,
          scalePercent: 0,
          angle: 0,
          snapActive: false,
        });
      }
    };
    interactiveState.onSelectionRotated = (angle: number) => {
      const box = selectionBox();
      if (box) {
        setSelectionBoxSignal({ ...box, angle });
      }
    };
    interactiveState.onRotateStart = (centerX: number, centerY: number) => {
      interactiveState.dragMode = "rotate-selection";
      interactiveState.rotateCenter = { x: centerX, y: centerY };
      interactiveState.rotateStartAngle = 0;
      interactiveState.selectionAngle = selectionBox()?.angle ?? 0;
    };
    interactiveState.onCropCreated = (x, y, w, h) => {
      const nextRect = createCropRectFromDocumentPoints(
        interactiveState.dragStart,
        interactiveState.dragCurrent
      );
      if (nextRect) {
        setHiddenCropPreview(null);
        setCropRotation(0);
        setCropRect(nextRect);
      }
    };
    interactiveState.onHoverHandle = setHoverHandle;

    interactiveState.paintSettings = getActivePaintToolSettings(activeTool(), {
      brushSize: brushSize(),
      brushHardness: brushHardness(),
      brushOpacity: brushOpacity(),
      brushFlow: brushFlow(),
      brushSmoothing: brushSmoothing(),
      eraserSize: eraserSize(),
      eraserHardness: eraserHardness(),
      eraserOpacity: eraserOpacity(),
      eraserFlow: eraserFlow(),
      eraserSmoothing: eraserSmoothing(),
    });
    interactiveState.brushSize = interactiveState.paintSettings.size;
    interactiveState.brushHardness = interactiveState.paintSettings.hardness;
    interactiveState.brushOpacity = interactiveState.paintSettings.opacity;

    const activeEngineForTargets = workspace.getActiveEngine();
    const movingId = activeEngineForTargets ? activeEngineForTargets.getActiveLayerId() : null;
    const docW = activeEngineForTargets ? activeEngineForTargets.getWidth() : 0;
    const docH = activeEngineForTargets ? activeEngineForTargets.getHeight() : 0;

    const layerTargets: SnapRect[] = activeEngineForTargets
      ? activeEngineForTargets.getLayers()
        .filter((l) => l.visible && l.id !== movingId)
        .map((l) => {
          const aabb = getLayerAabb(l.transform, l.width, l.height);
          return { x: aabb.x, y: aabb.y, w: aabb.width, h: aabb.height };
        })
      : [];

    const snapTargets: SnapRect[] = [
      { x: 0, y: 0, w: docW, h: docH, snapThreshold: 12, snapPriority: 3 },
      { x: docW / 2, y: -Infinity, w: 0, h: Infinity, snapThreshold: 6, snapPriority: 2 },
      { x: -Infinity, y: docH / 2, w: Infinity, h: 0, snapThreshold: 6, snapPriority: 2 },
      ...layerTargets,
    ];

    if (moveSnapEnabled()) {
      interactiveState.onComputeSnap = (rect: SnapRect) => {
        if (!activeEngineForTargets) {
          setSnapLines([]);
          return { dx: 0, dy: 0, lines: [] };
        }
        return computeSnapAdjustment(rect, snapTargets, 5, zoom());
      };
    } else {
      interactiveState.onComputeSnap = undefined;
      setSnapLines([]);
    }
    interactiveState.onSnapLines = (lines) => setSnapLines(lines);
    interactiveState.onPaintStroke = params.onPaintStroke;
  }

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

    if (activeTool() === "crop" && e.button === 0) {
      if (cropInteractionMode() === "modern") {
        // Track drag start for drag-to-create even when frame exists.
        // The ModernCropOverlay SVG on top catches clicks on the frame
        // (move rect, handles, rotate ring) with stopPropagation().
        // Clicks on the mask area (outside the frame) fall through to
        // the canvas →those start a new drag-create.
        const viewport = params.getCanvasContainerRef();
        if (!viewport) return;
        const rect = viewport.getBoundingClientRect();
        modernDragStart = {
          x: e.clientX - rect.left,
          y: e.clientY - rect.top,
        };
        modernDragExceededThreshold = false;
        isPendingCropClick = false;
      } else {
        isPendingCropClick = !cropRect();
      }
    } else {
      isPendingCropClick = false;
    }

    if (activeTool() === "move" && moveAutoSelect()) {
      const coords = getDocCoords(e);
      const allLayers = [...engine.getLayers()];
      const hit = hitTestLayers(coords, allLayers as LayerInfo[]);
      if (hit && hit.id !== engine.getActiveLayerId()) {
        engine.setActiveLayer(hit.id);
        setSelectedLayerId(hit.id);
        scheduler.requestRender();
      } else if (!hit) {
        setSelectedLayerId(null);
      }
    }

    // Guard: prevent blocked brush/eraser strokes from starting an overlay command.
    if (activeTool() === "brush" || activeTool() === "eraser") {
      const layerId = engine.getActiveLayerId();
      let activePaintLayer: ReturnType<DocumentEngine["getLayer"]> | null = null;
      if (layerId) {
        activePaintLayer = engine.getLayer(layerId);
      }
      const blockReason = getPaintToolBlockReason(activePaintLayer, activeTool() === "eraser");
      if (blockReason) {
        showToast(blockReason, "warn");
        return;
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
    if (activeTool() === "paintBucket") {
      const engine = workspace.getActiveEngine();
      const history = workspace.getActiveHistory();
      if (!engine || !history) return;

      const layerId = engine.getActiveLayerId();
      if (!layerId) { showToast("No editable layer selected", "warn"); trySetPointerCapture(params.getCanvasRef(), e.pointerId); return; }
      const layer = engine.getLayer(layerId);
      if (!layer || layer.locked) { showToast("Layer is locked", "warn"); trySetPointerCapture(params.getCanvasRef(), e.pointerId); return; }
      if (!layer.visible) { showToast("Layer is hidden", "warn"); trySetPointerCapture(params.getCanvasRef(), e.pointerId); return; }
      if (layer.lockTransparency) { showToast("Transparent pixels protected", "warn"); trySetPointerCapture(params.getCanvasRef(), e.pointerId); return; }

      const coords = getDocCoords(e);
      const bitmap = engine.getLayerImageBitmap(layerId);
      if (!bitmap) { showToast("Layer has no image data", "warn"); return; }

      const offscreen = new OffscreenCanvas(layer.width, layer.height);
      const ctx = offscreen.getContext("2d");
      if (!ctx) return;
      ctx.drawImage(bitmap, 0, 0);
      const imgData = ctx.getImageData(0, 0, layer.width, layer.height);

      // Layer-local click coords (accounts for scale/rotation/flip, not just translation)
      const localPt = documentToLayerLocal(coords.x, coords.y, layer.transform, layer.width, layer.height);
      const lx = Math.floor(localPt.x);
      const ly = Math.floor(localPt.y);

      // Fill colour from foreground
      const hex = fgColor().replace("#", "");
      const fillR = parseInt(hex.slice(0, 2), 16);
      const fillG = parseInt(hex.slice(2, 4), 16);
      const fillB = parseInt(hex.slice(4, 6), 16);

      // Build selection mask
      const sel = engine.getSelection();
      let fillMask: FillMask | undefined;
      if (sel) {
        const aabb = SelectionOperations.selectionToLayerAabb(sel, layer.transform, layer.width, layer.height);
        fillMask = {
          x: Math.round(aabb.x), y: Math.round(aabb.y),
          w: Math.max(0, Math.round(aabb.width)), h: Math.max(0, Math.round(aabb.height)),
          shape: sel.shape, inverted: sel.inverted,
        };
      }

      floodFill(imgData, lx, ly, fillR, fillG, fillB, 255, fillTolerance(), fillMask ?? null);

      const preSnapshot = engine.snapshot();
      ctx.putImageData(imgData, 0, 0);
      const newBitmap = offscreen.transferToImageBitmap();
      try {
        engine.setLayerImageBitmap(layerId, newBitmap);
        renderer.uploadImage(layerId, newBitmap);
      } catch (err) {
        showToast(`Fill failed: ${err instanceof Error ? err.message : 'Unknown error'}`, "error");
        trySetPointerCapture(params.getCanvasRef(), e.pointerId);
        return;
      }
      history.commit(preSnapshot, "Paint Bucket Fill");
      scheduler.requestRender();

      trySetPointerCapture(params.getCanvasRef(), e.pointerId);
      return;
    }

    // ── Gradient: start drag ──
    if (activeTool() === "gradient") {
      const engine = workspace.getActiveEngine();
      const history = workspace.getActiveHistory();
      if (!engine || !history) return;

      const layerId = engine.getActiveLayerId();
      if (!layerId) { showToast("No editable layer selected", "warn"); trySetPointerCapture(params.getCanvasRef(), e.pointerId); return; }
      const layer = engine.getLayer(layerId);
      if (!layer || layer.locked) { showToast("Layer is locked", "warn"); trySetPointerCapture(params.getCanvasRef(), e.pointerId); return; }
      if (!layer.visible) { showToast("Layer is hidden", "warn"); trySetPointerCapture(params.getCanvasRef(), e.pointerId); return; }
      if (layer.lockTransparency) { showToast("Transparent pixels protected", "warn"); trySetPointerCapture(params.getCanvasRef(), e.pointerId); return; }

      const coords = getDocCoords(e);
      gradientDragStart = { x: coords.x, y: coords.y };
      isGradientDragging = true;
      trySetPointerCapture(params.getCanvasRef(), e.pointerId);
      return;
    }

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

    // ── Gradient: track end point during drag ──
    if (activeTool() === "gradient" && isGradientDragging) {
      const coords = getDocCoords(e);
      gradientDragEnd = { x: coords.x, y: coords.y };
      scheduler.requestRender();
      return;
    }

    // Modern crop drag-to-create: show selection preview rect
    if (
      activeTool() === "crop" &&
      cropInteractionMode() === "modern" &&
      modernDragStart
    ) {
      const container = params.getCanvasContainerRef();
      if (!container) return;
      const rect = container.getBoundingClientRect();
      const ex = e.clientX - rect.left;
      const ey = e.clientY - rect.top;
      const dx = ex - modernDragStart.x;
      const dy = ey - modernDragStart.y;

      if (!modernDragExceededThreshold) {
        if (Math.abs(dx) < DRAG_CREATE_THRESHOLD && Math.abs(dy) < DRAG_CREATE_THRESHOLD) {
          setCropDragPreview(null);
          return;
        }
        modernDragExceededThreshold = true;
        // Clear existing frame once drag exceeds threshold →visual
        // feedback that a new crop is being created instead of moved.
        setModernCropFrame(null);
      }

      modernDragEnd = { x: ex, y: ey };

      // Build screen-space rect
      const sx = Math.min(modernDragStart.x, ex);
      const sy = Math.min(modernDragStart.y, ey);
      const sw = Math.abs(dx);
      const sh = Math.abs(dy);

      // Apply snap if enabled
      const z = zoom();
      const canvasEl = params.getCanvasRef();
      const canvasRect = canvasEl?.getBoundingClientRect();
      // Document origin in viewport space. In Modern mode the canvas uses
      // CSS transforms (not left/top from pan), so compute visual offset
      // directly from element bounds.
      const docOriginX = canvasRect ? canvasRect.left - rect.left : 0;
      const docOriginY = canvasRect ? canvasRect.top - rect.top : 0;
      const cst = params.cropSnapTargets?.();
      if (
        cst &&
        params.moveSnapEnabled?.() !== false &&
        !e.altKey
      ) {
        const snapTargets = cst;
        // Convert screen rect to doc-space for snapping
        const docRect = {
          x: (sx - docOriginX) / z,
          y: (sy - docOriginY) / z,
          w: sw / z,
          h: sh / z,
        };
        const threshold = 12 / z;
        const snapped = snapCropRect(docRect, "new", snapTargets, threshold);
        setSnapLines(snapped.lines);
        // Convert snapped doc rect back to screen-space
        const screenSnapped = {
          x: snapped.rect.x * z + docOriginX,
          y: snapped.rect.y * z + docOriginY,
          w: snapped.rect.w * z,
          h: snapped.rect.h * z,
        };
        setCropDragPreview(screenSnapped);
        modernDragSnappedPreview = screenSnapped;
      } else {
        setSnapLines([]);
        setCropDragPreview({ x: sx, y: sy, w: sw, h: sh });
        modernDragSnappedPreview = null;
      }
      return; // Don't dispatch to handlePointerMove
    }

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
    if (activeTool() === "gradient" && isGradientDragging) {
      isGradientDragging = false;
      const engine = workspace.getActiveEngine();
      const history = workspace.getActiveHistory();
      if (!engine || !history || !gradientDragStart || !gradientDragEnd) {
        gradientDragStart = null;
        gradientDragEnd = null;
        return;
      }

      const layerId = engine.getActiveLayerId();
      if (!layerId) { gradientDragStart = null; gradientDragEnd = null; return; }
      const layer = engine.getLayer(layerId);
      if (!layer) { gradientDragStart = null; gradientDragEnd = null; return; }

      const bitmap = engine.getLayerImageBitmap(layerId);
      if (!bitmap) { showToast("Layer has no image data", "warn"); gradientDragStart = null; gradientDragEnd = null; return; }

      const offscreen = new OffscreenCanvas(layer.width, layer.height);
      const ctx = offscreen.getContext("2d");
      if (!ctx) { gradientDragStart = null; gradientDragEnd = null; return; }
      ctx.drawImage(bitmap, 0, 0);
      const imgData = ctx.getImageData(0, 0, layer.width, layer.height);

      // Build color stops from preset
      const hex = fgColor().replace("#", "");
      const fgR = parseInt(hex.slice(0, 2), 16);
      const fgG = parseInt(hex.slice(2, 4), 16);
      const fgB = parseInt(hex.slice(4, 6), 16);
      const bgHex = bgColor().replace("#", "");
      const bgR = parseInt(bgHex.slice(0, 2), 16);
      const bgG = parseInt(bgHex.slice(2, 4), 16);
      const bgB = parseInt(bgHex.slice(4, 6), 16);

      let stops: ColorStop[];
      if (gradientPreset() === "fg-transparent") {
        stops = [
          { offset: 0, r: fgR, g: fgG, b: fgB, a: 255 },
          { offset: 1, r: fgR, g: fgG, b: fgB, a: 0 },
        ];
      } else {
        stops = [
          { offset: 0, r: fgR, g: fgG, b: fgB, a: 255 },
          { offset: 1, r: bgR, g: bgG, b: bgB, a: 255 },
        ];
      }

      // Build selection mask
      const sel = engine.getSelection();
      let fillMask: FillMask | undefined;
      if (sel) {
        const aabb = SelectionOperations.selectionToLayerAabb(sel, layer.transform, layer.width, layer.height);
        fillMask = {
          x: Math.round(aabb.x), y: Math.round(aabb.y),
          w: Math.max(0, Math.round(aabb.width)), h: Math.max(0, Math.round(aabb.height)),
          shape: sel.shape, inverted: sel.inverted,
        };
      }

      // Convert gradient start/end to layer-local space (accounts for transform)
      const startLocal = documentToLayerLocal(gradientDragStart.x, gradientDragStart.y, layer.transform, layer.width, layer.height);
      const endLocal = documentToLayerLocal(gradientDragEnd.x, gradientDragEnd.y, layer.transform, layer.width, layer.height);

      gradientFill(
        imgData, gradientType(),
        startLocal.x, startLocal.y,
        endLocal.x, endLocal.y,
        stops, fillMask ?? null,
      );

      const preSnapshot = engine.snapshot();
      ctx.putImageData(imgData, 0, 0);
      const newBitmap = offscreen.transferToImageBitmap();
      try {
        engine.setLayerImageBitmap(layerId, newBitmap);
        renderer.uploadImage(layerId, newBitmap);
      } catch (err) {
        showToast(`Gradient fill failed: ${err instanceof Error ? err.message : 'Unknown error'}`, "error");
        gradientDragStart = null;
        gradientDragEnd = null;
        return;
      }
      history.commit(preSnapshot, "Gradient Fill");
      scheduler.requestRender();

      gradientDragStart = null;
      gradientDragEnd = null;
      return;
    }

    // Modern crop: handle drag end or click fallback
    if (
      activeTool() === "crop" &&
      cropInteractionMode() === "modern" &&
      modernDragStart
    ) {
      if (modernDragExceededThreshold && modernDragEnd) {
        commitDragCreateFrame(
          modernDragStart.x, modernDragStart.y,
          modernDragEnd.x, modernDragEnd.y,
          e.shiftKey,
        );
      } else if (!modernCropFrame()) {
        // Click behavior →create default frame and reset canvas position to center
        const mode = cropMode();
        const ratioAspect = cropAspect();
        const sizeTarget = cropSizeTarget();
        const aspect = mode === "ratio" && ratioAspect
          ? ratioAspect
          : mode === "size" && sizeTarget && sizeTarget.w > 0 && sizeTarget.h > 0
            ? { w: sizeTarget.w, h: sizeTarget.h }
            : null;

        const scale = 1;
        const centerPanX = (viewportWidth() - docWidth() * zoom() * scale) / 2;
        const centerPanY = (viewportHeight() - docHeight() * zoom() * scale) / 2;
        setViewportState({ x: centerPanX, y: centerPanY, zoom: zoom() });
        setModernCropImageTransform({ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 });

        setModernCropFrame(getDefaultModernCropFrame({
          viewportWidth: viewportWidth(),
          viewportHeight: viewportHeight(),
          docWidth: docWidth(),
          docHeight: docHeight(),
          zoom: zoom(),
          aspect,
          panX: centerPanX,
          panY: centerPanY,
        }));
        scheduler.requestRender();
      }
      setCropDragPreview(null);
      resetModernDragState();
    }

    if (tool === "crop" && isPendingCropClick) {
      const dx = Math.abs(coords.x - interactiveState.dragStart.x);
      const dy = Math.abs(coords.y - interactiveState.dragStart.y);
      if (dx <= 2 && dy <= 2) {
        // Reset canvas position to center
        const centerPanX = (viewportWidth() - docWidth() * zoom()) / 2;
        const centerPanY = (viewportHeight() - docHeight() * zoom()) / 2;
        setViewportState({ x: centerPanX, y: centerPanY, zoom: zoom() });

        const restored = restoreHiddenCropPreview({
          cropRect,
          cropRotation,
          hiddenCropPreview,
          setCropRect,
          setCropRotation,
          setHiddenCropPreview,
        });
        if (!restored) {
          resetCropPreviewToCanvas({ engine, setCropRect, setCropRotation, setHiddenCropPreview });
        }
        scheduler.requestRender();
      }
      isPendingCropClick = false;
    }

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

  function commitDragCreateFrame(
    startX: number, startY: number, endX: number, endY: number, shiftKey: boolean,
  ) {
    const vw = viewportWidth();
    const vh = viewportHeight();
    const z = zoom();
    const p = pan();
    const snappedPreview = modernDragSnappedPreview;

    // Compute selection bounds in DOCUMENT coordinates
    let docSelW: number;
    let docSelH: number;
    let docSelCenterX: number;
    let docSelCenterY: number;

    if (snappedPreview) {
      // snappedPreview is in screen space → convert to doc coords
      docSelW = snappedPreview.w / z;
      docSelH = snappedPreview.h / z;
      docSelCenterX = (snappedPreview.x + snappedPreview.w / 2 - p.x) / z;
      docSelCenterY = (snappedPreview.y + snappedPreview.h / 2 - p.y) / z;
    } else {
      const docStartX = (startX - p.x) / z;
      const docStartY = (startY - p.y) / z;
      const docEndX = (endX - p.x) / z;
      const docEndY = (endY - p.y) / z;
      docSelW = Math.abs(docEndX - docStartX);
      docSelH = Math.abs(docEndY - docStartY);
      docSelCenterX = Math.min(docStartX, docEndX) + docSelW / 2;
      docSelCenterY = Math.min(docStartY, docEndY) + docSelH / 2;
    }

    const mode = cropMode();
    const ratioAspect = cropAspect();
    const sizeTarget = cropSizeTarget();

    let frameW: number;
    let frameH: number;

    if (mode === "free" && shiftKey) {
      const size = Math.max(docSelW, docSelH);
      frameW = size;
      frameH = size;
    } else if (mode === "free") {
      frameW = docSelW;
      frameH = docSelH;
    } else if (mode === "ratio" && ratioAspect && ratioAspect.w > 0 && ratioAspect.h > 0) {
      const ar = ratioAspect.w / ratioAspect.h;
      const area = Math.max(docSelW * docSelH, MIN_CROP_SIZE * MIN_CROP_SIZE);
      frameW = Math.sqrt(area * ar);
      frameH = frameW / ar;
    } else if (mode === "size" && sizeTarget && sizeTarget.w > 0 && sizeTarget.h > 0) {
      frameW = sizeTarget.w;
      frameH = sizeTarget.h;
    } else {
      frameW = docSelW;
      frameH = docSelH;
    }

    frameW = Math.max(MIN_CROP_SIZE, frameW);
    frameH = Math.max(MIN_CROP_SIZE, frameH);

    const clamped = clampFrameToProjectedBounds(
      { x: 0, y: 0, w: frameW, h: frameH },
      { w: docWidth(), h: docHeight() },
      MIN_CROP_SIZE,
    );

    // Center frame at viewport center in document coordinates
    const docCenterX = (vw / 2 - p.x) / z;
    const docCenterY = (vh / 2 - p.y) / z;
    const frame = {
      ...clamped,
      x: Math.round(docCenterX - clamped.w / 2),
      y: Math.round(docCenterY - clamped.h / 2),
    };

    setModernCropFrame(frame);

    // Shift image so selection center maps to viewport center (screen pixels)
    const vpCenterX = vw / 2;
    const vpCenterY = vh / 2;
    const selCenterScreenX = docSelCenterX * z + p.x;
    const selCenterScreenY = docSelCenterY * z + p.y;
    setModernCropImageTransform({
      ...modernCropImageTransform(),
      offsetX: vpCenterX - selCenterScreenX,
      offsetY: vpCenterY - selCenterScreenY,
    });
    scheduler.requestRender();
  }

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
    resetModernDragState();
    // ── Reset gradient drag state ──
    isGradientDragging = false;
    gradientDragStart = null;
    gradientDragEnd = null;
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
    resetModernDragState();
    // ── Reset gradient drag state ──
    isGradientDragging = false;
    gradientDragStart = null;
    gradientDragEnd = null;
  };

  const startSelectionRotation = () =>
    startSelectionRotationFn(
      () => selectionBox(),
      (box) => setSelectionBoxSignal(box),
      () => params.getCanvasContainerRef(),
      () => workspace.getActiveEngine(),
      () => ({ panX: pan().x, panY: pan().y, zoom: zoom() }),
    );

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
