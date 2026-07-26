// SPDX-License-Identifier: AGPL-3.0-or-later
import { For, Show, createEffect, createResource, createSignal, untrack } from "solid-js";
import { showToast } from "../Toast";
import { invoke } from "@tauri-apps/api/core";
import type {
  PrintOptions,
  PrintOrientation,
  PrintUnit,
} from "./printTypes";
import {
  calculateEffectivePPI,
  calculateScaleToFit,
  convertMmToUnit,
  convertUnitToMm,
  getPPIQualityLevel,
  TARGET_PRINT_DPI,
  MM_PER_INCH,
} from "./printTypes";

interface PrintInspectorProps {
  docWidthPx: number;
  docHeightPx: number;
  options: PrintOptions;
  setOptions: (value: PrintOptions | ((prev: PrintOptions) => PrintOptions)) => PrintOptions;
  loading: boolean;
  /** True while a user-initiated set_paper IPC is in-flight — Effect 2
   *  should skip auto-select to avoid overwriting the pending change. */
  isPendingSetPaper: () => boolean;
  setPaper: (name: string, paperIndex: number, widthMm: number, heightMm: number) => Promise<void>;
  toggleOrientation: () => Promise<void>;
  setMarginMm: (mm: number) => Promise<void>;
  setScaleToFit: (enabled: boolean) => Promise<void>;
  setScalePercent: (pct: number) => Promise<void>;
  setCenterImage: (center: boolean) => Promise<void>;
  setTopOffsetMm: (offset: number) => Promise<void>;
  setLeftOffsetMm: (offset: number) => Promise<void>;
  setCopies: (n: number) => Promise<void>;
  setUnit: (u: string) => Promise<void>;
  setShowPaperWhite: (show: boolean) => Promise<void>;
  setPrinter: (p: string) => Promise<void>;
  openPrinterProperties: () => Promise<any>;
}

interface PrintersData {
  printers: string[];
  default?: string;
  defaultPaperSize?: { preset: string; widthMm: number; heightMm: number };
  defaultMargins?: { leftMm: number; topMm: number; rightMm: number; bottomMm: number };
}

interface PaperSizeEntry {
  name: string;
  widthMm: number;
  heightMm: number;
  dmPaperIndex: number;
}

export function PrintInspector(props: PrintInspectorProps) {
  // Print state is a single source of truth from PrintDialog / usePrintSettings.
  // All callbacks invoke Rust commands directly — no fallback instance.
  const o = () => props.options;
  const {
    setOptions,
    setPaper,
    toggleOrientation,
    setMarginMm,
    setScaleToFit,
    setScalePercent,
    setCenterImage,
    setTopOffsetMm,
    setLeftOffsetMm,
    setCopies,
    setUnit,
    setShowPaperWhite,
    setPrinter,
    openPrinterProperties,
    isPendingSetPaper,
  } = props;

  // ── Refresh trigger signal — increment to force re-fetch printer list ─
  const [refreshKey, setRefreshKey] = createSignal(0);

  // ── DOM refs for one-way signal→input sync (avoids SolidJS controlled-input fight) ─
  let scaleCheckboxEl: HTMLInputElement | undefined;
  let scaleSliderEl: HTMLInputElement | undefined;
  let scaleNumberEl: HTMLInputElement | undefined;
  let paperSelectEl: HTMLSelectElement | undefined;

  // ── Resource 1: Printer list ──────────────────────────────────────
  // Fetches on mount (refreshKey=0) and whenever refreshKey increments.
  // NOTE: loading/error are getter PROPERTIES on the actions object, not standalone
  // functions. Destructuring them separately breaks the getter binding, so we keep
  // the full actions object (printersActions) and access .loading / .error on it.
  const [printersRes, printersActions] = createResource(
    refreshKey,
    async (): Promise<PrintersData> => {
      const res = await invoke("get_system_printers") as { ok: boolean; data?: PrintersData; error?: { message?: string } };
      if (res.ok && res.data) return res.data;
      throw new Error(res.error?.message || "Failed to retrieve printer list");
    },
    { initialValue: { printers: [] } }
  );
  const refetchPrinters = printersActions.refetch;

  // Derived: printer list, loading, error
  const printers = (): string[] => printersRes()?.printers ?? [];
  const loadingPrinters = (): boolean => printersRes.loading;
  const printerError = (): string | null => {
    const err = printersRes.error;
    if (err) return err instanceof Error ? err.message : String(err);
    if (printers().length === 0 && !printersRes.loading) {
      return "No printers found. Please connect a printer and try again.";
    }
    return null;
  };

  // ── Resource 2: Paper sizes — auto-fetches when selectedPrinter changes ─
  // Source returns null when no printer is selected → no fetch.
  const [paperSizesRes, { refetch: refetchPaperSizes }] = createResource(
    () => {
      return (o().selectedPrinter || null) as string | null;
    },
    async (printer: string): Promise<PaperSizeEntry[]> => {
      if (!printer) return [];
      console.log("[PRINT:Inspector] Fetching paper sizes for printer:", printer);
      const res = await invoke("get_printer_paper_sizes", { printer }) as {
        ok: boolean; data?: { sizes?: PaperSizeEntry[] };
      };
      console.log("[PRINT:Inspector] Paper sizes response:", JSON.stringify(res));
      if (res.ok && res.data?.sizes) return res.data.sizes;
      return [];
    },
    { initialValue: [] }
  );

  const printerPaperSizes = (): PaperSizeEntry[] => {
    const sizes = paperSizesRes() ?? [];
    return sizes;
  };

  // ── Effect 1: Select default printer (fallback) ───────────────────
  // In normal operation the printer is already initialised by
  // PrintSettings::initialize_default_printer() in main.rs::setup(),
  // so this effect exits early (currentPrinter is truthy).  It serves
  // as a fallback for environments without a system printer driver
  // (tests, macOS/Linux without CUPS, etc.).
  createEffect(() => {
    const data = printersRes();
    const currentPrinter = o().selectedPrinter;
    if (!data || data.printers.length === 0 || currentPrinter) return;
    const defaultP = (data as PrintersData).default || data.printers[0];
    console.log("[PRINT:Inspector] Effect 1 — selecting default printer:", defaultP);
    setPrinter(defaultP);
    // Paper size auto-select happens in Effect 2 once paper sizes load
  });

  // ── DIAG: Track paper reverts — log stack when paperPreset goes from non-A4 to A4 ─
  let _diagPrevPaper = "";
  createEffect(() => {
    const cur = o().paperPreset;
    if (_diagPrevPaper && _diagPrevPaper !== "A4" && cur === "A4") {
      console.warn(`[PRINT:Inspector] DIAG: PAPER REVERTED TO A4! was="${_diagPrevPaper}" now="${cur}"`, new Error().stack);
    }
    _diagPrevPaper = cur;
  });

  // ── Track previous printer for auto-select guard ─────────────────
  let prevPrinter: string | undefined;

  // ── Effect 2: Sync paper size when printer changes ─────────────────
  // Priority: current Rust state > printer driver's default > sizes[0]
  // This ensures user's paper selection is preserved across printer switch
  // and prevents the printer driver's DEVMODE default from overriding the
  // application state. For printers that report "Custom" as the DEVMODE
  // paper name (e.g., EPSON L1110 with A4), this avoids wrongly matching
  // the driver's raw dmFormName instead of the actual standard paper.
  //
    // Guards:
    //   1. Skip until initial state is loaded from Rust (props.loading).
    //      Prevents auto-select seeing placeholder DEFAULT_OPTIONS ("A4").
    //   2. Skip if a user-initiated set_paper IPC is in-flight (pending).
    //      Without this guard, Effect 2 reads stale curPreset ("A4") and
    //      calls setPaper("A4",…), overwriting the user's pending change.
    //      See usePrintSettings.ts — invokeSet sets _hookPendingSetPaper.
    createEffect(() => {
    if (props.loading) return;
    if (isPendingSetPaper()) return;
    const sizes = paperSizesRes();
    const printer = o().selectedPrinter;
    const printersData = printersRes();

    const curPreset = o().paperPreset;
    const curW = o().paperWidthMm;
    const curH = o().paperHeightMm;

    console.log("[PRINT:Inspector] Effect 2 — sizes:", sizes?.length, "printer:", printer, "prevPrinter:", prevPrinter, "current state:", curPreset, `(${curW}\u00d7${curH})`, "defaultPaper:", JSON.stringify((printersData as PrintersData | undefined)?.defaultPaperSize));

    // Guard: only run on printer change when sizes are available
    if (sizes && sizes.length > 0 && printer && printer !== prevPrinter) {
      prevPrinter = printer;

      const defaultPaper = (printersData as PrintersData | undefined)?.defaultPaperSize;

      // Helper: find by name in supported list
      const findByName = (name: string) => sizes.find((s) => s.name === name);
      // Helper: find by dimensions (with \u00b10.5mm tolerance)
      const findByDim = (w: number, h: number) =>
        sizes.find((s) => Math.abs(s.widthMm - w) <= 0.5 && Math.abs(s.heightMm - h) <= 0.5);

      // ── Priority 1: Current state name in supported list ──────────
      let selected = findByName(curPreset);
      let matchedPreset = curPreset;

      // ── Priority 2: Current state dimensions match ────────────────
      if (!selected) {
        selected = findByDim(curW, curH);
        if (selected) matchedPreset = selected.name;
      }

      // ── Priority 3: Current state swapped dims (orientation) ──────
      if (!selected) {
        selected = findByDim(curH, curW);
        if (selected) matchedPreset = selected.name;
      }

      // ── Priority 4: Default paper name from printer driver ────────
      if (!selected && defaultPaper && defaultPaper.preset !== "Custom") {
        selected = findByName(defaultPaper.preset);
        if (selected) matchedPreset = defaultPaper.preset;
      }

      // ── Priority 5: Default paper dimensions from printer driver ──
      if (!selected && defaultPaper && defaultPaper.widthMm > 0) {
        selected = findByDim(defaultPaper.widthMm, defaultPaper.heightMm);
        if (selected) matchedPreset = selected.name;
      }

      // ── Priority 6: First supported size ──────────────────────────
      if (!selected) {
        selected = sizes[0];
        matchedPreset = sizes[0].name;
      }

      console.log("[PRINT:Inspector] Effect 2 — selected:", matchedPreset, JSON.stringify(selected));
      setPaper(matchedPreset, selected.dmPaperIndex, selected.widthMm, selected.heightMm);

      // ── Hardware margin ───────────────────────────────────────────
      const margins = (printersData as PrintersData | undefined)?.defaultMargins;
      const printerMinMargin = margins
        ? Math.max(margins.leftMm, margins.topMm, margins.rightMm, margins.bottomMm, 1.0)
        : 5.0;
      setMarginMm(printerMinMargin);

      if (o().scaleToFit) {
        const paperW = o().orientation === "landscape" ? selected.heightMm : selected.widthMm;
        const paperH = o().orientation === "landscape" ? selected.widthMm : selected.heightMm;
        const fit = calculateScaleToFit(
          props.docWidthPx, props.docHeightPx,
          paperW, paperH, printerMinMargin,
        );
        // Clamp to Rust max limit to prevent infinite loop
        const MAX_SCALE = 1000;
        setScalePercent(Math.min(fit.scalePercent, MAX_SCALE));
      }
    }
  });

  // ── Effect 3: Recalculate ScaleToFit on paper/orientation/margin change ──
  // Fires when paper size, orientation, or margin changes while scaleToFit is ON.
  // Separate from Effect 2 (which only fires on printer change).
  // Uses untrack to read scalePercent without creating a dependency, preventing
  // infinite loops when setScalePercent triggers a Rust event → state update.
  createEffect(() => {
    const opts = o();
    if (!opts.scaleToFit) return;

    // Track these specific inputs only (not scalePercent)
    // Rust stores canonical (portrait) dims regardless of orientation,
    // so swap for landscape when computing available area.
    const pw = opts.orientation === "landscape" ? opts.paperHeightMm : opts.paperWidthMm;
    const ph = opts.orientation === "landscape" ? opts.paperWidthMm : opts.paperHeightMm;
    const mm = opts.marginMm;
    const orient = opts.orientation;
    console.log("[PRINT:Inspector] Effect 3 — paperW:", opts.paperWidthMm, "paperH:", opts.paperHeightMm, "orientation:", orient, "effective pw:", pw, "ph:", ph, "marginMm:", mm, "scaleToFit:", opts.scaleToFit, "currentScale:", untrack(() => o().scalePercent));

    const fit = calculateScaleToFit(
      props.docWidthPx, props.docHeightPx,
      pw, ph, mm,
    );

    // Read current scale without tracking — prevents dependency on scalePercent
    const currentScale = untrack(() => o().scalePercent);
    // Clamp to Rust max limit — prevents infinite loop when calculated value
    // exceeds the allowed max and can never be reached (see print_settings.rs)
    const MAX_SCALE = 1000;
    const newScale = Math.min(fit.scalePercent, MAX_SCALE);
    console.log("[PRINT:Inspector] Effect 3 — fit result:", JSON.stringify(fit), "currentScale:", currentScale, "newScale:", newScale, "willUpdate:", Math.abs(newScale - currentScale) > 0.01);
    if (Math.abs(newScale - currentScale) > 0.01) {
      setScalePercent(newScale);
    }
  });

  // ── Effect 4: One-way signal→DOM sync for controlled inputs ─────────
  // SolidJS's `value={signal}` / `checked={signal}` causes the DOM to snap back
  // to the stale signal value after async event handlers (or flash the first
  // <option> when <For> recreates DOM nodes during reactive updates). Instead,
  // we use refs + this effect to push signal changes to the DOM one-way. The
  // inputs are "browser-controlled" between IPC roundtrips — no snap-back.
  createEffect(() => {
    const opts = o();
    if (scaleCheckboxEl) scaleCheckboxEl.checked = opts.scaleToFit;
    if (scaleSliderEl) scaleSliderEl.value = String(Math.round(opts.scalePercent));
    if (scaleNumberEl) scaleNumberEl.value = String(opts.scalePercent);
    // Paper size dropdown: ref-based sync prevents the native <select> from
    // flashing the first option when <For> recreates <option> elements.
    if (paperSelectEl) paperSelectEl.value = currentSelectedSizeId();
  });

  // ── Build dropdown options from printer-reported sizes ───────────
  const UNLISTED_PREFIX = "_unlisted:";

  const paperSizeOptions = () => {
    const sizes = printerPaperSizes();
    const currentName = o().paperPreset;
    const currentW = o().paperWidthMm;
    const currentH = o().paperHeightMm;
    // Match by name only (not dims) to avoid false conflict when
    // orientation swaps width ⇄ height for the same paper.
    const isListed = currentName != null && sizes.some((s) => s.name === currentName);
    const currentEntry = (!currentName || isListed) ? [] : [{
      id: `${UNLISTED_PREFIX}${currentName}`,
      label: `${currentName} (${currentW} × ${currentH} mm)`,
      widthMm: currentW,
      heightMm: currentH,
      isUnlisted: true,
    }];
    const result = [
      ...currentEntry,
      ...sizes.map((s) => ({
        id: s.name,
        label: `${s.name} (${s.widthMm} × ${s.heightMm} mm)`,
        widthMm: s.widthMm,
        heightMm: s.heightMm,
      })),
    ];
    return result;
  };

  // ── Find matching option ID from current dimensions ──────────────
  const currentSelectedSizeId = () => {
    const presetName = o().paperPreset;
    // Strategy 1: match by preset name (preserves orientation state correctly)
    for (const opt of paperSizeOptions()) {
      if (opt.id === presetName || opt.id === `${UNLISTED_PREFIX}${presetName}`) {
        return opt.id;
      }
    }
    // Strategy 2: match by dimensions (with tolerance, both original and swapped)
    const w = o().paperWidthMm;
    const h = o().paperHeightMm;
    for (const opt of paperSizeOptions()) {
      if (Math.abs(w - opt.widthMm) <= 0.5 && Math.abs(h - opt.heightMm) <= 0.5) {
        return opt.id;
      }
      // also check swapped dimensions (orientation state may differ from natural orientation)
      if (Math.abs(w - opt.heightMm) <= 0.5 && Math.abs(h - opt.widthMm) <= 0.5) {
        return opt.id;
      }
    }
    // Fallback: return the unlisted ID if it exists, otherwise the preset name
    const unlistedId = `${UNLISTED_PREFIX}${presetName}`;
    if (paperSizeOptions().some((opt) => opt.id === unlistedId)) {
      return unlistedId;
    }
    return presetName;
  };

  // ── Track whether running on non-Windows ──────────────────────────
  const isNonWindows = () => typeof navigator !== "undefined" &&
    (/mac/i.test(navigator.platform) || /linux/i.test(navigator.platform) || /x11/i.test(navigator.platform));

  // ── Accordion collapsible section states ─────────────────────────
  const [printerOpen, setPrinterOpen] = createSignal(true);
  const [colorOpen, setColorOpen] = createSignal(false);
  const [positionOpen, setPositionOpen] = createSignal(true);

  // ── Manual refresh handler ────────────────────────────────────────
  const refreshPrinters = () => {
    setRefreshKey((k) => k + 1);
    refetchPaperSizes();
  };

  // ── Live PPI calculation ─────────────────────────────────────────
  const imageMmWidth = () => {
    const scaleFactor = o().scalePercent / 100;
    return (props.docWidthPx / TARGET_PRINT_DPI) * MM_PER_INCH * scaleFactor;
  };

  const imageMmHeight = () => {
    const scaleFactor = o().scalePercent / 100;
    return (props.docHeightPx / TARGET_PRINT_DPI) * MM_PER_INCH * scaleFactor;
  };

  const currentPPI = () =>
    calculateEffectivePPI(props.docWidthPx, props.docHeightPx, imageMmWidth(), imageMmHeight());

  const ppiQuality = () => getPPIQualityLevel(currentPPI());

  // ── Open native printer settings dialog ──────────────────────────
  const handleOpenPrinterProperties = async () => {
    if (!o().selectedPrinter) return;
    if (isNonWindows()) {
      showToast("Printer properties are managed by your system's Print dialog", "info");
      return;
    }
    try {
      const res = await openPrinterProperties();
      console.log("[PRINT:Inspector] handleOpenPrinterProperties — result:", JSON.stringify(res));
      if (res.ok && res.data) {
        refetchPaperSizes();
        showToast("Printer settings updated", "info");
      }
    } catch (e) {
      console.log("[PRINT:Inspector] handleOpenPrinterProperties — error/cancel:", e);
      // Native driver dialog unavailable or cancelled
    }
  };

  // ── Paper size selection from dropdown ───────────────────────────
  const handlePaperSizeSelect = (optionId: string) => {
    console.log("[PRINT:Inspector] handlePaperSizeSelect — optionId:", optionId, "orientation:", o().orientation, "state before:", JSON.stringify({paperPreset: o().paperPreset, paperWidthMm: o().paperWidthMm, paperHeightMm: o().paperHeightMm}));
    // If the selected option starts with the unlisted prefix, it's the
    // "active but unlisted" fallback entry (= current paper) — do nothing.
    if (optionId.startsWith(UNLISTED_PREFIX)) {
      console.log("[PRINT:Inspector] handlePaperSizeSelect — unlisted prefix, returning early");
      return;
    }
    const selected = printerPaperSizes().find((s) => s.name === optionId);
    if (!selected) {
      console.log("[PRINT:Inspector] handlePaperSizeSelect — selected not found in printerPaperSizes:", optionId, "sizes:", printerPaperSizes().map(s => s.name));
      return;
    }
    let w = selected.widthMm;
    let h = selected.heightMm;
    // Swap dimensions to match user's current orientation
    if (o().orientation === "landscape" && w < h) {
      const tmp = w; w = h; h = tmp;
      console.log("[PRINT:Inspector] handlePaperSizeSelect — swapped for landscape:", w, h);
    } else if (o().orientation === "portrait" && w > h) {
      const tmp = w; w = h; h = tmp;
      console.log("[PRINT:Inspector] handlePaperSizeSelect — swapped for portrait:", w, h);
    }
    console.log("[PRINT:Inspector] handlePaperSizeSelect — calling setPaper(", optionId, ",", selected.dmPaperIndex, ",", w, ",", h, ")");
    // Use setPaper with the driver-reported index (no hardcoded mapping)
    setPaper(optionId, selected.dmPaperIndex, w, h);
  };

  // ── Orientation toggle ───────────────────────────────────────────
  const handleOrientationToggle = (newOrientation: PrintOrientation) => {
    console.log("[PRINT:Inspector] handleOrientationToggle — new:", newOrientation, "current:", o().orientation, "state before:", JSON.stringify({paperWidthMm: o().paperWidthMm, paperHeightMm: o().paperHeightMm}));
    if (o().orientation === newOrientation) return;
    toggleOrientation();
  };

  // ── Scale to Fit toggle ──────────────────────────────────────────
  // Mutable ref to store the scale BEFORE Effect 3 calculates the fit.
  // Restored when disabled so the user returns to their original scale.
  const preFitScaleRef = { current: 100 };

  const handleScaleToFitToggle = async (fit: boolean) => {
    if (fit) {
      // Save pre-fit scale now (before Effect 3 changes it on the next event)
      preFitScaleRef.current = untrack(() => o().scalePercent);
      await setScaleToFit(true);
      // Effect 3 handles the fit calculation from here
    } else {
      // 1. Disable scale-to-fit first so Effect 3 won't re-calculate
      await setScaleToFit(false);
      // 2. Then restore the user's original scale
      await setScalePercent(preFitScaleRef.current);
    }
  };

  // ── Unit input changes ───────────────────────────────────────────
  const handleWidthChange = (valUnit: number) => {
    const valMm = convertUnitToMm(valUnit, o().unit);
    const refMm = (props.docWidthPx / TARGET_PRINT_DPI) * MM_PER_INCH;
    const scale = Number(((valMm / refMm) * 100).toFixed(2));
    setScalePercent(scale);
  };

  const handleHeightChange = (valUnit: number) => {
    const valMm = convertUnitToMm(valUnit, o().unit);
    const refMm = (props.docHeightPx / TARGET_PRINT_DPI) * MM_PER_INCH;
    const scale = Number(((valMm / refMm) * 100).toFixed(2));
    setScalePercent(scale);
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
          <span class="text-[11px] text-editor-text-dim font-normal">System Spooler</span>
        </button>

        <Show when={printerOpen()}>
          <div class="p-3.5 flex flex-col gap-3 bg-editor-panel/40">
            {/* Printer Dropdown + Refresh Button */}
            <div class="flex items-center justify-between gap-1.5">
              <label class="w-[72px] shrink-0 text-editor-text-dim text-[11px] font-medium">Printer:</label>
              <div class="flex flex-1 items-center gap-1">
                <select
                  class="flex-1 rounded-[4px] border border-editor-field-border bg-editor-field px-2.5 py-1 text-[11px] text-editor-text focus:border-editor-accent focus:outline-none transition-colors"
                  value={o().selectedPrinter}
                  disabled={loadingPrinters()}
                  onChange={(e) => {
                    const newPrinter = e.currentTarget.value;
                    console.log("[PRINT:Inspector] Printer dropdown onChange — selected:", newPrinter, "previous:", o().selectedPrinter);
                    setPrinter(newPrinter);
                    // Paper sizes auto-fetch via createResource when selectedPrinter changes.
                    // Paper auto-select via createEffect.
                  }}
                >
                  <Show when={loadingPrinters()}>
                    <option>Loading printers...</option>
                  </Show>
                  <Show when={!loadingPrinters() && printers().length === 0}>
                    <option disabled>No printers found</option>
                  </Show>
                  <Show when={!loadingPrinters() && printers().length > 0}>
                    <For each={printers()}>
                      {(p) => <option value={p}>{p}</option>}
                    </For>
                  </Show>
                </select>

                {/* Refresh Button */}
                <button
                  type="button"
                  class="flex size-6 shrink-0 items-center justify-center rounded-[4px] text-editor-text-dim hover:bg-editor-hover active:bg-editor-active transition-colors disabled:opacity-40"
                  onClick={refreshPrinters}
                  disabled={loadingPrinters()}
                  title="Refresh printer list"
                  aria-label="Refresh printer list"
                >
                  <svg
                    class={`size-3.5 ${loadingPrinters() ? "animate-spin" : ""}`}
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  >
                    <path d="M1 4v6h6" />
                    <path d="M23 20v-6h-6" />
                    <path d="M20.49 9A9 9 0 0 0 5.64 5.64L1 10" />
                    <path d="m22 14-4.64 4.36A9 9 0 0 1 3.51 15" />
                  </svg>
                </button>
              </div>
            </div>

            {/* Printer Error Message */}
            <Show when={printerError()}>
              <div class="flex items-center gap-1.5 rounded-[4px] border border-rose-500/30 bg-rose-950/20 px-2.5 py-1.5 text-[10.5px] text-rose-400">
                <svg class="size-3 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <circle cx="12" cy="12" r="10" />
                  <path d="M12 8v4m0 4h.01" />
                </svg>
                <span>{printerError()}</span>
              </div>
            </Show>

            {/* Copies & Print Settings... */}
            <div class="flex items-center justify-between gap-2">
              <div class="flex items-center gap-2">
                <label class="w-[72px] shrink-0 text-editor-text-dim text-[11px] font-medium">Copies:</label>
                <input
                  type="number"
                  min="1"
                  max="999"
                  class="w-[64px] rounded-[4px] border border-editor-field-border bg-editor-field px-2 py-1 text-[11px] text-editor-text focus:border-editor-accent focus:outline-none transition-colors"
                  value={o().copies}
                  onInput={(e) => {
                    const val = Math.max(1, parseInt(e.currentTarget.value, 10) || 1);
                    setCopies(val);
                  }}
                />
              </div>

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
              <label class="w-[72px] shrink-0 text-editor-text-dim text-[11px] font-medium">Layout:</label>
              <div class="flex items-center rounded-[4px] border border-editor-field-border bg-editor-field p-0.5 flex-1">
                <button
                  type="button"
                  title="Portrait"
                  class={`flex flex-1 items-center justify-center gap-1.5 h-[24px] rounded-[3px] text-[11px] transition-colors cursor-pointer ${
                    o().orientation === "portrait"
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
                    o().orientation === "landscape"
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
            <div class="flex items-center justify-between py-0.5">
              <span class="text-editor-text-dim font-medium">Document Profile:</span>
              <span class="font-semibold text-editor-text">sRGB IEC61966-2.1</span>
            </div>
            <div class="flex items-center justify-between gap-2">
              <label class="text-editor-text-dim font-medium">Color Handling:</label>
              <select class="rounded-[4px] border border-editor-field-border bg-editor-field px-2.5 py-1 text-[11px] text-editor-text opacity-70" disabled>
                <option>Printer Manages Colors</option>
              </select>
            </div>
            <div class="flex items-center justify-between gap-2 opacity-50 cursor-not-allowed" title="ICC Soft-proofing deferred to post-v1 release">
              <label class="text-editor-text-dim font-medium">Rendering Intent:</label>
              <select class="rounded-[4px] border border-editor-field-border bg-editor-field px-2.5 py-1 text-[11px] text-editor-text" disabled>
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
          <select
            class="rounded-[4px] border border-editor-field-border bg-editor-field px-2 py-0.5 text-[11px] text-editor-text focus:border-editor-accent focus:outline-none cursor-pointer"
            value={o().unit}
            onClick={(e) => e.stopPropagation()}
            onChange={(e) => {
              const val = e.currentTarget.value as PrintUnit;
              setUnit(val);
            }}
          >
            <option value="cm">Centimeters</option>
            <option value="in">Inches</option>
            <option value="mm">Millimeters</option>
            <option value="px">Pixels</option>
          </select>
        </button>

        <Show when={positionOpen()}>
          <div class="p-3.5 flex flex-col gap-3 bg-editor-panel/40">
            {/* Paper Size Selector */}
            <div class="flex items-center justify-between gap-2">
              <label class="w-[72px] shrink-0 text-editor-text-dim text-[11px] font-medium">Paper Size:</label>
              <select
                class="flex-1 rounded-[4px] border border-editor-field-border bg-editor-field px-2.5 py-1 text-[11px] text-editor-text focus:border-editor-accent focus:outline-none transition-colors cursor-pointer"
                ref={paperSelectEl}
                onChange={(e) => handlePaperSizeSelect(e.currentTarget.value)}
              >
                <For each={paperSizeOptions()}>
                  {(opt) => <option value={opt.id}>{opt.label}</option>}
                </For>
              </select>
            </div>

            {/* Margin */}
            <div class="flex items-center justify-between gap-2">
              <label class="w-[72px] shrink-0 text-editor-text-dim text-[11px] font-medium">Margin:</label>
              <div class="flex items-center gap-1.5 flex-1">
                <input
                  type="number" min="0" max="100" step="1"
                  class="w-[68px] rounded-[4px] border border-editor-field-border bg-editor-field px-2 py-0.5 text-[11px] text-editor-text focus:border-editor-accent focus:outline-none"
                  value={o().marginMm}
                  onInput={(e) => {
                    const newMargin = Math.max(0, parseFloat(e.currentTarget.value) || 0);
                    setMarginMm(newMargin);
                  }}
                />
                <span class="text-[11px] text-editor-text-dim">mm</span>
              </div>
            </div>

            {/* Printer Hardware Margin Info */}
            <Show when={(printersRes() as PrintersData | undefined)?.defaultMargins}>
              {(margins) => {
                const maxMargin = Math.max(
                  margins().leftMm,
                  margins().topMm,
                  margins().rightMm,
                  margins().bottomMm,
                );
                return (
                  <div class="flex items-center justify-between gap-2">
                    <label class="w-[72px] shrink-0 text-editor-text-dim text-[10px] font-medium">Printer Min:</label>
                    <span class="text-[10px] text-editor-text-dim">
                      L: {margins().leftMm.toFixed(1)} T: {margins().topMm.toFixed(1)} 
                      R: {margins().rightMm.toFixed(1)} B: {margins().bottomMm.toFixed(1)} mm
                      <Show when={maxMargin > 1}>
                        <span class="text-amber-400 ml-1">(min {maxMargin.toFixed(1)}mm)</span>
                      </Show>
                    </span>
                  </div>
                );
              }}
            </Show>

            {/* Center on Page */}
            <div class="flex flex-col gap-1.5 py-1 border-t border-editor-divider/40">
              <label class="flex items-center gap-2 font-medium text-editor-text text-[11px] cursor-pointer">
                <input
                  type="checkbox"
                  class="size-3.5 rounded border-editor-field-border accent-[#E15A17] text-editor-accent focus:ring-0 cursor-pointer"
                  checked={o().centerImage}
                  onChange={(e) => setCenterImage(e.currentTarget.checked)}
                />
                Center on Page
              </label>

              <Show when={!o().centerImage}>
                <div class="flex items-center gap-3 mt-1 pl-5">
                  <div class="flex items-center gap-1.5">
                    <span class="text-[11px] text-editor-text-dim">Top:</span>
                    <input
                      type="number" step="0.1"
                      class="w-[60px] rounded-[4px] border border-editor-field-border bg-editor-field px-2 py-0.5 text-[11px] text-editor-text focus:border-editor-accent focus:outline-none"
                      value={convertMmToUnit(o().topOffsetMm, o().unit)}
                      onInput={(e) => {
                        const valMm = convertUnitToMm(parseFloat(e.currentTarget.value) || 0, o().unit);
                        setTopOffsetMm(valMm);
                      }}
                    />
                    <span class="text-[11px] text-editor-text-dim">{o().unit}</span>
                  </div>
                  <div class="flex items-center gap-1.5">
                    <span class="text-[11px] text-editor-text-dim">Left:</span>
                    <input
                      type="number" step="0.1"
                      class="w-[60px] rounded-[4px] border border-editor-field-border bg-editor-field px-2 py-0.5 text-[11px] text-editor-text focus:border-editor-accent focus:outline-none"
                      value={convertMmToUnit(o().leftOffsetMm, o().unit)}
                      onInput={(e) => {
                        const valMm = convertUnitToMm(parseFloat(e.currentTarget.value) || 0, o().unit);
                        setLeftOffsetMm(valMm);
                      }}
                    />
                    <span class="text-[11px] text-editor-text-dim">{o().unit}</span>
                  </div>
                </div>
              </Show>
            </div>

            {/* Scaled Print Size */}
            <div class="flex flex-col gap-2 pt-1.5 border-t border-editor-divider/40">
              <div class="flex items-center justify-between">
                <label class="text-[11.5px] font-semibold text-editor-text-header">Scaled Print Size</label>
                <label class="flex items-center gap-1.5 text-[11px] text-editor-text cursor-pointer font-medium">
                  <input
                    type="checkbox"
                    class="size-3.5 rounded border-editor-field-border accent-[#E15A17] text-editor-accent focus:ring-0 cursor-pointer"
                    ref={scaleCheckboxEl}
                    onChange={(e) => handleScaleToFitToggle(e.currentTarget.checked)}
                  />
                  Scale to Fit
                </label>
              </div>

              {/* Scale Slider */}
              <div class="flex items-center gap-2">
                <span class="w-[42px] shrink-0 text-[11px] text-editor-text-dim font-medium">Scale:</span>
                <input
                  type="range" min="10" max="400" step="1"
                  class="flex-1 accent-[#E15A17] h-1.5 bg-editor-divider rounded-lg cursor-pointer"
                  ref={scaleSliderEl}
                  onInput={(e) => {
                    const scale = parseFloat(e.currentTarget.value) || 100;
                    setScalePercent(scale);
                  }}
                />
                <input
                  type="number" min="1" max="1000" step="0.1"
                  class="w-[60px] rounded-[4px] border border-editor-field-border bg-editor-field px-1.5 py-0.5 text-[11px] text-editor-text focus:border-editor-accent focus:outline-none"
                  ref={scaleNumberEl}
                  onInput={(e) => {
                    const scale = parseFloat(e.currentTarget.value) || 100;
                    setScalePercent(scale);
                  }}
                />
                <span class="text-[11px] text-editor-text-dim">%</span>
              </div>

              {/* Width / Height */}
              <div class="flex items-center justify-between gap-2 mt-1">
                <div class="flex items-center gap-1.5">
                  <span class="text-[11px] text-editor-text-dim font-medium">Width:</span>
                  <input
                    type="number" step="0.1"
                    class="w-[68px] rounded-[4px] border border-editor-field-border bg-editor-field px-2 py-0.5 text-[11px] text-editor-text focus:border-editor-accent focus:outline-none"
                    value={convertMmToUnit(imageMmWidth(), o().unit)}
                    onInput={(e) => handleWidthChange(parseFloat(e.currentTarget.value) || 0)}
                  />
                  <span class="text-[11px] text-editor-text-dim font-medium">{o().unit}</span>
                </div>
                <div class="flex items-center gap-1.5">
                  <span class="text-[11px] text-editor-text-dim font-medium">Height:</span>
                  <input
                    type="number" step="0.1"
                    class="w-[68px] rounded-[4px] border border-editor-field-border bg-editor-field px-2 py-0.5 text-[11px] text-editor-text focus:border-editor-accent focus:outline-none"
                    value={convertMmToUnit(imageMmHeight(), o().unit)}
                    onInput={(e) => handleHeightChange(parseFloat(e.currentTarget.value) || 0)}
                  />
                  <span class="text-[11px] text-editor-text-dim font-medium">{o().unit}</span>
                </div>
              </div>

              {/* PPI Quality Badge */}
              <div
                class={`mt-1.5 flex items-center justify-between rounded-[4px] border px-2.5 py-1.5 text-[11px] font-semibold transition-all ${
                  ppiQuality().colorClass
                }`}
              >
                <span>Print Resolution:</span>
                <span>{currentPPI()} PPI — {ppiQuality().badgeText}</span>
              </div>
            </div>
          </div>
        </Show>
      </div>
    </div>
  );
}
