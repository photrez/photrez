// SPDX-License-Identifier: AGPL-3.0-or-later
import { For, Show, createSignal, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import type {
  PaperDimensions,
  PaperPresetId,
  PrintOptions,
  PrintOrientation,
  PrintUnit,
} from "./printTypes";
import { PAPER_PRESETS } from "./printTypes";
import {
  calculateEffectivePPI,
  calculateScaleToFit,
  convertMmToUnit,
  convertUnitToMm,
  getPPIQualityLevel,
} from "./printGeometry";

interface PrintInspectorProps {
  options: PrintOptions;
  setOptions: (updater: (prev: PrintOptions) => PrintOptions) => void;
  docWidthPx: number;
  docHeightPx: number;
}

export function PrintInspector(props: PrintInspectorProps) {
  const [printers, setPrinters] = createSignal<string[]>([]);
  const [loadingPrinters, setLoadingPrinters] = createSignal(true);

  // Accordion collapsible section states
  const [printerOpen, setPrinterOpen] = createSignal(true);
  const [colorOpen, setColorOpen] = createSignal(false);
  const [positionOpen, setPositionOpen] = createSignal(true);

  // Fetch printers on mount
  onMount(async () => {
    try {
      const res = (await invoke("get_system_printers")) as {
        ok: boolean;
        data?: { printers: string[]; default?: string };
      };
      if (res.ok && res.data) {
        const list = res.data.printers || [];
        setPrinters(list);
        if (list.length > 0 && !props.options.selectedPrinter) {
          const defaultP = res.data.default || list[0];
          props.setOptions((prev) => ({ ...prev, selectedPrinter: defaultP }));
        }
      }
    } catch {
      // Fallback if printers command fails
      setPrinters(["System Default Printer"]);
    } finally {
      setLoadingPrinters(false);
    }
  });

  // Calculate live effective PPI
  const imageMmWidth = () => {
    const scaleFactor = props.options.scalePercent / 100;
    return (props.docWidthPx / 300) * 25.4 * scaleFactor;
  };

  const imageMmHeight = () => {
    const scaleFactor = props.options.scalePercent / 100;
    return (props.docHeightPx / 300) * 25.4 * scaleFactor;
  };

  const currentPPI = () =>
    calculateEffectivePPI(
      props.docWidthPx,
      props.docHeightPx,
      imageMmWidth(),
      imageMmHeight()
    );

  const ppiQuality = () => getPPIQualityLevel(currentPPI());

  // Open native OS printer properties preferences dialog
  const handleOpenPrinterProperties = async () => {
    if (!props.options.selectedPrinter) return;
    try {
      await invoke("open_printer_properties", {
        printer: props.options.selectedPrinter,
      });
    } catch {
      // Native driver dialog unavailable or cancelled
    }
  };

  // Handle preset change
  const handlePaperPresetChange = (presetId: PaperPresetId) => {
    const preset = PAPER_PRESETS[presetId];
    props.setOptions((prev) => {
      let w = preset.widthMm;
      let h = preset.heightMm;

      if (prev.orientation === "landscape") {
        if (w < h) {
          const tmp = w;
          w = h;
          h = tmp;
        }
      } else {
        if (w > h) {
          const tmp = w;
          w = h;
          h = tmp;
        }
      }

      return {
        ...prev,
        paperPreset: presetId,
        paperWidthMm: w,
        paperHeightMm: h,
      };
    });
  };

  // Handle Orientation toggle
  const handleOrientationToggle = (newOrientation: PrintOrientation) => {
    if (props.options.orientation === newOrientation) return;
    props.setOptions((prev) => {
      const oldW = prev.paperWidthMm;
      const oldH = prev.paperHeightMm;
      return {
        ...prev,
        orientation: newOrientation,
        paperWidthMm: oldH,
        paperHeightMm: oldW,
      };
    });
  };

  // Handle Scale to Fit toggle
  const handleScaleToFitToggle = (fit: boolean) => {
    if (fit) {
      const result = calculateScaleToFit(
        props.docWidthPx,
        props.docHeightPx,
        props.options.paperWidthMm,
        props.options.paperHeightMm,
        props.options.marginMm
      );
      props.setOptions((prev) => ({
        ...prev,
        scaleToFit: true,
        centerImage: true,
        scalePercent: result.scalePercent,
        leftOffsetMm: result.leftOffsetMm,
        topOffsetMm: result.topOffsetMm,
      }));
    } else {
      props.setOptions((prev) => ({ ...prev, scaleToFit: false }));
    }
  };

  // Handle unit input changes
  const handleWidthChange = (valUnit: number) => {
    const valMm = convertUnitToMm(valUnit, props.options.unit);
    const refMm = (props.docWidthPx / 300) * 25.4;
    const scale = Number(((valMm / refMm) * 100).toFixed(2));
    props.setOptions((prev) => ({
      ...prev,
      scalePercent: scale,
      scaleToFit: false,
    }));
  };

  const handleHeightChange = (valUnit: number) => {
    const valMm = convertUnitToMm(valUnit, props.options.unit);
    const refMm = (props.docHeightPx / 300) * 25.4;
    const scale = Number(((valMm / refMm) * 100).toFixed(2));
    props.setOptions((prev) => ({
      ...prev,
      scalePercent: scale,
      scaleToFit: false,
    }));
  };

  return (
    <div class="flex w-[380px] shrink-0 flex-col border-l border-editor-divider bg-editor-panel text-editor-text text-[11.5px] select-none overflow-y-auto">
      {/* --- 1. PRINTER SETUP CONTINUOUS SECTION --- */}
      <div class="flex flex-col border-b border-editor-divider">
        <button
          type="button"
          class="flex h-[38px] shrink-0 items-center justify-between px-3.5 bg-editor-panel hover:bg-editor-hover/40 text-[12px] font-semibold text-editor-text-header cursor-pointer transition-colors border-b border-editor-divider"
          onClick={() => setPrinterOpen(!printerOpen())}
        >
          <div class="flex items-center gap-2">
            <svg
              class={`size-3.5 text-editor-text-dim transition-transform duration-150 ${
                printerOpen() ? "rotate-90" : ""
              }`}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              stroke-width="2"
            >
              <path stroke-linecap="round" stroke-linejoin="round" d="M9 5l7 7-7 7" />
            </svg>
            <span>Printer Setup</span>
          </div>
          <span class="text-[11px] text-editor-text-dim font-normal">
            System Spooler
          </span>
        </button>

        <Show when={printerOpen()}>
          <div class="p-3.5 flex flex-col gap-3 bg-editor-panel/40">
            {/* Printer Dropdown */}
            <div class="flex items-center justify-between gap-2">
              <label class="w-[72px] shrink-0 text-editor-text-dim text-[11px] font-medium">
                Printer:
              </label>
              <select
                class="flex-1 rounded-[4px] border border-editor-field-border bg-editor-field px-2.5 py-1 text-[11px] text-editor-text focus:border-editor-accent focus:outline-none transition-colors"
                value={props.options.selectedPrinter}
                disabled={loadingPrinters()}
                onChange={(e) =>
                  props.setOptions((prev) => ({
                    ...prev,
                    selectedPrinter: e.currentTarget.value,
                  }))
                }
              >
                <Show
                  when={!loadingPrinters()}
                  fallback={<option>Loading printers...</option>}
                >
                  <For each={printers()}>
                    {(p) => <option value={p}>{p}</option>}
                  </For>
                </Show>
              </select>
            </div>

            {/* Copies & Print Settings... */}
            <div class="flex items-center justify-between gap-2">
              <div class="flex items-center gap-2">
                <label class="w-[72px] shrink-0 text-editor-text-dim text-[11px] font-medium">
                  Copies:
                </label>
                <input
                  type="number"
                  min="1"
                  max="999"
                  class="w-[64px] rounded-[4px] border border-editor-field-border bg-editor-field px-2 py-1 text-[11px] text-editor-text focus:border-editor-accent focus:outline-none transition-colors"
                  value={props.options.copies}
                  onInput={(e) =>
                    props.setOptions((prev) => ({
                      ...prev,
                      copies: Math.max(1, parseInt(e.currentTarget.value) || 1),
                    }))
                  }
                />
              </div>

              {/* Native OS Driver Dialog Trigger */}
              <button
                type="button"
                class="h-[28px] rounded-[4px] border border-editor-field-border bg-editor-field px-3 text-[11px] font-medium text-editor-text hover:bg-editor-hover active:bg-editor-active transition-colors"
                onClick={handleOpenPrinterProperties}
              >
                Print Settings...
              </button>
            </div>

            {/* Layout Orientation Segmented Buttons */}
            <div class="flex items-center justify-between gap-2 mt-0.5">
              <label class="w-[72px] shrink-0 text-editor-text-dim text-[11px] font-medium">
                Layout:
              </label>
              <div class="flex items-center rounded-[4px] border border-editor-field-border bg-editor-field p-0.5 flex-1">
                <button
                  type="button"
                  title="Portrait"
                  class={`flex flex-1 items-center justify-center gap-1.5 h-[24px] rounded-[3px] text-[11px] transition-colors cursor-pointer ${
                    props.options.orientation === "portrait"
                      ? "bg-editor-accent text-white font-semibold shadow-xs"
                      : "text-editor-text-dim hover:text-editor-text hover:bg-editor-hover/50 font-normal"
                  }`}
                  onClick={() => handleOrientationToggle("portrait")}
                >
                  <svg class="size-3.5 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
                    <rect x="6" y="4" width="12" height="16" rx="1.5" />
                  </svg>
                  Portrait
                </button>
                <button
                  type="button"
                  title="Landscape"
                  class={`flex flex-1 items-center justify-center gap-1.5 h-[24px] rounded-[3px] text-[11px] transition-colors cursor-pointer ${
                    props.options.orientation === "landscape"
                      ? "bg-editor-accent text-white font-semibold shadow-xs"
                      : "text-editor-text-dim hover:text-editor-text hover:bg-editor-hover/50 font-normal"
                  }`}
                  onClick={() => handleOrientationToggle("landscape")}
                >
                  <svg class="size-3.5 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
                    <rect x="4" y="6" width="16" height="12" rx="1.5" />
                  </svg>
                  Landscape
                </button>
              </div>
            </div>
          </div>
        </Show>
      </div>

      {/* --- 2. COLOR MANAGEMENT CONTINUOUS SECTION --- */}
      <div class="flex flex-col border-b border-editor-divider">
        <button
          type="button"
          class="flex h-[38px] shrink-0 items-center justify-between px-3.5 bg-editor-panel hover:bg-editor-hover/40 text-[12px] font-semibold text-editor-text-header cursor-pointer transition-colors border-b border-editor-divider"
          onClick={() => setColorOpen(!colorOpen())}
        >
          <div class="flex items-center gap-2">
            <svg
              class={`size-3.5 text-editor-text-dim transition-transform duration-150 ${
                colorOpen() ? "rotate-90" : ""
              }`}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              stroke-width="2"
            >
              <path stroke-linecap="round" stroke-linejoin="round" d="M9 5l7 7-7 7" />
            </svg>
            <span>Color Management</span>
          </div>
          <span class="text-[10.5px] text-amber-400 font-medium">v1 Deferred</span>
        </button>

        <Show when={colorOpen()}>
          <div class="p-3.5 flex flex-col gap-2.5 text-[11px] bg-editor-panel/40">
            {/* Clean Property Row */}
            <div class="flex items-center justify-between py-0.5">
              <span class="text-editor-text-dim font-medium">Document Profile:</span>
              <span class="font-semibold text-editor-text">sRGB IEC61966-2.1</span>
            </div>

            <div class="flex items-center justify-between gap-2">
              <label class="text-editor-text-dim font-medium">Color Handling:</label>
              <select
                class="rounded-[4px] border border-editor-field-border bg-editor-field px-2.5 py-1 text-[11px] text-editor-text opacity-70"
                disabled
              >
                <option>Printer Manages Colors</option>
              </select>
            </div>

            <div
              class="flex items-center justify-between gap-2 opacity-50 cursor-not-allowed"
              title="ICC Soft-proofing deferred to post-v1 release"
            >
              <label class="text-editor-text-dim font-medium">Rendering Intent:</label>
              <select
                class="rounded-[4px] border border-editor-field-border bg-editor-field px-2.5 py-1 text-[11px] text-editor-text"
                disabled
              >
                <option>Perceptual</option>
              </select>
            </div>
          </div>
        </Show>
      </div>

      {/* --- 3. POSITION AND SIZE CONTINUOUS SECTION --- */}
      <div class="flex flex-col">
        <button
          type="button"
          class="flex h-[38px] shrink-0 items-center justify-between px-3.5 bg-editor-panel hover:bg-editor-hover/40 text-[12px] font-semibold text-editor-text-header cursor-pointer transition-colors border-b border-editor-divider"
          onClick={() => setPositionOpen(!positionOpen())}
        >
          <div class="flex items-center gap-2">
            <svg
              class={`size-3.5 text-editor-text-dim transition-transform duration-150 ${
                positionOpen() ? "rotate-90" : ""
              }`}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              stroke-width="2"
            >
              <path stroke-linecap="round" stroke-linejoin="round" d="M9 5l7 7-7 7" />
            </svg>
            <span>Position and Size</span>
          </div>
          {/* Units Selector */}
          <select
            class="rounded-[4px] border border-editor-field-border bg-editor-field px-2 py-0.5 text-[11px] text-editor-text focus:border-editor-accent focus:outline-none cursor-pointer"
            value={props.options.unit}
            onClick={(e) => e.stopPropagation()}
            onChange={(e) =>
              props.setOptions((prev) => ({
                ...prev,
                unit: e.currentTarget.value as PrintUnit,
              }))
            }
          >
            <option value="cm">Centimeters</option>
            <option value="in">Inches</option>
            <option value="mm">Millimeters</option>
            <option value="px">Pixels</option>
          </select>
        </button>

        <Show when={positionOpen()}>
          <div class="p-3.5 flex flex-col gap-3 bg-editor-panel/40">
            {/* Paper Preset Selector */}
            <div class="flex items-center justify-between gap-2">
              <label class="w-[72px] shrink-0 text-editor-text-dim text-[11px] font-medium">
                Paper Size:
              </label>
              <select
                class="flex-1 rounded-[4px] border border-editor-field-border bg-editor-field px-2.5 py-1 text-[11px] text-editor-text focus:border-editor-accent focus:outline-none transition-colors cursor-pointer"
                value={props.options.paperPreset}
                onChange={(e) =>
                  handlePaperPresetChange(e.currentTarget.value as PaperPresetId)
                }
              >
                <For each={Object.entries(PAPER_PRESETS) as Array<[PaperPresetId, PaperDimensions]>}>
                  {([id, preset]) => (
                    <option value={id}>{preset.label}</option>
                  )}
                </For>
              </select>
            </div>

            {/* Margin Field */}
            <div class="flex items-center justify-between gap-2">
              <label class="w-[72px] shrink-0 text-editor-text-dim text-[11px] font-medium">
                Margin:
              </label>
              <div class="flex items-center gap-1.5 flex-1">
                <input
                  type="number"
                  min="0"
                  max="100"
                  step="1"
                  class="w-[68px] rounded-[4px] border border-editor-field-border bg-editor-field px-2 py-0.5 text-[11px] text-editor-text focus:border-editor-accent focus:outline-none"
                  value={props.options.marginMm}
                  onInput={(e) =>
                    props.setOptions((prev) => ({
                      ...prev,
                      marginMm: Math.max(0, parseFloat(e.currentTarget.value) || 0),
                    }))
                  }
                />
                <span class="text-[11px] text-editor-text-dim">mm</span>
              </div>
            </div>

            {/* Center on Page Property Row */}
            <div class="flex flex-col gap-1.5 py-1 border-t border-editor-divider/40">
              <label class="flex items-center gap-2 font-medium text-editor-text text-[11px] cursor-pointer">
                <input
                  type="checkbox"
                  class="size-3.5 rounded border-editor-field-border accent-[#E15A17] text-editor-accent focus:ring-0 cursor-pointer"
                  checked={props.options.centerImage}
                  onChange={(e) =>
                    props.setOptions((prev) => ({
                      ...prev,
                      centerImage: e.currentTarget.checked,
                    }))
                  }
                />
                Center on Page
              </label>

              <Show when={!props.options.centerImage}>
                <div class="flex items-center gap-3 mt-1 pl-5">
                  <div class="flex items-center gap-1.5">
                    <span class="text-[11px] text-editor-text-dim">Top:</span>
                    <input
                      type="number"
                      step="0.1"
                      class="w-[60px] rounded-[4px] border border-editor-field-border bg-editor-field px-2 py-0.5 text-[11px] text-editor-text focus:border-editor-accent focus:outline-none"
                      value={convertMmToUnit(
                        props.options.topOffsetMm,
                        props.options.unit
                      )}
                      onInput={(e) => {
                        const valMm = convertUnitToMm(
                          parseFloat(e.currentTarget.value) || 0,
                          props.options.unit
                        );
                        props.setOptions((prev) => ({
                          ...prev,
                          topOffsetMm: valMm,
                        }));
                      }}
                    />
                    <span class="text-[11px] text-editor-text-dim">{props.options.unit}</span>
                  </div>
                  <div class="flex items-center gap-1.5">
                    <span class="text-[11px] text-editor-text-dim">Left:</span>
                    <input
                      type="number"
                      step="0.1"
                      class="w-[60px] rounded-[4px] border border-editor-field-border bg-editor-field px-2 py-0.5 text-[11px] text-editor-text focus:border-editor-accent focus:outline-none"
                      value={convertMmToUnit(
                        props.options.leftOffsetMm,
                        props.options.unit
                      )}
                      onInput={(e) => {
                        const valMm = convertUnitToMm(
                          parseFloat(e.currentTarget.value) || 0,
                          props.options.unit
                        );
                        props.setOptions((prev) => ({
                          ...prev,
                          leftOffsetMm: valMm,
                        }));
                      }}
                    />
                    <span class="text-[11px] text-editor-text-dim">{props.options.unit}</span>
                  </div>
                </div>
              </Show>
            </div>

            {/* Scaled Print Size Property Row */}
            <div class="flex flex-col gap-2 pt-1.5 border-t border-editor-divider/40">
              <div class="flex items-center justify-between">
                <label class="text-[11.5px] font-semibold text-editor-text-header">
                  Scaled Print Size
                </label>
                <label class="flex items-center gap-1.5 text-[11px] text-editor-text cursor-pointer font-medium">
                  <input
                    type="checkbox"
                    class="size-3.5 rounded border-editor-field-border accent-[#E15A17] text-editor-accent focus:ring-0 cursor-pointer"
                    checked={props.options.scaleToFit}
                    onChange={(e) => handleScaleToFitToggle(e.currentTarget.checked)}
                  />
                  Scale to Fit
                </label>
              </div>

              {/* Scale Slider & Percentage */}
              <div class="flex items-center gap-2">
                <span class="w-[42px] shrink-0 text-[11px] text-editor-text-dim font-medium">
                  Scale:
                </span>
                <input
                  type="range"
                  min="10"
                  max="400"
                  step="1"
                  class="flex-1 accent-[#E15A17] h-1.5 bg-editor-divider rounded-lg cursor-pointer"
                  value={props.options.scalePercent}
                  onInput={(e) => {
                    const scale = parseFloat(e.currentTarget.value) || 100;
                    props.setOptions((prev) => ({
                      ...prev,
                      scalePercent: scale,
                      scaleToFit: false,
                    }));
                  }}
                />
                <input
                  type="number"
                  min="1"
                  max="1000"
                  step="0.1"
                  class="w-[60px] rounded-[4px] border border-editor-field-border bg-editor-field px-1.5 py-0.5 text-[11px] text-editor-text focus:border-editor-accent focus:outline-none"
                  value={props.options.scalePercent}
                  onInput={(e) => {
                    const scale = parseFloat(e.currentTarget.value) || 100;
                    props.setOptions((prev) => ({
                      ...prev,
                      scalePercent: scale,
                      scaleToFit: false,
                    }));
                  }}
                />
                <span class="text-[11px] text-editor-text-dim">%</span>
              </div>

              {/* Width / Height linked inputs with unit suffix */}
              <div class="flex items-center justify-between gap-2 mt-1">
                <div class="flex items-center gap-1.5">
                  <span class="text-[11px] text-editor-text-dim font-medium">Width:</span>
                  <input
                    type="number"
                    step="0.1"
                    class="w-[68px] rounded-[4px] border border-editor-field-border bg-editor-field px-2 py-0.5 text-[11px] text-editor-text focus:border-editor-accent focus:outline-none"
                    value={convertMmToUnit(imageMmWidth(), props.options.unit)}
                    onInput={(e) =>
                      handleWidthChange(parseFloat(e.currentTarget.value) || 0)
                    }
                  />
                  <span class="text-[11px] text-editor-text-dim font-medium">{props.options.unit}</span>
                </div>
                <div class="flex items-center gap-1.5">
                  <span class="text-[11px] text-editor-text-dim font-medium">Height:</span>
                  <input
                    type="number"
                    step="0.1"
                    class="w-[68px] rounded-[4px] border border-editor-field-border bg-editor-field px-2 py-0.5 text-[11px] text-editor-text focus:border-editor-accent focus:outline-none"
                    value={convertMmToUnit(imageMmHeight(), props.options.unit)}
                    onInput={(e) =>
                      handleHeightChange(parseFloat(e.currentTarget.value) || 0)
                    }
                  />
                  <span class="text-[11px] text-editor-text-dim font-medium">{props.options.unit}</span>
                </div>
              </div>

              {/* LIVE PPI QUALITY READOUT BADGE */}
              <div
                class={`mt-1.5 flex items-center justify-between rounded-[4px] border px-2.5 py-1.5 text-[11px] font-semibold transition-all ${
                  ppiQuality().colorClass
                }`}
              >
                <span>Print Resolution:</span>
                <span>
                  {currentPPI()} PPI — {ppiQuality().badgeText}
                </span>
              </div>
            </div>
          </div>
        </Show>
      </div>
    </div>
  );
}
