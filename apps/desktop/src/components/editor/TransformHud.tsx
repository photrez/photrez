import { Show } from "solid-js";

export type HudMode = "move" | "resize" | "rotate" | "brush";

interface TransformHudProps {
  mode: HudMode;
  clientX: number;
  clientY: number;
  zoom: number;
  deltaX?: number;
  deltaY?: number;
  width?: number;
  height?: number;
  scalePercent?: number;
  angle?: number;
  snapActive?: boolean;
}

export function TransformHud(props: TransformHudProps) {
  // Offset to place the HUD floating at top-right of cursor pointer (X+16, Y-14 with -translate-y-full)
  const posX = () => Math.max(12, props.clientX + 16);
  const posY = () => Math.max(50, props.clientY - 14);

  return (
    <div
      class="pointer-events-none absolute z-50 rounded-[6px] border border-neutral-700/80 bg-[#18181B] px-2.5 py-1.5 text-[11px] text-neutral-100 shadow-[0_4px_12px_rgba(0,0,0,0.4)] select-none transform -translate-y-full"
      style={{
        left: `${posX()}px`,
        top: `${posY()}px`,
      }}
    >
      <Show when={props.mode === "resize"}>
        <div class="flex flex-col gap-0.5">
          <div class="flex items-center justify-between gap-3">
            <div class="flex items-center gap-1.5">
              <span class="w-3 text-[10px] font-semibold text-neutral-400 uppercase">W</span>
              <span class="font-medium text-neutral-100 tabular-nums">{Math.round(props.width ?? 0)}</span>
              <span class="text-[10px] text-neutral-400">px</span>
            </div>
            <Show when={props.scalePercent !== undefined}>
              <span class="text-[10px] text-neutral-400 tabular-nums">{Math.round(props.scalePercent!)}%</span>
            </Show>
          </div>
          <div class="flex items-center justify-between gap-3">
            <div class="flex items-center gap-1.5">
              <span class="w-3 text-[10px] font-semibold text-neutral-400 uppercase">H</span>
              <span class="font-medium text-neutral-100 tabular-nums">{Math.round(props.height ?? 0)}</span>
              <span class="text-[10px] text-neutral-400">px</span>
            </div>
            <Show when={props.snapActive}>
              <span class="rounded-[3px] border border-emerald-700/60 bg-emerald-950/80 px-1 text-[9.5px] font-medium tracking-wider text-emerald-300 uppercase">
                SNAP
              </span>
            </Show>
          </div>
        </div>
      </Show>

      <Show when={props.mode === "move"}>
        <div class="flex flex-col gap-0.5">
          <div class="flex items-center justify-between gap-3">
            <div class="flex items-center gap-1.5">
              <span class="w-3 text-[10px] font-semibold text-neutral-400 uppercase">X</span>
              <span class="font-medium text-neutral-100 tabular-nums">
                {(props.deltaX ?? 0) >= 0 ? `+${Math.round(props.deltaX ?? 0)}` : Math.round(props.deltaX ?? 0)}
              </span>
              <span class="text-[10px] text-neutral-400">px</span>
            </div>
          </div>
          <div class="flex items-center justify-between gap-3">
            <div class="flex items-center gap-1.5">
              <span class="w-3 text-[10px] font-semibold text-neutral-400 uppercase">Y</span>
              <span class="font-medium text-neutral-100 tabular-nums">
                {(props.deltaY ?? 0) >= 0 ? `+${Math.round(props.deltaY ?? 0)}` : Math.round(props.deltaY ?? 0)}
              </span>
              <span class="text-[10px] text-neutral-400">px</span>
            </div>
            <Show when={props.snapActive}>
              <span class="rounded-[3px] border border-emerald-700/60 bg-emerald-950/80 px-1 text-[9.5px] font-medium tracking-wider text-emerald-300 uppercase">
                SNAP
              </span>
            </Show>
          </div>
        </div>
      </Show>

      <Show when={props.mode === "brush"}>
        <div class="flex flex-col gap-0.5">
          <div class="flex items-center justify-between gap-3">
            <div class="flex items-center gap-1.5">
              <span class="w-7 text-[10px] font-semibold text-neutral-400 uppercase">Size</span>
              <span class="font-medium text-neutral-100 tabular-nums">{Math.round(props.width ?? 0)}</span>
              <span class="text-[10px] text-neutral-400">px</span>
            </div>
          </div>
          <div class="flex items-center justify-between gap-3">
            <div class="flex items-center gap-1.5">
              <span class="w-7 text-[10px] font-semibold text-neutral-400 uppercase">Hard</span>
              <span class="font-medium text-neutral-100 tabular-nums">{Math.round(props.height ?? 0)}</span>
              <span class="text-[10px] text-neutral-400">%</span>
            </div>
          </div>
        </div>
      </Show>

      <Show when={props.mode === "rotate"}>
        <div class="flex items-center gap-2">
          <span class="text-[10px] font-semibold text-neutral-400 uppercase">R</span>
          <span class="font-medium text-neutral-100 tabular-nums">{(props.angle ?? 0).toFixed(1)}°</span>
          <Show when={props.snapActive}>
            <span class="ml-1 rounded-[3px] border border-emerald-700/60 bg-emerald-950/80 px-1 text-[9.5px] font-medium tracking-wider text-emerald-300 uppercase">
              SNAP
            </span>
          </Show>
        </div>
      </Show>
    </div>
  );
}
