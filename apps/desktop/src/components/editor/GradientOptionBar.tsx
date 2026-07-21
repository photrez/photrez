import { useEditor } from "./shell/EditorContext";
import { ToolPill, Divider } from "./shell/OptionBarShared";

export function GradientOptionBar() {
  const {
    gradientType, setGradientType,
    gradientPreset, setGradientPreset,
    fgColor, setFgColor,
    bgColor, setBgColor,
  } = useEditor();

  const handleReverse = () => {
    const temp = fgColor();
    setFgColor(bgColor());
    setBgColor(temp);
  };

  const previewGradientCss = () => {
    if (gradientPreset() === "fg-transparent") {
      return `linear-gradient(to right, ${fgColor()}, transparent)`;
    }
    return `linear-gradient(to right, ${fgColor()}, ${bgColor()})`;
  };

  return (
    <div class="flex items-center gap-2.5 px-2 text-[11px] select-none">
      {/* Tool Pill Badge */}
      <ToolPill icon="swatch" label="Gradient" />

      <Divider />

      {/* Type Dropdown */}
      <div class="flex items-center gap-1.5 text-editor-text-dim">
        <span>Type:</span>
        <select
          value={gradientType()}
          onChange={(e) => setGradientType(e.currentTarget.value as "linear" | "radial")}
          class="h-5.5 rounded border border-editor-field-border bg-editor-field px-2 py-0 text-[11px] text-editor-text outline-none focus:border-editor-accent"
        >
          <option value="linear">Linear</option>
          <option value="radial">Radial</option>
        </select>
      </div>

      <Divider />

      {/* Preset Selector */}
      <div class="flex items-center gap-1.5 text-editor-text-dim">
        <span>Preset:</span>
        <select
          value={gradientPreset()}
          onChange={(e) => setGradientPreset(e.currentTarget.value as "fg-bg" | "fg-transparent")}
          class="h-5.5 rounded border border-editor-field-border bg-editor-field px-2 py-0 text-[11px] text-editor-text outline-none focus:border-editor-accent"
        >
          <option value="fg-bg">Foreground → Background</option>
          <option value="fg-transparent">Foreground → Transparent</option>
        </select>
      </div>

      <Divider />

      {/* Live Gradient Color Preview Strip & Active Color Swatches */}
      <div class="flex items-center gap-1.5">
        {/* Start Color Swatch */}
        <div
          class="size-3.5 rounded-full border border-editor-field-border shadow-sm"
          style={{ "background-color": fgColor() }}
          title={`Start Color (Foreground): ${fgColor()}`}
        />

        {/* Gradient Strip */}
        <div
          class="relative h-5 w-24 rounded border border-editor-field-border overflow-hidden shadow-inner"
          style={{
            "background-image":
              "linear-gradient(45deg, #2a2a2a 25%, transparent 25%), linear-gradient(-45deg, #2a2a2a 25%, transparent 25%), linear-gradient(45deg, transparent 75%, #2a2a2a 75%), linear-gradient(-45deg, transparent 75%, #2a2a2a 75%)",
            "background-size": "8px 8px",
            "background-position": "0 0, 0 4px, 4px -4px, -4px 0px",
            "background-color": "#1a1a1a",
          }}
          title="Live Gradient Preview"
        >
          <div
            class="size-full"
            style={{ background: previewGradientCss() }}
          />
        </div>

        {/* End Color Swatch */}
        <div
          class="size-3.5 rounded-full border border-editor-field-border shadow-sm"
          style={{ "background-color": gradientPreset() === "fg-transparent" ? "transparent" : bgColor() }}
          title={gradientPreset() === "fg-transparent" ? "End Color: Transparent" : `End Color (Background): ${bgColor()}`}
        />
      </div>

      {/* Reverse Button (1-Click FG ↔ BG Swap) */}
      <button
        onClick={handleReverse}
        title="Reverse gradient colors (Swap Foreground ↔ Background)"
        class="flex items-center gap-1.5 h-5.5 rounded border border-editor-field-border bg-editor-field/50 px-2 text-[11px] text-editor-text-dim transition-colors hover:bg-editor-field hover:text-editor-text active:scale-95"
      >
        <span class="text-[12px]">⇄</span>
        <span>Reverse</span>
      </button>
    </div>
  );
}
