import { useEditor } from "./shell/EditorContext";
import { ToolPill, Divider, SelectDropdown } from "./shell/OptionBarShared";

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
      <SelectDropdown
        labelPrefix="Type"
        value={gradientType()}
        options={[
          { value: "linear", label: "Linear" },
          { value: "radial", label: "Radial" },
        ]}
        onChange={(v) => setGradientType(v as "linear" | "radial")}
      />

      <Divider />

      {/* Preset Selector */}
      <SelectDropdown
        labelPrefix="Preset"
        value={gradientPreset()}
        options={[
          { value: "fg-bg", label: "Foreground → Background" },
          { value: "fg-transparent", label: "Foreground → Transparent" },
        ]}
        onChange={(v) => setGradientPreset(v as "fg-bg" | "fg-transparent")}
      />

      <Divider />

      {/* Live Gradient Color Preview Strip & Active Color Swatches */}
      <div class="flex items-center gap-1.5">
        {/* Start Color Swatch */}
        <div
          class="size-3.5 rounded-full border border-editor-field-border ring-1 ring-white/20 shadow-sm"
          style={{ "background-color": fgColor() }}
          title={`Start Color (Foreground): ${fgColor()}`}
        />

        {/* Gradient Strip */}
        <div
          class="relative h-5 w-24 rounded border border-[#363B44] overflow-hidden shadow-inner ring-1 ring-white/10"
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
          class="size-3.5 rounded-full border border-editor-field-border ring-1 ring-white/20 shadow-sm"
          style={{ "background-color": gradientPreset() === "fg-transparent" ? "transparent" : bgColor() }}
          title={gradientPreset() === "fg-transparent" ? "End Color: Transparent" : `End Color (Background): ${bgColor()}`}
        />
      </div>

      {/* Reverse Button (1-Click FG ↔ BG Swap) */}
      <button
        type="button"
        onClick={handleReverse}
        title="Reverse gradient colors (Swap Foreground ↔ Background)"
        class="flex h-6 items-center gap-1.5 rounded-[4px] border border-editor-field-border/60 bg-editor-field/40 px-2 text-[11px] font-semibold text-[#A1A1AA] transition-colors hover:border-editor-field-border hover:bg-editor-field hover:text-white cursor-pointer select-none"
      >
        <span class="text-[12px]">⇄</span>
        <span>Reverse</span>
      </button>
    </div>
  );
}
