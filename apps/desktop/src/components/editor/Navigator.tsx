import { createEffect, createSignal, Show, onMount, onCleanup } from "solid-js";
import { useEditor } from "./shell/EditorContext";
import { Icon } from "./icons";
import { drawLayerToContext } from "@/engine/layerComposite";

// Canvas 2D cannot use CSS var(); read the accent token once (the palette is
// static dark per design lock, so a module-level read stays accurate).
const editorAccent =
  (typeof document !== "undefined" &&
    getComputedStyle(document.documentElement).getPropertyValue("--color-editor-accent").trim()) ||
  "#E15A17";
const editorAccentSoft = `color-mix(in srgb, ${editorAccent} 8%, transparent)`;

interface NavigatorPoint {
  x: number;
  y: number;
}

interface NavigatorFrame {
  left: number;
  top: number;
  width: number;
  height: number;
}

interface NavigatorDragState {
  pointerId: number;
  startClientX: number;
  startClientY: number;
  startPanX: number;
  startPanY: number;
  zoom: number;
}

export function Navigator() {
  const {
    workspace,
    zoom,
    pan,
    setViewportState,
    docWidth,
    docHeight,
    viewportWidth,
    viewportHeight,
    activeDocumentId,
    layers,
    scheduler,
  } = useEditor();

  let canvasRef!: HTMLCanvasElement;
  let containerRef!: HTMLDivElement;

  const [scale, setScale] = createSignal(1);
  const [offsetX, setOffsetX] = createSignal(0);
  const [offsetY, setOffsetY] = createSignal(0);
  const [thumbW, setThumbW] = createSignal(0);
  const [thumbH, setThumbH] = createSignal(0);
  const [isDraggingFrame, setIsDraggingFrame] = createSignal(false);

  const drawNavigatorPreview = () => {
    const hasDocument = activeDocumentId();
    const dw = docWidth();
    const dh = docHeight();
    const currentZoom = zoom();
    const currentPan = pan();
    const vw = viewportWidth();
    const vh = viewportHeight();
    const s = scale();
    const ox = offsetX();
    const oy = offsetY();
    const tw = thumbW();
    const th = thumbH();

    if (!canvasRef || !hasDocument || dw <= 0 || dh <= 0) return;

    const ctx = canvasRef.getContext("2d");
    if (!ctx) return;

    ctx.clearRect(0, 0, 208, 88);

    ctx.save();
    ctx.beginPath();
    ctx.rect(ox, oy, tw, th);
    ctx.clip();

    for (let y = 0; y < th; y += 4) {
      for (let x = 0; x < tw; x += 4) {
        ctx.fillStyle = (x + y) % 8 === 0 ? "rgba(255,255,255,0.04)" : "rgba(0,0,0,0.15)";
        ctx.fillRect(ox + x, oy + y, 4, 4);
      }
    }
    ctx.restore();

    const allLayers = [...layers()].reverse();
    for (const layer of allLayers) {
      if (!layer.visible || !layer.imageBitmap) continue;

      ctx.save();
      ctx.translate(ox, oy);
      ctx.scale(s, s);
      try {
        // Same doc-space compositing as the canvas/export paths — the shared
        // helper draws text bitmaps (RASTER_SCALE 2x) at their DOC width/height;
        // the old inline copy drew at native bitmap size, making text 2x too
        // big in the mini-map (@bug WYSIWYG 2026-08-09). Also brings blend-mode
        // parity to the navigator.
        drawLayerToContext(ctx as unknown as OffscreenCanvasRenderingContext2D, layer);
      } catch {
        // bitmap may be closed/detached (snapshot reference)
      }
      ctx.restore();
    }

    const frame = getViewportFrame({
      currentPan,
      currentZoom,
      viewportW: vw,
      viewportH: vh,
      scaleValue: s,
      offsetLeft: ox,
      offsetTop: oy,
    });

    ctx.strokeStyle = editorAccent;
    ctx.lineWidth = 1.5;
    ctx.strokeRect(frame.left, frame.top, frame.width, frame.height);
    ctx.fillStyle = editorAccentSoft;
    ctx.fillRect(frame.left, frame.top, frame.width, frame.height);
  };

  createEffect(() => {
    const dw = docWidth();
    const dh = docHeight();
    if (dw <= 0 || dh <= 0) return;

    const containerW = 208;
    const containerH = 88;

    const s = Math.min(containerW / dw, containerH / dh);
    setScale(s);

    const w = dw * s;
    const h = dh * s;
    setThumbW(w);
    setThumbH(h);

    setOffsetX((containerW - w) / 2);
    setOffsetY((containerH - h) / 2);
  });

  createEffect(drawNavigatorPreview);
  onMount(drawNavigatorPreview);

  const getNavigatorPoint = (clientX: number, clientY: number): NavigatorPoint | null => {
    if (!canvasRef) return null;

    const rect = canvasRef.getBoundingClientRect();
    return {
      x: clientX - rect.left,
      y: clientY - rect.top,
    };
  };

  const getCurrentViewportFrame = (): NavigatorFrame => {
    return getViewportFrame({
      currentPan: pan(),
      currentZoom: zoom(),
      viewportW: viewportWidth(),
      viewportH: viewportHeight(),
      scaleValue: scale(),
      offsetLeft: offsetX(),
      offsetTop: offsetY(),
    });
  };

  const isPointInViewportFrame = (point: NavigatorPoint) => {
    const frame = getCurrentViewportFrame();
    return (
      point.x >= frame.left &&
      point.x <= frame.left + frame.width &&
      point.y >= frame.top &&
      point.y <= frame.top + frame.height
    );
  };

  const isPointInThumbnail = (point: NavigatorPoint) => {
    const ox = offsetX();
    const oy = offsetY();
    return (
      point.x >= ox &&
      point.x <= ox + thumbW() &&
      point.y >= oy &&
      point.y <= oy + thumbH()
    );
  };

  const setViewportPan = (panX: number, panY: number) => {
    const engine = workspace.getActiveEngine();
    if (!engine) return null;

    setViewportState({ x: panX, y: panY, zoom: zoom() });
    scheduler.requestRender();
    return { x: panX, y: panY };
  };

  const panToNavigatorPoint = (point: NavigatorPoint) => {
    const ox = offsetX();
    const oy = offsetY();
    const s = scale();
    if (s <= 0) return null;

    const docX = (point.x - ox) / s;
    const docY = (point.y - oy) / s;

    const vw = viewportWidth();
    const vh = viewportHeight();
    const currentZoom = zoom();

    const panX = vw / 2 - docX * currentZoom;
    const panY = vh / 2 - docY * currentZoom;

    return setViewportPan(panX, panY);
  };

  const panByNavigatorDelta = (drag: NavigatorDragState, clientX: number, clientY: number) => {
    const s = scale();
    if (s <= 0) return;

    const deltaX = clientX - drag.startClientX;
    const deltaY = clientY - drag.startClientY;
    const panX = drag.startPanX - (deltaX / s) * drag.zoom;
    const panY = drag.startPanY - (deltaY / s) * drag.zoom;
    setViewportPan(panX, panY);
  };

  let dragState: NavigatorDragState | null = null;

  const handlePointerDown = (e: PointerEvent) => {
    const point = getNavigatorPoint(e.clientX, e.clientY);
    if (!point) return;
    if (!isPointInThumbnail(point)) return;

    const target = e.currentTarget as HTMLElement;
    target.setPointerCapture(e.pointerId);
    setIsDraggingFrame(true);

    let startPan = pan();
    if (!isPointInViewportFrame(point)) {
      startPan = panToNavigatorPoint(point) ?? startPan;
    }

    dragState = {
      pointerId: e.pointerId,
      startClientX: e.clientX,
      startClientY: e.clientY,
      startPanX: startPan.x,
      startPanY: startPan.y,
      zoom: zoom(),
    };
  };

  const handlePointerMove = (e: PointerEvent) => {
    if (!dragState || e.pointerId !== dragState.pointerId) return;
    panByNavigatorDelta(dragState, e.clientX, e.clientY);
  };

  const finishDrag = (e: PointerEvent) => {
    if (!dragState || e.pointerId !== dragState.pointerId) return;
    const target = e.currentTarget as HTMLElement;
    if (!target.hasPointerCapture || target.hasPointerCapture(e.pointerId)) {
      target.releasePointerCapture(e.pointerId);
    }
    dragState = null;
    setIsDraggingFrame(false);
  };

  return (
    <div ref={containerRef} class="px-4 pt-4 select-none">
      <Show
        when={activeDocumentId()}
        fallback={
          <div class="flex h-[88px] flex-col items-center justify-center gap-2 rounded-[3px] border border-dashed border-editor-divider/50 text-center">
            <Icon name="crop" class="size-5 text-editor-text-dim opacity-50" strokeWidth={1.5} />
            <span class="text-[12px] text-editor-text-dim">No image open</span>
          </div>
        }
      >
        <div
          class="relative h-[88px] w-full rounded-[3px] border border-editor-divider bg-editor-panel overflow-hidden flex items-center justify-center"
          classList={{
            "cursor-grabbing": isDraggingFrame(),
            "cursor-crosshair": !isDraggingFrame(),
          }}
          onPointerDown={handlePointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={finishDrag}
          onPointerCancel={finishDrag}
        >
          <canvas
            ref={canvasRef}
            width={208}
            height={88}
            class="pointer-events-none"
          />
        </div>
      </Show>
    </div>
  );
}

function getViewportFrame(params: {
  currentPan: { x: number; y: number };
  currentZoom: number;
  viewportW: number;
  viewportH: number;
  scaleValue: number;
  offsetLeft: number;
  offsetTop: number;
}): NavigatorFrame {
  const vLeft = -params.currentPan.x / params.currentZoom;
  const vTop = -params.currentPan.y / params.currentZoom;
  const vWidth = params.viewportW / params.currentZoom;
  const vHeight = params.viewportH / params.currentZoom;

  return {
    left: params.offsetLeft + vLeft * params.scaleValue,
    top: params.offsetTop + vTop * params.scaleValue,
    width: vWidth * params.scaleValue,
    height: vHeight * params.scaleValue,
  };
}
