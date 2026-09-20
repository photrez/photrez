import type { DocumentEngine } from "@/engine/document";
import type { CommandHistory } from "@/engine/history";
import type { LayerNode } from "@/engine/types";
import { PAINT_SIZE_STEP_HARDNESS, paintSizeStep, adjustPaintSize, adjustPaintHardness } from "../../brushToolState";
import {
  decideTransformSetRoute,
  routeNumericTransformBatch,
} from "../../layers/transformRouting";
import type { KeyboardShortcutContext } from "./context";

/**
 * Tool selection (B/E/V/M/C/G/I), brush size/hardness ([ / ]), Alt key
 * tracking, Spacebar panning, Move-tool Escape deselect + arrow nudge,
 * and Ctrl+0 fit-to-screen. `key`/`ctrl` are computed once by the caller.
 */

// Open legacy nudge burst: held-arrow repeats write the model silently and
// the workspace/Rust sync fires once at burst end. One engine at a time.
// Cleared by keyup, window blur, or the next non-repeat key that reaches
// this handler. Keys taken by an earlier handler close the burst at keyup
// or blur instead, so a missed keyup cannot strand the sync.
let pendingNudgeEngine: DocumentEngine | null = null;

// Fire the deferred sync for an open nudge burst. Safe with none open.
export function flushPendingNudge(): void {
  const engine = pendingNudgeEngine;
  pendingNudgeEngine = null;
  if (engine) engine.flushChangeNotification();
}
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
    selectedLayerIds,
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

  // A key reaching this handler that does not continue the burst closes it
  // first, so a missed keyup cannot strand the sync (tool switch and Escape
  // included). Runs before the new key commits, keeping the flush before
  // the snapshot. Keys taken by earlier handlers never reach this flush
  // and close the burst at keyup or blur instead.
  if (!(e.key.startsWith("Arrow") && e.repeat) && pendingNudgeEngine) {
    flushPendingNudge();
  }

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

  // Escape deselects layer(s) in Move tool
  if (activeTool() === "move" && e.key === "Escape" && (selectedLayerId() || (selectedLayerIds && selectedLayerIds().length > 0))) {
    e.preventDefault();
    engine.setActiveLayer(null);
    setSelectedLayerId(null);
    scheduler.requestRender();
    return true;
  }

  // Keyboard nudge for Move Tool: Arrow = 1px, Shift+Arrow = 10px.
  // Works whether or not the layer transform overlay (handles/rotate ring)
  // is active, matching standard raster editors — arrow nudges the selected
  // layer(s) 1px (10px with Shift) even while the transform session is live.
  if (activeTool() === "move" && e.key.startsWith("Arrow")) {
    const multiIds = selectedLayerIds ? selectedLayerIds() : [];
    const activeId = engine.getActiveLayerId();
    if (!activeId && multiIds.length === 0) return true;

    const targetIds = multiIds.length > 0 ? multiIds : (activeId ? [activeId] : []);
    const layersToNudge = targetIds
      .map((id) => engine.getLayer(id))
      .filter((l): l is LayerNode => Boolean(l) && !l!.locked && !l!.lockPosition);

    if (layersToNudge.length === 0) return true;

    e.preventDefault();
    const step = e.shiftKey ? 10 : 1;
    let dx = 0, dy = 0;
    if (e.key === "ArrowUp") dy = -step;
    else if (e.key === "ArrowDown") dy = step;
    else if (e.key === "ArrowLeft") dx = -step;
    else if (e.key === "ArrowRight") dx = step;

    // Relative + repeatable, so the routed patch is a resolver: a hop must add the
    // step to the transform the PREVIOUS hop projected. A position read at keydown
    // would make every key in a repeat burst send the same absolute x/y, and the
    // unchanged-value guard would silently drop all but the first.
    const nudgeIds = layersToNudge.map((l) => l.id);
    if (decideTransformSetRoute(nudgeIds) !== "legacy") {
      void routeNumericTransformBatch(
        engine,
        nudgeIds.map((id) => ({
          layerId: id,
          patch: (current) => ({ x: current.x + dx, y: current.y + dy }),
        })),
        {
          requestRender: () => scheduler.requestRender(),
          notifyVisualChange: () => editor.workspace.notifyVisualChange(),
        },
      );
      return true;
    }

    // Commit when no burst is open, not on the first-press flag: the first
    // keydown seen here can already be marked repeat when focus returns
    // while the key is held, and skipping the commit then would leave
    // silent moves with no history entry.
    if (!pendingNudgeEngine) {
      history.commit(engine.snapshot(), layersToNudge.length > 1 ? "Move Layers" : "Move Layer");
    }
    for (const l of layersToNudge) {
      engine.moveLayerSilent(l.id, l.transform.x + dx, l.transform.y + dy);
    }
    pendingNudgeEngine = engine;
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
