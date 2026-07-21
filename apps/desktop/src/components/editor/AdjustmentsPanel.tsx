import { For, Show, createSignal, createEffect, batch } from "solid-js";
import { Icon, type IconName } from "./icons";
import { useEditor } from "./shell/EditorContext";
import { SectionHeader } from "./layers/SectionHeader";
import { LayerThumb } from "./layers/LayerThumb";
import type { BasicAdjustment } from "@/engine/layerAdjustments";
import { Slider } from "./primitives";

const COMING_SOON_SECTIONS: readonly {
  icon: IconName;
  iconClass: string;
  label: string;
}[] = [
  { icon: "spline", iconClass: "text-editor-text-dim", label: "Tone Curve" },
  { icon: "palette", iconClass: "text-sky-400", label: "HSL / Color" },
  { icon: "swatch", iconClass: "text-amber-400", label: "Color Grading" },
  { icon: "sparkles", iconClass: "text-sky-300", label: "Detail" },
  {
    icon: "aperture",
    iconClass: "text-emerald-400",
    label: "Lens Corrections",
  },
] as const;

const COMING_SOON_DESCRIPTIONS: Record<string, string> = {
  "Tone Curve":
    "Non-destructive spline-based RGB tone and contrast adjustment.",
  "HSL / Color":
    "Selective color tuning for Hue, Saturation, and Luminance channels.",
  "Color Grading":
    "Three-way color wheels control for shadows, midtones, and highlights.",
  Detail:
    "Unsharp masking, high-pass sharpening, and bilateral noise reduction.",
  "Lens Corrections":
    "Chromatic aberration control, barrel distortion, and vignette corrections.",
};

export function AdjustmentsPanel() {
  const { workspace, layers, selectedLayerId, scheduler, activeDocumentId } =
    useEditor();
  // Adjustment is applied non-destructively: the slider writes the adjustment
  // param to the engine and the GPU shader re-composites instantly. No CPU
  // pixel loop, no debounce needed for the live preview.
  const [basicAdjustment, setBasicAdjustment] = createSignal<BasicAdjustment>({
    brightness: 0,
    contrast: 0,
    saturation: 0,
  });
  // Tracks the current adjustment gesture so undo gets exactly one checkpoint
  // per drag (or per property switch). Plain closure var — not reactive.
  let sessionBase: { layerId: string; lastProperty: string } | null = null;

  // Reset slider values whenever the selected layer changes
  createEffect(() => {
    selectedLayerId();
    batch(() => {
      setBasicAdjustment({ brightness: 0, contrast: 0, saturation: 0 });
      sessionBase = null;
    });
  });

  // Sync slider values from layer adjustments (for undo/redo). During an
  // active drag the engine does NOT notify the layers signal, so this effect
  // stays dormant and the slider keeps leading the engine.

  createEffect(() => {
    const layer = activeLayer();
    if (!layer) return;

    // Sync slider from layer state on external changes (layer switch, undo).
    // During an active drag the engine does NOT notify the layers signal, so
    // this effect stays dormant and the slider keeps leading.
    if (layer.basicAdjustment) {
      const cur = basicAdjustment();
      const { brightness, contrast, saturation } = layer.basicAdjustment;
      const same =
        brightness === cur.brightness &&
        contrast === cur.contrast &&
        saturation === cur.saturation;
      if (!same) setBasicAdjustment({ ...layer.basicAdjustment });
    } else if (
      basicAdjustment().brightness !== 0 ||
      basicAdjustment().contrast !== 0 ||
      basicAdjustment().saturation !== 0
    ) {
      setBasicAdjustment({ brightness: 0, contrast: 0, saturation: 0 });
    }
  });

  const activeLayer = () => {
    const id = selectedLayerId();
    if (!id) return null;
    return layers().find((l) => l.id === id) || null;
  };

  // Commit an undo checkpoint when starting a new adjustment session or
  // switching slider properties. Cheap (no pixel work).
  const commitAdjustmentSession = (propName: string) => {
    const engine = workspace.getActiveEngine();
    const history = workspace.getActiveHistory();
    const layer = activeLayer();
    if (!engine || !history || !layer?.imageBitmap || layer.locked) return;

    const switchingProp =
      sessionBase !== null && sessionBase.lastProperty !== propName;
    if (
      sessionBase === null ||
      sessionBase.layerId !== layer.id ||
      switchingProp
    ) {
      const label =
        propName === "brightness"
          ? "Adjust Brightness"
          : propName === "contrast"
            ? "Adjust Contrast"
            : "Adjust Saturation";
      history.commit(engine.snapshot(), label);
      sessionBase = { layerId: layer.id, lastProperty: propName };
    }
  };

  const setAdjustmentValue = (key: keyof BasicAdjustment, value: number) => {
    const next = { ...basicAdjustment(), [key]: value };
    setBasicAdjustment(next);
    // Commit undo checkpoint once per gesture/property switch (cheap).
    commitAdjustmentSession(key);
    // Non-destructive: push the param to the engine; the GPU shader applies
    // it instantly on the next render — no CPU pixel loop, no debounce.
    const engine = workspace.getActiveEngine();
    const layer = activeLayer();
    if (engine && layer) {
      engine.applyBasicAdjustment(layer.id, next);
      scheduler.requestRender();
    }
  };

  const hasPendingAdjustment = () => {
    const adjustment = basicAdjustment();
    return (
      adjustment.brightness !== 0 ||
      adjustment.contrast !== 0 ||
      adjustment.saturation !== 0
    );
  };

  const basicStatusText = () => {
    const layer = activeLayer();
    if (!layer) return null;
    if (layer.locked)
      return "Layer is locked. Unlock it before applying pixel adjustments.";
    if (!layer.imageBitmap)
      return "This layer has no pixels yet. Add image pixels before adjusting tone.";
    return null;
  };

  const resetBasicAdjustment = () => {
    const engine = workspace.getActiveEngine();
    const history = workspace.getActiveHistory();
    const layer = activeLayer();
    if (engine && history && layer) {
      history.commit(engine.snapshot(), "Reset Adjustments");
      engine.clearBasicAdjustments(layer.id);
      scheduler.requestRender();
    }
    sessionBase = null;
    setBasicAdjustment({ brightness: 0, contrast: 0, saturation: 0 });
  };

  return (
    <section class="flex flex-1 shrink-0 flex-col overflow-hidden bg-editor-panel">
      <div
        class="flex-1 overflow-y-auto"
        style={{ "scrollbar-gutter": "stable" }}
      >
        <Show
          when={activeDocumentId()}
          fallback={
            <div class="flex h-full flex-col items-center justify-center gap-3 text-center px-6">
              <Icon
                name="sliders"
                class="size-6 text-editor-text-dim opacity-50"
                strokeWidth={1.5}
              />
              <div class="space-y-1">
                <p class="text-[13px] font-medium text-editor-text">
                  No image open
                </p>
                <p class="text-[12px] text-editor-text-dim leading-snug">
                  Open or create an image to adjust pixels.
                </p>
              </div>
            </div>
          }
        >
          <Show
            when={activeLayer()}
            fallback={
              <div class="flex h-full flex-col items-center justify-center gap-3 text-center px-6">
                <Icon
                  name="sun"
                  class="size-6 text-editor-text-dim opacity-50"
                  strokeWidth={1.5}
                />
                <div class="space-y-1">
                  <p class="text-[13px] font-medium text-editor-text">
                    No layer selected
                  </p>
                  <p class="text-[12px] text-editor-text-dim leading-snug">
                    Select a layer to adjust its pixels.
                  </p>
                </div>
              </div>
            }
          >
            {(layer) => (
              <>
                <div class="border-b border-editor-divider px-3 py-2.5">
                  <SectionHeader
                    icon="layers"
                    iconClass="text-editor-text-dim"
                    label="Selected Layer"
                  />
                  <div class="mt-2 flex items-center gap-2.5 rounded-[4px] border border-editor-divider bg-editor-field p-2">
                    <LayerThumb layer={layer()} isActive={true} />
                    <div class="min-w-0 flex-1">
                      <p
                        class="truncate text-[11.5px] font-medium text-editor-text leading-tight"
                        title={layer().name}
                      >
                        {layer().name}
                      </p>
                      <p class="truncate text-[10.5px] text-editor-text-dim leading-snug mt-0.5">
                        {layer().type === "raster"
                          ? "Image layer"
                          : `${layer().type.charAt(0).toUpperCase()}${layer().type.slice(1)} layer`}{" "}
                        · {layer().width} × {layer().height} px
                      </p>
                    </div>
                  </div>
                </div>

                <div class="border-b border-editor-divider px-3 py-2.5">
                  <SectionHeader
                    icon="sun"
                    iconClass="text-editor-text-dim"
                    label="Basic"
                    trailing={
                      <button
                        type="button"
                        aria-label="Reset basic adjustments"
                        disabled={!hasPendingAdjustment()}
                        onClick={resetBasicAdjustment}
                        class="flex size-5 items-center justify-center rounded-[3px] text-editor-text-dim hover:bg-white/[0.045] hover:text-editor-text disabled:pointer-events-none disabled:opacity-40"
                      >
                        <Icon name="x" class="size-3.5" strokeWidth={1.75} />
                      </button>
                    }
                  />

                  <div class="mt-2 flex flex-col gap-2">
                    <AdjustmentSliderRow
                      label="Bright"
                      value={basicAdjustment().brightness}
                      onInput={(value) =>
                        setAdjustmentValue("brightness", value)
                      }
                    />
                    <AdjustmentSliderRow
                      label="Contrast"
                      value={basicAdjustment().contrast}
                      onInput={(value) => setAdjustmentValue("contrast", value)}
                    />
                    <AdjustmentSliderRow
                      label="Saturate"
                      value={basicAdjustment().saturation}
                      onInput={(value) =>
                        setAdjustmentValue("saturation", value)
                      }
                    />
                    <p class="mt-1 text-[11px] leading-snug text-editor-text-dim">
                      Drag to preview directly on the active layer. Undo
                      restores the previous pixels.
                    </p>
                    <Show when={basicStatusText()}>
                      {(message) => <StatusHint>{message()}</StatusHint>}
                    </Show>
                  </div>
                </div>

                <For each={COMING_SOON_SECTIONS}>
                  {(section) => (
                    <CollapsibleSection
                      icon={section.icon}
                      iconClass={section.iconClass}
                      label={section.label}
                    />
                  )}
                </For>
              </>
            )}
          </Show>
        </Show>
      </div>
    </section>
  );
}

function StatusHint(props: { children: string }) {
  return (
    <div class="flex items-start gap-2 rounded-[4px] border border-editor-divider bg-editor-field px-2.5 py-2 text-[11px] leading-snug text-editor-text-dim">
      <Icon
        name="sliders"
        class="mt-0.5 size-3.5 shrink-0 text-editor-text-dim"
        strokeWidth={1.75}
      />
      <span>{props.children}</span>
    </div>
  );
}

function AdjustmentSliderRow(props: {
  label: string;
  value: number;
  onInput: (value: number) => void;
}) {
  const displayValue = () =>
    props.value > 0 ? `+${props.value}` : `${props.value}`;
  const type = () => {
    if (props.label === "Bright") return "brightness";
    if (props.label === "Contrast") return "contrast";
    if (props.label === "Saturate") return "saturation";
    return "default";
  };

  return (
    <div class="flex min-h-[28px] items-center gap-2.5">
      <span class="w-[58px] shrink-0 text-[12px] font-medium text-editor-text-dim">
        {props.label}
      </span>
      <div class="flex flex-1 items-center gap-2.5">
        <div class="relative flex h-[18px] flex-1 items-center">
          <Slider
            percent={(props.value + 100) / 2}
            value={props.value}
            type={type()}
          />
          <input
            aria-label={props.label}
            type="range"
            min="-100"
            max="100"
            value={props.value}
            onInput={(e) => props.onInput(parseInt(e.currentTarget.value, 10))}
            class="absolute inset-0 h-[18px] w-full cursor-pointer opacity-0"
          />
        </div>
        <span class="flex h-[22px] w-[40px] shrink-0 items-center justify-end rounded-[3px] border border-editor-field-border bg-editor-field px-1.5 text-right text-[11px] tabular-nums text-editor-text">
          {displayValue()}
        </span>
      </div>
    </div>
  );
}

function CollapsibleSection(props: {
  icon: IconName;
  iconClass: string;
  label: string;
}) {
  const [open, setOpen] = createSignal(false);

  return (
    <div class="border-b border-editor-divider">
      <button
        onClick={() => setOpen(!open())}
        class="flex h-[42px] w-full items-center justify-between px-4 hover:bg-white/[0.03]"
      >
        <div class="flex items-center gap-2.5">
          <span class="flex size-4 items-center justify-center">
            <Icon
              name={props.icon}
              class={`size-[15px] ${props.iconClass}`}
              strokeWidth={1.75}
            />
          </span>
          <span class="text-[12.5px] text-editor-text">{props.label}</span>
        </div>
        <Icon
          name={open() ? "chevron-up" : "chevron-right"}
          class="size-4 text-editor-text-dim"
          strokeWidth={1.75}
        />
      </button>
      <Show when={open()}>
        <div class="px-4 pt-2 pb-5">
          <div class="flex flex-col gap-2 rounded-[6px] border border-dashed border-editor-field-border bg-white/[0.015] p-3 transition-colors hover:bg-white/[0.025]">
            <div class="flex items-center gap-1.5 text-editor-text-dim text-[10px] font-bold uppercase tracking-wider">
              <Icon name="sparkles" class="size-3 opacity-70" strokeWidth={2} />
              <span>In Development</span>
            </div>
            <p class="text-[11.5px] leading-relaxed text-editor-text-dim">
              {COMING_SOON_DESCRIPTIONS[props.label] ||
                "This professional adjustment tool is currently in development."}
            </p>
          </div>
        </div>
      </Show>
    </div>
  );
}
