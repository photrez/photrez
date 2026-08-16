import { createSignal, onMount, onCleanup, Show, For } from "solid-js";
import { useEditor } from "./shell/EditorContext";
import {
  clampPaintPercent,
  clampPaintSize,
  BRUSH_PRESETS,
  applyPaintPreset,
  sizeSliderToPaintSize,
  paintSizeToSizeSlider,
} from "./brushToolState";
import { Slider } from "./primitives";
import { useI18n } from "@/i18n/I18nProvider";

export function BrushContextMenu() {
  const { t } = useI18n();
  const {
    activeTool,
    brushSize,
    setBrushSize,
    brushHardness,
    setBrushHardness,
    brushOpacity,
    setBrushOpacity,
    brushFlow,
    setBrushFlow,
    brushSmoothing,
    setBrushSmoothing,
    eraserSize,
    setEraserSize,
    eraserHardness,
    setEraserHardness,
    eraserOpacity,
    setEraserOpacity,
    eraserFlow,
    setEraserFlow,
    eraserSmoothing,
    setEraserSmoothing,
    brushPresetId,
    setBrushPresetId,
    eraserPresetId,
    setEraserPresetId,
  } = useEditor();

  const [isOpen, setIsOpen] = createSignal(false);
  const [menuPos, setMenuPos] = createSignal({ x: 0, y: 0 });

  const isEraser = () => activeTool() === "eraser";
  const isPaintTool = () =>
    activeTool() === "brush" || activeTool() === "eraser";

  const size = () => (isEraser() ? eraserSize() : brushSize());
  const hardness = () => (isEraser() ? eraserHardness() : brushHardness());
  const opacity = () => (isEraser() ? eraserOpacity() : brushOpacity());
  const presetId = () => (isEraser() ? eraserPresetId() : brushPresetId());
  const setPresetId = (value: string | null) => {
    if (isEraser()) setEraserPresetId(value);
    else setBrushPresetId(value);
  };

  const clearPresetId = () => setPresetId(null);

  const setSizeValue = (value: number) => {
    const next = clampPaintSize(value);
    clearPresetId();
    if (isEraser()) setEraserSize(next);
    else setBrushSize(next);
  };

  const setHardnessValue = (value: number) => {
    const next = clampPaintPercent(value / 100);
    clearPresetId();
    if (isEraser()) setEraserHardness(next);
    else setBrushHardness(next);
  };

  const setOpacityValue = (value: number) => {
    const next = clampPaintPercent(value / 100);
    clearPresetId();
    if (isEraser()) setEraserOpacity(next);
    else setBrushOpacity(next);
  };

  const handleSizeInput = (e: Event) => {
    setSizeValue(sizeSliderToPaintSize(Number((e.target as HTMLInputElement).value)));
  };

  const handleHardnessInput = (e: Event) => {
    setHardnessValue(Number((e.target as HTMLInputElement).value));
  };

  const handleStrengthInput = (e: Event) => {
    setOpacityValue(Number((e.target as HTMLInputElement).value));
  };

  const applyPreset = (preset: (typeof BRUSH_PRESETS)[number]) => {
    const tool = isEraser() ? "eraser" : "brush";
    const changes = applyPaintPreset(preset, tool, {
      brushSize: brushSize(),
      brushHardness: brushHardness(),
      brushOpacity: brushOpacity(),
      brushFlow: brushFlow(),
      brushSmoothing: brushSmoothing(),
      eraserSize: eraserSize(),
      eraserHardness: eraserHardness(),
      eraserOpacity: eraserOpacity(),
      eraserFlow: eraserFlow(),
      eraserSmoothing: eraserSmoothing(),
    });
    if (isEraser()) {
      if (changes.eraserSize !== undefined) setEraserSize(changes.eraserSize);
      if (changes.eraserHardness !== undefined)
        setEraserHardness(changes.eraserHardness);
      if (changes.eraserOpacity !== undefined)
        setEraserOpacity(changes.eraserOpacity);
      if (changes.eraserFlow !== undefined) setEraserFlow(changes.eraserFlow);
      if (changes.eraserSmoothing !== undefined)
        setEraserSmoothing(changes.eraserSmoothing);
    } else {
      if (changes.brushSize !== undefined) setBrushSize(changes.brushSize);
      if (changes.brushHardness !== undefined)
        setBrushHardness(changes.brushHardness);
      if (changes.brushOpacity !== undefined)
        setBrushOpacity(changes.brushOpacity);
      if (changes.brushFlow !== undefined) setBrushFlow(changes.brushFlow);
      if (changes.brushSmoothing !== undefined)
        setBrushSmoothing(changes.brushSmoothing);
    }
    setPresetId(preset.id);
    setIsOpen(false);
  };

  const handleContextMenu = (e: MouseEvent) => {
    if (!isPaintTool()) return;
    e.preventDefault();
    setIsOpen(true);
    setMenuPos({ x: e.clientX, y: e.clientY });
  };

  const close = () => setIsOpen(false);

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape" && isOpen()) {
      close();
    }
  };

  const filteredPresets = () =>
    BRUSH_PRESETS.filter(
      (p) => p.tool === "both" || p.tool === (isEraser() ? "eraser" : "brush"),
    );

  onMount(() => {
    const container = document.getElementById("canvas-container");
    if (container) {
      container.addEventListener("contextmenu", handleContextMenu);
    }
    window.addEventListener("keydown", handleKeyDown);
    onCleanup(() => {
      if (container) {
        container.removeEventListener("contextmenu", handleContextMenu);
      }
      window.removeEventListener("keydown", handleKeyDown);
    });
  });

  return (
    <Show when={isOpen()}>
      <div class="fixed inset-0 z-40" onClick={close} />
      <div
        class="fixed z-50 flex flex-col gap-3 rounded-md border border-editor-field-border bg-editor-panel p-3 shadow-xl"
        style={{
          left: `${Math.min(menuPos().x, window.innerWidth - 220)}px`,
          top: `${Math.min(menuPos().y, window.innerHeight - 260)}px`,
          width: "200px",
        }}
      >
        <div class="flex flex-col gap-1">
          <span class="text-[10px] font-medium text-editor-text-dim">
            {t("tools.options.size", "Size")}: {size()}px
          </span>
          <div class="relative flex items-center h-[14px]">
            <Slider
              percent={paintSizeToSizeSlider(size())}
              type="brush-size"
            />
            <input
              data-context-size
              type="range"
              min="0"
              max="100"
              value={paintSizeToSizeSlider(size())}
              onInput={handleSizeInput}
              class="absolute inset-0 w-full h-[14px] opacity-0 cursor-pointer"
            />
          </div>
        </div>

        <div class="flex flex-col gap-1">
          <span class="text-[10px] font-medium text-editor-text-dim">
            {t("tools.options.hardness", "Hardness")}: {Math.round(hardness() * 100)}%
          </span>
          <div class="relative flex items-center h-[14px]">
            <Slider
              percent={Math.round(hardness() * 100)}
              type="brush-hardness"
            />
            <input
              data-context-hardness
              type="range"
              min="0"
              max="100"
              value={Math.round(hardness() * 100)}
              onInput={handleHardnessInput}
              class="absolute inset-0 w-full h-[14px] opacity-0 cursor-pointer"
            />
          </div>
        </div>

        <div class="flex flex-col gap-1">
          <span class="text-[10px] font-medium text-editor-text-dim">
            {t("tools.options.strength", "Strength")}: {Math.round(opacity() * 100)}%
          </span>
          <div class="relative flex items-center h-[14px]">
            <Slider
              percent={Math.round(opacity() * 100)}
              type="brush-strength"
            />
            <input
              data-context-strength
              type="range"
              min="0"
              max="100"
              value={Math.round(opacity() * 100)}
              onInput={handleStrengthInput}
              class="absolute inset-0 w-full h-[14px] opacity-0 cursor-pointer"
            />
          </div>
        </div>

        <div class="grid grid-cols-2 gap-1">
          <For each={filteredPresets()}>
            {(preset) => (
              <button
                type="button"
                class={`rounded-[3px] px-2 py-1 text-[10px] text-left border ${
                  preset.id === presetId()
                    ? "border-editor-accent/40 bg-editor-accent/10 text-editor-accent"
                    : "border-editor-field-border bg-editor-field text-editor-text hover:border-editor-accent"
                }`}
                onClick={() => applyPreset(preset)}
              >
                {preset.name}
              </button>
            )}
          </For>
        </div>

        <button
          type="button"
          class="rounded-[3px] border border-editor-field-border bg-editor-field px-2 py-1 text-[10px] text-editor-text hover:border-editor-accent"
          onClick={() => {
            if (isEraser()) {
              setEraserSize(32);
              setEraserHardness(1);
              setEraserOpacity(1);
              setEraserFlow(1);
              setEraserSmoothing(0);
            } else {
              setBrushSize(20);
              setBrushHardness(0.8);
              setBrushOpacity(1);
              setBrushFlow(1);
              setBrushSmoothing(0);
            }
            clearPresetId();
            close();
          }}
        >
          {t("common.reset", "Reset")}
        </button>
      </div>
    </Show>
  );
}
