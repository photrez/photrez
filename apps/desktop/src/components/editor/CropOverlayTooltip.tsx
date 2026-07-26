import { Show } from "solid-js";

interface CropOverlayTooltipProps {
  x: number;
  y: number;
  w: number;
  h: number;
  zoom: number;
  cropRotation: number;
  isRotate: boolean;
}

export function CropOverlayTooltip(props: CropOverlayTooltipProps) {
  const posX = () => Math.max(12, props.x + 16);
  const posY = () => Math.max(50, props.y - 14);

  return (
    <div
      class="pointer-events-none absolute z-50 rounded-[6px] border border-neutral-700/80 bg-[#18181B] px-2.5 py-1.5 text-[11px] text-neutral-100 shadow-[0_4px_12px_rgba(0,0,0,0.4)] select-none transform -translate-y-full"
      style={{
        left: `${posX()}px`,
        top: `${posY()}px`,
      }}
    >
      <Show
        when={!props.isRotate}
        fallback={
          <div class="flex items-center gap-2">
            <span class="text-[10px] font-semibold text-neutral-400 uppercase">R</span>
            <span class="font-medium text-neutral-100 tabular-nums">{props.cropRotation.toFixed(1)}°</span>
          </div>
        }
      >
        <div class="flex flex-col gap-0.5">
          <div class="flex items-center gap-1.5">
            <span class="w-3 text-[10px] font-semibold text-neutral-400 uppercase">W</span>
            <span class="font-medium text-neutral-100 tabular-nums">{Math.round(props.w)}</span>
            <span class="text-[10px] text-neutral-400">px</span>
          </div>
          <div class="flex items-center gap-1.5">
            <span class="w-3 text-[10px] font-semibold text-neutral-400 uppercase">H</span>
            <span class="font-medium text-neutral-100 tabular-nums">{Math.round(props.h)}</span>
            <span class="text-[10px] text-neutral-400">px</span>
          </div>
        </div>
      </Show>
    </div>
  );
}
