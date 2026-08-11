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
              stroke="var(--color-editor-accent, #E15A17)"
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
            stroke="var(--color-editor-accent, #E15A17)"
            stroke-width="2"
            stroke-linecap="round"
            stroke-dasharray="6 3"
          />

          {/* Start Point Handle */}
          <circle
            cx={screenStart().x}
            cy={screenStart().y}
            r="6"
            fill="var(--color-editor-accent, #E15A17)"
            stroke="#FFFFFF"
            stroke-width="2"
          />

          {/* End Point Handle */}
          <circle
            cx={screenEnd().x}
            cy={screenEnd().y}
            r="5"
            fill="#FFFFFF"
            stroke="var(--color-editor-accent, #E15A17)"
            stroke-width="2.5"
          />
        </svg>

        {/* Distance & Angle Readout Badge (Unified HTML Floating Card) */}
        <div
          class="pointer-events-none absolute z-50 rounded-[6px] border border-neutral-700/80 bg-[#18181B] px-2.5 py-1.5 text-[11px] text-neutral-100 shadow-[0_4px_12px_rgba(0,0,0,0.4)] select-none transform -translate-y-full"
          style={{
            left: `${Math.max(12, screenEnd().x + 16)}px`,
            top: `${Math.max(50, screenEnd().y - 14)}px`,
          }}
        >
          <div class="flex flex-col gap-0.5">
            <div class="flex items-center gap-1.5">
              <span class="w-7 text-[10px] font-semibold text-neutral-400 uppercase">Dist</span>
              <span class="font-medium text-neutral-100 tabular-nums">{lineData()?.distance}</span>
              <span class="text-[10px] text-neutral-400">px</span>
            </div>
            <div class="flex items-center gap-1.5">
              <span class="w-7 text-[10px] font-semibold text-neutral-400 uppercase">Angle</span>
              <span class="font-medium text-neutral-100 tabular-nums">{lineData()?.angle}°</span>
            </div>
          </div>
        </div>
      </div>
    </Show>
  );
}
