// SPDX-License-Identifier: AGPL-3.0-or-later
import type { DocumentEngine } from "@/engine/document";
import type { CommandHistory } from "@/engine/history";
import { PAINT_SIZE_STEP_HARDNESS, paintSizeStep, adjustPaintSize, adjustPaintHardness } from "../../brushToolState";
import type { KeyboardShortcutContext } from "./context";

/**
 * Tool selection (B/E/V/M/C/G/I), brush size/hardness ([ / ]), Alt key
 * tracking, Spacebar panning, Move-tool Escape deselect + arrow nudge,
 * and Ctrl+0 fit-to-screen. `key`/`ctrl` are computed once by the caller.
 */
export function handleToolNavKey(
  ctx: KeyboardShortcutContext,
  e: KeyboardEvent,
  engine: DocumentEngine,
  history: CommandHistory,
  key: string,
  ctrl: boolean,
): boolean {
  const { editor, options } = ctx;
  const {
    scheduler,
    activeTool,
    setActiveTool,
    selectionShape,
    setSelectionShape,
    brushSize,
    setBrushSize,
    eraserSize,
    setEraserSize,
    brushHardness,
    setBrushHardness,
    eraserHardness,
    setEraserHardness,
    brushOpacity,
    brushFlow,
    brushSmoothing,
    eraserOpacity,
    eraserFlow,
    eraserSmoothing,
    selectedLayerId,
    setSelectedLayerId,
  } = editor;

  // Paint tool shortcuts
  if (!ctrl && key === "b") {
    e.preventDefault();
    setActiveTool("brush");
    scheduler.requestRender();
    return true;
  }

  if (!ctrl && key === "e") {
    e.preventDefault();
    setActiveTool("eraser");
    scheduler.requestRender();
    return true;
  }

  // Tool selection shortcuts
  if (!ctrl && key === "v") {
    e.preventDefault();
    setActiveTool("move");
    scheduler.requestRender();
    return true;
  }

  if (!ctrl && key === "m") {
    e.preventDefault();
    if (e.shiftKey) {
      // Shift+M toggles the marquee shape between rect and ellipse.
      setSelectionShape(selectionShape() === "ellipse" ? "rect" : "ellipse");
    } else {
      setSelectionShape("rect");
      setActiveTool("selection");
    }
    scheduler.requestRender();
    return true;
  }

  if (!ctrl && key === "c") {
    e.preventDefault();
    setActiveTool("crop");
    scheduler.requestRender();
    return true;
  }

  if (!ctrl && key === "g") {
    e.preventDefault();
    if (e.shiftKey) {
      setActiveTool("gradient");
    } else {
      setActiveTool("paintBucket");
    }
    scheduler.requestRender();
    return true;
  }

  if (!ctrl && key === "i") {
    e.preventDefault();
    setActiveTool("eyedropper");
    scheduler.requestRender();
    return true;
  }

  if (!ctrl && key === "u") {
    e.preventDefault();
    setActiveTool("shape");
    scheduler.requestRender();
    return true;
  }

  if (!ctrl && key === "t") {
    e.preventDefault();
    setActiveTool("text");
    scheduler.requestRender();
    return true;
  }

  if (!ctrl && (e.key === "[" || e.key === "]") && (activeTool() === "brush" || activeTool() === "eraser")) {
    e.preventDefault();
    if (e.shiftKey) {
      const delta = e.key === "[" ? -PAINT_SIZE_STEP_HARDNESS : PAINT_SIZE_STEP_HARDNESS;
      const next = adjustPaintHardness(activeTool(), {
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
      }, delta);
      setBrushHardness(next.brushHardness);
      setEraserHardness(next.eraserHardness);
    } else {
      const currentSize = activeTool() === "eraser" ? eraserSize() : brushSize();
      const step = paintSizeStep(currentSize);
      const delta = e.key === "[" ? -step : step;
      const next = adjustPaintSize(activeTool(), {
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
      }, delta);
      setBrushSize(next.brushSize);
      setEraserSize(next.eraserSize);
    }
    scheduler.requestRender();
    return true;
  }

  // Alt key tracking for eyedropper shortcut
  if (e.key === "Alt") {
    e.preventDefault();
    options.setIsAltPressed(true);
    return true;
  }

  // Spacebar panning toggle
  if (e.code === "Space") {
    e.preventDefault();
    // Blur any focused element (e.g. OptionCheckbox native input) so Space
    // doesn't toggle the control while we're trying to pan. The native checkbox
    // default behavior fires before the window keydown handler even with
    // preventDefault, so we proactively remove focus from the active element.
    (document.activeElement as HTMLElement)?.blur();
    options.stopMomentum();
    if (!options.isSpacePressed()) {
      options.setIsSpacePressed(true);
    }
    return true;
  }

  // Escape deselects layer in Move tool
  if (activeTool() === "move" && e.key === "Escape" && selectedLayerId()) {
    e.preventDefault();
    engine.setActiveLayer(null);
    setSelectedLayerId(null);
    scheduler.requestRender();
    return true;
  }

  // Keyboard nudge for Move Tool: Arrow = 1px, Shift+Arrow = 10px.
  // Works whether or not the layer transform overlay (handles/rotate ring)
  // is active, matching standard raster editors — arrow nudges the selected
  // layer 1px (10px with Shift) even while the transform session is live.
  if (activeTool() === "move" && e.key.startsWith("Arrow")) {
    const activeId = engine.getActiveLayerId();
    if (!activeId) return true;
    const layer = engine.getLayer(activeId);
    if (!layer || layer.locked) return true;

    e.preventDefault();
    const step = e.shiftKey ? 10 : 1;
    let dx = 0, dy = 0;
    if (e.key === "ArrowUp") dy = -step;
    else if (e.key === "ArrowDown") dy = step;
    else if (e.key === "ArrowLeft") dx = -step;
    else if (e.key === "ArrowRight") dx = step;

    if (!e.repeat) {
      history.commit(engine.snapshot(), "Move Layer");
    }
    engine.moveLayer(activeId, layer.transform.x + dx, layer.transform.y + dy);
    scheduler.requestRender();
    return true;
  }

  // Fit Screen Shortcuts: Ctrl + 0
  if (
    ctrl &&
    (key === "0" || e.code === "Digit0" || e.code === "Numpad0")
  ) {
    e.preventDefault();
    e.stopPropagation();
    options.stopMomentum();
    options.fitToScreenAndRender(false);
    return true;
  }

  return false;
}
