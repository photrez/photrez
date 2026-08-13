import { Show, createSignal, For } from "solid-js";
import { useEditor } from "./shell/EditorContext";
import { clampPaintPercent, clampPaintSize, clampPaintSmoothing, BRUSH_PRESETS, applyPaintPreset, sizeSliderToPaintSize, paintSizeToSizeSlider } from "./brushToolState";
import { ToolPill, MoreDropdown, Divider } from "./shell/OptionBarShared";
import { Tooltip } from "./Tooltip";
import { Icon } from "./icons";
import { Slider } from "./primitives";

function formatPercent(value: number): number {
  return Math.round(value * 100);
}

export function BrushOptionBar() {
  const {
    activeTool,
    brushSize, setBrushSize,
    brushHardness, setBrushHardness,
    brushOpacity, setBrushOpacity,
    brushFlow, setBrushFlow,
    brushSmoothing, setBrushSmoothing,
    eraserSize, setEraserSize,
    eraserHardness, setEraserHardness,
    eraserOpacity, setEraserOpacity,
    eraserFlow, setEraserFlow,
    eraserSmoothing, setEraserSmoothing,
    brushPresetId, setBrushPresetId,
    eraserPresetId, setEraserPresetId,
  } = useEditor();

  const isEraser = () => activeTool() === "eraser";
  const label = () => (isEraser() ? "Eraser" : "Brush");
  const size = () => (isEraser() ? eraserSize() : brushSize());
  const hardness = () => (isEraser() ? eraserHardness() : brushHardness());
  const opacity = () => (isEraser() ? eraserOpacity() : brushOpacity());
  const flow = () => (isEraser() ? eraserFlow() : brushFlow());
  const smoothing = () => (isEraser() ? eraserSmoothing() : brushSmoothing());
  const presetId = () => (isEraser() ? eraserPresetId() : brushPresetId());
  const setPresetId = isEraser() ? setEraserPresetId : setBrushPresetId;

  const clearPresetId = () => setPresetId(null);

  const setSize = (value: number) => {
    const next = clampPaintSize(value);
    clearPresetId();
    if (isEraser()) setEraserSize(next);
    else setBrushSize(next);
  };

  const setHardness = (value: number) => {
    const next = clampPaintPercent(value / 100);
    clearPresetId();
    if (isEraser()) setEraserHardness(next);
    else setBrushHardness(next);
  };

  const setOpacity = (value: number) => {
    const next = clampPaintPercent(value / 100);
    clearPresetId();
    if (isEraser()) setEraserOpacity(next);
    else setBrushOpacity(next);
  };

  const setFlowValue = (value: number) => {
    const next = clampPaintPercent(value / 100);
    clearPresetId();
    if (isEraser()) setEraserFlow(next);
    else setBrushFlow(next);
  };

  const setSmoothingValue = (value: number) => {
    const next = clampPaintSmoothing(value);
    clearPresetId();
    if (isEraser()) setEraserSmoothing(next);
    else setBrushSmoothing(next);
  };

  const handleSizeSliderInput = (event: Event) => {
    setSize(sizeSliderToPaintSize(Number((event.target as HTMLInputElement).value)));
  };

  const applyPreset = (preset: typeof BRUSH_PRESETS[number]) => {
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
      if (changes.eraserHardness !== undefined) setEraserHardness(changes.eraserHardness);
      if (changes.eraserOpacity !== undefined) setEraserOpacity(changes.eraserOpacity);
      if (changes.eraserFlow !== undefined) setEraserFlow(changes.eraserFlow);
      if (changes.eraserSmoothing !== undefined) setEraserSmoothing(changes.eraserSmoothing);
    } else {
      if (changes.brushSize !== undefined) setBrushSize(changes.brushSize);
      if (changes.brushHardness !== undefined) setBrushHardness(changes.brushHardness);
      if (changes.brushOpacity !== undefined) setBrushOpacity(changes.brushOpacity);
      if (changes.brushFlow !== undefined) setBrushFlow(changes.brushFlow);
      if (changes.brushSmoothing !== undefined) setBrushSmoothing(changes.brushSmoothing);
    }
    setPresetId(preset.id);
  };

  const [showPresets, setShowPresets] = createSignal(false);

  const currentPresetName = () => {
    const id = presetId();
    if (!id) return "Custom";
    const found = BRUSH_PRESETS.find(p => p.id === id);
    return found ? found.name : "Custom";
  };

  return (
    <>
      <ToolPill icon={isEraser() ? "eraser" : "brush"} label={label()} />

      <Divider />

      <div class="flex items-center gap-1.5 shrink-0">
        <label class="flex h-[24px] items-center gap-1 rounded-[4px] border border-editor-field-border bg-editor-field px-1.5 transition-colors focus-within:border-editor-accent hover:border-[#4B515D]">
          <span class="text-[10px] font-medium text-[#A1A1AA] select-none">Size</span>
          <input
            data-paint-size
            type="number"
            min="1"
            max="2000"
            value={size()}
            onInput={(e) => setSize(Number(e.currentTarget.value))}
            class="w-12 bg-transparent font-mono text-[11px] font-semibold text-white outline-none"
          />
          <span class="text-[10px] font-medium text-[#A1A1AA] select-none">px</span>
        </label>

        <div class="relative w-[72px] flex items-center h-[14px] @max-[900px]:hidden">
          <Slider
            percent={paintSizeToSizeSlider(size())}
            type="brush-size"
          />
          <input
            data-paint-size-slider
            type="range"
            min="0"
            max="100"
            value={paintSizeToSizeSlider(size())}
            onInput={handleSizeSliderInput}
            class="absolute inset-0 w-full h-[14px] opacity-0 cursor-pointer"
          />
        </div>
      </div>

      {/* Core brush params — always visible on the bar */}
      <Divider />
      <div class="flex items-center gap-1.5 shrink-0">
        <label class="flex h-[24px] items-center gap-1 rounded-[4px] border border-editor-field-border bg-editor-field px-1.5 transition-colors focus-within:border-editor-accent hover:border-[#4B515D]">
          <span class="text-[10px] font-medium text-[#A1A1AA] select-none">Hard</span>
          <input
            data-paint-hardness
            type="number"
            min="0"
            max="100"
            value={formatPercent(hardness())}
            onInput={(e) => setHardness(Number(e.currentTarget.value))}
            class="w-11 bg-transparent font-mono text-[11px] font-semibold text-white outline-none"
          />
          <span class="text-[10px] font-medium text-[#A1A1AA] select-none">%</span>
        </label>

        <label class="flex h-[24px] items-center gap-1 rounded-[4px] border border-editor-field-border bg-editor-field px-1.5 transition-colors focus-within:border-editor-accent hover:border-[#4B515D]">
          <span class="text-[10px] font-medium text-[#A1A1AA] select-none">Strength</span>
          <input
            data-paint-opacity
            type="number"
            min="0"
            max="100"
            value={formatPercent(opacity())}
            onInput={(e) => setOpacity(Number(e.currentTarget.value))}
            class="w-11 bg-transparent font-mono text-[11px] font-semibold text-white outline-none"
          />
          <span class="text-[10px] font-medium text-[#A1A1AA] select-none">%</span>
        </label>
      </div>

      {/* Overflow group — hidden at narrow widths; lives in MoreDropdown */}
      <div class="hidden @min-[880px]:flex items-center gap-1.5 shrink-0">
        <Divider />

        <label class="flex h-[24px] items-center gap-1 rounded-[4px] border border-editor-field-border bg-editor-field px-1.5 transition-colors focus-within:border-editor-accent hover:border-[#4B515D]">
          <span class="text-[10px] font-medium text-[#A1A1AA] select-none">Flow</span>
          <input
            data-paint-flow
            type="number"
            min="0"
            max="100"
            value={formatPercent(flow())}
            onInput={(e) => setFlowValue(Number(e.currentTarget.value))}
            class="w-11 bg-transparent font-mono text-[11px] font-semibold text-white outline-none"
          />
          <span class="text-[10px] font-medium text-[#A1A1AA] select-none">%</span>
        </label>

        <label class="flex h-[24px] items-center gap-1 rounded-[4px] border border-editor-field-border bg-editor-field px-1.5 transition-colors focus-within:border-editor-accent hover:border-[#4B515D]">
          <span class="text-[10px] font-medium text-[#A1A1AA] select-none">Smooth</span>
          <input
            data-paint-smoothing
            type="number"
            min="0"
            max="100"
            value={smoothing()}
            onInput={(event) => setSmoothingValue(Number(event.currentTarget.value))}
            class="w-11 bg-transparent font-mono text-[11px] font-semibold text-white outline-none"
          />
          <span class="text-[10px] font-medium text-[#A1A1AA] select-none">%</span>
        </label>

        <div class="relative">
          <Tooltip content="Brush presets">
            <button
              type="button"
              data-paint-preset
              onClick={() => setShowPresets(!showPresets())}
              class="group flex h-[24px] items-center gap-1 rounded-[4px] border border-editor-field-border bg-editor-field px-2 text-[11px] font-semibold text-white transition-colors hover:border-[#4B515D] cursor-pointer select-none"
            >
              <span>{currentPresetName()}</span>
              <Icon name="chevron-down" class="size-3 text-[#A1A1AA] group-hover:text-white transition-colors" />
            </button>
          </Tooltip>
          <Show when={showPresets()}>
            <div class="absolute top-full left-0 z-50 mt-1.5 flex flex-col rounded-[6px] border border-[#363B44] bg-[#1D2026] py-1 shadow-2xl min-w-[140px]">
              <div class="fixed inset-0 z-[-1]" onClick={() => setShowPresets(false)} />
              <For each={BRUSH_PRESETS.filter(p => p.tool === "both" || p.tool === (isEraser() ? "eraser" : "brush"))}>
                {(preset) => (
                  <button
                    type="button"
                    class={`flex items-center gap-2 px-3 py-1.5 text-[11px] whitespace-nowrap hover:bg-white/10 transition-colors cursor-pointer text-left w-full ${preset.id === presetId() ? "border-l-2 border-editor-accent bg-editor-accent/20 text-white font-semibold" : "text-[#A1A1AA] hover:text-white"}`}
                    onClick={() => { applyPreset(preset); setShowPresets(false); }}
                  >
                    {preset.name}
                  </button>
                )}
              </For>
            </div>
          </Show>
        </div>

        <Show when={isEraser()}>
          <Tooltip content="Set eraser to full hard strength">
            <button
              type="button"
              class="h-[24px] rounded-[4px] border border-editor-field-border/60 bg-editor-field/40 px-2 text-[11px] font-semibold text-[#A1A1AA] transition-colors hover:border-editor-field-border hover:bg-editor-field hover:text-white cursor-pointer select-none"
              aria-label="Set eraser to full hard strength"
              onClick={() => {
                setEraserHardness(1);
                setEraserOpacity(1);
                setEraserFlow(1);
                clearPresetId();
              }}
            >
              Hard 100
            </button>
          </Tooltip>
        </Show>
      </div>
 
      {/* Overflow dropdown for narrow container */}
      <MoreDropdown>
          {/* Flow */}
         <div class="flex flex-col gap-1">
           <span class="text-[10px] font-bold text-editor-text-dim uppercase tracking-wider">Flow</span>
           <label class="flex h-[24px] items-center gap-1 rounded-[3px] border border-editor-field-border bg-editor-field px-1.5">
             <input
               type="number"
               min="0"
               max="100"
               value={formatPercent(flow())}
               onInput={(e) => setFlowValue(Number(e.currentTarget.value))}
               class="w-full bg-transparent text-[11px] text-editor-text outline-none"
             />
             <span class="text-[10px] text-editor-text-dim">%</span>
           </label>
         </div>

         {/* Smoothing */}
         <div class="flex flex-col gap-1">
           <span class="text-[10px] font-bold text-editor-text-dim uppercase tracking-wider">Smoothing</span>
           <label class="flex h-[24px] items-center gap-1 rounded-[3px] border border-editor-field-border bg-editor-field px-1.5">
              <input
                type="number"
                min="0"
                max="100"
                value={smoothing()}
                onInput={(e) => setSmoothingValue(Number(e.currentTarget.value))}
               class="w-full bg-transparent text-[11px] text-editor-text outline-none"
             />
             <span class="text-[10px] text-editor-text-dim">%</span>
           </label>
        </div>

        <div class="h-px bg-editor-divider my-1" />

        {/* Preset Selector */}
        <div class="flex flex-col gap-1">
          <span class="text-[10px] font-bold text-editor-text-dim uppercase tracking-wider">Presets</span>
          <div class="relative">
            <Tooltip content="Brush presets">
              <button
                type="button"
                onClick={() => setShowPresets(!showPresets())}
                class="flex h-[24px] w-full items-center justify-between rounded-[3px] border border-editor-field-border bg-editor-field px-2 text-[11px] text-editor-text hover:border-editor-accent"
              >
                <span>{currentPresetName()}</span>
                <Icon name="chevron-down" class="size-3" strokeWidth={1.5} />
              </button>
            </Tooltip>
            <Show when={showPresets()}>
              <div class="absolute bottom-full right-0 z-50 mb-1 flex flex-col rounded-[4px] border border-editor-field-border bg-editor-panel py-1 shadow-lg max-h-[150px] overflow-y-auto">
                <div class="fixed inset-0 z-[-1]" onClick={() => setShowPresets(false)} />
                <For each={BRUSH_PRESETS.filter(p => p.tool === "both" || p.tool === (isEraser() ? "eraser" : "brush"))}>
                  {(preset) => (
                    <button
                      type="button"
                      class={`flex items-center gap-2 px-3 py-1.5 text-[11px] whitespace-nowrap hover:bg-editor-field/60 ${preset.id === presetId() ? "text-editor-accent font-medium" : "text-editor-text"}`}
                      onClick={() => { applyPreset(preset); setShowPresets(false); }}
                    >
                      {preset.name}
                    </button>
                  )}
                </For>
              </div>
            </Show>
          </div>
        </div>

        {/* Eraser hard-100 helper */}
        <Show when={isEraser()}>
          <Tooltip content="Set eraser to full hard strength">
            <button
              type="button"
              class="h-[24px] w-full rounded-[3px] border border-editor-field-border bg-editor-field px-2 text-[11px] text-editor-text hover:bg-editor-field/85 transition-colors mt-1"
              aria-label="Set eraser to full hard strength"
              onClick={() => {
                setEraserHardness(1);
                setEraserOpacity(1);
                setEraserFlow(1);
                clearPresetId();
              }}
            >
              Hard 100
            </button>
          </Tooltip>
        </Show>
      </MoreDropdown>
    </>
  );
}
