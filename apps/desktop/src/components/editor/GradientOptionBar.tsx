import { useEditor } from "./shell/EditorContext";

export function GradientOptionBar() {
  const { gradientType, setGradientType, gradientPreset, setGradientPreset, fgColor, bgColor } = useEditor();

  return (
    <div class="flex items-center gap-3 px-2 text-[11px]">
      <select
        value={gradientType()}
        onChange={(e) => setGradientType(e.currentTarget.value as "linear" | "radial")}
        class="rounded border border-editor-field-border bg-editor-field px-2 py-1 text-[11px] text-editor-text outline-none"
      >
        <option value="linear">Linear</option>
        <option value="radial">Radial</option>
      </select>

      <div class="flex items-center gap-1.5 text-editor-text-dim">
        <span class="text-editor-text-dim">Preset:</span>
        <select
          value={gradientPreset()}
          onChange={(e) => setGradientPreset(e.currentTarget.value as "fg-bg" | "fg-transparent")}
          class="rounded border border-editor-field-border bg-editor-field px-2 py-1 text-[11px] text-editor-text outline-none"
        >
          <option value="fg-bg">FG → BG</option>
          <option value="fg-transparent">FG → Transparent</option>
        </select>
      </div>

      <div class="flex items-center gap-1.5 text-editor-text-dim">
        <span class="text-[10px]">Start: </span>
        <span
          class="inline-block size-4 rounded border border-editor-field-border"
          style={{ "background-color": fgColor() }}
        />
        <span class="text-[10px]">End: </span>
        <span
          class="inline-block size-4 rounded border border-editor-field-border"
          style={{ "background-color": bgColor() }}
        />
      </div>
    </div>
  );
}
