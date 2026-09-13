import { For, Show, createSignal, createEffect, batch } from "solid-js";
import { Icon, type IconName } from "./icons";
import { useEditor } from "./shell/EditorContext";
import { SectionHeader } from "./layers/SectionHeader";
import { LayerThumb } from "./layers/LayerThumb";
import type { BasicAdjustment } from "@/engine/layerAdjustments";
import type { DocumentEngine } from "@/engine/document";
import { Slider } from "./primitives";
import { useI18n } from "@/i18n/I18nProvider";
import { showToast } from "./Toast";
import {
  commitFacadeAdjustment,
  isFacadeEnabled,
  setAdjustmentPreview,
  clearAdjustmentPreview,
  MIXED_OWNERSHIP_MESSAGE,
} from "@/lib/protocol/facadeRegistry";

const COMING_SOON_SECTIONS = [
  {
    icon: "spline" as IconName,
    iconClass: "text-editor-text-dim",
    labelKey: "adjustments.toneCurve",
    defaultLabel: "Tone Curve",
    descKey: "adjustments.toneCurveDesc",
    defaultDesc: "Non-destructive spline-based RGB tone and contrast adjustment.",
  },
  {
    icon: "palette" as IconName,
    iconClass: "text-sky-400",
    labelKey: "adjustments.hslColor",
    defaultLabel: "HSL / Color",
    descKey: "adjustments.hslColorDesc",
    defaultDesc: "Selective color tuning for Hue, Saturation, and Luminance channels.",
  },
  {
    icon: "swatch" as IconName,
    iconClass: "text-amber-400",
    labelKey: "adjustments.colorGrading",
    defaultLabel: "Color Grading",
    descKey: "adjustments.colorGradingDesc",
    defaultDesc: "Three-way color wheels control for shadows, midtones, and highlights.",
  },
  {
    icon: "sparkles" as IconName,
    iconClass: "text-sky-300",
    labelKey: "adjustments.detail",
    defaultLabel: "Detail",
    descKey: "adjustments.detailDesc",
    defaultDesc: "Unsharp masking, high-pass sharpening, and bilateral noise reduction.",
  },
  {
    icon: "aperture" as IconName,
    iconClass: "text-emerald-400",
    labelKey: "adjustments.lensCorrections",
    defaultLabel: "Lens Corrections",
    descKey: "adjustments.lensCorrectionsDesc",
    defaultDesc: "Chromatic aberration control, barrel distortion, and vignette corrections.",
  },
] as const;

export function AdjustmentsPanel() {
  const { t } = useI18n();
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

  // Active routed-transient gesture: set on the first slider tick of a
  // facade-owned drag, cleared at the gesture boundary. While non-null the
  // engine model is intentionally held at its pre-gesture value and the live
  // preview rides the transient adjustment preview (see facadeRegistry
  // applyFacadePreviews). Plain closure var - not reactive.
  let pendingAdjustment: { layerId: string; start: BasicAdjustment; key: keyof BasicAdjustment } | null = null;

  const zeroAdjustment = (): BasicAdjustment => ({ brightness: 0, contrast: 0, saturation: 0 });

  // Reset slider values whenever the selected layer changes
  createEffect(() => {
    selectedLayerId();
    batch(() => {
      setBasicAdjustment({ brightness: 0, contrast: 0, saturation: 0 });
      sessionBase = null;
      pendingAdjustment = null;
      clearAdjustmentPreview();
    });
  });

  // Sync slider values from layer adjustments (for undo/redo). During an
  // active drag the engine does NOT notify the layers signal, so this effect
  // stays dormant and the slider keeps leading the engine.

  createEffect(() => {
    const layer = activeLayer();
    if (!layer) return;

    // A routed drag holds the model at its pre-gesture value while the slider
    // leads and the transient preview supplies the render; skip the
    // model->slider sync until the gesture boundary commits.
    if (pendingAdjustment) return;

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

  // Route an adjustment (or a clear when adjustment is undefined) to the native
  // engine, falling back to the caller's legacy path only when the shared
  // ownership policy says the target is not facade-owned ("legacy"). Dispatches
  // are serialized: the EditorFacade requires each command to be awaited before
  // the next (expectedVersion is bumped post-await), and a discrete reset can
  // overlap a drag-end commit, so an un-chained pair could reject with
  // E_VERSION_MISMATCH.
  let adjustmentRouteChain: Promise<void> = Promise.resolve();

  // Re-sync the slider from the authoritative model after a routed dispatch
  // failed: the panel signal was advanced optimistically, so leaving it would
  // show a value the model never adopted.
  const resyncAdjustmentFromModel = () => {
    // A newer gesture may already own the preview. Overwriting the slider here
    // would make that gesture read moved=false at its boundary and drop its
    // commit. Only resync when no gesture is in flight.
    if (pendingAdjustment !== null) return;
    const current = activeLayer()?.basicAdjustment;
    setBasicAdjustment(current ? { ...current } : zeroAdjustment());
  };

  const routeAdjustment = (
    engine: DocumentEngine,
    layerId: string,
    adjustment: BasicAdjustment | undefined,
    legacy: () => void,
  ) => {
    adjustmentRouteChain = adjustmentRouteChain
      .then(async () => {
        try {
          const r = await commitFacadeAdjustment(engine as never, [layerId], adjustment);
          if (r.status === "mixed-rejected") {
            showToast(MIXED_OWNERSHIP_MESSAGE, "error");
            resyncAdjustmentFromModel();
            return;
          }
          if (r.status === "applied" || r.status === "noop" || r.status === "empty") {
            scheduler.requestRender();
            return;
          }
          legacy(); // status === "legacy"
        } catch (err) {
          showToast(`Cannot set adjustment: ${(err as Error).message}`, "error");
          resyncAdjustmentFromModel();
        }
      })
      .catch(() => {});
  };

  const setAdjustmentValue = (key: keyof BasicAdjustment, value: number) => {
    const engine = workspace.getActiveEngine();
    const layer = activeLayer();
    const next = { ...basicAdjustment(), [key]: value };

    // Facade-owned drag: mark the gesture BEFORE writing the panel signal. The
    // model->slider sync effect runs synchronously on that write; without the
    // pending marker it would immediately reset the slider to the (unchanged)
    // model value. Every tick stays transient here - the single SetAdjustment
    // fires at the gesture boundary, so undo gets ONE native entry per gesture.
    if (engine && layer && layer.imageBitmap && isFacadeEnabled() && !layer.locked) {
      if (pendingAdjustment === null) {
        pendingAdjustment = {
          layerId: layer.id,
          start: layer.basicAdjustment ? { ...layer.basicAdjustment } : zeroAdjustment(),
          key,
        };
      } else {
        pendingAdjustment.key = key;
      }
      setBasicAdjustment(next);
      setAdjustmentPreview({ layerId: layer.id, adjustment: next });
      scheduler.requestRender();
      return;
    }

    setBasicAdjustment(next);
    if (!engine || !layer) return;
    // Oracle parity: the legacy apply no-ops on a layer with no pixels
    // (DocumentEngine.applyBasicAdjustment guards !imageBitmap). Keep the guard
    // on the routed path too, so the native arm never applies an adjustment to a
    // bitmap-less layer the legacy path would have skipped.
    if (!layer.imageBitmap) return;

    // Legacy path (flag OFF) plus the locked-layer oracle case: a locked layer
    // makes no undo entry (commitAdjustmentSession returns early on `locked`),
    // so flag ON must not create a native entry legacy would not.
    commitAdjustmentSession(key);
    // Non-destructive: push the param to the engine; the GPU shader applies
    // it instantly on the next render - no CPU pixel loop, no debounce.
    engine.applyBasicAdjustment(layer.id, next);
    scheduler.requestRender();
  };

  // Slider gesture boundary (pointerup / change / blur): dispatch ONE routed
  // SetAdjustment with the final value. A no-op when no routed gesture is
  // pending (flag OFF already committed per tick) or the value never moved.
  const finishAdjustmentEdit = () => {
    const pending = pendingAdjustment;
    if (!pending) return;
    pendingAdjustment = null;
    clearAdjustmentPreview();
    const engine = workspace.getActiveEngine();
    const layer = activeLayer();
    if (!engine || !layer || layer.id !== pending.layerId) {
      scheduler.requestRender();
      return;
    }
    const final = basicAdjustment();
    const start = pending.start;
    const moved =
      final.brightness !== start.brightness ||
      final.contrast !== start.contrast ||
      final.saturation !== start.saturation;
    if (!moved) {
      scheduler.requestRender();
      return;
    }
    const legacy = () => {
      commitAdjustmentSession(pending.key);
      engine.applyBasicAdjustment(layer.id, final);
      scheduler.requestRender();
    };
    routeAdjustment(engine, layer.id, final, legacy);
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
    if (engine && layer) {
      const legacy = () => {
        if (!history) return;
        history.commit(engine.snapshot(), "Reset Adjustments");
        engine.clearBasicAdjustments(layer.id);
        scheduler.requestRender();
      };
      if (isFacadeEnabled()) {
        // The clear has no !imageBitmap guard in the oracle (clearBasicAdjustments
        // drops the param on any existing layer), so route it unconditionally.
        routeAdjustment(engine, layer.id, undefined, legacy);
      } else {
        legacy();
      }
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
                  {t("history.noImageOpen", "No image open")}
                </p>
                <p class="text-[12px] text-editor-text-dim leading-snug">
                  {t("properties.noImageOpenDesc", "Open or create an image to adjust pixels.")}
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
                    {t("adjustments.noLayerSelected", "No layer selected")}
                  </p>
                  <p class="text-[12px] text-editor-text-dim leading-snug">
                    {t("adjustments.selectLayerToAdjust", "Select a layer to adjust its pixels.")}
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
                    label={t("adjustments.selectedLayer", "Selected Layer")}
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
                    label={t("adjustments.basic", "Basic")}
                    trailing={
                      <button
                        type="button"
                        aria-label={t("common.reset", "Reset basic adjustments")}
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
                      label={t("adjustments.bright", "Bright")}
                      value={basicAdjustment().brightness}
                      type="brightness"
                      onCommit={finishAdjustmentEdit}
                      onInput={(value) =>
                        setAdjustmentValue("brightness", value)
                      }
                    />
                    <AdjustmentSliderRow
                      label={t("adjustments.contrast", "Contrast")}
                      value={basicAdjustment().contrast}
                      type="contrast"
                      onCommit={finishAdjustmentEdit}
                      onInput={(value) =>
                        setAdjustmentValue("contrast", value)
                      }
                    />
                    <AdjustmentSliderRow
                      label={t("adjustments.saturate", "Saturate")}
                      value={basicAdjustment().saturation}
                      type="saturation"
                      onCommit={finishAdjustmentEdit}
                      onInput={(value) =>
                        setAdjustmentValue("saturation", value)
                      }
                    />
                    <p class="mt-1 text-[11px] leading-snug text-editor-text-dim">
                      {t("adjustments.dragToPreview", "Drag to preview directly on the active layer. Undo restores the previous pixels.")}
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
                      label={t(section.labelKey, section.defaultLabel)}
                      description={t(section.descKey, section.defaultDesc)}
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
  type?: "brightness" | "contrast" | "saturation" | "default";
  onInput: (value: number) => void;
  onCommit: () => void;
}) {
  const displayValue = () =>
    props.value > 0 ? `+${props.value}` : `${props.value}`;
  const type = () => props.type || "default";

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
            onPointerUp={props.onCommit}
            onPointerCancel={props.onCommit}
            onBlur={props.onCommit}
            onChange={props.onCommit}
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
  description: string;
}) {
  const { t } = useI18n();
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
              <span>{t("adjustments.inDevelopment", "In Development")}</span>
            </div>
            <p class="text-[11.5px] leading-relaxed text-editor-text-dim">
              {props.description}
            </p>
          </div>
        </div>
      </Show>
    </div>
  );
}
