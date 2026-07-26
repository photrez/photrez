import { createMemo, Show } from "solid-js";
import { useEditor } from "./shell/EditorContext";
import type { CropRect } from "@/viewport/cropGeometry";
import type { CropSnapTargets } from "@/viewport/cropSnap";
import type { SnapLine } from "@/viewport/smartGuides";
import { getRotateBandPath, ROTATE_BAND_PX, ROTATE_CORNER_EXTRA, HANDLE_SIZE, HANDLE_HIT } from "@/viewport/rotateBand";
import { useCropOverlayDrag } from "./useCropOverlayDrag";
import { CropOverlayGuides } from "./CropOverlayGuides";
import { CropOverlayHandles } from "./CropOverlayHandles";
import { CropOverlayTooltip } from "./CropOverlayTooltip";
import type { CropPreview } from "./cropState";

interface CropOverlayProps {
  isNavigationMode?: boolean;
  cropRect: CropRect | null;
  guideMode: "none" | "thirds" | "grid" | "diagonal" | "golden";
  canvasWidth: number;
  canvasHeight: number;
  zoom: number;
  cropMode: "free" | "ratio" | "size";
  cropAspect: { w: number; h: number } | null;
  cropRotation?: number;
  deleteCropped?: boolean;
  onCropRectChange: (rect: CropRect) => void;
  onCropRotationChange?: (angle: number) => void;
  onHoverHandleChange?: (handle: string | null) => void;
  snapTargets?: CropSnapTargets;
  snapEnabled?: boolean;
  onSnapLines?: (lines: SnapLine[]) => void;
  onRotationStart?: () => void;
  onRotationCommit?: () => void;
  onDragStateChange?: (isDragging: boolean) => void;
  onApplyCrop?: () => void;
  hiddenCropPreview?: CropPreview | null;
  onHiddenCropPreviewChange?: (preview: CropPreview | null) => void;
  isAltPressed?: () => boolean;
}

const HANDLE_TYPES = ["nw", "n", "ne", "e", "se", "s", "sw", "w"] as const;

export function CropOverlay(props: CropOverlayProps) {
  const { pan } = useEditor();
  let svgRef: SVGSVGElement | undefined;

  const navMode = () => props.isNavigationMode ?? false;

  const {
    activeHandle,
    hoverHandle,
    tooltip,
    dragState,
    resolvedCursor,
    startDrag,
    handlePointerMove,
    clearDrag,
    handleLostPointerCapture,
    clearHover,
    handleSvgPointerDown,
    setHover,
    cropRotationValue,
    setHoverPos,
  } = useCropOverlayDrag({
    isNavigationMode: navMode,
    cropRect: () => props.cropRect,
    canvasWidth: props.canvasWidth,
    canvasHeight: props.canvasHeight,
    zoom: props.zoom,
    cropMode: props.cropMode,
    cropAspect: props.cropAspect,
    cropRotation: () => props.cropRotation,
    onCropRectChange: props.onCropRectChange,
    onCropRotationChange: props.onCropRotationChange,
    onHoverHandleChange: props.onHoverHandleChange,
    snapTargets: props.snapTargets,
    snapEnabled: props.snapEnabled,
    onSnapLines: props.onSnapLines,
    onRotationStart: props.onRotationStart,
    onRotationCommit: props.onRotationCommit,
    onDragStateChange: props.onDragStateChange,
    getSvgRef: () => svgRef,
    onHiddenCropPreviewChange: props.onHiddenCropPreviewChange,
    isAltPressed: props.isAltPressed,
  });

  const screenCenter = createMemo(() => {
    const rect = props.cropRect;
    if (!rect) return { x: 0, y: 0 };
    const p = pan();
    return {
      x: (rect.x + rect.w / 2) * props.zoom + p.x,
      y: (rect.y + rect.h / 2) * props.zoom + p.y,
    };
  });

  const screenTL = createMemo(() => {
    const rect = props.cropRect;
    if (!rect) return { x: 0, y: 0 };
    const p = pan();
    return {
      x: rect.x * props.zoom + p.x,
      y: rect.y * props.zoom + p.y,
    };
  });

  const screenW = createMemo(() => (props.cropRect?.w ?? 0) * props.zoom);
  const screenH = createMemo(() => (props.cropRect?.h ?? 0) * props.zoom);

  const hs = () => HANDLE_SIZE;
  const ht = () => HANDLE_HIT;

  const handles = createMemo(() => {
    const rect = props.cropRect;
    if (!rect) return [];
    const tl = screenTL();
    const sw = screenW();
    const sh = screenH();
    const _hs = hs();
    return HANDLE_TYPES.map((type) => {
      const cx = type.includes("w") ? tl.x : type.includes("e") ? tl.x + sw : tl.x + sw / 2;
      const cy = type.includes("n") ? tl.y : type.includes("s") ? tl.y + sh : tl.y + sh / 2;
      return { type, cx, cy, size: _hs };
    });
  });

  return (
    <Show when={props.cropRect}>
      <div class="pointer-events-none absolute inset-0 z-[35] overflow-visible">
        <svg
          ref={svgRef}
          data-crop-overlay
          style={{
            position: "absolute",
            inset: 0,
            width: "100%",
            height: "100%",
            overflow: "visible",
            "pointer-events": navMode() ? "none" : "auto",
          }}
          style:cursor={resolvedCursor()}
          onPointerDown={handleSvgPointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={clearDrag}
          onPointerCancel={clearDrag}
          onLostPointerCapture={handleLostPointerCapture}
          onPointerLeave={clearHover}
        >
          <defs>
            <mask id="crop-shield">
              <rect
                x={-5000}
                y={-5000}
                width={10000}
                height={10000}
                fill="white"
              />
              <rect
                x={screenTL().x}
                y={screenTL().y}
                width={screenW()}
                height={screenH()}
                fill="black"
                transform={cropRotationValue() !== 0 ? `rotate(${cropRotationValue()} ${screenCenter().x} ${screenCenter().y})` : undefined}
              />
            </mask>
          </defs>
          <rect
            x={-5000}
            y={-5000}
            width={10000}
            height={10000}
            fill={props.deleteCropped ? "#161618" : "rgba(0,0,0,0.5)"}
            fill-opacity={props.deleteCropped ? 0.98 : 1}
            mask="url(#crop-shield)"
            style={{ "pointer-events": "none" }}
          />
          <g transform={cropRotationValue() !== 0 ? `rotate(${cropRotationValue()} ${screenCenter().x} ${screenCenter().y})` : undefined}>
            <rect
              x={screenTL().x} y={screenTL().y} width={screenW()} height={screenH()}
              fill="none" stroke="white"
              stroke-width={1}
              vector-effect="non-scaling-stroke"
              style={{ "pointer-events": "none" }}
            />
            <CropOverlayGuides
              x={screenTL().x}
              y={screenTL().y}
              w={screenW()}
              h={screenH()}
              zoom={1}
              guideMode={props.guideMode}
            />
            {/* 360° rotate band — behind move zone and handles */}
            <path
              d={getRotateBandPath(
                screenTL().x,
                screenTL().y,
                screenW(),
                screenH(),
                ROTATE_BAND_PX,
                ROTATE_CORNER_EXTRA,
              )}
              fill="transparent"
              fill-rule="evenodd"
              data-crop-rotate-band
              style={{ "pointer-events": navMode() ? "none" : "all" }}
              onPointerDown={(e) => startDrag(e, "rotate")}
              onPointerEnter={(e) => {
                setHover("rotate");
                setHoverPos({ x: e.clientX, y: e.clientY });
              }}
              onPointerMove={(e) => {
                if (hoverHandle() === "rotate" || dragState()?.handle === "rotate")
                  setHoverPos({ x: e.clientX, y: e.clientY });
              }}
              onPointerLeave={() => { if (!dragState()) setHover(null); }}
            />
            {/* Move hit zone */}
            <rect
              x={screenTL().x}
              y={screenTL().y}
              width={screenW()}
              height={screenH()}
              fill="transparent"
              data-crop-move
              style={{ cursor: "move", "pointer-events": navMode() ? "none" : "all" }}
              onPointerDown={(e) => startDrag(e, "move")}
              onPointerEnter={() => setHover("move")}
              onPointerLeave={() => { if (!dragState()) setHover(null); }}
              onDblClick={(e) => {
                e.stopPropagation();
                props.onApplyCrop?.();
              }}
            />
            <CropOverlayHandles
              isNavigationMode={navMode()}
              handles={handles()}
              zoom={1}
              hitSize={ht()}
              activeHandle={activeHandle()}
              hoverHandle={hoverHandle()}
              isDragging={!!dragState()}
              startDrag={startDrag}
              setHover={setHover}
              setHoverPos={setHoverPos}
              cropRotation={cropRotationValue()}
            />
          </g>
        </svg>

        <Show when={tooltip()}>
          {(t) => {
            const screenTooltipPos = createMemo(() => {
              const p = pan();
              return {
                x: t().x * props.zoom + p.x,
                y: t().y * props.zoom + p.y,
              };
            });
            return (
              <CropOverlayTooltip
                x={screenTooltipPos().x}
                y={screenTooltipPos().y}
                w={t().w * props.zoom}
                h={t().h * props.zoom}
                zoom={1}
                cropRotation={props.cropRotation ?? 0}
                isRotate={!!dragState()?.handle.startsWith("rotate")}
              />
            );
          }}
        </Show>
      </div>
    </Show>
  );
}
