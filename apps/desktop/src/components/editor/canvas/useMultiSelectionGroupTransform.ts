import { createSignal, createMemo, onCleanup, onMount } from "solid-js";
import { useEditor } from "../shell/EditorContext";
import type { DocumentModel, Transform2D } from "@/engine/types";
import type { HudMode } from "../TransformHud";
import { getLayerAabb, applyResizeHandle } from "@/viewport/transformGeometry";
import { showToast } from "../Toast";
import {
  MIXED_OWNERSHIP_MESSAGE,
  clearTransformPreview,
  getFacade,
  peekFacade,
  previewedTransformOf,
  setTransformPreview,
  type FacadeTransformPreview,
} from "@/lib/protocol/facadeRegistry";
import {
  decideTransformSetRoute,
  routeNumericTransformBatch,
  type TransformEdit,
} from "../layers/transformRouting";

interface UseMultiSelectionGroupTransformParams {
  isNavigationMode?: boolean;
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
  onScreenToDoc?: (clientX: number, clientY: number) => { x: number; y: number };
}

interface LayerStartInfo {
  id: string;
  transform: Transform2D;
  width: number;
  height: number;
  lockPosition: boolean;
  lockRotation: boolean;
}

interface Frame {
  item: LayerStartInfo;
  // Transform the group math produced for this member this frame, before lock
  // filtering.
  next: Transform2D;
}

interface GroupDragState {
  handle: string;
  pointerId: number;
  startDocX: number;
  startDocY: number;
  groupStart: { x: number; y: number; width: number; height: number };
  layerStarts: LayerStartInfo[];
  preSnapshot: DocumentModel;
  // Document the gesture took the facade slot on. Captured at pointerdown because
  // the ACTIVE document may differ by the time the gesture ends, and the release
  // has to address where the slot was taken.
  docId: string;
  // True while the members are owned by the native editor state: the model is then
  // never touched mid-gesture, each frame writes a transient preview, and the whole
  // travel goes out as one committed edit per member at pointerup.
  facade: boolean;
  // Last previewed transform per member. The commit sends these absolute values
  // rather than re-deriving from a pointer position, so a frame that previewed
  // nothing cannot move a member back to a half-way value.
  live: FacadeTransformPreview[];
  // True once a legacy frame wrote the model through the silent path.
  // Pointerup flushes one notification only then, so a click without travel
  // stays notification-free exactly as before.
  moved: boolean;
}

/**
 * What one member may actually change, mirroring the model mutator's lock rules:
 * a position-locked member keeps its x/y, a rotation-locked member keeps its
 * rotation, scale always applies. Applied to the preview so a locked member cannot
 * drift on screen and snap back at commit, and to the committed patch so the routed
 * result matches what the per-frame mutator used to produce.
 */
function memberTransform(start: Transform2D, next: Transform2D, item: LayerStartInfo): Transform2D {
  return {
    ...start,
    x: item.lockPosition ? start.x : next.x,
    y: item.lockPosition ? start.y : next.y,
    rotation: item.lockRotation ? start.rotation : next.rotation,
    scaleX: next.scaleX,
    scaleY: next.scaleY,
  };
}

/**
 * Group transform gesture: dragging a handle of the multi-selection bounding box
 * scales or rotates every selected layer around the group anchor.
 *
 * All anchor math stays in TypeScript. Two authority paths, decided once at
 * pointerdown and never re-decided mid-gesture:
 *  - Members owned by the native editor state: zero protocol calls and zero model
 *    writes per frame. Each frame writes a transient preview the render scheduler
 *    projects onto the outgoing RenderState, and pointerup sends one committed
 *    edit per member through the same seam the option bar uses. The native side
 *    owns the undo entry, so no history entry is written here.
 *  - Everything else: the original behavior, one engine.transformLayer per frame
 *    and a single combined history entry at pointerup.
 * While a routed gesture is live it holds the facade's one transient transform slot,
 * so a numeric edit issued from elsewhere in that window fails with a toast instead
 * of silently overwriting what the pointer is holding.
 */
export function useMultiSelectionGroupTransform(params: UseMultiSelectionGroupTransformParams = {}) {
  const { workspace, layers, selectedLayerIds, zoom, pan, scheduler, activeTool } = useEditor();

  const [dragState, setDragState] = createSignal<GroupDragState | null>(null);

  const groupAabb = createMemo(() => {
    const multi = typeof selectedLayerIds === "function" ? selectedLayerIds() : [];
    if (multi.length < 2) return null;
    const all = typeof layers === "function" ? layers() : [];
    const selected = all.filter((l) => multi.includes(l.id) && l.visible);
    if (selected.length < 2) return null;

    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;

    for (const l of selected) {
      const aabb = getLayerAabb(previewedTransformOf(l.id, l.transform), l.width, l.height);
      minX = Math.min(minX, aabb.x);
      minY = Math.min(minY, aabb.y);
      maxX = Math.max(maxX, aabb.x + aabb.width);
      maxY = Math.max(maxY, aabb.y + aabb.height);
    }

    if (!isFinite(minX) || !isFinite(minY)) return null;

    return {
      x: minX,
      y: minY,
      width: Math.max(1, maxX - minX),
      height: Math.max(1, maxY - minY),
    };
  });

  // Release everything a routed gesture holds without committing it. The facade's
  // transient transform slot is the gate the numeric commit funnel refuses on, so a
  // slot left behind by an abandoned gesture turns every later numeric edit of that
  // document (option bar, nudge, align, flip) into a permanent refusal with no
  // handle showing to release it. cancelTransform() is idempotent, so calling it
  // after a commit that already cleared the slot is harmless.
  const releaseSlot = (state: GroupDragState): void => {
    if (!state.facade) return;
    // peek, not get: a document that was already closed evicted its facade, and
    // creating one here to release a slot it no longer has would hand the next
    // document with the same id a stale empty facade.
    peekFacade(state.docId)?.cancelTransform();
    clearTransformPreview();
  };

  /**
   * End a routed gesture without applying it: preview away, slot released, gesture
   * over. A no-op for the legacy gesture, which has no slot and whose revert path is
   * the model write in handlePointerCancel.
   */
  const cancelRoutedGesture = (): void => {
    const state = dragState();
    if (!state?.facade) return;
    releaseSlot(state);
    setDragState(null);
    scheduler.requestRender();
    params.onHudUpdate?.(null);
  };

  const handlePointerDown = (e: PointerEvent, handle: string) => {
    if (params.isNavigationMode || e.button !== 0) return;
    if (activeTool() !== "move") return;

    const group = groupAabb();
    const engine = workspace.getActiveEngine();
    const multi = typeof selectedLayerIds === "function" ? selectedLayerIds() : [];
    if (!group || !engine || multi.length < 2) return;

    e.preventDefault();
    e.stopPropagation();

    const toDoc = params.onScreenToDoc ?? ((cx: number, cy: number) => ({
      x: (cx - pan().x) / zoom(),
      y: (cy - pan().y) / zoom(),
    }));
    const docPos = toDoc(e.clientX, e.clientY);

    const layerStarts: LayerStartInfo[] = [];
    for (const id of multi) {
      const l = engine.getLayer(id);
      if (l && !l.locked && !l.isBackground) {
        layerStarts.push({
          id,
          transform: { ...l.transform },
          width: l.width,
          height: l.height,
          lockPosition: !!l.lockPosition,
          lockRotation: !!l.lockRotation,
        });
      }
    }

    if (layerStarts.length === 0) return;

    // The route is decided here, synchronously, before any await: a per-frame write
    // to a member the native editor state owns is refused by the engine, so the
    // gesture has to know up front whether it previews or mutates. With the flag off
    // no member is owned, the decision is always "legacy", and this function keeps
    // its original shape.
    const decision = decideTransformSetRoute(layerStarts.map((item) => item.id));
    if (decision === "mixed-rejected") {
      // Owned and legacy members in one selection: dragging would move the legacy
      // ones and throw on the owned ones. Refuse the gesture atomically, the same
      // way the option bar refuses a mixed batch.
      showToast(MIXED_OWNERSHIP_MESSAGE, "error");
      return;
    }
    if (decision === "route") {
      // Hold the facade's one transient transform slot for the whole gesture. It is
      // the gate the numeric commit funnel refuses on, which is what keeps a
      // mid-gesture option-bar edit or nudge from overwriting the gesture out from
      // under the pointer. Released on every exit through releaseSlot().
      getFacade(engine.getId()).beginTransform(layerStarts[0].id, { ...layerStarts[0].transform });
    }

    const target = e.currentTarget as HTMLElement | SVGElement;
    if (target?.setPointerCapture) {
      target.setPointerCapture(e.pointerId);
    }

    setDragState({
      handle,
      pointerId: e.pointerId,
      startDocX: docPos.x,
      startDocY: docPos.y,
      groupStart: { ...group },
      layerStarts,
      preSnapshot: engine.snapshot(),
      // Only a routed gesture takes a slot, so only it needs the key it took it on.
      docId: decision === "route" ? engine.getId() : "",
      facade: decision === "route",
      live: [],
      moved: false,
    });
  };

  const applyFrames = (
    engine: { transformLayerSilent: (id: string, t: Partial<Transform2D>) => void },
    state: GroupDragState,
    frames: Frame[],
  ): void => {
    if (!state.facade) {
      // Silent per frame: one notifyChange per move (whole-model stringify +
      // mirror push + fan-out) is what janks the drag. The model still updates
      // every frame; pointerup flushes once.
      for (const frame of frames) engine.transformLayerSilent(frame.item.id, frame.next);
      state.moved = true;
      return;
    }
    const previews: FacadeTransformPreview[] = frames.map((frame) => ({
      layerId: frame.item.id,
      transform: memberTransform(frame.item.transform, frame.next, frame.item),
    }));
    state.live = previews;
    setTransformPreview(previews);
    const primary = previews[0];
    if (primary) peekFacade(state.docId)?.updateTransform(primary.transform);
  };

  const handlePointerMove = (e: PointerEvent) => {
    const state = dragState();
    if (!state || e.pointerId !== state.pointerId) return;

    const engine = workspace.getActiveEngine();
    if (!engine) {
      cancelRoutedGesture();
      return;
    }

    if (state.facade) {
      // A member that was deleted or deselected since the last frame can no longer be
      // addressed, and no pointerup would commit it. Drop it from the gesture so the
      // rest still land; the group anchor stays where the gesture started.
      const selected = typeof selectedLayerIds === "function" ? selectedLayerIds() : [];
      state.layerStarts = state.layerStarts.filter(
        (item) => !!engine.getLayer(item.id) && selected.includes(item.id),
      );
      if (state.layerStarts.length === 0) {
        cancelRoutedGesture();
        return;
      }
    }

    e.preventDefault();
    e.stopPropagation();

    const toDoc = params.onScreenToDoc ?? ((cx: number, cy: number) => ({
      x: (cx - pan().x) / zoom(),
      y: (cy - pan().y) / zoom(),
    }));
    const docPos = toDoc(e.clientX, e.clientY);

    if (state.handle === "rotate") {
      // Group rotation around group center
      const groupCenterX = state.groupStart.x + state.groupStart.width / 2;
      const groupCenterY = state.groupStart.y + state.groupStart.height / 2;

      const startAngle = Math.atan2(state.startDocY - groupCenterY, state.startDocX - groupCenterX);
      const currentAngle = Math.atan2(docPos.y - groupCenterY, docPos.x - groupCenterX);

      let deltaDeg = ((currentAngle - startAngle) * 180) / Math.PI;
      if (e.shiftKey) {
        deltaDeg = Math.round(deltaDeg / 15) * 15;
      }

      const deltaRad = (deltaDeg * Math.PI) / 180;
      const cosA = Math.cos(deltaRad);
      const sinA = Math.sin(deltaRad);

      const frames: Frame[] = [];
      for (const item of state.layerStarts) {
        const rx = item.transform.x - groupCenterX;
        const ry = item.transform.y - groupCenterY;

        const newRx = rx * cosA - ry * sinA;
        const newRy = rx * sinA + ry * cosA;

        const newRot = ((item.transform.rotation + deltaDeg) % 360 + 360) % 360;

        frames.push({
          item,
          next: {
            ...item.transform,
            x: Math.round(groupCenterX + newRx),
            y: Math.round(groupCenterY + newRy),
            rotation: Math.round(newRot * 10) / 10,
          },
        });
      }
      applyFrames(engine, state, frames);

      scheduler.requestRender();

      if (params.onHudUpdate) {
        params.onHudUpdate({
          mode: "rotate",
          clientX: e.clientX,
          clientY: e.clientY,
          deltaX: 0,
          deltaY: 0,
          width: state.groupStart.width,
          height: state.groupStart.height,
          scalePercent: 100,
          angle: Math.round(deltaDeg * 10) / 10,
          snapActive: e.shiftKey,
        });
      }
      return;
    }

    const screenDx = (docPos.x - state.startDocX) * zoom();
    const screenDy = (docPos.y - state.startDocY) * zoom();

    const initialGroupTransform: Transform2D = {
      x: state.groupStart.x,
      y: state.groupStart.y,
      rotation: 0,
      scaleX: 1,
      scaleY: 1,
      flipH: false,
      flipV: false,
    };

    // applyResizeHandle expects screen-space deltas, unscaled layer dimensions
    const res = applyResizeHandle(
      initialGroupTransform,
      state.groupStart.width,
      state.groupStart.height,
      state.handle,
      screenDx / zoom(),
      screenDy / zoom(),
      e.shiftKey,
      e.altKey,
    );

    const scaleXFactor = Math.abs(res.scaleX);
    const scaleYFactor = Math.abs(res.scaleY);
    const newGroupX = res.x;
    const newGroupY = res.y;

    const frames: Frame[] = [];
    for (const item of state.layerStarts) {
      const origRelX = item.transform.x - state.groupStart.x;
      const origRelY = item.transform.y - state.groupStart.y;

      frames.push({
        item,
        next: {
          ...item.transform,
          x: Math.round(newGroupX + origRelX * scaleXFactor),
          y: Math.round(newGroupY + origRelY * scaleYFactor),
          scaleX: item.transform.scaleX * scaleXFactor,
          scaleY: item.transform.scaleY * scaleYFactor,
        },
      });
    }
    applyFrames(engine, state, frames);

    scheduler.requestRender();

    if (params.onHudUpdate) {
      const curW = Math.round(state.groupStart.width * scaleXFactor);
      const curH = Math.round(state.groupStart.height * scaleYFactor);
      params.onHudUpdate({
        mode: "resize",
        clientX: e.clientX,
        clientY: e.clientY,
        deltaX: Math.round(docPos.x - state.startDocX),
        deltaY: Math.round(docPos.y - state.startDocY),
        width: curW,
        height: curH,
        scalePercent: Math.round(scaleXFactor * 100),
        angle: 0,
        snapActive: false,
      });
    }
  };

  const handlePointerUp = (e: PointerEvent) => {
    const state = dragState();
    if (!state || e.pointerId !== state.pointerId) return;

    e.preventDefault();
    e.stopPropagation();

    const engine = workspace.getActiveEngine();

    if (state.facade) {
      // The slot and the preview come down FIRST: the numeric funnel refuses a commit
      // while the slot is held, so the batch below has to run after the release.
      releaseSlot(state);
      const edits: TransformEdit[] = state.live.map((preview) => ({
        layerId: preview.layerId,
        patch: preview.transform,
      }));
      if (engine && edits.length > 0) {
        void routeNumericTransformBatch(engine, edits, {
          requestRender: () => scheduler.requestRender(),
          notifyVisualChange: () => workspace.notifyVisualChange(),
        });
      } else {
        // Nothing to commit (a plain click on the handle, or the document went away):
        // repaint so the released preview cannot stay on screen. No history entry,
        // because the native transform arm owns the gesture's undo entry.
        scheduler.requestRender();
      }
      setDragState(null);
      params.onHudUpdate?.(null);
      return;
    }

    const history = workspace.getActiveHistory();
    if (engine && history) {
      history.commit(state.preSnapshot, "Transform Layers");
      // Single end-of-gesture notify for the silent per-frame writes above.
      if (state.moved) engine.flushChangeNotification();
      scheduler.requestRender();
      workspace.notifyVisualChange();
    }

    setDragState(null);
    if (params.onHudUpdate) {
      params.onHudUpdate(null);
    }
  };

  const handlePointerCancel = (e: PointerEvent) => {
    const state = dragState();
    if (!state || e.pointerId !== state.pointerId) return;

    if (state.facade) {
      // The model was never touched, so dropping the preview IS the revert.
      cancelRoutedGesture();
      return;
    }

    const engine = workspace.getActiveEngine();
    if (engine) {
      for (const item of state.layerStarts) {
        engine.transformLayerSilent(item.id, item.transform);
      }
      // Single flush for the whole revert: same final pixels, one notify.
      engine.flushChangeNotification();
      scheduler.requestRender();
    }

    setDragState(null);
    if (params.onHudUpdate) {
      params.onHudUpdate(null);
    }
  };

  onMount(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Escape aborts a routed gesture only: adding a legacy Escape abort would be new
      // behavior on the flag-off path, which this gesture keeps byte-identical.
      if (e.key === "Escape") cancelRoutedGesture();
    };
    const handleLostPointerCapture = (e: PointerEvent) => {
      // Capture stolen (or the device gone) mid-routed-gesture: no pointerup follows
      // for that pointer, so this is the only place the slot can come down.
      const state = dragState();
      if (state?.facade && e.pointerId === state.pointerId) cancelRoutedGesture();
    };
    window.addEventListener("keydown", handleKeyDown);
    document.addEventListener("lostpointercapture", handleLostPointerCapture, true);
    onCleanup(() => {
      window.removeEventListener("keydown", handleKeyDown);
      document.removeEventListener("lostpointercapture", handleLostPointerCapture, true);
      // Unmounting mid-gesture (document switch, viewport teardown) skips every
      // pointer handler above, so the release has to happen here too.
      const state = dragState();
      if (state) releaseSlot(state);
    });
  });

  return {
    groupAabb,
    isTransforming: () => dragState() !== null,
    activeHandle: () => dragState()?.handle ?? null,
    handlePointerDown,
    handlePointerMove,
    handlePointerUp,
    handlePointerCancel,
  };
}
