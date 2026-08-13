import { createSignal, createMemo, onMount, onCleanup } from "solid-js";
import { useEditor } from "./shell/EditorContext";
import type { DocumentModel, Transform2D } from "@/engine/types";
import type { TextData } from "@/engine/textTypes";
import type { HudMode } from "./TransformHud";
import type { SnapRect, SnapResult } from "@/viewport/smartGuides";
import {
  getLayerCenter,
  getLayerAabb,
  getCursorForHandle,
  applyResizeHandle,
  applyRotationDrag,
  detectHandle,
  getNearestRotateCorner,
} from "@/viewport/transformGeometry";
import { shapeRenderMargin } from "@/engine/shapeRaster";
import { getRotateCursorByPos } from "@/viewport/cursorRotate";

interface UseSelectionTransformDragParams {
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
  onComputeSnap?: (rect: SnapRect) => SnapResult;
  onSnapClear?: () => void;
  onScreenToDoc?: (clientX: number, clientY: number) => { x: number; y: number };
  snapActive?: boolean;
  moveSnapEnabled?: boolean;
  getSvgRef: () => SVGSVGElement | undefined;
  onStopMomentum?: () => void;
}

export function useSelectionTransformDrag(props: UseSelectionTransformDragParams) {
  const { workspace, renderer, selectedLayerId, layers, zoom, pan, scheduler, activeTool, hoverHandle, setHoverHandle, moveSnapEnabled, setHoverPos, hoverPos, layerTransformSession, setLayerTransformSession, commitTransformState, constrainRatio } = useEditor();

  const activeLayer = createMemo(() => {
    const id = selectedLayerId();
    if (!id) return null;
    return layers().find((l) => l.id === id) || null;
  });

  const [dragState, setDragState] = createSignal<{
    type: string;
    startX: number;
    startY: number;
    startTransform: Transform2D;
    pointerId: number;
    layerId: string;
    // Deferred-history pattern for the "move" handle drag: stash the pre-move
    // snapshot here and only commit it on pointerUp IF the layer actually
    // moved. Prevents ghost undo entries on click-without-drag. Other handle
    // types (rotate / resize) bundle their commit into the parent transform
    // session, not here. (@regression 2026-06-18 follow-up.)
    pendingMoveSnapshot?: DocumentModel | null;
  } | null>(null);

  const getLayer = () => {
    const layer = activeLayer();
    if (!layer || !layer.visible || layer.locked) return null;
    // Background layers (and layers with both position + rotation locked) cannot
    // be transformed. Suppress the transform box and its interactions for them so
    // the overlay does not render (e.g. on document open with the Background selected).
    if (layer.isBackground) return null;
    if (layer.lockPosition && layer.lockRotation) return null;
    return layer;
  };

  // Shape layers rasterize with a stroke margin (see shapeRenderMargin), so
  // layer.width/height include 2x that margin while the visible shape occupies
  // the inner region. Derive the visible box once and feed it to all geometry so
  // the selection overlay/handles hug the actual shape (no floating gap) and the
  // resize/rotate math stays correct. The margin is in layer-local pixels; the
  // x/y offset must be scaled by the layer's scale so it lands correctly when
  // the layer is already scaled.
  const visBoxOf = (
    layer: NonNullable<ReturnType<typeof getLayer>>,
    transform: Transform2D,
    w: number,
    h: number
  ) => {
    const m = layer.type === "shape" && layer.shapeParams ? shapeRenderMargin(layer.shapeParams) : 0;
    const sx = Math.abs(transform.scaleX) || 1;
    const sy = Math.abs(transform.scaleY) || 1;
    return {
      transform: { ...transform, x: transform.x + m * sx, y: transform.y + m * sy },
      w: w - 2 * m,
      h: h - 2 * m,
    };
  };

  const visBox = createMemo<{ transform: Transform2D; w: number; h: number } | null>(() => {
    const layer = getLayer();
    if (!layer) return null;
    return visBoxOf(layer, layer.transform, layer.width, layer.height);
  });

  // Convert a transform expressed in the VISIBLE (shape-path) box frame back to
  // the FULL layer frame the engine stores. The engine's transformLayer /
  // applyResizeHandle operate on the full layer (which includes the 2*margin
  // bitmap padding), so any geometry computed against visBox must be shifted
  // back by `m * scale` before being committed — otherwise the shape drifts by
  // `margin * scaleX` on every resize. See regression test "resize keeps the
  // selection box glued to a stroked shape".
  const fullTransformFromVisible = (layer: NonNullable<ReturnType<typeof getLayer>>, vbTransform: Transform2D): Transform2D => {
    const m = layer.type === "shape" && layer.shapeParams ? shapeRenderMargin(layer.shapeParams) : 0;
    return {
      ...vbTransform,
      x: vbTransform.x - m * vbTransform.scaleX,
      y: vbTransform.y - m * vbTransform.scaleY,
    };
  };

  const center = createMemo(() => {
    const vb = visBox();
    if (!vb) return { x: 0, y: 0 };
    return getLayerCenter(vb.transform, vb.w, vb.h);
  });

  const aabb = createMemo(() => {
    const vb = visBox();
    if (!vb) return null;
    return getLayerAabb(vb.transform, vb.w, vb.h);
  });

  const rotation = createMemo(() => {
    const layer = getLayer();
    return layer ? layer.transform.rotation : 0;
  });

  const scaleX = createMemo(() => {
    const layer = getLayer();
    return layer ? layer.transform.scaleX : 1;
  });

  const scaleY = createMemo(() => {
    const layer = getLayer();
    return layer ? layer.transform.scaleY : 1;
  });

  const layerX = createMemo(() => {
    const vb = visBox();
    return vb ? vb.transform.x : 0;
  });

  const layerY = createMemo(() => {
    const vb = visBox();
    return vb ? vb.transform.y : 0;
  });

  const effW = createMemo(() => {
    const vb = visBox();
    return vb ? vb.w * Math.abs(scaleX()) : 0;
  });

  const effH = createMemo(() => {
    const vb = visBox();
    return vb ? vb.h * Math.abs(scaleY()) : 0;
  });

  const rotateCursor = createMemo(() => {
    const hp = hoverPos();
    if (!hp) return "crosshair";
    const z = zoom();
    const p = pan();
    const bb = {
      x: layerX() * z + p.x,
      y: layerY() * z + p.y,
      w: effW() * z,
      h: effH() * z,
    };
    const svg = props.getSvgRef();
    if (svg) {
      const rect = svg.getBoundingClientRect();
      const localHp = { x: hp.x - rect.left, y: hp.y - rect.top };
      return getRotateCursorByPos(localHp, bb);
    }
    return getRotateCursorByPos(hp, bb);
  });

  const resolvedCursor = createMemo(() => {
    const handle = hoverHandle();
    const layer = getLayer();
    if (!handle || !layer) return "default";
    if (handle.startsWith("rotate")) return rotateCursor();
    if (handle === "move") return "move";
    return getCursorForHandle(handle, rotation(), scaleX(), scaleY());
  });

  const activeDragCursor = createMemo(() => {
    const drag = dragState();
    const layer = getLayer();
    if (!drag || !layer) return null;
    if (drag.type === "move") return "move";
    if (drag.type === "rotate") return rotateCursor();
    return getCursorForHandle(drag.type, rotation(), scaleX(), scaleY());
  });

  const handlePointerMoveUpdateHover = (e: PointerEvent) => {
    if (dragState()) return;
    const vb = visBox();
    if (!vb) return;
    const toDoc = props.onScreenToDoc ?? ((cx: number, cy: number) => ({ x: (cx - pan().x) / zoom(), y: (cy - pan().y) / zoom() }));
    const docPos = toDoc(e.clientX, e.clientY);
    const hit = detectHandle(docPos, vb.transform, vb.w, vb.h, zoom());
    if (hit) {
      setHoverHandle(hit);
      if (hit === "rotate") {
        const corner = getNearestRotateCorner(docPos, vb.transform, vb.w, vb.h);
        setHoverHandle(`rotate-${corner}`);
      }
      setHoverPos({ x: e.clientX, y: e.clientY });
    } else {
      setHoverHandle(null);
      setHoverPos(null);
    }
  };

  const isLayerTransformSessionType = (type: string) => type !== "move";

  const handlePointerDown = (e: PointerEvent, type: string) => {
    if (props.isNavigationMode) return;
    e.stopPropagation();
    e.preventDefault();
    props.onStopMomentum?.();
    const engine = workspace.getActiveEngine();
    const history = workspace.getActiveHistory();
    const layer = getLayer();
    if (!engine || !history || !layer) return;

    const svg = props.getSvgRef();
    if (svg) {
      try { svg.setPointerCapture(e.pointerId); } catch {}
    }

    const existing = layerTransformSession();
    if (existing && (existing.documentId !== engine.getId() || existing.layerId !== layer.id)) {
      return;
    }

    if (isLayerTransformSessionType(type)) {
      // Save current transform to mini undo stack BEFORE modifying it.
      // Each pointerDown for resize/rotate creates an undo entry so
      // Ctrl+Z during the active session can revert individual gestures.
      commitTransformState({ ...layer.transform });

      if (!layerTransformSession()) {
        const originalSnapshot = engine.snapshot();
        setLayerTransformSession({
          documentId: engine.getId(),
          layerId: layer.id,
          originalSnapshot,
          originalTransform: { ...layer.transform },
          mode: type === "rotate" ? "rotate" : "resize",
          lockRatio: constrainRatio(),
          startedAt: Date.now(),
        });
      }
    }

    // Stash the pre-move snapshot for the "move" handle drag. We defer the
    // commit to pointerUp so a click-without-drag does NOT produce a ghost
    // undo entry. The other handle types (rotate/resize) are bundled into the
    // surrounding transform session and committed via Enter / Apply.
    const pendingMoveSnapshot = type === "move" ? engine.snapshot() : null;

    setDragState({
      type,
      startX: e.clientX,
      startY: e.clientY,
      startTransform: { ...layer.transform },
      pointerId: e.pointerId,
      layerId: layer.id,
      pendingMoveSnapshot,
    });
  };


  const handlePointerMove = (e: PointerEvent) => {
    const drag = dragState();
    if (!drag || e.pointerId !== drag.pointerId) {
      handlePointerMoveUpdateHover(e);
      return;
    }

    const engine = workspace.getActiveEngine();
    const layer = getLayer();
    if (!engine || !layer) return;

    if (layer.id !== drag.layerId) {
      setDragState(null);
      return;
    }

    const z = zoom();
    const dx = (e.clientX - drag.startX) / z;
    const dy = (e.clientY - drag.startY) / z;

    // Visible box for the drag-start transform, so move-snap and rotate pivots
    // align with the actual shape (not the margin padding).
    const startVb = visBoxOf(layer, drag.startTransform, layer.width, layer.height);
    const cent = getLayerCenter(startVb.transform, startVb.w, startVb.h);

    if (drag.type === "move") {
      let nextX = drag.startTransform.x + dx;
      let nextY = drag.startTransform.y + dy;
      let snapActive = false;
      const snapEnabled = props.moveSnapEnabled ?? moveSnapEnabled();
      if (!e.altKey && snapEnabled && props.onComputeSnap) {
        const aabb = getLayerAabb(startVb.transform, startVb.w, startVb.h);
        const baseX = aabb.x;
        const baseY = aabb.y;
        const snap = props.onComputeSnap({
          x: baseX + (nextX - drag.startTransform.x),
          y: baseY + (nextY - drag.startTransform.y),
          w: aabb.width,
          h: aabb.height,
        });
        nextX += snap.dx;
        nextY += snap.dy;
        snapActive = snap.lines.length > 0;
      } else {
        props.onSnapClear?.();
      }
      engine.transformLayer(layer.id, { x: nextX, y: nextY });
      props.onHudUpdate?.({
        mode: "move",
        clientX: e.clientX,
        clientY: e.clientY,
        deltaX: nextX - drag.startTransform.x,
        deltaY: nextY - drag.startTransform.y,
        width: 0, height: 0, scalePercent: 0, angle: 0, snapActive,
      });
    } else if (drag.type === "rotate") {
      const toDoc = props.onScreenToDoc ?? ((cx, cy) => ({ x: (cx - pan().x) / z, y: (cy - pan().y) / z }));
      const startDoc = toDoc(drag.startX, drag.startY);
      const currDoc = toDoc(e.clientX, e.clientY);
      const newRot = applyRotationDrag(
        cent,
        startDoc,
        currDoc,
        drag.startTransform.rotation,
        e.shiftKey
      );
      engine.transformLayer(layer.id, { rotation: newRot });
      setHoverPos({ x: e.clientX, y: e.clientY });
      props.onHudUpdate?.({
        mode: "rotate",
        clientX: e.clientX,
        clientY: e.clientY,
        angle: newRot - drag.startTransform.rotation,
        deltaX: 0, deltaY: 0, width: 0, height: 0, scalePercent: 0, snapActive: props.snapActive ?? false,
      });
    } else {
      // applyResizeHandle treats its `shiftKey` arg as "break aspect ratio":
      // true = free resize, false = keep aspect. We want constrainRatio ON to
      // KEEP aspect by default; holding Shift INVERTS that. So the effective
      // "break" state is: constrainRatio ? shiftKey : !shiftKey.
      // This is the single source of truth shared with PropertiesPanel's
      // "Constrain proportions" toggle and TransformOptionBar's "Ratio" toggle.
      const breakAspect = constrainRatio() ? e.shiftKey : !e.shiftKey;
      // Resize against the VISIBLE box so the handle grabbed at the shape edge
      // keeps tracking the pointer (no margin gap). The result is in the visible
      // frame, so convert it back to the full-layer frame before committing.
      const newTransform = fullTransformFromVisible(
        layer,
        applyResizeHandle(
          startVb.transform,
          startVb.w,
          startVb.h,
          drag.type,
          dx,
          dy,
          breakAspect,
          e.altKey
        )
      );
      engine.transformLayer(layer.id, newTransform);
      const effW = startVb.w * Math.abs(newTransform.scaleX);
      const effH = startVb.h * Math.abs(newTransform.scaleY);
      props.onHudUpdate?.({
        mode: "resize",
        clientX: e.clientX,
        clientY: e.clientY,
        width: effW,
        height: effH,
        scalePercent: Math.abs(newTransform.scaleX) * 100,
        deltaX: 0, deltaY: 0, angle: 0, snapActive: props.snapActive ?? false,
      });
    }
    scheduler.requestRender();
  };

  const handlePointerUp = (e: PointerEvent) => {
    const drag = dragState();
    if (!drag || e.pointerId !== drag.pointerId) return;
    const svg = props.getSvgRef();
    if (svg) {
      try { svg.releasePointerCapture(e.pointerId); } catch {}
    }

    // Commit the deferred "move" snapshot ONLY if the layer's position
    // actually changed since pointerDown. Click-without-drag → no entry.
    if (drag.type === "move" && drag.pendingMoveSnapshot) {
      const engine = workspace.getActiveEngine();
      const history = workspace.getActiveHistory();
      const layer = engine?.getLayer(drag.layerId);
      if (engine && history && layer) {
        const moved =
          layer.transform.x !== drag.startTransform.x ||
          layer.transform.y !== drag.startTransform.y;
        if (moved) {
          history.commit(drag.pendingMoveSnapshot, "Move Layer");
        }
      }
    }

    // Bake scale transform into fontSize for text layers on resize end:
    const isResize = ["nw", "ne", "se", "sw", "n", "s", "e", "w"].includes(drag.type);
    if (isResize) {
      const engine = workspace.getActiveEngine();
      const layer = engine?.getLayer(drag.layerId);
      if (engine && layer && layer.type === "text" && layer.textData) {
        const scaleFactor = Math.abs(layer.transform.scaleX);
        if (scaleFactor > 0 && Math.abs(scaleFactor - 1.0) > 0.001) {
          const curTd = layer.textData;
          const newFontSize = Math.max(4, Math.min(1000, Math.round(curTd.fontSize * scaleFactor)));
          const patch: Partial<TextData> = { fontSize: newFontSize };
          if (curTd.stroke && curTd.stroke.width > 0) {
            patch.stroke = {
              ...curTd.stroke,
              width: Math.max(1, Math.min(100, Math.round(curTd.stroke.width * scaleFactor))),
            };
          }
          if (curTd.boxMode === "area" && curTd.boxWidth > 0) {
            patch.boxWidth = Math.round(curTd.boxWidth * scaleFactor);
          }
          engine.updateTextData(layer.id, { ...curTd, ...patch });
          engine.transformLayer(layer.id, { scaleX: 1.0, scaleY: 1.0 });
          const bitmap = typeof engine.getLayerImageBitmap === "function" ? engine.getLayerImageBitmap(layer.id) : null;
          if (bitmap && renderer) renderer.uploadImage(layer.id, bitmap);
        }
      }
    }

    props.onSnapClear?.();
    props.onHudUpdate?.(null);
    if (drag.type === "rotate") setHoverPos(null);
    setDragState(null);
  };

  const handlePointerCancel = (e: PointerEvent) => {
    const drag = dragState();
    if (!drag || e.pointerId !== drag.pointerId) return;
    const svg = props.getSvgRef();
    if (svg) {
      try { svg.releasePointerCapture(e.pointerId); } catch {}
    }
    // On cancel we drop the pending move snapshot WITHOUT committing —
    // the gesture never completed so there's nothing to undo back to.
    props.onSnapClear?.();
    props.onHudUpdate?.(null);
    if (drag.type === "rotate") setHoverPos(null);
    setDragState(null);
  };

  const handleLostPointerCapture = (e: PointerEvent) => {
    const drag = dragState();
    if (!drag) return;
    props.onSnapClear?.();
    props.onHudUpdate?.(null);
    if (drag.type === "rotate") setHoverPos(null);
    setDragState(null);
  };

  onMount(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const drag = dragState();
      if (e.key === "Escape" && drag) {
        const engine = workspace.getActiveEngine();
        const layer = getLayer();
        if (engine && layer) {
          const session = layerTransformSession();
          if (session?.documentId === engine.getId() && session.layerId === layer.id) {
            engine.restore(session.originalSnapshot);
            setLayerTransformSession(null);
          } else {
            engine.transformLayer(layer.id, drag.startTransform);
          }
          scheduler.requestRender();
        }
        const svg = props.getSvgRef();
        if (svg) {
          try { svg.releasePointerCapture(drag.pointerId); } catch {}
        }
        props.onSnapClear?.();
        props.onHudUpdate?.(null);
        if (drag.type === "rotate") setHoverPos(null);
        setDragState(null);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    onCleanup(() => {
      window.removeEventListener("keydown", handleKeyDown);
    });
  });

  return {
    getLayer,
    center,
    aabb,
    rotation,
    scaleX,
    scaleY,
    layerX,
    layerY,
    effW,
    effH,
    rotateCursor,
    resolvedCursor,
    activeDragCursor,
    handlePointerMove,
    handlePointerUp,
    handlePointerCancel,
    handleLostPointerCapture,
    handlePointerDown,
    dragState,
  };
}
