import { createSignal, createMemo } from "solid-js";
import { useEditor } from "../shell/EditorContext";
import type { DocumentModel, Transform2D } from "@/engine/types";
import type { HudMode } from "../TransformHud";
import { getLayerAabb, applyResizeHandle } from "@/viewport/transformGeometry";

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
}

export function useMultiSelectionGroupTransform(params: UseMultiSelectionGroupTransformParams = {}) {
  const { workspace, layers, selectedLayerIds, zoom, pan, scheduler, activeTool } = useEditor();

  const [dragState, setDragState] = createSignal<{
    handle: string;
    pointerId: number;
    startDocX: number;
    startDocY: number;
    groupStart: { x: number; y: number; width: number; height: number };
    layerStarts: LayerStartInfo[];
    preSnapshot: DocumentModel;
  } | null>(null);

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
      const aabb = getLayerAabb(l.transform, l.width, l.height);
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
        });
      }
    }

    if (layerStarts.length === 0) return;

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
    });
  };

  const handlePointerMove = (e: PointerEvent) => {
    const state = dragState();
    if (!state || e.pointerId !== state.pointerId) return;

    const engine = workspace.getActiveEngine();
    if (!engine) return;

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

      for (const item of state.layerStarts) {
        const rx = item.transform.x - groupCenterX;
        const ry = item.transform.y - groupCenterY;

        const newRx = rx * cosA - ry * sinA;
        const newRy = rx * sinA + ry * cosA;

        const newRot = ((item.transform.rotation + deltaDeg) % 360 + 360) % 360;

        const nextTransform: Transform2D = {
          ...item.transform,
          x: Math.round(groupCenterX + newRx),
          y: Math.round(groupCenterY + newRy),
          rotation: Math.round(newRot * 10) / 10,
        };

        engine.transformLayer(item.id, nextTransform);
      }

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

    for (const item of state.layerStarts) {
      const origRelX = item.transform.x - state.groupStart.x;
      const origRelY = item.transform.y - state.groupStart.y;

      const nextTransform: Transform2D = {
        ...item.transform,
        x: Math.round(newGroupX + origRelX * scaleXFactor),
        y: Math.round(newGroupY + origRelY * scaleYFactor),
        scaleX: item.transform.scaleX * scaleXFactor,
        scaleY: item.transform.scaleY * scaleYFactor,
      };

      engine.transformLayer(item.id, nextTransform);
    }

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
    const history = workspace.getActiveHistory();
    if (engine && history) {
      history.commit(state.preSnapshot, "Transform Layers");
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

    const engine = workspace.getActiveEngine();
    if (engine) {
      for (const item of state.layerStarts) {
        engine.transformLayer(item.id, item.transform);
      }
      scheduler.requestRender();
    }

    setDragState(null);
    if (params.onHudUpdate) {
      params.onHudUpdate(null);
    }
  };

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
