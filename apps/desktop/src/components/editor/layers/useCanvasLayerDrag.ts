import { createSignal } from "solid-js";
import { useEditor } from "../shell/EditorContext";
import { useDragController } from "../DragController";
import { addLayerFromCrossDoc } from "../crossDocLayerOps";
import type { LayerNode } from "@/engine/types";
import { computeSnapAdjustment, type SnapRect, type SnapLine } from "@/viewport/smartGuides";
import { getLayerAabb } from "@/viewport/transformGeometry";
import { ALPHA_HIT_THRESHOLD, isBoxHittable } from "@/viewport/layerHitTest";
import type { HudMode } from "../TransformHud";

interface CanvasLayerDrag {
  layerId: string;
  sourceDocId: string;
  startDocX: number;
  startDocY: number;
  startTransformX: number;
  startTransformY: number;
  rect: { left: number; top: number };
  preDragSnapshot: import("@/engine/types").DocumentModel;
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
 */
export function useCanvasLayerDrag(opts: CanvasLayerDragOptions = {}): CanvasLayerDragApi {
  const { workspace, renderer, camera, activeDocumentId, activeTool, scheduler, moveSnapEnabled, moveAutoSelect, selectedLayerId, setSelectedLayerId, zoom } = useEditor();
  const dragController = useDragController();

  const [drag, setDrag] = createSignal<CanvasLayerDrag | null>(null);

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

    const altHeld = e.altKey;
    let snapActive = false;
    if (!altHeld && moveSnapEnabled()) {
      const docW = engine.getWidth();
      const docH = engine.getHeight();
      const movingId = layer.id;
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
      };
      const otherLayers: SnapRect[] = engine
        .getLayers()
        .filter((l) => l.visible && l.id !== movingId && l.name !== "Background")
        .map((l) => {
          const laabb = getLayerAabb(l.transform, l.width, l.height);
          return { x: laabb.x, y: laabb.y, w: laabb.width, h: laabb.height };
        });
      const snapTargets: SnapRect[] = [
        { x: 0, y: 0, w: docW, h: docH, snapThreshold: 12, snapPriority: 3 },
        { x: docW / 2, y: -Infinity, w: 0, h: Infinity, snapThreshold: 6, snapPriority: 2 },
        { x: -Infinity, y: docH / 2, w: Infinity, h: 0, snapThreshold: 6, snapPriority: 2 },
        ...otherLayers,
      ];
      const result = computeSnapAdjustment(rect, snapTargets, 5, zoom());
      newX += result.dx;
      newY += result.dy;
      snapActive = result.lines.length > 0;
      opts.onSnapLinesChange?.(result.lines);
    } else {
      opts.onSnapLinesChange?.([]);
    }

    engine.transformLayer(d.layerId, { x: newX, y: newY });
    scheduler.requestRender();

    const actualDx = newX - d.startTransformX;
    const actualDy = newY - d.startTransformY;
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

    // Update drop target for non-cross-doc hover.
    const tabBarEl = el?.closest("[data-tab-bar-empty]") as HTMLElement | null;
    if (tabBarEl) {
      dragController.setDropTarget({ type: "tab-empty" });
    } else {
      dragController.setDropTarget(null);
    }
    dragController.cancelTabHover();

    scheduler.requestRender();
  }

  function onPointerUp(e: PointerEvent) {
    const d = drag();
    if (!d) return;

    document.removeEventListener("pointermove", onPointerMove);
    document.removeEventListener("pointerup", onPointerUp);
    document.removeEventListener("pointercancel", onPointerCancel);

    opts.onSnapLinesChange?.([]);
    opts.onHudUpdate?.(null);

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
        if (!e.altKey) {
          // Copy (default) leaves the source untouched in place.
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
      if (sourceEngine) {
        sourceEngine.transformLayer(d.layerId, {
          x: d.startTransformX,
          y: d.startTransformY,
        });
        scheduler.requestRender();
      }
    }

    // Commit history for the SOURCE doc so the user can undo the drag.
    // useCanvasLayerDrag is the sole history owner for move tool:
    // input-handler.handlePointerUp does NOT commit for "move" because
    // the SVG overlay (SelectionTransformOverlay, z-index 40) intercepts
    // clicks before they reach the canvas — so input-handler.handlePointerDown
    // never fires for SVG overlay clicks, and pendingHistorySnapshot stays
    // null. Only useCanvasLayerDrag.handlePointerDown can reliably track
    // and commit move tool history for both SVG and canvas click paths.
    if (src) {
      const sourceEngine = workspace.getEngine(src);
      const history = workspace.getHistory(src);
      if (sourceEngine && history) {
        history.commit(d.preDragSnapshot, "Move Layer");
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
    const d = drag();
    if (!d) return;
    document.removeEventListener("pointermove", onPointerMove);
    document.removeEventListener("pointerup", onPointerUp);
    document.removeEventListener("pointercancel", onPointerCancel);
    opts.onSnapLinesChange?.([]);
    opts.onHudUpdate?.(null);

    const src = d?.sourceDocId ?? activeDocumentId();
    if (src) {
      const sourceEngine = workspace.getEngine(src);
      if (sourceEngine) {
        sourceEngine.transformLayer(d.layerId, {
          x: d.startTransformX,
          y: d.startTransformY,
        });
        scheduler.requestRender();
      }
    }
    dragController.setDropTarget(null);
    setDrag(null);
    dragController.endDrag();
  }

  function handlePointerDown(e: PointerEvent) {
    if (activeTool() !== "move") return;
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

    setDrag({
      layerId: dragLayerId,
      sourceDocId: src,
      startDocX: docPos.x,
      startDocY: docPos.y,
      startTransformX: layer.transform.x,
      startTransformY: layer.transform.y,
      rect: { left: rect.left, top: rect.top },
      preDragSnapshot,
    });

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

  return {
    handlePointerDown,
    isDragging: () => drag() !== null,
  };
}
