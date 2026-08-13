import { For } from "solid-js";
import { useEditor } from "./shell/EditorContext";
import { Slider } from "./primitives";
import { ToolPill, OptionCheckbox, Divider } from "./shell/OptionBarShared";

const TOLERANCE_PRESETS = [0, 16, 32, 64, 128];

export function PaintBucketOptionBar() {
  const { fillTolerance, setFillTolerance, fillContiguous, setFillContiguous } = useEditor();

  return (
    <div class="flex items-center gap-2.5 px-2 text-[11px] select-none">
      {/* Tool Pill Badge */}
      <ToolPill icon="paint-bucket" label="Paint Bucket" />

      <Divider />

      {/* Tolerance Slider & Input */}
      <div class="flex items-center gap-2">
        <span class="text-[10px] font-medium text-[#A1A1AA] select-none">Tolerance:</span>
        <div class="relative flex items-center w-24">
          <input
            type="range"
            min={0}
            max={255}
            value={fillTolerance()}
            onInput={(e) => setFillTolerance(Number(e.currentTarget.value))}
            class="w-full h-4 cursor-pointer accent-amber-500 bg-transparent opacity-0 absolute inset-0 z-10"
          />
          <Slider percent={Math.round((fillTolerance() / 255) * 100)} accent />
        </div>
        <input
          type="number"
          min={0}
          max={255}
          value={fillTolerance()}
          onInput={(e) => {
            const val = parseInt(e.currentTarget.value, 10);
            if (!isNaN(val)) setFillTolerance(Math.max(0, Math.min(255, val)));
          }}
          class="h-6 w-11 rounded-[4px] border border-editor-field-border bg-editor-field px-1.5 text-right font-mono text-[11px] font-semibold text-white outline-none focus:border-editor-accent hover:border-[#4B515D] transition-colors"
        />
      </div>

      {/* Tolerance Quick Presets */}
      <div class="flex items-center gap-1">
        <For each={TOLERANCE_PRESETS}>
          {(preset) => (
            <button
              type="button"
              onClick={() => setFillTolerance(preset)}
              class={`h-6 rounded-[4px] px-1.5 font-mono text-[10px] font-medium transition-all cursor-pointer select-none ${
                fillTolerance() === preset
                  ? "bg-editor-accent/20 border border-editor-accent text-white font-bold shadow-xs"
                  : "bg-editor-field/40 border border-editor-field-border/60 text-[#A1A1AA] hover:text-white hover:bg-editor-field hover:border-editor-field-border"
              }`}
            >
              {preset}
            </button>
          )}
        </For>
      </div>

      <Divider />

      {/* Contiguous Fill Checkbox */}
      <OptionCheckbox
        checked={fillContiguous()}
        onChange={setFillContiguous}
        label="Contiguous"
      />
    </div>
  );
}
