import {
  createMemo,
  createSignal,
  For,
  Show,
  onMount,
  onCleanup,
} from "solid-js";
import {
  getCursorForHandle,
  normalizeRotation,
} from "@/viewport/transformGeometry";
import { getRotateCursorByPos } from "@/viewport/cursorRotate";
import {
  getModernCropFrameScreenRect,
  modernScreenDeltaToImageOffsetDelta,
  resizeModernFrameOneSided,
  type ModernCropFrame,
  type ModernCropImageTransform,
} from "@/viewport/modernCropGeometry";
import { CropOverlayGuides } from "./CropOverlayGuides";
import { CropOverlayTooltip } from "./CropOverlayTooltip";
import { ROTATE_BAND_PX, HANDLE_SIZE, HANDLE_HIT } from "@/viewport/rotateBand";

const HANDLE_TYPES = ["nw", "n", "ne", "e", "se", "s", "sw", "w"] as const;

interface ModernCropOverlayProps {
  isNavigationMode?: boolean;
  frame: ModernCropFrame;
  imageTransform: ModernCropImageTransform;
  viewportWidth: number;
  viewportHeight: number;
  projectedWidth: number;
  projectedHeight: number;
  /** Screen-space rect of the projected canvas (non-rotated) */
  canvasScreenRect?: { x: number; y: number; w: number; h: number } | null;
  cropMode: "free" | "ratio" | "size";
  cropAspect: { w: number; h: number } | null;
  guideMode: "none" | "thirds" | "grid" | "diagonal" | "golden";
  onFrameChange: (frame: ModernCropFrame) => void;
  onImageTransformChange: (transform: ModernCropImageTransform) => void;
  onHoverHandleChange?: (handle: string | null) => void;
  onDragStateChange?: (isDragging: boolean) => void;
  onApplyCrop?: () => void;
  onModernCropCommit?: () => void;
  isAltPressed?: () => boolean;
}

type DragState =
  | {
      kind: "move";
      pointerId: number;
      startClientX: number;
      startClientY: number;
      startTransform: ModernCropImageTransform;
    }
  | {
      kind: "resize";
      handle: string;
      pointerId: number;
      startClientX: number;
      startClientY: number;
      startFrame: ModernCropFrame;
      startTransform: ModernCropImageTransform;
    }
  | {
      kind: "rotate";
      pointerId: number;
      startAngle: number;
      startRotation: number;
      startTransform: ModernCropImageTransform;
    };

export function ModernCropOverlay(props: ModernCropOverlayProps) {
  let svgRef!: SVGSVGElement;
  const [hoverHandle, setHoverHandle] = createSignal<string | null>(null);
  const [hoverPos, setHoverPos] = createSignal<{ x: number; y: number } | null>(
    null,
  );
  const [dragState, setDragState] = createSignal<DragState | null>(null);
  const [tooltip, setTooltip] = createSignal<{
    x: number;
    y: number;
    w: number;
    h: number;
  } | null>(null);
  let lastPointerDownTime = 0;

  const navMode = () => props.isNavigationMode ?? false;
  const screenRect = createMemo(() =>
    getModernCropFrameScreenRect(
      props.frame,
      props.viewportWidth,
      props.viewportHeight,
    )!,
  );
  const center = createMemo(() => {
    const rect = screenRect();
    return { x: rect.x + rect.w / 2, y: rect.y + rect.h / 2 };
  });

  const setHover = (handle: string | null) => {
    setHoverHandle(handle);
    props.onHoverHandleChange?.(handle);
  };

  const handles = createMemo(() => {
    const rect = screenRect();
    return HANDLE_TYPES.map((type) => {
      const cx = type.includes("w")
        ? rect.x
        : type.includes("e")
          ? rect.x + rect.w
          : rect.x + rect.w / 2;
      const cy = type.includes("n")
        ? rect.y
        : type.includes("s")
          ? rect.y + rect.h
          : rect.y + rect.h / 2;
      return { type, cx, cy };
    });
  });

  const rotateRingPath = createMemo(() => {
    const r = screenRect();
    const ox = r.x - ROTATE_BAND_PX;
    const oy = r.y - ROTATE_BAND_PX;
    const ow = r.w + ROTATE_BAND_PX * 2;
    const oh = r.h + ROTATE_BAND_PX * 2;
    return `M ${ox} ${oy} H ${ox + ow} V ${oy + oh} H ${ox} Z M ${r.x} ${r.y} V ${r.y + r.h} H ${r.x + r.w} V ${r.y} Z`;
  });

  const rotateCursor = createMemo(() => {
    const hp = hoverPos();
    const drag = dragState();
    const rect = screenRect();
    if (!hp) {
      if (drag?.kind === "rotate") return "grabbing";
      return "crosshair";
    }
    const bb = { x: rect.x, y: rect.y, w: rect.w, h: rect.h };
    if (svgRef) {
      const svgRect = svgRef.getBoundingClientRect();
      const localHp = { x: hp.x - svgRect.left, y: hp.y - svgRect.top };
      return getRotateCursorByPos(localHp, bb);
    }
    return getRotateCursorByPos(hp, bb);
  });

  const cursor = () => {
    const drag = dragState();
    if (navMode()) return "grab";
    if (drag?.kind === "move") return "grabbing";
    if (drag?.kind === "rotate") return rotateCursor();
    const handle = hoverHandle();
    if (!handle) return "crosshair";
    if (handle === "move") return "move";
    if (handle === "rotate") return rotateCursor();
    return getCursorForHandle(handle, 0, 1, 1);
  };

  const capture = (e: PointerEvent) => {
    svgRef.setPointerCapture(e.pointerId);
    e.stopPropagation();
    props.onDragStateChange?.(true);
  };

  const startMove = (e: PointerEvent) => {
    if (navMode()) return;
    capture(e);
    props.onModernCropCommit?.();
    setDragState({
      kind: "move",
      pointerId: e.pointerId,
      startClientX: e.clientX,
      startClientY: e.clientY,
      startTransform: { ...props.imageTransform },
    });
  };

  const startResize = (e: PointerEvent, handle: string) => {
    if (navMode()) return;
    capture(e);
    props.onModernCropCommit?.();
    setDragState({
      kind: "resize",
      handle,
      pointerId: e.pointerId,
      startClientX: e.clientX,
      startClientY: e.clientY,
      startFrame: { ...props.frame },
      startTransform: { ...props.imageTransform },
    });
  };

  const pointerAngle = (e: PointerEvent) =>
    Math.atan2(e.clientY - center().y, e.clientX - center().x) *
    (180 / Math.PI);

  const startRotate = (e: PointerEvent) => {
    if (navMode()) return;
    capture(e);
    props.onModernCropCommit?.();
    setDragState({
      kind: "rotate",
      pointerId: e.pointerId,
      startAngle: pointerAngle(e),
      startRotation: props.imageTransform.rotation,
      startTransform: { ...props.imageTransform },
    });
  };

  const handlePointerMove = (e: PointerEvent) => {
    const drag = dragState();
    if (!drag || drag.pointerId !== e.pointerId) return;

    if (drag.kind === "move") {
      const offsetDelta = modernScreenDeltaToImageOffsetDelta(
        {
          x: e.clientX - drag.startClientX,
          y: e.clientY - drag.startClientY,
        },
        drag.startTransform.rotation,
      );
      props.onImageTransformChange({
        ...drag.startTransform,
        offsetX: drag.startTransform.offsetX + offsetDelta.x,
        offsetY: drag.startTransform.offsetY + offsetDelta.y,
      });
      return;
    }

    if (drag.kind === "resize") {
      const aspect =
        props.cropMode === "ratio"
          ? props.cropAspect
          : props.cropMode === "size"
            ? props.cropAspect
            : null;
      const { frame, compensation } = resizeModernFrameOneSided({
        frame: drag.startFrame,
        handle: drag.handle,
        deltaX: e.clientX - drag.startClientX,
        deltaY: e.clientY - drag.startClientY,
        viewportWidth: props.viewportWidth,
        viewportHeight: props.viewportHeight,
        projectedWidth: props.projectedWidth,
        projectedHeight: props.projectedHeight,
        aspect,
        cropMode: props.cropMode,
        shift: e.shiftKey,
        alt: props.isAltPressed?.() || e.altKey,
      });
      props.onFrameChange(frame);
      if (compensation.x !== 0 || compensation.y !== 0) {
        const offsetDelta = modernScreenDeltaToImageOffsetDelta(
          compensation,
          drag.startTransform.rotation,
        );
        props.onImageTransformChange({
          ...drag.startTransform,
          offsetX: drag.startTransform.offsetX + offsetDelta.x,
          offsetY: drag.startTransform.offsetY + offsetDelta.y,
        });
      }
      let cx = e.clientX;
      let cy = e.clientY;
      if (svgRef) {
        const svgRect = svgRef.getBoundingClientRect();
        cx -= svgRect.left;
        cy -= svgRect.top;
      }
      setTooltip({ x: cx, y: cy, w: frame.w, h: frame.h });
      return;
    }

    const rotation = normalizeRotation(
      drag.startRotation + pointerAngle(e) - drag.startAngle,
    );
    setHoverPos({ x: e.clientX, y: e.clientY });
    props.onImageTransformChange({
      ...props.imageTransform,
      rotation: e.shiftKey ? Math.round(rotation / 15) * 15 : rotation,
    });
  };

  const clearDrag = (e?: PointerEvent) => {
    const drag = dragState();
    if (e && drag && drag.pointerId !== e.pointerId) return;
    setDragState(null);
    setTooltip(null);
    setHoverPos(null);
    props.onDragStateChange?.(false);
  };

  onMount(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && dragState()) {
        const drag = dragState()!;
        if (drag.kind === "resize") {
          props.onFrameChange({ ...drag.startFrame });
          props.onImageTransformChange({ ...drag.startTransform });
        } else if (drag.kind === "move") {
          props.onImageTransformChange({ ...drag.startTransform });
        } else if (drag.kind === "rotate") {
          props.onImageTransformChange({ ...drag.startTransform });
        }
        clearDrag();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    onCleanup(() => {
      window.removeEventListener("keydown", handleKeyDown);
    });
  });

  /* extracted IIFEs → createMemo + <For> for identity-based DOM diffing */
  const expansionRects = createMemo(() => {
    const cr = props.canvasScreenRect;
    if (!cr) return [];
    const sr = screenRect();
    const rects: Array<{ x: number; y: number; w: number; h: number }> = [];
    const lW = cr.x - sr.x;
    if (lW > 0) rects.push({ x: sr.x, y: sr.y, w: lW, h: sr.h });
    const rW = sr.x + sr.w - (cr.x + cr.w);
    if (rW > 0) rects.push({ x: cr.x + cr.w, y: sr.y, w: rW, h: sr.h });
    const tW = Math.min(sr.x + sr.w, cr.x + cr.w) - Math.max(sr.x, cr.x);
    const tH = cr.y - sr.y;
    if (tW > 0 && tH > 0)
      rects.push({ x: Math.max(sr.x, cr.x), y: sr.y, w: tW, h: tH });
    const bH = sr.y + sr.h - (cr.y + cr.h);
    if (tW > 0 && bH > 0)
      rects.push({ x: Math.max(sr.x, cr.x), y: cr.y + cr.h, w: tW, h: bH });
    return rects;
  });

  const darkRects = createMemo(() => {
    const sr = screenRect();
    const vw = props.viewportWidth;
    const vh = props.viewportHeight;
    const rects: Array<{ x: number; y: number; w: number; h: number }> = [];
    // Clamp frame bounds to viewport so dark overlay always covers visible area
    const left = Math.max(0, sr.x);
    const right = Math.min(vw, sr.x + sr.w);
    const top = Math.max(0, sr.y);
    const bottom = Math.min(vh, sr.y + sr.h);
    const visibleW = right - left;
    const visibleH = bottom - top;
    // Left strip — full height
    const lW = left;
    if (lW > 0) rects.push({ x: 0, y: 0, w: lW, h: vh });
    // Right strip — full height
    const rW = vw - right;
    if (rW > 0) rects.push({ x: right, y: 0, w: rW, h: vh });
    // Top strip — only the visible portion of the frame width
    if (visibleW > 0 && top > 0)
      rects.push({ x: left, y: 0, w: visibleW, h: top });
    // Bottom strip
    const bH = vh - bottom;
    if (visibleW > 0 && bH > 0)
      rects.push({ x: left, y: bottom, w: visibleW, h: bH });
    return rects;
  });

  const isExpanded = createMemo(() => {
    const sr = screenRect();
    const cr = props.canvasScreenRect;
    if (!cr) return false;
    return (
      sr.x < cr.x ||
      sr.y < cr.y ||
      sr.x + sr.w > cr.x + cr.w ||
      sr.y + sr.h > cr.y + cr.h
    );
  });

  return (
    <div class="pointer-events-none absolute inset-0 z-40 overflow-visible">
      <svg
        ref={svgRef}
        data-modern-crop-overlay
        style={{
          position: "absolute",
          inset: 0,
          width: "100%",
          height: "100%",
          overflow: "visible",
          "pointer-events": navMode() ? "none" : "auto",
        }}
        style:cursor={cursor()}
        onPointerMove={handlePointerMove}
        onPointerUp={clearDrag}
        onPointerCancel={clearDrag}
        onLostPointerCapture={() => clearDrag()}
        onPointerLeave={() => {
          if (!dragState()) {
            setHover(null);
            setHoverPos(null);
          }
        }}
        onDblClick={(e) => {
          if (navMode() || dragState()) return;
          const el = document.elementFromPoint(e.clientX, e.clientY);
          if (!el || !el.closest("[data-modern-crop-move]")) return;
          props.onApplyCrop?.();
        }}
      >
        <defs></defs>
        {/* Canvas expansion fill — non-overlapping strips for areas where frame exceeds canvas */}
        <For each={expansionRects()}>
          {(r) => (
            <rect
              x={r.x}
              y={r.y}
              width={r.w}
              height={r.h}
              fill="rgba(255,255,255,0.08)"
              style={{ "pointer-events": "none" }}
            />
          )}
        </For>
        <For each={darkRects()}>
          {(r) => (
            <rect
              x={r.x}
              y={r.y}
              width={r.w}
              height={r.h}
              fill="rgba(0,0,0,0.55)"
              style={{ "pointer-events": "none" }}
            />
          )}
        </For>
        <rect
          x={screenRect().x}
          y={screenRect().y}
          width={screenRect().w}
          height={screenRect().h}
          fill="none"
          stroke="rgba(0,0,0,0.55)"
          stroke-width={1.5}
          style={{ "pointer-events": "none" }}
        />
        <rect
          x={screenRect().x}
          y={screenRect().y}
          width={screenRect().w}
          height={screenRect().h}
          fill="none"
          stroke="rgba(255,255,255,0.9)"
          stroke-width={0.75}
          style={{ "pointer-events": "none" }}
        />
        {/* Canvas expansion indicator — dashed outline of original canvas when frame exceeds it */}
        <Show when={isExpanded()}>
          <rect
            x={props.canvasScreenRect!.x}
            y={props.canvasScreenRect!.y}
            width={props.canvasScreenRect!.w}
            height={props.canvasScreenRect!.h}
            fill="none"
            stroke="rgba(255,255,255,0.5)"
            stroke-width={1}
            stroke-dasharray="6,4"
            style={{ "pointer-events": "none" }}
          />
        </Show>

        <CropOverlayGuides
          x={screenRect().x}
          y={screenRect().y}
          w={screenRect().w}
          h={screenRect().h}
          zoom={1}
          guideMode={props.guideMode}
        />
        {/* 360-degree rotate hit ring — rendered behind move area and handles */}
        <path
          d={rotateRingPath()}
          fill="transparent"
          fill-rule="evenodd"
          data-modern-crop-rotate="ring"
          style={{ "pointer-events": navMode() ? "none" : "all" }}
          onPointerDown={startRotate}
          onPointerEnter={(e) => {
            setHover("rotate");
            setHoverPos({ x: e.clientX, y: e.clientY });
          }}
          onPointerMove={(e) => {
            if (hoverHandle() === "rotate" || dragState()?.kind === "rotate")
              setHoverPos({ x: e.clientX, y: e.clientY });
          }}
          onPointerLeave={() => {
            if (!dragState()) {
              setHover(null);
              setHoverPos(null);
            }
          }}
        />
        <rect
          x={screenRect().x}
          y={screenRect().y}
          width={screenRect().w}
          height={screenRect().h}
          fill="transparent"
          data-modern-crop-move
          style={{ cursor: "move", "pointer-events": navMode() ? "none" : "all" }}
          onPointerDown={startMove}
          onPointerEnter={() => setHover("move")}
          onPointerLeave={() => {
            if (!dragState()) setHover(null);
          }}
        />
        <For each={handles()}>
          {(h) => (
            <g>
              <rect
                x={h.cx - HANDLE_HIT / 2}
                y={h.cy - HANDLE_HIT / 2}
                width={HANDLE_HIT}
                height={HANDLE_HIT}
                fill="transparent"
                data-modern-crop-handle={h.type}
                style={{
                  cursor: getCursorForHandle(h.type, 0, 1, 1),
                  "pointer-events": navMode() ? "none" : "all",
                }}
                onPointerDown={(e) => startResize(e, h.type)}
                onPointerEnter={() => setHover(h.type)}
                onPointerLeave={() => {
                  if (!dragState()) setHover(null);
                }}
              />
              <rect
                x={h.cx - HANDLE_SIZE / 2}
                y={h.cy - HANDLE_SIZE / 2}
                width={HANDLE_SIZE}
                height={HANDLE_SIZE}
                rx={1}
                ry={1}
                fill={
                  hoverHandle() === h.type
                    ? "rgba(255,255,255,0.95)"
                    : "rgba(255,255,255,0.78)"
                }
                stroke="rgba(0,0,0,0.35)"
                stroke-width={1}
                style={{ "pointer-events": "none" }}
              />
            </g>
          )}
        </For>
      </svg>
      <Show when={tooltip()}>
        {(t) => (
          <CropOverlayTooltip
            x={t().x}
            y={t().y}
            w={t().w}
            h={t().h}
            zoom={1}
            cropRotation={props.imageTransform.rotation}
            isRotate={false}
          />
        )}
      </Show>
    </div>
  );
};
