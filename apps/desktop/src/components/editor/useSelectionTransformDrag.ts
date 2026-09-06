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
import { commitLayerTransformSession } from "./transformSession";
import { isFacadeEnabled, getFacade, transformPreview, setTransformPreview, clearTransformPreview } from "@/lib/protocol/facadeRegistry";
import { isFacadeOwnedLayer } from "@/engine/document";

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
    pendingSnapshot?: DocumentModel | null;
    // Ticket 2.2: facade drag — transient-only moves, Rust commit on pointerup.
    facade?: boolean;
    liveTransform?: Transform2D;
  } | null>(null);

  // Ticket 2.2: a drag targets the facade path when the flag is on and the
  // layer is facade-owned (Rust persistent owner). Legacy drags are unchanged.
  const isFacadeDrag = (layer: { id: string }) => isFacadeEnabled() && isFacadeOwnedLayer(layer.id);

  // Effective transform = transient facade preview while dragging, else the
  // persisted layer transform. Overlay memos read this so handles/HUD track
  // the pointer during facade drags without any engine mutation.
  const effTransformOf = (layer: { id: string; transform: Transform2D }): Transform2D => {
    const p = transformPreview();
    if (p && p.layerId === layer.id) return p.transform;
    return layer.transform;
  };

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
    return visBoxOf(layer, effTransformOf(layer), layer.width, layer.height);
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
    return layer ? effTransformOf(layer).rotation : 0;
  });

  const scaleX = createMemo(() => {
    const layer = getLayer();
    return layer ? effTransformOf(layer).scaleX : 1;
  });

  const scaleY = createMemo(() => {
    const layer = getLayer();
    return layer ? effTransformOf(layer).scaleY : 1;
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

  // Ticket 2.2: single mutation funnel for drags.
  // - facade drag: TRANSIENT ONLY — updateTransform + renderer preview. Zero
  //   adapter/IPC calls, zero persistent TS mutation. Rust receives one
  //   TransformLayer command (with expectedVersion) at pointerup.
  // - legacy drag: unchanged per-move engine.transformLayer mutation.
  const applyDragTransform = (
    engine: { getId: () => string; transformLayer: (id: string, t: Partial<Transform2D>) => void },
    layerId: string,
    drag: { pointerId: number; facade?: boolean; startTransform: Transform2D },
    partial: Partial<Transform2D>
  ) => {
    if (drag.facade) {
      const full = { ...drag.startTransform, ...partial } as Transform2D;
      getFacade(engine.getId()).updateTransform(full);
      setTransformPreview({ layerId, transform: full });
      setDragState((d) => (d && d.pointerId === drag.pointerId ? { ...d, liveTransform: full } : d));
    } else {
      engine.transformLayer(layerId, partial);
    }
  };

  const handlePointerDown = (e: PointerEvent, type: string) => {
    if (props.isNavigationMode) return;
    if (type === "rotate" && (e.ctrlKey || e.metaKey)) {
      return;
    }
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

    // Ticket 2.2: facade drag — no TS history snapshot; Rust owns persistence
    // and its history is appended once at commitTransform (pointerup).
    const facade = isFacadeDrag(layer);
    if (facade) {
      getFacade(engine.getId()).beginTransform(layer.id, { ...layer.transform });
    }

    setDragState({
      type,
      startX: e.clientX,
      startY: e.clientY,
      startTransform: { ...layer.transform },
      pointerId: e.pointerId,
      layerId: layer.id,
      pendingSnapshot: facade ? null : engine.snapshot(),
      facade,
      liveTransform: facade ? { ...layer.transform } : undefined,
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
      const bypassSnap = e.ctrlKey || e.metaKey;
      if (!bypassSnap && snapEnabled && props.onComputeSnap) {
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
      applyDragTransform(engine, layer.id, drag, { x: nextX, y: nextY });
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
      applyDragTransform(engine, layer.id, drag, { rotation: newRot });
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
      let effectiveDx = dx;
      let effectiveDy = dy;
      let snapActive = false;
      const snapEnabled = props.moveSnapEnabled ?? moveSnapEnabled();
      const bypassSnap = e.ctrlKey || e.metaKey;

      if (!bypassSnap && snapEnabled && props.onComputeSnap) {
        const candidateVbTransform = applyResizeHandle(
          startVb.transform,
          startVb.w,
          startVb.h,
          drag.type,
          dx,
          dy,
          breakAspect,
          e.altKey,
        );
        const candidateAabb = getLayerAabb(candidateVbTransform, startVb.w, startVb.h);
        const snap = props.onComputeSnap({
          x: candidateAabb.x,
          y: candidateAabb.y,
          w: candidateAabb.width,
          h: candidateAabb.height,
        });

        if (snap.dx !== 0 || snap.dy !== 0) {
          if (drag.type.includes("w") || drag.type.includes("e")) {
            effectiveDx += snap.dx;
          }
          if (drag.type.includes("n") || drag.type.includes("s")) {
            effectiveDy += snap.dy;
          }
          snapActive = snap.lines.length > 0;
        }
      } else {
        props.onSnapClear?.();
      }

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
          effectiveDx,
          effectiveDy,
          breakAspect,
          e.altKey,
        )
      );
      applyDragTransform(engine, layer.id, drag, newTransform);
      const effW = startVb.w * Math.abs(newTransform.scaleX);
      const effH = startVb.h * Math.abs(newTransform.scaleY);
      props.onHudUpdate?.({
        mode: "resize",
        clientX: e.clientX,
        clientY: e.clientY,
        width: effW,
        height: effH,
        scalePercent: Math.abs(newTransform.scaleX) * 100,
        deltaX: 0, deltaY: 0, angle: 0, snapActive: snapActive || (props.snapActive ?? false),
      });
    }
    scheduler.requestRender();
  };

  const handlePointerUp = async (e: PointerEvent) => {
    const drag = dragState();
    if (!drag || e.pointerId !== drag.pointerId) return;
    const svg = props.getSvgRef();
    if (svg) {
      try { svg.releasePointerCapture(e.pointerId); } catch {}
    }

    // ── Ticket 2.2: facade commit path ────────────────────────────────────
    // Exactly ONE Rust TransformLayer command (expectedVersion enforced) when
    // the gesture changed something; zero protocol calls for a plain click.
    // The RenderDelta projection is authoritative and clears the preview.
    // Legacy TS history/text-bake are skipped: Rust owns transform history,
    // and facade-owned layers are raster-only today.
    if (drag.facade) {
      try {
        const engine = workspace.getActiveEngine();
        if (engine) {
          const facade = getFacade(engine.getId());
          const live = drag.liveTransform ?? drag.startTransform;
          const changed =
            live.x !== drag.startTransform.x ||
            live.y !== drag.startTransform.y ||
            live.scaleX !== drag.startTransform.scaleX ||
            live.scaleY !== drag.startTransform.scaleY ||
            live.rotation !== drag.startTransform.rotation;
          if (changed) {
            const snap = await facade.commitTransform();
            if (snap) engine.applyFacadeSnapshot(snap as never);
          } else {
            facade.cancelTransform();
          }
        }
      } catch {
        // Facade commit rejected: teardown in `finally` must still run so the
        // drag state never gets stuck.
      } finally {
        clearTransformPreview();
        scheduler.requestRender();
        props.onSnapClear?.();
        props.onHudUpdate?.(null);
        if (drag.type === "rotate") setHoverPos(null);
        setDragState(null);
      }
      return;
    }

    // Commit the gesture snapshot ONLY if the layer transform actually changed.
    // Click-without-drag produces no history entry.
    if (drag.pendingSnapshot) {
      const engine = workspace.getActiveEngine();
      const history = workspace.getActiveHistory();
      const layer = engine?.getLayer(drag.layerId);
      if (engine && history && layer) {
        const moved =
          layer.transform.x !== drag.startTransform.x ||
          layer.transform.y !== drag.startTransform.y;
        const scaled =
          layer.transform.scaleX !== drag.startTransform.scaleX ||
          layer.transform.scaleY !== drag.startTransform.scaleY;
        const rotated =
          layer.transform.rotation !== drag.startTransform.rotation;

        if (moved || scaled || rotated) {
          const label = drag.type === "move" ? "Move Layer" : drag.type === "rotate" ? "Rotate Layer" : "Transform Layer";
          history.commit(drag.pendingSnapshot, label);
          scheduler.requestRender();
          workspace.notifyVisualChange();
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
    // Ticket 2.2: facade cancel — the engine was never mutated during the
    // drag, so there is nothing to restore; drop the transient session.
    if (drag.facade) {
      getFacade(workspace.getActiveEngine()?.getId() ?? "").cancelTransform();
      clearTransformPreview();
      scheduler.requestRender();
      props.onSnapClear?.();
      props.onHudUpdate?.(null);
      if (drag.type === "rotate") setHoverPos(null);
      setDragState(null);
      return;
    }
    const engine = workspace.getActiveEngine();
    const layer = getLayer();
    if (engine && layer) {
      engine.transformLayer(layer.id, drag.startTransform);
      scheduler.requestRender();
    }
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
        // Ticket 2.2: facade Escape — drop transient session, engine untouched.
        if (drag.facade) {
          getFacade(engine?.getId() ?? "").cancelTransform();
          clearTransformPreview();
          scheduler.requestRender();
          const svg = props.getSvgRef();
          if (svg) {
            try { svg.releasePointerCapture(drag.pointerId); } catch {}
          }
          props.onSnapClear?.();
          props.onHudUpdate?.(null);
          if (drag.type === "rotate") setHoverPos(null);
          setDragState(null);
          return;
        }
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
