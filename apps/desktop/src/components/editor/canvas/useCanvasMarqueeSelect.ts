// SPDX-License-Identifier: AGPL-3.0-or-later
import { createSignal, createMemo } from "solid-js";
import { useEditor } from "../shell/EditorContext";
import { getLayerAabb } from "@/viewport/transformGeometry";

export interface MarqueeRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

interface CanvasMarqueeSelectOptions {
  isSpacePressed?: () => boolean;
  isPanning?: () => boolean;
}

interface MarqueeSession {
  startScreenX: number;
  startScreenY: number;
  startDocX: number;
  startDocY: number;
  currentDocX: number;
  currentDocY: number;
  rect: { left: number; top: number };
  isAdditive: boolean;
  initialSelectedIds: string[];
  hasExceededThreshold: boolean;
}

export function useCanvasMarqueeSelect(opts: CanvasMarqueeSelectOptions = {}) {
  const {
    workspace,
    pan,
    zoom,
    activeTool,
    selectedLayerIds,
    setSelectedLayerIds,
    setSelectedLayerId,
    scheduler,
  } = useEditor();

  const [session, setSession] = createSignal<MarqueeSession | null>(null);

  const marqueeRect = createMemo<MarqueeRect | null>(() => {
    const s = session();
    if (!s || !s.hasExceededThreshold) return null;
    const minX = Math.min(s.startDocX, s.currentDocX);
    const minY = Math.min(s.startDocY, s.currentDocY);
    const maxX = Math.max(s.startDocX, s.currentDocX);
    const maxY = Math.max(s.startDocY, s.currentDocY);
    return {
      x: minX,
      y: minY,
      width: Math.max(1, maxX - minX),
      height: Math.max(1, maxY - minY),
    };
  });

  function updateSelectionFromMarquee(s: MarqueeSession) {
    const engine = workspace.getActiveEngine();
    if (!engine) return;

    const minX = Math.min(s.startDocX, s.currentDocX);
    const minY = Math.min(s.startDocY, s.currentDocY);
    const maxX = Math.max(s.startDocX, s.currentDocX);
    const maxY = Math.max(s.startDocY, s.currentDocY);

    const layers = engine.getLayers();
    const hitIds: string[] = [];

    for (const layer of layers) {
      if (!layer.visible || layer.locked || layer.isBackground) continue;
      const aabb = getLayerAabb(layer.transform, layer.width, layer.height);
      const layerMinX = aabb.x;
      const layerMinY = aabb.y;
      const layerMaxX = aabb.x + aabb.width;
      const layerMaxY = aabb.y + aabb.height;

      const overlaps = !(
        maxX < layerMinX ||
        minX > layerMaxX ||
        maxY < layerMinY ||
        minY > layerMaxY
      );

      if (overlaps) {
        hitIds.push(layer.id);
      }
    }

    let finalIds: string[];
    if (s.isAdditive) {
      const set = new Set([...s.initialSelectedIds, ...hitIds]);
      finalIds = Array.from(set);
    } else {
      finalIds = hitIds;
    }

    if (typeof setSelectedLayerIds === "function") {
      setSelectedLayerIds(finalIds);
    }
  }

  function onPointerMove(e: PointerEvent) {
    const s = session();
    if (!s) return;

    const p = pan();
    const z = zoom();
    const screenX = e.clientX - s.rect.left;
    const screenY = e.clientY - s.rect.top;
    const docX = (screenX - p.x) / z;
    const docY = (screenY - p.y) / z;

    const screenDx = e.clientX - s.startScreenX;
    const screenDy = e.clientY - s.startScreenY;
    const screenDistSq = screenDx * screenDx + screenDy * screenDy;
    const hasExceeded = s.hasExceededThreshold || screenDistSq > 9; // 3px drag threshold in screen pixels

    const updated: MarqueeSession = {
      ...s,
      currentDocX: docX,
      currentDocY: docY,
      hasExceededThreshold: hasExceeded,
    };

    setSession(updated);

    if (hasExceeded) {
      updateSelectionFromMarquee(updated);
      scheduler.requestRender();
    }
  }

  function onPointerUp(e: PointerEvent) {
    const s = session();
    if (!s) return;

    window.removeEventListener("pointermove", onPointerMove, { capture: true });
    window.removeEventListener("pointerup", onPointerUp, { capture: true });
    window.removeEventListener("pointercancel", onPointerCancel, { capture: true });

    if (!s.hasExceededThreshold && !s.isAdditive) {
      // Single click on empty canvas with no modifier -> deselect
      if (typeof setSelectedLayerId === "function") {
        setSelectedLayerId(null);
      }
      if (typeof setSelectedLayerIds === "function") {
        setSelectedLayerIds([]);
      }
    }

    setSession(null);
    scheduler.requestRender();
  }

  function onPointerCancel(e: PointerEvent) {
    window.removeEventListener("pointermove", onPointerMove, { capture: true });
    window.removeEventListener("pointerup", onPointerUp, { capture: true });
    window.removeEventListener("pointercancel", onPointerCancel, { capture: true });

    const s = session();
    if (s && s.hasExceededThreshold) {
      // Restore initial selection if cancelled
      if (typeof setSelectedLayerIds === "function") {
        setSelectedLayerIds(s.initialSelectedIds);
      }
    }
    setSession(null);
    scheduler.requestRender();
  }

  function handlePointerDown(e: PointerEvent, containerEl?: HTMLElement | null): boolean {
    if (activeTool() !== "move") return false;
    if (e.button !== 0) return false;
    if (opts.isSpacePressed?.() || opts.isPanning?.()) return false;

    const target = e.target as HTMLElement;
    if (target?.closest?.("[data-handle], [data-overlay-svg] [data-transform-box]")) {
      return false;
    }

    const container = containerEl || (e.currentTarget as HTMLElement);
    if (!container) return false;
    const rect = container.getBoundingClientRect();
    const screenX = e.clientX - rect.left;
    const screenY = e.clientY - rect.top;
    const p = pan();
    const z = zoom();
    const docX = (screenX - p.x) / z;
    const docY = (screenY - p.y) / z;

    const isAdditive = e.shiftKey || e.ctrlKey || e.metaKey;
    const initialSelected = typeof selectedLayerIds === "function" ? [...selectedLayerIds()] : [];

    setSession({
      startScreenX: e.clientX,
      startScreenY: e.clientY,
      startDocX: docX,
      startDocY: docY,
      currentDocX: docX,
      currentDocY: docY,
      rect: { left: rect.left, top: rect.top },
      isAdditive,
      initialSelectedIds: initialSelected,
      hasExceededThreshold: false,
    });

    window.addEventListener("pointermove", onPointerMove, { capture: true });
    window.addEventListener("pointerup", onPointerUp, { capture: true });
    window.addEventListener("pointercancel", onPointerCancel, { capture: true });

    return true;
  }

  return {
    handlePointerDown,
    marqueeRect,
    isMarqueeActive: () => session()?.hasExceededThreshold ?? false,
  };
}
