import type { DocumentEngine } from "../engine/document";
import type { CommandHistory } from "../engine/history";
import type { DocumentModel } from "../engine/types";
import type { SnapLine, SnapRect, SnapResult } from "./smartGuides";
import type { PaintToolSettings } from "@/components/editor/brushToolState";
import type { ToolId } from "@/components/editor/tools/toolTypes";
import { getLayerAabb } from "./transformGeometry";

/**
 * Clamp a freshly drawn selection rect to the document (canvas) bounds so the
 * marquee can never extend outside the canvas — matches the standard
 * raster-editor behavior (the rectangular marquee is constrained to the
 * document; only selections derived from layer/mask content may exceed it).
 */
function clampSelectionToCanvas(
  x: number,
  y: number,
  w: number,
  h: number,
  engine: DocumentEngine
): { x: number; y: number; w: number; h: number } {
  const dw = engine.getWidth();
  const dh = engine.getHeight();
  const cx = Math.max(0, Math.min(dw, x));
  const cy = Math.max(0, Math.min(dh, y));
  const cw = Math.max(0, Math.min(dw, x + w) - cx);
  const ch = Math.max(0, Math.min(dh, y + h) - cy);
  return { x: cx, y: cy, w: cw, h: ch };
}

export type ToolType = ToolId;

export interface ToolContext {
  fgColor: string;
  bgColor: string;
  brushSize: number;
  brushHardness: number;
  brushOpacity: number;
  selectedLayerId: string | null;
  isAltPressed: boolean;
  isShiftPressed?: boolean;

  // Transient interactive state
  isDragging: boolean;
  dragStart: { x: number; y: number };
  dragCurrent: { x: number; y: number };
  strokePoints: { x: number; y: number }[];
  dragTool: ToolType | null;
  /** Set by the move drag path when moveLayerSilent suppressed onChange. */
  dragMovePendingNotify?: boolean;
  // Screen-space cursor position (for HUD positioning inside the screen-space SVG overlay)
  screenPos?: { x: number; y: number };
  
  // Custom updates
  setFgColor?: (c: string) => void;
  setBgColor?: (c: string) => void;
  onSelectionCreated?: (x: number, y: number, w: number, h: number) => void;
  onCropCreated?: (x: number, y: number, w: number, h: number) => void;
  paintSettings: PaintToolSettings;
  onPaintStroke?: (
    points: { x: number; y: number }[],
    isEraser: boolean,
    settings: PaintToolSettings,
    isFinal?: boolean,
  ) => void;
  onHoverHandle?: (handle: string | null) => void;
  onComputeSnap?: (rect: SnapRect) => SnapResult;
  onSnapLines?: (lines: SnapLine[]) => void;

  // Selection move support
  selectionBounds?: { x: number; y: number; width: number; height: number; angle?: number } | null;
  onSelectionMoved?: (x: number, y: number) => void;
  onSelectionRotated?: (angle: number) => void;
  onRotateStart?: (centerX: number, centerY: number) => void;
  dragMode?: "draw" | "move-selection" | "rotate-selection" | null;
  rotateCenter?: { x: number; y: number };
  rotateStartAngle?: number;
  selectionAngle?: number;
  selectionConstraintMode?: "normal" | "ratio" | "size";
  selectionShape?: "rect" | "ellipse";
  selectionRatioW?: number;
  selectionRatioH?: number;
  selectionSizeW?: number;
  selectionSizeH?: number;

  // Deferred-history pattern: pointerDown records the pre-mutation snapshot
  // here without committing. pointerUp commits IF and ONLY IF the operation
  // actually mutated state (compared via pendingOriginal*). Prevents ghost
  // undo entries on click-without-drag (@regression 2026-06-18 — user-visible
  // as "history kadang ke-save kadang tidak" because consecutive ghost entries
  // make undo appear to skip steps with no visual change).
  pendingHistorySnapshot?: DocumentModel | null;
  pendingOriginalLayerPos?: { x: number; y: number } | null;
  pendingOriginalSelectionPos?: { x: number; y: number } | null;

  // Brush shift (Shift+drag straight line): the `lastPaintCoords` anchor that
  // existed BEFORE this stroke began. Captured at pointerDown and committed as
  // the undo snapshot value so undo restores the pre-stroke anchor
  // deterministically, independent of `commitBrushStroke`'s async timing.
  brushStrokeAnchor?: { x: number; y: number } | null;
}

export function handlePointerDown(
  tool: ToolType,
  docX: number,
  docY: number,
  engine: DocumentEngine,
  history: CommandHistory,
  requestRender: () => void,
  context: ToolContext
): void {
  context.dragTool = tool;
  context.isDragging = true;
  context.dragStart = { x: docX, y: docY };
  context.dragCurrent = { x: docX, y: docY };

  // Defensive clear: a previous pointerdown might have stashed pending
  // history state and crashed/cancelled before pointerup. Always start
  // fresh so we never commit a stale snapshot from an unrelated gesture.
  context.pendingHistorySnapshot = null;
  context.pendingOriginalLayerPos = null;
  context.pendingOriginalSelectionPos = null;

  if (tool === "selection") {
    const bounds = context.selectionBounds;
    if (bounds && isPointInSelection(docX, docY, bounds)) {
      // Stash pre-move snapshot for pointerUp. Do NOT commit here — a click
      // that never drags should not produce an undo entry.
      context.pendingHistorySnapshot = engine.snapshot();
      context.pendingOriginalSelectionPos = { x: bounds.x, y: bounds.y };
      context.dragMode = "move-selection";
      context.dragStart = { x: docX - bounds.x, y: docY - bounds.y };
    } else {
      context.dragMode = "draw";
      if (context.selectionConstraintMode === "size") {
        const sw = context.selectionSizeW ?? 100;
        const sh = context.selectionSizeH ?? 100;
          const x = docX - sw / 2;
          const y = docY - sh / 2;
          const r = clampSelectionToCanvas(x, y, sw, sh, engine);
          context.onSelectionCreated?.(r.x, r.y, r.w, r.h);
      } else {
        context.onSelectionCreated?.(docX, docY, 0, 0);
      }
    }
  } else if (tool === "crop") {
    context.onCropCreated?.(docX, docY, 0, 0);
  } else if (tool === "brush" || tool === "eraser") {
    if (!context.strokePoints || context.strokePoints.length === 0) {
      context.strokePoints = [{ x: docX, y: docY }];
    }
    context.onPaintStroke?.(context.strokePoints, tool === "eraser", context.paintSettings);
  } else if (tool === "eyedropper") {
    const color = engine.samplePixel(docX, docY);
    const hex = rgbToHex(color[0], color[1], color[2]);
    context.setFgColor?.(hex);
  } else if (tool === "move" && context.selectedLayerId) {
      const layer = engine.getLayer(context.selectedLayerId);
      if (layer && !layer.locked && !layer.lockPosition && !layer.isBackground) {
      // Stash pre-move snapshot for pointerUp. Do NOT commit here — a click
      // that never drags should not produce an undo entry. (history param is
      // intentionally not used in this branch; pointerUp commits if moved.)
      void history;
      context.pendingHistorySnapshot = engine.snapshot();
      context.pendingOriginalLayerPos = { x: layer.transform.x, y: layer.transform.y };
      context.dragStart = { x: docX - layer.transform.x, y: docY - layer.transform.y };
    }
  }
  requestRender();
}

export function handlePointerMove(
  _tool: ToolType,
  docX: number,
  docY: number,
  engine: DocumentEngine,
  requestRender: () => void,
  context: ToolContext
): void {
  if (!context.isDragging) return;
  context.dragCurrent = { x: docX, y: docY };

  const tool = context.dragTool ?? _tool;
  if (tool === "selection") {
    if (context.dragMode === "move-selection") {
      const newX = docX - context.dragStart.x;
      const newY = docY - context.dragStart.y;
      context.onSelectionMoved?.(newX, newY);
    } else if (context.dragMode === "rotate-selection" && context.rotateCenter) {
      const cx = context.rotateCenter.x;
      const cy = context.rotateCenter.y;
      const startAngle = context.rotateStartAngle ?? 0;
      const currentAngle = Math.atan2(docY - cy, docX - cx) * (180 / Math.PI);
      let newAngle = currentAngle - startAngle;
      if (context.isShiftPressed) {
        newAngle = Math.round(newAngle / 15) * 15;
      }
      newAngle = ((newAngle % 360) + 360) % 360;
      if (newAngle > 180) newAngle -= 360;
      context.onSelectionRotated?.(newAngle);
    } else {
      const centerX = context.dragStart.x;
      const centerY = context.dragStart.y;
      const dx = docX - centerX;
      const dy = docY - centerY;

      let w = Math.abs(dx);
      let h = Math.abs(dy);

      if (context.selectionConstraintMode === "size") {
        const sw = context.selectionSizeW ?? 100;
        const sh = context.selectionSizeH ?? 100;
        w = sw;
        h = sh;
        let x = centerX - w / 2;
        let y = centerY - h / 2;
        const r = clampSelectionToCanvas(x, y, w, h, engine);
        context.onSelectionCreated?.(r.x, r.y, r.w, r.h);
      } else if (context.selectionConstraintMode === "ratio") {
        const rw = context.selectionRatioW ?? 1;
        const rh = context.selectionRatioH ?? 1;
        if (rw > 0 && rh > 0) {
          const targetAspect = rw / rh;
          const currentAspect = w / h;
          if (currentAspect > targetAspect) {
            h = w / targetAspect;
          } else {
            w = h * targetAspect;
          }
        }
        let x: number;
        let y: number;
        if (context.isAltPressed) {
          w *= 2;
          h *= 2;
          x = centerX - w / 2;
          y = centerY - h / 2;
        } else {
          x = dx >= 0 ? centerX : centerX - w;
          y = dy >= 0 ? centerY : centerY - h;
        }
        const r = clampSelectionToCanvas(x, y, w, h, engine);
        context.onSelectionCreated?.(r.x, r.y, r.w, r.h);
      } else {
        if (context.isShiftPressed) {
          const side = Math.max(w, h);
          w = side;
          h = side;
        }

        if (context.isAltPressed) {
          w *= 2;
          h *= 2;
        }

        let x: number;
        let y: number;

        if (context.isAltPressed) {
          x = centerX - w / 2;
          y = centerY - h / 2;
        } else {
          x = Math.min(centerX, docX);
          y = Math.min(centerY, docY);
        }

        const r = clampSelectionToCanvas(x, y, w, h, engine);
        context.onSelectionCreated?.(r.x, r.y, r.w, r.h);
      }
    }
  } else if (tool === "crop") {
    const x = Math.min(context.dragStart.x, docX);
    const y = Math.min(context.dragStart.y, docY);
    const w = Math.abs(context.dragStart.x - docX);
    const h = Math.abs(context.dragStart.y - docY);
    context.onCropCreated?.(x, y, w, h);
  } else if (tool === "brush" || tool === "eraser") {
    context.strokePoints.push({ x: docX, y: docY });
    context.onPaintStroke?.(context.strokePoints, tool === "eraser", context.paintSettings);
  } else if (tool === "eyedropper") {
    const color = engine.samplePixel(docX, docY);
    const hex = rgbToHex(color[0], color[1], color[2]);
    context.setFgColor?.(hex);
  } else if (tool === "move" && context.selectedLayerId) {
      const layer = engine.getLayer(context.selectedLayerId);
      if (layer && !layer.locked && !layer.lockPosition && !layer.isBackground) {
      const newX = docX - context.dragStart.x;
      const newY = docY - context.dragStart.y;
      let nextX = newX;
      let nextY = newY;
      if (!context.isAltPressed && context.onComputeSnap) {
        const baseAabb = getLayerAabb(layer.transform, layer.width, layer.height);
        const snap = context.onComputeSnap({
          x: baseAabb.x + (newX - layer.transform.x),
          y: baseAabb.y + (newY - layer.transform.y),
          w: baseAabb.width,
          h: baseAabb.height,
        });
        nextX += snap.dx;
        nextY += snap.dy;
        context.onSnapLines?.(snap.lines);
      } else {
        context.onSnapLines?.([]);
      }
      // Silent move during drag: engine.moveLayer() fires onChange per
      // pointermove (50+ fps) → workspace sync per frame. Use the silent
      // variant and flush ONE notification on pointer-up. The layer model
      // still updates every move (renderer reads it directly).
      engine.moveLayerSilent(context.selectedLayerId, nextX, nextY);
      context.dragMovePendingNotify = true;
    }
  }
  requestRender();
}

/**
 * Creates a selection, preserving backward-compatible arity: rect selections
 * use the 4-arg form, ellipse selections append the shape so the engine
 * stores `shape: "ellipse"`.
 */
function commitSelection(
  engine: DocumentEngine,
  r: { x: number; y: number; w: number; h: number },
  shape: "rect" | "ellipse",
): void {
  if (shape === "ellipse") {
    engine.createSelection(r.x, r.y, r.w, r.h, 0, "ellipse");
  } else {
    engine.createSelection(r.x, r.y, r.w, r.h);
  }
}

export function handlePointerUp(
  _tool: ToolType,
  docX: number,
  docY: number,
  engine: DocumentEngine,
  history: CommandHistory,
  requestRender: () => void,
  context: ToolContext
): void {
  if (!context.isDragging) return;
  context.isDragging = false;
  context.dragCurrent = { x: docX, y: docY };

  const tool = context.dragTool ?? _tool;
  if (tool === "selection") {
    if (context.dragMode === "move-selection") {
      const newX = docX - context.dragStart.x;
      const newY = docY - context.dragStart.y;
      context.onSelectionMoved?.(newX, newY);

      // Commit deferred history snapshot ONLY if the selection actually moved.
      // Click-without-drag must not produce an undo entry.
      const pending = context.pendingHistorySnapshot;
      const orig = context.pendingOriginalSelectionPos;
      if (pending && orig && (newX !== orig.x || newY !== orig.y)) {
        history.commit(pending);
      }
      context.pendingHistorySnapshot = null;
      context.pendingOriginalSelectionPos = null;
    } else if (context.dragMode === "rotate-selection") {
      // Rotation is already applied live via onSelectionRotated in move handler
    } else {
      const centerX = context.dragStart.x;
      const centerY = context.dragStart.y;
      const dx = docX - centerX;
      const dy = docY - centerY;

      let w = Math.abs(dx);
      let h = Math.abs(dy);

      if (context.selectionConstraintMode === "size") {
        const sw = context.selectionSizeW ?? 100;
        const sh = context.selectionSizeH ?? 100;
        w = sw;
        h = sh;
        let x = centerX - w / 2;
        let y = centerY - h / 2;
        const r = clampSelectionToCanvas(x, y, w, h, engine);
        commitSelection(engine, r, context.selectionShape ?? "rect");
      } else if (context.selectionConstraintMode === "ratio") {
        const rw = context.selectionRatioW ?? 1;
        const rh = context.selectionRatioH ?? 1;
        if (rw > 0 && rh > 0) {
          const targetAspect = rw / rh;
          const currentAspect = w / h;
          if (currentAspect > targetAspect) {
            h = w / targetAspect;
          } else {
            w = h * targetAspect;
          }
        }
        let x: number;
        let y: number;
        if (context.isAltPressed) {
          w *= 2;
          h *= 2;
          x = centerX - w / 2;
          y = centerY - h / 2;
        } else {
          x = dx >= 0 ? centerX : centerX - w;
          y = dy >= 0 ? centerY : centerY - h;
        }
        if (w > 2 && h > 2) {
          const r = clampSelectionToCanvas(x, y, w, h, engine);
        commitSelection(engine, r, context.selectionShape ?? "rect");
        } else {
          engine.clearSelection();
        }
      } else {
        if (context.isShiftPressed) {
          const side = Math.max(w, h);
          w = side;
          h = side;
        }

        if (context.isAltPressed) {
          w *= 2;
          h *= 2;
        }

        let x: number;
        let y: number;

        if (context.isAltPressed) {
          x = centerX - w / 2;
          y = centerY - h / 2;
        } else {
          x = Math.min(centerX, docX);
          y = Math.min(centerY, docY);
        }

        if (w > 2 && h > 2) {
          const r = clampSelectionToCanvas(x, y, w, h, engine);
        commitSelection(engine, r, context.selectionShape ?? "rect");
        } else {
          engine.clearSelection();
        }
      }
    }
    context.dragMode = null;
  } else if (tool === "brush" || tool === "eraser") {
    if (context.selectedLayerId && context.strokePoints.length > 0) {
      const lastPoint = context.strokePoints.at(-1);
      if (!lastPoint || lastPoint.x !== docX || lastPoint.y !== docY) {
        context.strokePoints.push({ x: docX, y: docY });
      }
      context.onPaintStroke?.(context.strokePoints, tool === "eraser", context.paintSettings, true);
    }
    context.strokePoints = [];
  } else if (tool === "crop") {
    const x = Math.min(context.dragStart.x, docX);
    const y = Math.min(context.dragStart.y, docY);
    const w = Math.abs(context.dragStart.x - docX);
    const h = Math.abs(context.dragStart.y - docY);
    if (w > 2 && h > 2) {
      context.onCropCreated?.(x, y, w, h);
    }
  } else if (tool === "move") {
    // History is NOT committed here because the SVG overlay
    // (SelectionTransformOverlay, z-index 40) intercepts most canvas clicks
    // before they reach input-handler — so pendingHistorySnapshot is often
    // null. useCanvasLayerDrag.onPointerUp owns all move-tool history.
    // We still clean up the pending snapshot and snap lines so stale state
    // doesn't leak into the next gesture.
    context.pendingHistorySnapshot = null;
    context.pendingOriginalLayerPos = null;
    context.onSnapLines?.([]);
    // Flush the deferred onChange from moveLayerSilent drag moves. Note:
    // a pointer-cancel mid-drag skips this branch; the dirty flag is already
    // set in the model, so the next interaction notifies. (Pointer cancel is
    // rare and only delays tab-dirty sync, never loses data.)
    if (context.dragMovePendingNotify) {
      engine.flushChangeNotification();
      context.dragMovePendingNotify = false;
    }
  }
  requestRender();
}

// Hex converter helper
function rgbToHex(r: number, g: number, b: number): string {
  const toHex = (c: number) => {
    const hex = Math.max(0, Math.min(255, c)).toString(16);
    return hex.length === 1 ? "0" + hex : hex;
  };
  return "#" + toHex(r) + toHex(g) + toHex(b);
}

export function isPointInSelection(
  px: number,
  py: number,
  bounds: { x: number; y: number; width: number; height: number; angle?: number },
): boolean {
  const angle = bounds.angle ?? 0;
  if (angle === 0) {
    // Fast path for axis-aligned selections
    return (
      px >= bounds.x &&
      px <= bounds.x + bounds.width &&
      py >= bounds.y &&
      py <= bounds.y + bounds.height
    );
  }
  // Rotated selection: inverse-rotate the test point around the selection
  // center by -angle, then check against the axis-aligned bounds.
  const cx = bounds.x + bounds.width / 2;
  const cy = bounds.y + bounds.height / 2;
  const rad = -angle * Math.PI / 180;
  const cos = Math.cos(rad);
  const sin = Math.sin(rad);
  const dx = px - cx;
  const dy = py - cy;
  const rx = dx * cos - dy * sin + cx;
  const ry = dx * sin + dy * cos + cy;
  return (
    rx >= bounds.x &&
    rx <= bounds.x + bounds.width &&
    ry >= bounds.y &&
    ry <= bounds.y + bounds.height
  );
}
