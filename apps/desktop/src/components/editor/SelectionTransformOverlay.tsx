import { Show, For, createMemo } from "solid-js";
import { useEditor } from "./shell/EditorContext";
import type { HudMode } from "./TransformHud";
import type { SnapRect, SnapResult } from "@/viewport/smartGuides";
import { getCursorForHandle, documentToLayerLocal } from "@/viewport/transformGeometry";
import { HANDLE_SIZE, HANDLE_HIT, ROTATE_BAND_PX } from "@/viewport/rotateBand";
import { useSelectionTransformDrag } from "./useSelectionTransformDrag";
import { commitLayerTransformSession } from "./transformSession";

interface SelectionTransformOverlayProps {
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
  onStopMomentum?: () => void;
}

const HANDLE_TYPES = ["nw", "n", "ne", "e", "se", "s", "sw", "w"] as const;

export function getRotatePath(type: string, cx: number, cy: number, ro: number, ri: number): string {
  if (type === "nw") {
    return `M ${cx} ${cy + ro} A ${ro} ${ro} 0 1 1 ${cx + ro} ${cy} L ${cx + ri} ${cy} A ${ri} ${ri} 0 1 0 ${cx} ${cy + ri} Z`;
  }
  if (type === "ne") {
    return `M ${cx - ro} ${cy} A ${ro} ${ro} 0 1 1 ${cx} ${cy + ro} L ${cx} ${cy + ri} A ${ri} ${ri} 0 1 0 ${cx - ri} ${cy} Z`;
  }
  if (type === "se") {
    return `M ${cx} ${cy - ro} A ${ro} ${ro} 0 1 1 ${cx - ro} ${cy} L ${cx - ri} ${cy} A ${ri} ${ri} 0 1 0 ${cx} ${cy - ri} Z`;
  }
  if (type === "sw") {
    return `M ${cx + ro} ${cy} A ${ro} ${ro} 0 1 1 ${cx} ${cy - ro} L ${cx} ${cy - ri} A ${ri} ${ri} 0 1 0 ${cx + ri} ${cy} Z`;
  }
  return "";
}

export function SelectionTransformOverlay(props: SelectionTransformOverlayProps = {}) {
  const { zoom, pan, activeTool, setHoverHandle, setHoverPos, layerTransformSession, setLayerTransformSession, workspace, scheduler, selection } = useEditor();

  let overlaySvgRef: SVGSVGElement | undefined;

  const {
    getLayer,
    center,
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
  } = useSelectionTransformDrag({
    isNavigationMode: props.isNavigationMode,
    onHudUpdate: props.onHudUpdate,
    onComputeSnap: props.onComputeSnap,
    onSnapClear: props.onSnapClear,
    onScreenToDoc: props.onScreenToDoc,
    snapActive: props.snapActive,
    moveSnapEnabled: props.moveSnapEnabled,
    getSvgRef: () => overlaySvgRef,
    onStopMomentum: props.onStopMomentum,
  });

  const handleDblClick = (e: MouseEvent) => {
    const session = layerTransformSession();
    if (!session) return;
    const layer = getLayer();
    if (!layer) return;

    const toDoc = props.onScreenToDoc ?? ((cx: number, cy: number) => ({ x: (cx - pan().x) / zoom(), y: (cy - pan().y) / zoom() }));
    const docPos = toDoc(e.clientX, e.clientY);
    const local = documentToLayerLocal(docPos.x, docPos.y, layer.transform, layer.width, layer.height);

    const isInside = local.x >= 0 && local.x <= layer.width && local.y >= 0 && local.y <= layer.height;
    if (isInside) {
      const engine = workspace.getActiveEngine();
      const history = workspace.getActiveHistory();
      if (commitLayerTransformSession(session, engine, history)) {
        setLayerTransformSession(null);
        scheduler.requestRender();
      }
    }
  };

  const hs = () => HANDLE_SIZE;
  const ht = () => HANDLE_HIT;

  const screenCenter = createMemo(() => {
    const c = center();
    const z = zoom();
    const p = pan();
    return { x: c.x * z + p.x, y: c.y * z + p.y };
  });

  const screenTL = createMemo(() => {
    const z = zoom();
    const p = pan();
    return { x: layerX() * z + p.x, y: layerY() * z + p.y };
  });

  const screenW = createMemo(() => effW() * zoom());
  const screenH = createMemo(() => effH() * zoom());

  // Donut rotate-band width — mirrors the crop overlay's ROTATE_BAND_PX (100px
  // screen-space) so the move-tool rotate zone matches other editors' feel.
  // - Idle: fixed 100px band around the layer (was 20-60px, felt too small).
  // - Active: Infinite (10000px) to allow viewport-wide rotation clicks.
  const ringWidth = () => {
    const isSessionActive = layerTransformSession() !== null;
    return isSessionActive ? 10000 : ROTATE_BAND_PX;
  };

  return (
    <Show when={getLayer()}>
      {(layer) => (
        <svg
          ref={overlaySvgRef}
          data-overlay-svg
          onPointerMove={handlePointerMove}
          onPointerUp={handlePointerUp}
          onPointerCancel={handlePointerCancel}
          onLostPointerCapture={handleLostPointerCapture}
          onDblClick={handleDblClick}
          style={{
            position: "absolute",
            inset: "0",
            width: "100%",
            height: "100%",
            overflow: "visible",
            "pointer-events": "none",
            "z-index": 40,
            cursor: activeDragCursor() ?? "default",
          }}
        >
          {/* Rotated group for handles and interactions in screen-space */}
          <g transform={`rotate(${rotation()} ${screenCenter().x} ${screenCenter().y})`}>
            {/* Rotated bounding box outline with high contrast accent stroke */}
            <rect
              data-transform-box
              x={screenTL().x}
              y={screenTL().y}
              width={screenW()}
              height={screenH()}
              fill="none"
              stroke="var(--color-editor-accent)"
              stroke-width={1.25}
              vector-effect="non-scaling-stroke"
              style={{
                "pointer-events": "none",
                filter: "drop-shadow(0px 0px 1px rgba(0, 0, 0, 0.6))",
              }}
            />

            {/* Center pivot dot */}
            <circle
              cx={screenCenter().x}
              cy={screenCenter().y}
              r={3}
              fill="var(--color-editor-accent)"
              vector-effect="non-scaling-stroke"
              style={{ "pointer-events": "none" }}
            />

            {/* Move hit zone — pointer-events disabled so clicks pass through to canvas.
                Canvas's useCanvasLayerDrag handles layer translation in Move tool. */}
            <rect
              x={screenTL().x}
              y={screenTL().y}
              width={screenW()}
              height={screenH()}
              fill="transparent"
              style={{ "pointer-events": "none" }}
              data-move
            />

            {/* Full-perimeter donut rotate ring — replaces 4 corner arc paths */}
            {(() => {
              const sw = screenW();
              const sh = screenH();
              const g = Math.max(0, Math.min(3, Math.min(sw, sh) / 2 - 1));
              return (
                <path
                  d={`
                    M ${screenTL().x - ringWidth()} ${screenTL().y - ringWidth()}
                    L ${screenTL().x + screenW() + ringWidth()} ${screenTL().y - ringWidth()}
                    L ${screenTL().x + screenW() + ringWidth()} ${screenTL().y + screenH() + ringWidth()}
                    L ${screenTL().x - ringWidth()} ${screenTL().y + screenH() + ringWidth()}
                    Z
                    M ${screenTL().x + g} ${screenTL().y + g}
                    L ${screenTL().x + g} ${screenTL().y + screenH() - g}
                    L ${screenTL().x + screenW() - g} ${screenTL().y + screenH() - g}
                    L ${screenTL().x + screenW() - g} ${screenTL().y + g}
                    Z
                  `}
                  fill="transparent"
                  fill-rule="evenodd"
                  style={{ "pointer-events": props.isNavigationMode ? "none" : "all", cursor: activeDragCursor() ?? rotateCursor() }}
                  onPointerDown={(e) => handlePointerDown(e, "rotate")}
                  onPointerEnter={(e) => {
                    setHoverHandle("rotate");
                    setHoverPos({ x: e.clientX, y: e.clientY });
                  }}
                  onPointerMove={(e) => {
                    setHoverPos({ x: e.clientX, y: e.clientY });
                  }}
                  onPointerLeave={() => {
                    setHoverHandle(null);
                    setHoverPos(null);
                  }}
                />
              );
            })()}

            {/* 8 handles at unrotated edges, in screen coords */}
            <For each={HANDLE_TYPES}>
              {(type) => {
                const hx = () => type === "nw" || type === "sw" || type === "w" ? screenTL().x
                          : type === "ne" || type === "se" || type === "e" ? screenTL().x + screenW()
                          : screenTL().x + screenW() / 2;
                const hy = () => type === "nw" || type === "n" || type === "ne" ? screenTL().y
                          : type === "sw" || type === "s" || type === "se" ? screenTL().y + screenH()
                          : screenTL().y + screenH() / 2;
                const cursor = () => getCursorForHandle(type, rotation(), scaleX(), scaleY());
                return (
                  <g>
                    {/* Transparent hit zone for resize */}
                    <rect
                      x={hx() - ht() / 2}
                      y={hy() - ht() / 2}
                      width={ht()}
                      height={ht()}
                      fill="transparent"
                      style={{ "pointer-events": props.isNavigationMode ? "none" : "all", cursor: activeDragCursor() ?? cursor() }}
                      data-handle={type}
                      onPointerDown={(e) => handlePointerDown(e, type)}
                      onPointerEnter={() => setHoverHandle(type)}
                      onPointerLeave={() => setHoverHandle(null)}
                    />

                    {/* Visible square handle */}
                    <rect
                      x={hx() - hs() / 2}
                      y={hy() - hs() / 2}
                      width={hs()}
                      height={hs()}
                      fill="var(--color-editor-accent)"
                      stroke="#FFFFFF"
                      stroke-width={1}
                      vector-effect="non-scaling-stroke"
                      style={{ "pointer-events": "none" }}
                    />
                  </g>
                );
              }}
            </For>
          </g>
        </svg>
      )}
    </Show>
  );
}
