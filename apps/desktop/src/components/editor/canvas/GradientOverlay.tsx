import { Show } from "solid-js";
import { useEditor } from "../shell/EditorContext";

export function GradientOverlay() {
  const { gradientDragLine, zoom, pan } = useEditor();

  const lineData = () => gradientDragLine();

  const screenStart = () => {
    const d = lineData();
    if (!d) return { x: 0, y: 0 };
    const z = zoom();
    const p = pan();
    return {
      x: d.start.x * z + p.x,
      y: d.start.y * z + p.y,
    };
  };

  const screenEnd = () => {
    const d = lineData();
    if (!d) return { x: 0, y: 0 };
    const z = zoom();
    const p = pan();
    return {
      x: d.end.x * z + p.x,
      y: d.end.y * z + p.y,
    };
  };

  const radiusPx = () => {
    const d = lineData();
    if (!d) return 0;
    return d.distance * zoom();
  };

  return (
    <Show when={lineData()}>
      <div class="pointer-events-none absolute inset-0 z-30 overflow-hidden">
        <svg class="size-full">
          {/* Radial guide circle if radial mode */}
          <Show when={lineData()?.type === "radial"}>
            <circle
              cx={screenStart().x}
              cy={screenStart().y}
              r={radiusPx()}
              fill="none"
              stroke="#000000"
              stroke-width="3"
              opacity="0.4"
            />
            <circle
              cx={screenStart().x}
              cy={screenStart().y}
              r={radiusPx()}
              fill="none"
              stroke="#E15A17"
              stroke-width="1.5"
              stroke-dasharray="4 4"
              opacity="0.9"
            />
          </Show>

          {/* Shadow line */}
          <line
            x1={screenStart().x}
            y1={screenStart().y}
            x2={screenEnd().x}
            y2={screenEnd().y}
            stroke="#000000"
            stroke-width="3"
            stroke-linecap="round"
            opacity="0.5"
          />

          {/* Vector Direction Line */}
          <line
            x1={screenStart().x}
            y1={screenStart().y}
            x2={screenEnd().x}
            y2={screenEnd().y}
            stroke="#E15A17"
            stroke-width="2"
            stroke-linecap="round"
            stroke-dasharray="6 3"
          />

          {/* Start Point Handle */}
          <circle
            cx={screenStart().x}
            cy={screenStart().y}
            r="6"
            fill="#E15A17"
            stroke="#FFFFFF"
            stroke-width="2"
          />

          {/* End Point Handle */}
          <circle
            cx={screenEnd().x}
            cy={screenEnd().y}
            r="5"
            fill="#FFFFFF"
            stroke="#E15A17"
            stroke-width="2.5"
          />
        </svg>

        {/* Distance & Angle Readout Badge */}
        <div
          class="absolute pointer-events-none rounded bg-neutral-900/90 px-2 py-0.5 text-[10px] font-mono font-medium text-white shadow-lg backdrop-blur-sm border border-neutral-700/80 transform -translate-x-1/2 -translate-y-8"
          style={{
            left: `${screenEnd().x}px`,
            top: `${screenEnd().y}px`,
          }}
        >
          <span class="text-amber-400">{lineData()?.distance} px</span>
          <span class="mx-1 text-neutral-500">•</span>
          <span>{lineData()?.angle}°</span>
        </div>
      </div>
    </Show>
  );
}
