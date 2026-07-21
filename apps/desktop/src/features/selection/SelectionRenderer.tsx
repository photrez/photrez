import { For, Show, onCleanup, onMount } from "solid-js";
import { SelectionState } from "./SelectionTypes";

// WAAPI drives stroke-dashoffset on its own thread — avoids the
// choppy CSS-keyframe path for stroke-dashoffset in Chromium/WebView2.
const MARCH_PERIOD = 1200;
const MARCH_OFFSET = -8; // one full dash cycle (4+4) → seamless loop

interface SelectionRendererProps {
  selection: SelectionState | null;
  zoom: number;
  pan: { x: number; y: number };
  canvasWidth?: number;
  canvasHeight?: number;
  onHandlePointerDown?: (handleId: string) => void;
  onRotatePointerDown?: () => void;
  editMode?: boolean;
}

const CORNER_IDS = ["nw", "ne", "se", "sw"] as const;
const EDGE_IDS = ["n", "e", "s", "w"] as const;

const CORNER_SIZE = 8;
const EDGE_SIZE = 6;
const ROTATION_RADIUS = 5;
const ROTATION_OFFSET = 24;
const ROTATION_CONNECTOR_GAP = 4;

export function SelectionRenderer(props: SelectionRendererProps) {
  const editMode = () => props.editMode ?? false;

  let groupRef: SVGGElement | undefined;
  onMount(() => {
    // jsdom has no WAAPI; guard so tests + old engines don't throw.
    const rects = groupRef?.querySelectorAll<SVGRectElement>("[data-march]") ?? [];
    if (!rects.length || typeof rects[0].animate !== "function") return;
    const anims = Array.from(rects).map((el) =>
      el.animate(
        [{ strokeDashoffset: 0 }, { strokeDashoffset: MARCH_OFFSET }],
        { duration: MARCH_PERIOD, iterations: Infinity, easing: "linear" },
      ),
    );
    onCleanup(() => anims.forEach((a) => a.cancel()));
  });

  return (
    <Show when={props.selection}>
      {(sel) => {
        const screenX = () => sel().x * props.zoom + props.pan.x;
        const screenY = () => sel().y * props.zoom + props.pan.y;
        const screenW = () => sel().width * props.zoom;
        const screenH = () => sel().height * props.zoom;
        const centerX = () => screenX() + screenW() / 2;
        const centerY = () => screenY() + screenH() / 2;
        const angle = () => sel().angle;

        const cornerPos = (id: string) => {
          const x = screenX();
          const y = screenY();
          const w = screenW();
          const h = screenH();
          switch (id) {
            case "nw": return { x, y };
            case "ne": return { x: x + w, y };
            case "se": return { x: x + w, y: y + h };
            case "sw": return { x, y: y + h };
            default: return { x: 0, y: 0 };
          }
        };

        const edgePos = (id: string) => {
          const x = screenX();
          const y = screenY();
          const w = screenW();
          const h = screenH();
          switch (id) {
            case "n": return { x: x + w / 2, y };
            case "e": return { x: x + w, y: y + h / 2 };
            case "s": return { x: x + w / 2, y: y + h };
            case "w": return { x, y: y + h / 2 };
            default: return { x: 0, y: 0 };
          }
        };

        const rotationHandlePos = () => ({
          cx: centerX(),
          cy: screenY() - ROTATION_OFFSET,
        });

        return (
          <g ref={groupRef} data-selection-root>
            <Show when={sel().inverted && props.canvasWidth != null && props.canvasHeight != null}>
              <g
                data-selection-inverted-boundary
                style={{ "pointer-events": "none" }}
              >
                <rect
                  x={props.pan.x}
                  y={props.pan.y}
                  width={props.canvasWidth! * props.zoom}
                  height={props.canvasHeight! * props.zoom}
                  fill="none"
                  stroke="var(--color-editor-accent, #E15A17)"
                  stroke-width={1.5}
                  stroke-dasharray="4 4"
                  data-march
                  vector-effect="non-scaling-stroke"
                />
                <rect
                  x={props.pan.x}
                  y={props.pan.y}
                  width={props.canvasWidth! * props.zoom}
                  height={props.canvasHeight! * props.zoom}
                  fill="none"
                  stroke="white"
                  stroke-width={1}
                  stroke-dasharray="4 4"
                  stroke-dashoffset={4}
                  data-march
                  vector-effect="non-scaling-stroke"
                />
              </g>
            </Show>
            <g
              transform={`rotate(${angle()}, ${centerX()}, ${centerY()})`}
              data-selection-group
              data-selection-active="true"
              data-selection-inverted={sel().inverted ? "true" : "false"}
              data-mode={editMode() ? "edit" : "base"}
              style={{ "pointer-events": editMode() && !sel().inverted ? "auto" : "none" }}
            >
            {/* Main marquee rect - the boundary (always visible).
                Double stroke: a thicker accent line under a thinner white line,
                both walking in opposite phase, so the edge stays visible over
                any background (standard marching-ants treatment). */}
            <g data-selection-marquee style={{ "pointer-events": "none" }}>
              <Show when={sel().shape === "ellipse"} fallback={
                // Rect marquee: double-stroke marching-ants rect
                <>
                  <rect
                    x={screenX()}
                    y={screenY()}
                    width={screenW()}
                    height={screenH()}
                    fill="none"
                    stroke="var(--color-editor-accent, #E15A17)"
                    stroke-width={1.5}
                    stroke-dasharray="4 4"
                    data-march
                    vector-effect="non-scaling-stroke"
                  />
                  <rect
                    x={screenX()}
                    y={screenY()}
                    width={screenW()}
                    height={screenH()}
                    fill="none"
                    stroke="white"
                    stroke-width={1}
                    stroke-dasharray="4 4"
                    stroke-dashoffset={4}
                    data-march
                    vector-effect="non-scaling-stroke"
                  />
                </>
              }>
                {/* Ellipse marquee: double-stroke marching-ants ellipse */}
                <ellipse
                  cx={screenX() + screenW() / 2}
                  cy={screenY() + screenH() / 2}
                  rx={screenW() / 2}
                  ry={screenH() / 2}
                  fill="none"
                  stroke="var(--color-editor-accent, #E15A17)"
                  stroke-width={1.5}
                  stroke-dasharray="4 4"
                  data-march
                  vector-effect="non-scaling-stroke"
                />
                <ellipse
                  cx={screenX() + screenW() / 2}
                  cy={screenY() + screenH() / 2}
                  rx={screenW() / 2}
                  ry={screenH() / 2}
                  fill="none"
                  stroke="white"
                  stroke-width={1}
                  stroke-dasharray="4 4"
                  stroke-dashoffset={4}
                  data-march
                  vector-effect="non-scaling-stroke"
                />
              </Show>
            </g>

            {/* Edit-mode-only: rotation connector + handles */}
            <Show when={editMode() && !sel().inverted}>
              {/* Rotation handle connector line */}
              <line
                x1={centerX()}
                y1={screenY() - ROTATION_CONNECTOR_GAP}
                x2={rotationHandlePos().cx}
                y2={rotationHandlePos().cy + ROTATION_RADIUS}
                stroke="#E15A17"
                stroke-width={1}
                stroke-dasharray="2 2"
                vector-effect="non-scaling-stroke"
                data-rotation-connector
                style={{ "pointer-events": "none" }}
              />

              {/* Corner handles */}
              <For each={CORNER_IDS}>
                {(id) => {
                  const pos = cornerPos(id);
                  const half = CORNER_SIZE / 2;
                  return (
                    <rect
                      x={pos.x - half}
                      y={pos.y - half}
                      width={CORNER_SIZE}
                      height={CORNER_SIZE}
                      fill="white"
                      stroke="#E15A17"
                      stroke-width={1.5}
                      vector-effect="non-scaling-stroke"
                      data-handle-id={id}
                      data-handle-type="corner"
                      onPointerDown={(e) => {
                        e.stopPropagation();
                        props.onHandlePointerDown?.(id);
                      }}
                    />
                  );
                }}
              </For>

              {/* Edge handles */}
              <For each={EDGE_IDS}>
                {(id) => {
                  const pos = edgePos(id);
                  const half = EDGE_SIZE / 2;
                  return (
                    <rect
                      x={pos.x - half}
                      y={pos.y - half}
                      width={EDGE_SIZE}
                      height={EDGE_SIZE}
                      fill="white"
                      stroke="#E15A17"
                      stroke-width={1.5}
                      vector-effect="non-scaling-stroke"
                      data-handle-id={id}
                      data-handle-type="edge"
                      onPointerDown={(e) => {
                        e.stopPropagation();
                        props.onHandlePointerDown?.(id);
                      }}
                    />
                  );
                }}
              </For>

              {/* Rotation handle */}
              <circle
                cx={rotationHandlePos().cx}
                cy={rotationHandlePos().cy}
                r={ROTATION_RADIUS}
                fill="white"
                stroke="#E15A17"
                stroke-width={1.5}
                vector-effect="non-scaling-stroke"
                data-rotation-handle
                data-handle-type="rotation"
                onPointerDown={(e) => {
                  e.stopPropagation();
                  props.onRotatePointerDown?.();
                }}
              />
            </Show>
            </g>
          </g>
        );
      }}
    </Show>
  );
}
