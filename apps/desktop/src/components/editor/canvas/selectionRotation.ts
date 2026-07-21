import { screenToDocument } from "@/viewport/coords";
import type { ViewportState } from "@/engine/types";

export interface SelectionBox {
  x: number;
  y: number;
  w: number;
  h: number;
  angle: number;
  shape?: "rect" | "ellipse";
  inverted?: boolean;
}

/**
 * Fresh viewport coords to use INSTEAD of engine.getViewport().
 * engine.getViewport() goes stale during panning because usePanNavigation
 * skips engine.setViewport to avoid triggering layer re-selection (which
 * calls setSelectedLayerId as a side effect).
 * 
 * When getViewportCoords is provided, the function uses these always-fresh
 * pan/zoom values for coordinate conversion, preventing the rotation angle
 * from being computed against a stale viewport.
 */
export type ViewportCoordsProvider = () => { panX: number; panY: number; zoom: number };

export function startSelectionRotation(
  getSelectionBox: () => SelectionBox | null,
  setSelectionBox: (box: SelectionBox | null) => void,
  getContainerRef: () => HTMLDivElement | undefined,
  getActiveEngine: () => { getViewport(): ViewportState; createSelection(x: number, y: number, w: number, h: number, angle?: number, shape?: "rect" | "ellipse"): void } | null,
  getViewportCoords?: ViewportCoordsProvider,
): void {
  const box = getSelectionBox();
  if (!box) return;
  const engine = getActiveEngine();
  if (!engine) return;

  const centerX = box.x + box.w / 2;
  const centerY = box.y + box.h / 2;
  // Lazily capture the reference angle on first pointermove instead of hardcoding
  // to 180° (which caused the selection to jump to a wrong angle on every rotation
  // drag — bug: initialAngle was Math.atan2(0,-1)*180/π = 180 instead of the actual
  // angle from center to cursor at pointerdown).
  let referenceAngle: number | null = null;

  const handleDocPointerMove = (e: PointerEvent) => {
    const rect = getContainerRef()?.getBoundingClientRect();
    if (!rect) return;
    // Use fresh viewport coords if provided, otherwise fall back to engine (backward compat).
    // engine.getViewport() can be stale after panning (usePanNavigation skips
    // engine.setViewport to avoid triggering layer re-selection).
    const vp = getViewportCoords
      ? getViewportCoords()
      : engine.getViewport();
    const docPt = screenToDocument(e.clientX, e.clientY, rect, { ...vp, rotation: 0 });
    const currentAngle = Math.atan2(docPt.y - centerY, docPt.x - centerX) * (180 / Math.PI);

    if (referenceAngle === null) {
      referenceAngle = currentAngle;
      // First move: delta = 0, so angle = existing angle — no visual jump.
    }

    const delta = currentAngle - referenceAngle;
    let angle = box.angle + delta;
    if (e.shiftKey) {
      angle = Math.round(angle / 15) * 15;
    }
    angle = ((angle % 360) + 360) % 360;
    if (angle > 180) angle -= 360;
    setSelectionBox({ ...box, angle });
  };

  const handleDocPointerUp = (e: PointerEvent) => {
    document.removeEventListener("pointermove", handleDocPointerMove);
    document.removeEventListener("pointerup", handleDocPointerUp, true);
    // Persist the final rotation to the engine so it survives tool switches,
    // undo/redo, and pointerUp syncs (otherwise the rotation was only visual).
    const finalBox = getSelectionBox();
    if (finalBox) {
      engine.createSelection(finalBox.x, finalBox.y, finalBox.w, finalBox.h, finalBox.angle, finalBox.shape);
      // Re-sync the signal AFTER committing to engine so any concurrent
      // canvas-element pointerUp handler that read stale engine state is
      // overwritten with the correct angle. Also request a render so the
      // visual updates immediately (Bug #1 race condition fix).
      setSelectionBox(finalBox);
    }
  };

  document.addEventListener("pointermove", handleDocPointerMove);
  // Use capture phase so this fires BEFORE the canvas-element onPointerUp.
  // This ensures engine.createSelection() has already committed the new
  // angle before the canvas handler reads engine.getSelection() — preventing
  // the stale-angle race condition (Bug #1).
  document.addEventListener("pointerup", handleDocPointerUp, true);
}
