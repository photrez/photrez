import { createSignal, onCleanup, onMount } from "solid-js";
import { useEditor } from "../shell/EditorContext";
import { useDragController } from "../DragController";
import { addLayerFromCrossDoc } from "../crossDocLayerOps";
import { showToast } from "../Toast";
import type { LayerNode } from "@/engine/types";
import { computeSnapAdjustment, type SnapRect, type SnapLine } from "@/viewport/smartGuides";
import { buildTransformSnapTargets } from "@/viewport/transformSnapTargets";
import { getLayerAabb } from "@/viewport/transformGeometry";
import { ALPHA_HIT_THRESHOLD, isBoxHittable } from "@/viewport/layerHitTest";
import type { HudMode } from "../TransformHud";
import {
  MIXED_OWNERSHIP_MESSAGE,
  clearTransformPreview,
  getFacade,
  peekFacade,
  setTransformPreview,
  type FacadeTransformPreview,
} from "@/lib/protocol/facadeRegistry";
import {
  decideTransformSetRoute,
  routeNumericTransformBatch,
  type TransformEdit,
} from "./transformRouting";

export interface LayerTransformStart {
  id: string;
  startTransformX: number;
  startTransformY: number;
  lockPosition?: boolean;
}

interface CanvasLayerDrag {
  layerId: string;
  sourceDocId: string;
  startDocX: number;
  startDocY: number;
  startTransformX: number;
  startTransformY: number;
  rect: { left: number; top: number };
  preDragSnapshot: import("@/engine/types").DocumentModel;
  selectedLayerStarts: LayerTransformStart[];
  // Pointer that took the gesture, so an unrelated pointer losing capture cannot
  // abort it.
  pointerId: number;
  // True when the dragged layers are owned by the native editor state. The model
  // is then never touched mid-drag: the pointermove channel is the transient
  // render preview, and one committed edit per layer goes out at pointerup.
  facade: boolean;
  // Offset the last pointermove previewed. The commit sends absolute values
  // derived from these, so a hover-freeze frame (which previews nothing) cannot
  // move the layer back to a half-way position.
  liveDx: number;
  liveDy: number;
}

export interface CanvasLayerDragApi {
  handlePointerDown: (e: PointerEvent) => void;
  isDragging: () => boolean;
}

export interface CanvasLayerDragOptions {
  onSnapLinesChange?: (lines: SnapLine[]) => void;
  onHudUpdate?: (hud: {
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
  } | null) => void;
  isSpacePressed?: () => boolean;
  isPanning?: () => boolean;
}

/**
 * Canvas layer drag gesture: click+drag in the canvas (Move tool) to
 * translate a layer. If released over a different document's tab, copies
 * the layer to that doc (cross-doc drag). Otherwise the layer stays at
 * the new position in the current doc.
 *
 * Snapping: when the global `moveSnapEnabled` signal is on (and Alt is
 * not held), the layer snaps to the doc bounds, the doc center lines,
 * and other visible layers' edges.
 *
 * Tab switch on hover: when the cursor enters a different document's
 * tab, the workspace switches to that tab so the user sees the target
 * canvas in real time and can choose the landing position before
 * releasing. The source docId is captured at pointerdown so the
 * cross-doc add uses the original source even after the tab switch.
 *
 * Hit testing: walks the topmost layer stack from top to bottom and
 * picks the first non-locked, non-background, visible layer whose
 * axis-aligned bounding box contains the pointer. Rotation is ignored
 * for simplicity (matches the existing layer-helpers).
 *
 * Two authority paths, decided once at pointerdown and never re-decided
 * mid-gesture:
 *  - Layers owned by the native editor state: the model is not touched during the
 *    drag. Each frame writes a transient preview the render scheduler projects onto
 *    the outgoing RenderState, and pointerup sends one committed edit per layer.
 *    The native side owns the undo entry, so no history entry is written here.
 *  - Everything else: the original behavior, one engine.transformLayer per frame
 *    and a single history entry at pointerup.
 * While a routed drag is live it holds the facade's one transient transform slot,
 * so a numeric edit issued from elsewhere in that window fails with a toast
 * instead of silently overwriting what the pointer is holding.
 */
export function useCanvasLayerDrag(opts: CanvasLayerDragOptions = {}): CanvasLayerDragApi {
  const { workspace, renderer, camera, activeDocumentId, activeTool, scheduler, moveSnapEnabled, snapToLayersEnabled, snapToCanvasEnabled, moveAutoSelect, selectedLayerId, setSelectedLayerId, selectedLayerIds, toggleLayerSelection, zoom } = useEditor();
  const dragController = useDragController();

  const [drag, setDrag] = createSignal<CanvasLayerDrag | null>(null);

  // Snap targets list the edges the moving layer can catch on. They depend
  // only on the layers that are NOT moving, so one build per gesture is
  // enough and every later move reuses it. The on/off switch and the bypass
  // keys are still read on every move, only the target list is shared. Any
  // model change from outside this gesture drops the cache, because a stale
  // list would snap to edges that already moved. The gesture's own writes
  // touch only the dragged (excluded) layers, so they are held back from
  // dropping it while they run.
  let snapCache: {
    targets: SnapRect[];
    docW: number;
    docH: number;
    snapToLayers: boolean;
    snapToCanvas: boolean;
  } | null = null;
  let snapUnsubs: Array<() => void> = [];
  let snapWritesHeld = false;
  // Last move's snap inputs + outcome. A repeat pointer report with the same
  // switches re-applies the stored deltas and skips the recompute + emits.
  let lastSnapDx: number | null = null;
  let lastSnapDy: number | null = null;
  let lastSnapBypass = false;
  let lastSnapOn = false;
  let lastSnapZoom = 0;
  let lastSnapDeltaX = 0;
  let lastSnapDeltaY = 0;
  let lastSnapActive = false;

  function resetSnapRepeat(): void {
    lastSnapDx = null;
    lastSnapDy = null;
    lastSnapDeltaX = 0;
    lastSnapDeltaY = 0;
    lastSnapActive = false;
  }

  function endSnapCache(): void {
    for (const unsub of snapUnsubs) unsub();
    snapUnsubs = [];
    snapCache = null;
    resetSnapRepeat();
  }

  function beginSnapCache(): void {
    endSnapCache();
    resetSnapRepeat();
    snapUnsubs = [
      workspace.onChange(() => {
        // Drop the stored deltas with the targets: a repeat report after this
        // point must recompute against the new list, never re-apply deltas
        // measured against the old one.
        if (!snapWritesHeld) { snapCache = null; resetSnapRepeat(); }
      }),
      workspace.onVisualChange(() => {
        if (!snapWritesHeld) { snapCache = null; resetSnapRepeat(); }
      }),
    ];
  }

  // Release everything a routed gesture holds, for any exit that does NOT commit.
  // The facade's transient transform slot is the gate the numeric commit funnel
  // refuses on, so a slot left behind by an abandoned drag turns every later
  // numeric edit of that document (option bar, nudge, align, flip) into a
  // permanent refusal. cancelTransform() is idempotent, so calling it after a
  // commit that already cleared the slot is harmless.
  function releaseFacadeDrag(d: CanvasLayerDrag): void {
    if (!d.facade) return;
    // peek, not get: a document that was already closed evicted its facade, and
    // creating one here to release a slot it no longer has would hand the next
    // document with the same id a stale empty facade.
    peekFacade(d.sourceDocId)?.cancelTransform();
    clearTransformPreview();
  }

  // Preview every dragged layer at the given offset. This is the channel the
  // canvas actually renders through: EditorShell's render scheduler applies the
  // preview to the outgoing RenderState, so the move is visible without touching
  // the model, and the selection overlay tracks it off the same signal.
  function previewDragOffset(
    engine: { getLayer(id: string): LayerNode | null | undefined },
    d: CanvasLayerDrag,
    dx: number,
    dy: number,
  ): void {
    const entries: FacadeTransformPreview[] = [];
    for (const item of d.selectedLayerStarts) {
      if (item.lockPosition) continue;
      const layer = engine.getLayer(item.id);
      if (!layer) continue;
      entries.push({
        layerId: item.id,
        transform: { ...layer.transform, x: item.startTransformX + dx, y: item.startTransformY + dy },
      });
    }
    setTransformPreview(entries);
    const primary = entries[0];
    if (primary) peekFacade(d.sourceDocId)?.updateTransform(primary.transform);
  }

  // One committed edit per dragged layer, through the same seam the option bar and
  // the arrow-key nudge use: it owns the mixed-selection rejection, the per-layer
  // serialization, the failure toast, and the post-commit render/panel refresh.
  // The native arm writes the undo entry, so no legacy history entry is added.
  function commitRoutedDrag(d: CanvasLayerDrag): void {
    const engine = workspace.getEngine(d.sourceDocId);
    if (!engine) return;
    const edits: TransformEdit[] = [];
    for (const item of d.selectedLayerStarts) {
      if (item.lockPosition) continue;
      edits.push({
        layerId: item.id,
        patch: { x: item.startTransformX + d.liveDx, y: item.startTransformY + d.liveDy },
      });
    }
    if (edits.length === 0) return;
    void routeNumericTransformBatch(engine, edits, {
      requestRender: () => scheduler.requestRender(),
      notifyVisualChange: () => workspace.notifyVisualChange(),
    });
  }

  function detachDragListeners() {
    document.removeEventListener("pointermove", onPointerMove);
    document.removeEventListener("pointerup", onPointerUp);
    document.removeEventListener("pointercancel", onPointerCancel);
  }

  /** End the gesture without applying it: preview away, listeners gone, drag over. */
  function cancelDrag(): void {
    const d = drag();
    if (!d) return;
    detachDragListeners();
    endSnapCache();
    opts.onSnapLinesChange?.([]);
    opts.onHudUpdate?.(null);
    releaseFacadeDrag(d);

    const src = d.sourceDocId ?? activeDocumentId();
    if (src && !d.facade) {
      // Legacy: the model was mutated every frame, so the start transform has to be
      // written back. Under the routed path it never was, so dropping the preview
      // above IS the revert.
      const sourceEngine = workspace.getEngine(src);
      if (sourceEngine) {
        for (const item of d.selectedLayerStarts) {
          sourceEngine.transformLayer(item.id, {
            x: item.startTransformX,
            y: item.startTransformY,
          });
        }
        scheduler.requestRender();
      }
    }
    dragController.setDropTarget(null);
    setDrag(null);
    dragController.endDrag();
  }

  function findLayerAt(docX: number, docY: number): LayerNode | null {
    const engine = workspace.getActiveEngine();
    if (!engine) return null;
    const ls = engine.getLayers();
    for (let i = 0; i < ls.length; i++) {
      const layer = ls[i];
      // Skip fully locked or invisible layers. Position-locked and Background
      // layers ARE pickable — they can still be cross-doc copied even though
      // same-doc movement is blocked by engine guards (transformLayer checks
      // lockPosition, deleteLayer checks isBackground).
      if (layer.locked || !layer.visible) continue;
      const w = layer.width * Math.abs(layer.transform.scaleX);
      const h = layer.height * Math.abs(layer.transform.scaleY);
      if (
        docX >= layer.transform.x &&
        docX <= layer.transform.x + w &&
        docY >= layer.transform.y &&
        docY <= layer.transform.y + h
      ) {
        // Alpha-aware: transparent pixels fall through to the layer below,
        // matching the canvas click-select path (handleMoveAutoSelect). Two
        // hit-tests with different rules made the drag target diverge from
        // the panel selection (@bug 2026-08-03: "drag layer 2 but panel
        // selects layer background"). Parametric (text/shape) layers are
        // excluded from the alpha check so their box is the drag target —
        // same contract as isBoxHittable in layerHitTest.
        if (!isBoxHittable(layer) && engine.sampleLayerAlpha(layer.id, docX, docY) < ALPHA_HIT_THRESHOLD) {
          continue;
        }
        return layer;
      }
    }
    return null;
  }

  function onPointerMove(e: PointerEvent) {
    const d = drag();
    if (!d) return;

    // Check for tab hover FIRST. A tab hover is a drop-target action, not
    // a canvas move action, so pause at the current visual position while
    // the user aims at a tab or waits for hover-to-switch.
    const el = document.elementFromPoint(e.clientX, e.clientY);
    const tabEl = el?.closest("[data-document-tab]") as HTMLElement | null;
    const tabId = tabEl?.getAttribute("data-document-tab") ?? null;
    const currentActive = activeDocumentId();

    if (tabId) {
      dragController.setDropTarget({ type: "tab", docId: tabId });
      if (tabId !== currentActive) {
        dragController.startTabHover(tabId);
      } else {
        dragController.cancelTabHover();
      }
      opts.onSnapLinesChange?.([]);
      opts.onHudUpdate?.(null);
      return;
    }

    const engine = workspace.getActiveEngine();
    if (!engine) return;
    const layer = engine.getLayer(d.layerId);
    if (!layer) {
      // Cross-doc: the active document was switched to the target via
      // tab hover-to-switch, so the dragged source layer no longer lives
      // in this engine. Don't mutate it — mark the canvas as a cross-doc
      // drop target so pointerup adds the layer at the cursor
      // (plan: "drag A → canvas B → added at cursor"). Only when the
      // active doc actually differs from the drag's captured source doc.
      if (d.sourceDocId !== activeDocumentId()) {
        dragController.setDropTarget({ type: "canvas" });
        dragController.cancelTabHover();
        opts.onSnapLinesChange?.([]);
        opts.onHudUpdate?.(null);
        scheduler.requestRender();
        return;
      }
      // The layer is gone from its own document (deleted, or its identity changed
      // mid-gesture), so no pointerup can commit it and the routed gesture would
      // keep holding the facade slot forever.
      if (d.facade) cancelDrag();
      return;
    }

    // Same-doc (or no tab): mutate the source layer with snap.
    const docPos = camera.screenToDocument(
      e.clientX - d.rect.left,
      e.clientY - d.rect.top,
    );
    const dx = docPos.x - d.startDocX;
    const dy = docPos.y - d.startDocY;

    // Position-locked layers can only be cross-doc copied, not same-doc
    // moved. Skip snap and transformLayer — engine blocks transformLayer
    // (lockPosition) so snap guides would fire with no visible movement.
    if (layer.lockPosition) {
      opts.onSnapLinesChange?.([]);
      opts.onHudUpdate?.(null);
      return;
    }

    let newX = d.startTransformX + dx;
    let newY = d.startTransformY + dy;

    const bypassSnap = e.ctrlKey || e.metaKey;
    const snapOn = moveSnapEnabled();
    const zoomLevel = zoom();
    let snapActive = false;
    let snapRepeated = false;
    if (
      dx === lastSnapDx &&
      dy === lastSnapDy &&
      bypassSnap === lastSnapBypass &&
      snapOn === lastSnapOn &&
      zoomLevel === lastSnapZoom
    ) {
      // Repeat report: the snap outcome and both emits below match the
      // previous move exactly, so re-apply the stored deltas and skip the
      // recompute + notifications. Model writes and the render continue.
      newX += lastSnapDeltaX;
      newY += lastSnapDeltaY;
      snapActive = lastSnapActive;
      snapRepeated = true;
    } else if (!bypassSnap && snapOn) {
      const docW = engine.getWidth();
      const docH = engine.getHeight();
      const aabb = getLayerAabb(layer.transform, layer.width, layer.height);
      const baseX = aabb.x;
      const baseY = aabb.y;
      const targetAabbX = newX - layer.transform.x;
      const targetAabbY = newY - layer.transform.y;
      const rect: SnapRect = {
        x: baseX + targetAabbX,
        y: baseY + targetAabbY,
        w: aabb.width,
        h: aabb.height,
        kind: "layer",
      };
      const snapToLayers = typeof snapToLayersEnabled === "function" ? snapToLayersEnabled() : true;
      const snapToCanvas = typeof snapToCanvasEnabled === "function" ? snapToCanvasEnabled() : true;
      const excludeIds = d.selectedLayerStarts.map((s) => s.id);
      const cached = snapCache;
      let snapTargets: SnapRect[];
      if (
        cached &&
        cached.docW === docW &&
        cached.docH === docH &&
        cached.snapToLayers === snapToLayers &&
        cached.snapToCanvas === snapToCanvas
      ) {
        snapTargets = cached.targets;
      } else {
        snapTargets = buildTransformSnapTargets(engine, docW, docH, {
          excludeLayerId: layer.id,
          excludeLayerIds: excludeIds,
          snapToLayers,
          snapToCanvas,
        });
        snapCache = { targets: snapTargets, docW, docH, snapToLayers, snapToCanvas };
      }
      const result = computeSnapAdjustment(rect, snapTargets, 8, zoomLevel);
      newX += result.dx;
      newY += result.dy;
      snapActive = result.lines.length > 0;
      lastSnapDeltaX = result.dx;
      lastSnapDeltaY = result.dy;
      lastSnapActive = snapActive;
      opts.onSnapLinesChange?.(result.lines);
    } else {
      lastSnapDeltaX = 0;
      lastSnapDeltaY = 0;
      lastSnapActive = false;
      opts.onSnapLinesChange?.([]);
    }
    lastSnapDx = dx;
    lastSnapDy = dy;
    lastSnapBypass = bypassSnap;
    lastSnapOn = snapOn;
    lastSnapZoom = zoomLevel;

    const actualDx = newX - d.startTransformX;
    const actualDy = newY - d.startTransformY;

    d.liveDx = actualDx;
    d.liveDy = actualDy;

    if (d.facade) {
      // Zero protocol calls and zero model writes per frame: the pointer moves a
      // transient preview the renderer projects onto the outgoing RenderState.
      previewDragOffset(engine, d, actualDx, actualDy);
    } else {
      // Held back from the snap-cache drop above: these writes touch only the
      // dragged layers, which are excluded from the targets, so the held list
      // stays correct. The hold lifts in the finally, so a throw cannot wedge
      // it open and hide a later outside change.
      snapWritesHeld = true;
      try {
        for (const item of d.selectedLayerStarts) {
          if (item.lockPosition) continue;
          engine.transformLayer(item.id, {
            x: item.startTransformX + actualDx,
            y: item.startTransformY + actualDy,
          });
        }
      } finally {
        snapWritesHeld = false;
      }
    }
    // The single render request for this move. The scheduler joins repeats
    // for the frame, so nothing below may add a second one for the same move.
    scheduler.requestRender();

    // Repeat reports carry the identical payload, already emitted.
    if (!snapRepeated) {
      opts.onHudUpdate?.({
        mode: "move",
        clientX: e.clientX,
        clientY: e.clientY,
        deltaX: actualDx,
        deltaY: actualDy,
        width: layer.width,
        height: layer.height,
        scalePercent: 100,
        angle: layer.transform.rotation,
        snapActive,
      });
    }

    // Update drop target for non-cross-doc hover.
    const tabBarEl = el?.closest("[data-tab-bar-empty]") as HTMLElement | null;
    if (tabBarEl) {
      dragController.setDropTarget({ type: "tab-empty" });
    } else {
      dragController.setDropTarget(null);
    }
    dragController.cancelTabHover();
  }

  function onPointerUp(e: PointerEvent) {
    const d = drag();
    if (!d) return;

    detachDragListeners();
    endSnapCache();

    opts.onSnapLinesChange?.([]);
    opts.onHudUpdate?.(null);

    // The slot and the preview come down FIRST. The numeric funnel refuses a commit
    // while the slot is held, so the routed commit below has to run after the
    // release, and every exit from this function (including the no-active-engine
    // path that skips the commit branch) has to be covered by it.
    releaseFacadeDrag(d);

    const dropTarget = dragController.state().dropTarget;
    // Use the source docId captured at pointerdown, NOT the current
    // activeDocumentId →the user may have switched tab during drag.
    const src = d.sourceDocId;
    const currentActive = activeDocumentId();

    let crossDocAdded = false;
    const isCrossDocTab =
      dropTarget?.type === "tab" && dropTarget.docId !== src;
    const isCrossDocCanvas =
      dropTarget?.type === "canvas" && currentActive !== src;

    if (isCrossDocTab || isCrossDocCanvas) {
      const targetDocId = (isCrossDocTab
        ? (dropTarget as { type: "tab"; docId: string }).docId
        : currentActive) as string;
      const sourceEngine = workspace.getEngine(src);
      const sourceLayer = sourceEngine?.getLayer(d.layerId);
      if (sourceLayer && sourceEngine) {
        dragController.cancelTabHover();

        // Switch the active doc FIRST so the camera/active engine are in
        // the target's coordinate system before we map the cursor.
        if (currentActive !== targetDocId) {
          workspace.switchDocument(targetDocId);
        }

        // Position: canvas drop → addLayerFromCrossDoc centers the layer
        // under the cursor (raw document coords); tab drop → tab has no
        // canvas cursor, so addLayerFromCrossDoc centers in the doc.
        let targetPos: { x: number; y: number };
        if (isCrossDocCanvas) {
          const cursorInCanvas = {
            x: e.clientX - d.rect.left,
            y: e.clientY - d.rect.top,
          };
          targetPos = camera.screenToDocument(cursorInCanvas.x, cursorInCanvas.y);
        } else {
          targetPos = { x: 0, y: 0 };
        }

        const { newLayerId } = addLayerFromCrossDoc(
          {
            version: 1,
            sourceDocId: src,
            layerId: d.layerId,
            sourceName: sourceLayer.name,
            isAltPressed: e.altKey,
          },
          isCrossDocTab ? { type: "tab", docId: targetDocId } : { type: "canvas" },
          targetPos,
          workspace,
        );
        crossDocAdded = true;
        if (!e.altKey && !d.facade) {
          // Copy (default) leaves the source untouched in place. The routed path
          // never mutated it, so releasing the preview above already left it there.
          sourceEngine.transformLayer(d.layerId, {
            x: d.startTransformX,
            y: d.startTransformY,
          });
        }
        if (newLayerId) {
          const targetEngine = workspace.getActiveEngine();
          const newLayer = targetEngine?.getLayer(newLayerId);
          if (newLayer?.imageBitmap) {
            renderer.uploadImage(newLayerId, newLayer.imageBitmap);
          }
        }
        scheduler.requestRender();
      }
    } else if (dropTarget && dropTarget.type === "tab" && dropTarget.docId === src) {
      // Dropped on the same doc's tab →revert position (treat as cancel)
      dragController.cancelTabHover();
      const sourceEngine = workspace.getEngine(src);
      if (sourceEngine && !d.facade) {
        for (const item of d.selectedLayerStarts) {
          sourceEngine.transformLayer(item.id, {
            x: item.startTransformX,
            y: item.startTransformY,
          });
        }
        scheduler.requestRender();
      }
    } else if (d.facade) {
      // Ended inside the source document with no drop-target action: the gesture's
      // whole travel goes out as one committed edit per layer.
      commitRoutedDrag(d);
    }

    // Commit history for the SOURCE doc so the user can undo the drag.
    // Skipped on the routed path: the native transform arm owns that undo entry,
    // so a second one here would leave two undo steps for one drag.
    // useCanvasLayerDrag is the sole history owner for move tool:
    // input-handler.handlePointerUp does NOT commit for "move" because
    // the SVG overlay (SelectionTransformOverlay, z-index 40) intercepts
    // clicks before they reach the canvas — so input-handler.handlePointerDown
    // never fires for SVG overlay clicks, and pendingHistorySnapshot stays
    // null. Only useCanvasLayerDrag.handlePointerDown can reliably track
    // and commit move tool history for both SVG and canvas click paths.
    if (src && !d.facade) {
      const sourceEngine = workspace.getEngine(src);
      const history = workspace.getHistory(src);
      if (sourceEngine && history) {
        const label = d.selectedLayerStarts.length > 1 ? "Move Layers" : "Move Layer";
        history.commit(d.preDragSnapshot, label);
        // Trigger sync so the History Panel updates immediately.
        // history.commit only pushes to the history stack — without a
        // notify call, the UI won't know the history changed until the
        // next engine mutation (@regression 2026-07-03: user reports
        // "sebenarnya tercatat tapi kayak harus diklik layer dulu baru muncul").
        workspace.notifyVisualChange();
      }
    }

    dragController.setDropTarget(null);
    setDrag(null);
    dragController.endDrag();
  }

  function onPointerCancel() {
    cancelDrag();
  }

  function handlePointerDown(e: PointerEvent) {
    if (activeTool() !== "move" && activeTool() !== "shape") return;
    if (e.button !== 0) return;

    // Navigation mode (Space held or active pan): skip drag entirely.
    // The container's onPointerDown also guards this, but the SVG overlay
    // might re-render pointer-events after the signal update — if the
    // overlay still has pointer-events:auto when pointerdown fires, the
    // event reaches this handler before the container can early-return.
    if (opts.isSpacePressed?.() || opts.isPanning?.()) return;

    // Ignore if the click was on a transform handle or rotate path
    const target = e.target as HTMLElement;
    if (target.closest("[data-handle], [data-overlay-svg] [path]")) {
      return;
    }

    const container = e.currentTarget as HTMLElement;
    const rect = container.getBoundingClientRect();
    const screenX = e.clientX - rect.left;
    const screenY = e.clientY - rect.top;
    const docPos = camera.screenToDocument(screenX, screenY);
    let layer = findLayerAt(docPos.x, docPos.y);
    if (!layer) return;

    if (activeTool() === "shape" && layer.type !== "shape" && layer.id !== selectedLayerId()) {
      return;
    }

    const src = activeDocumentId();
    if (!src) return;
    const sourceEngine = workspace.getEngine(src);
    if (!sourceEngine) return;

    // Move-tool Alt+drag = duplicate the hit layer and drag the COPY
    // Duplicate here (the single drag owner) so the
    // subsequent move operates on the fresh copy instead of the original —
    // doing it in onCanvasPointerDown caused both the original (this drag)
    // and the copy to be tracked, so the original moved and no duplicate
    // appeared. preDragSnapshot is taken BEFORE the duplicate so undo
    // reverts the move, then the duplicate, cleanly.
    let dragLayerId = layer.id;
    let preDragSnapshot = sourceEngine.snapshot();
    if (e.altKey) {
      try {
        const dup = sourceEngine.duplicateLayer(layer.id);
        if (dup.imageBitmap) renderer.uploadImage(dup.id, dup.imageBitmap);
        sourceEngine.setActiveLayer(dup.id);
        setSelectedLayerId(dup.id);
        dragLayerId = dup.id;
        scheduler.requestRender();
      } catch (err) {
        // Resource/limit errors (e.g. layer limit): fall back to a normal
        // move of the original layer.
        console.warn("Alt+drag duplicate failed:", err);
      }
    }

    // When auto-select is OFF, the layer under cursor may differ from the
    // selected layer. The normal move handler (input-handler.ts) always moves
    // the selected layer via engine.getActiveLayerId(). If we move a different
    // layer here, both handlers fight — causing two layers to drift apart
    // (@regression 2026-07-03: user reports "saya drag layer b, layer b gerak"
    // even though layer A was selected with auto-select off).
    //
    // Fix: when auto-select is OFF and the layer under cursor is not selected,
    // use the selected layer's position for the drag offset. This keeps the
    // gesture aligned with what the normal move handler expects.
    if (!moveAutoSelect()) {
      // If UI says no layer is selected (pasteboard deselect), don't start
      // a drag at all. Otherwise a tiny mouse jitter would trigger
      // engine.transformLayer() -> notifyChange() -> sync -> re-select the
      // last active layer, overriding the user's deselection.
      if (selectedLayerId() === null) return;

      const engine = workspace.getActiveEngine();
      const activeId = selectedLayerId();
      const activeLayer = activeId ? engine?.getLayer(activeId) : null;
      if (
        activeLayer &&
        !activeLayer.locked &&
        !activeLayer.lockPosition &&
        !activeLayer.isBackground &&
        activeLayer.id !== layer.id
      ) {
        // Use the active layer instead of the layer under cursor
        const activeLayerNode = {
          id: activeLayer.id,
          name: activeLayer.name,
          transform: activeLayer.transform,
          width: activeLayer.width,
          height: activeLayer.height,
          visible: activeLayer.visible,
          locked: activeLayer.locked,
        } as LayerNode;
        layer = activeLayerNode;
      }
    }

    // When NOT duplicating, drag the (possibly auto-select-OFF reassigned)
    // layer. For the Alt case dragLayerId already holds the duplicate id.
    if (!e.altKey) dragLayerId = layer.id;

    const isModifier = e.shiftKey || e.ctrlKey || e.metaKey;
    if (isModifier && typeof toggleLayerSelection === "function") {
      toggleLayerSelection(layer.id, true);
    }

    // Check if clicked layer is part of an active multi-selection
    const currentMultiIds = typeof selectedLayerIds === "function" ? selectedLayerIds() : [];
    const isTargetInMulti = currentMultiIds.includes(layer.id) && currentMultiIds.length > 1;

    let selectedLayerStarts: LayerTransformStart[] = [];
    if (isTargetInMulti && !e.altKey) {
      for (const id of currentMultiIds) {
        const l = sourceEngine.getLayer(id);
        if (l && !l.locked && !l.isBackground) {
          selectedLayerStarts.push({
            id: l.id,
            startTransformX: l.transform.x,
            startTransformY: l.transform.y,
            lockPosition: l.lockPosition,
          });
        }
      }
    } else {
      const draggedNode = dragLayerId === layer.id ? layer : sourceEngine.getLayer(dragLayerId);
      selectedLayerStarts = [{
        id: dragLayerId,
        startTransformX: layer.transform.x,
        startTransformY: layer.transform.y,
        lockPosition: draggedNode?.lockPosition,
      }];
    }

    // The route is decided here, synchronously, before any await: a per-frame write
    // to a layer the native editor state owns is refused by the engine, so the
    // gesture has to know up front whether it previews or mutates. With the flag off
    // no layer is owned, the decision is always "legacy", and this function keeps
    // its original shape.
    const decision = decideTransformSetRoute(
      selectedLayerStarts.filter((item) => !item.lockPosition).map((item) => item.id),
    );
    if (decision === "mixed-rejected") {
      // Owned and legacy layers in one selection: dragging would move the legacy
      // ones and throw on the owned ones. Refuse the gesture atomically, the same
      // way the option bar refuses a mixed batch.
      showToast(MIXED_OWNERSHIP_MESSAGE, "error");
      return;
    }
    if (decision === "route") {
      // Hold the facade's one transient transform slot for the whole gesture. It is
      // the gate the numeric commit funnel refuses on, which is what keeps a
      // mid-drag option-bar edit or nudge from overwriting the gesture out from
      // under the pointer. Released on every exit through releaseFacadeDrag().
      getFacade(src).beginTransform(dragLayerId, { ...layer.transform });
    }

    setDrag({
      layerId: dragLayerId,
      sourceDocId: src,
      startDocX: docPos.x,
      startDocY: docPos.y,
      startTransformX: layer.transform.x,
      startTransformY: layer.transform.y,
      rect: { left: rect.left, top: rect.top },
      preDragSnapshot,
      selectedLayerStarts,
      pointerId: e.pointerId,
      facade: decision === "route",
      liveDx: 0,
      liveDy: 0,
    });
    beginSnapCache();

    // Notify the DragController so cross-cutting subscribers
    // (DocumentTabsBar's pointerenter →500ms hover-to-switch timer,
    // drop-target tracking) know a drag is in progress. Without this,
    // the tab's onPointerEnter sees dragKind === null and skips the
    // timer entirely →even though the user is mid-drag with the
    // pointer over the tab.
    dragController.beginLayerDrag(
      {
        version: 1,
        sourceDocId: src,
        layerId: dragLayerId,
        sourceName: layer.name,
        isAltPressed: e.altKey,
      },
      null,
    );

    document.addEventListener("pointermove", onPointerMove);
    document.addEventListener("pointerup", onPointerUp);
    document.addEventListener("pointercancel", onPointerCancel);
  }

  onMount(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Escape aborts a routed drag only: adding a legacy Escape abort would be new
      // behavior on the flag-off path, which this gesture keeps byte-identical.
      const d = drag();
      if (e.key === "Escape" && d?.facade) cancelDrag();
    };
    const handleLostPointerCapture = (e: PointerEvent) => {
      const d = drag();
      // Capture stolen (or the device gone) mid-routed-drag: no pointerup follows
      // for that pointer, so this is the only place the slot can come down.
      if (d?.facade && e.pointerId === d.pointerId) cancelDrag();
    };
    window.addEventListener("keydown", handleKeyDown);
    document.addEventListener("lostpointercapture", handleLostPointerCapture, true);
    onCleanup(() => {
      window.removeEventListener("keydown", handleKeyDown);
      document.removeEventListener("lostpointercapture", handleLostPointerCapture, true);
      detachDragListeners();
      endSnapCache();
      // Unmounting mid-gesture (document switch, viewport teardown) skips every
      // pointer handler above, so the release has to happen here too.
      const d = drag();
      if (d) releaseFacadeDrag(d);
    });
  });

  return {
    handlePointerDown,
    isDragging: () => drag() !== null,
  };
}
