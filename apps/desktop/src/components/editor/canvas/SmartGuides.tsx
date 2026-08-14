// SPDX-License-Identifier: AGPL-3.0-or-later
import { For, Show, createMemo } from "solid-js";
import type { SnapLine } from "@/viewport/smartGuides";
import { useEditor } from "../shell/EditorContext";

interface SmartGuidesProps {
  lines: SnapLine[];
}

export function SmartGuides(props: SmartGuidesProps) {
  const { zoom, pan } = useEditor();

  return (
    <Show when={props.lines.length > 0}>
      <For each={props.lines}>
        {(line) => {
          const screenStart = createMemo(() => {
            const z = zoom();
            const p = pan();
            return { x: line.x1 * z + p.x, y: line.y1 * z + p.y };
          });
          const screenEnd = createMemo(() => {
            const z = zoom();
            const p = pan();
            return { x: line.x2 * z + p.x, y: line.y2 * z + p.y };
          });

          const isHorizontal = () => Math.abs(line.y1 - line.y2) < 0.001;
          const isVertical = () => Math.abs(line.x1 - line.x2) < 0.001;
          const isGap = () => line.kind === "gap";

          return (
            <g class="pointer-events-none">
              <line
                x1={screenStart().x}
                y1={screenStart().y}
                x2={screenEnd().x}
                y2={screenEnd().y}
                stroke={
                  line.color ||
                  (line.kind === "layer"
                    ? "var(--guide-layer, #E03183)"
                    : line.kind === "gap"
                    ? "var(--guide-gap, #F59E0B)"
                    : "var(--guide-canvas, #00C3FF)")
                }
                stroke-width={1.5}
                vector-effect="non-scaling-stroke"
                stroke-dasharray={line.isDashed || line.color === "var(--guide-edge)" ? "4 2" : undefined}
                style={{ opacity: 0.9 }}
              />
              <Show when={isGap()}>
                <Show when={isHorizontal()}>
                  <line
                    x1={screenStart().x}
                    y1={screenStart().y - 4}
                    x2={screenStart().x}
                    y2={screenStart().y + 4}
                    stroke={line.color || "var(--guide-gap, #F59E0B)"}
                    stroke-width={1.5}
                    vector-effect="non-scaling-stroke"
                  />
                  <line
                    x1={screenEnd().x}
                    y1={screenEnd().y - 4}
                    x2={screenEnd().x}
                    y2={screenEnd().y + 4}
                    stroke={line.color || "var(--guide-gap, #F59E0B)"}
                    stroke-width={1.5}
                    vector-effect="non-scaling-stroke"
                  />
                </Show>
                <Show when={isVertical()}>
                  <line
                    x1={screenStart().x - 4}
                    y1={screenStart().y}
                    x2={screenStart().x + 4}
                    y2={screenStart().y}
                    stroke={line.color || "var(--guide-gap, #F59E0B)"}
                    stroke-width={1.5}
                    vector-effect="non-scaling-stroke"
                  />
                  <line
                    x1={screenEnd().x - 4}
                    y1={screenEnd().y}
                    x2={screenEnd().x + 4}
                    y2={screenEnd().y}
                    stroke={line.color || "var(--guide-gap, #F59E0B)"}
                    stroke-width={1.5}
                    vector-effect="non-scaling-stroke"
                  />
                </Show>
                <Show when={line.label}>
                  {(() => {
                    const midX = () => (screenStart().x + screenEnd().x) / 2;
                    const midY = () => (screenStart().y + screenEnd().y) / 2;
                    const badgeWidth = () => Math.max(32, (line.label?.length ?? 4) * 7 + 10);
                    return (
                      <g transform={`translate(${midX()}, ${midY()})`}>
                        <rect
                          x={-badgeWidth() / 2}
                          y={-8}
                          width={badgeWidth()}
                          height={16}
                          rx={3}
                          fill="#1B1D22"
                          stroke={line.color || "var(--guide-gap, #F59E0B)"}
                          stroke-width={1}
                        />
                        <text
                          x={0}
                          y={3.5}
                          text-anchor="middle"
                          fill={line.color || "var(--guide-gap, #F59E0B)"}
                          font-size="10px"
                          font-family="system-ui, -apple-system, sans-serif"
                          font-weight="bold"
                        >
                          {line.label}
                        </text>
                      </g>
                    );
                  })()}
                </Show>
              </Show>
            </g>
          );
        }}
      </For>
    </Show>
  );
}
