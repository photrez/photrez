// SPDX-License-Identifier: AGPL-3.0-or-later
// --- Print Settings Hook: Event-driven from Rust ---

import { createSignal, onCleanup, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { PrintOptions } from "./printTypes";

// Event name constant (matches Rust: EVENT_PRINT_SETTINGS_CHANGED)
const EVENT_PRINT_SETTINGS_CHANGED = "print-settings-changed";

const DEFAULT_OPTIONS: PrintOptions = {
  selectedPrinter: "",
  copies: 1,
  paperPreset: "A4",
  paperIndex: 9,
  paperWidthMm: 210,
  paperHeightMm: 297,
  orientation: "portrait",
  marginMm: 5,
  scaleToFit: false,
  scalePercent: 100,
  centerImage: true,
  topOffsetMm: 0,
  leftOffsetMm: 0,
  unit: "mm",
  showPaperWhite: true,
  colorHandling: "printer_manages",
  renderingIntent: "perceptual",
  blackPointCompensation: true,
  printerDpi: 300,
};

/// Map Rust snake_case to TypeScript camelCase
export function mapFromRust(raw: any): PrintOptions {
  return {
    selectedPrinter: raw.selected_printer ?? "",
    copies: raw.copies ?? 1,
    paperPreset: raw.paper_name ?? "A4",
    paperIndex: raw.paper_index ?? 9,
    paperWidthMm: raw.paper_width_mm ?? 210,
    paperHeightMm: raw.paper_height_mm ?? 297,
    orientation: raw.orientation ?? "portrait",
    marginMm: raw.margin_mm ?? 5,
    marginLeftMm: raw.margin_left_mm ?? raw.margin_mm ?? 5,
    marginRightMm: raw.margin_right_mm ?? raw.margin_mm ?? 5,
    marginTopMm: raw.margin_top_mm ?? raw.margin_mm ?? 5,
    marginBottomMm: raw.margin_bottom_mm ?? raw.margin_mm ?? 5,
    scaleToFit: raw.scale_to_fit ?? false,
    scalePercent: raw.scale_percent ?? 100,
    centerImage: raw.center_image ?? true,
    topOffsetMm: raw.top_offset_mm ?? 0,
    leftOffsetMm: raw.left_offset_mm ?? 0,
    unit: raw.unit ?? "mm",
    showPaperWhite: raw.show_paper_white ?? true,
    colorHandling: raw.color_handling ?? "printer_manages",
    renderingIntent: raw.rendering_intent ?? "perceptual",
    blackPointCompensation: raw.black_point_compensation ?? true,
    // Cap at 300 DPI to match printDocument.ts — physical-size math in
    // the preview must agree with the actual composite DPI, or PDF
    // drivers (reporting 600) would show wrong fit/scale on paper.
    printerDpi: Math.min(raw.printer_dpi ?? 300, 300),
  };
}

let _hookId = 0;

/** Track pending IPC operations so Effect 2 can skip auto-select
 *  while a user-initiated set_paper is still in-flight. Prevents
 *  a race where Effect 2 reads stale curPreset and overwrites the
 *  user's pending paper change.
 *  Module-level since both invokeSet and the exported hook share it. */
let _hookPendingSetPaper = false;

/** Serialise IPC calls for print settings. When the user rapidly
 *  toggles state (spam-click scale-to-fit, etc.), multiple invoke
 *  calls are in-flight simultaneously and responses can arrive out
 *  of order — a response from an EARLIER command can overwrite the
 *  signal AFTER a LATER command's response, regressing state.
 *
 *  Instead of sending all calls concurrently, queue them so only
 *  one IPC is in-flight at a time.  This guarantees responses are
 *  processed in the order the commands were issued. */
let _ipcQueue: Promise<void> = Promise.resolve();

export function usePrintSettings(src?: string) {
  const id = src || `hook-${++_hookId}`;
  const [options, setOptions] = createSignal<PrintOptions>(DEFAULT_OPTIONS);
  const [loading, setLoading] = createSignal(true);

  // Expose pending-op flag for PrintInspector's Effect 2 guard
  const isPendingSetPaper = () => _hookPendingSetPaper;

  // Store unlisten function for cleanup
  let unlisten: UnlistenFn | null = null;

  onMount(async () => {
    // 1. Register event listener FIRST — before any IPC calls.
    //    This ensures no events from Rust are lost during init.
    //
    //    IMPORTANT: the listener does NOT call setOptions — the invoke
    //    response is the single source of truth (see invokeSet below).
    //    Rust emits an event from every mutation command just before
    //    returning the invoke response.  Both carry the same state
    //    snapshot.  If the listener also called setOptions, a late
    //    event from an EARLIER command (e.g. set_scale_to_fit) could
    //    arrive AFTER a LATER invoke response (e.g. set_scale_percent)
    //    and overwrite the signal with stale values, causing Effect 3
    try {
      unlisten = await listen<any>(EVENT_PRINT_SETTINGS_CHANGED, (event) => {
        const data = event.payload?.data ?? event.payload;
        const mapped = mapFromRust(data);
        console.log(`[PRINT:${id}] Event (log only):`, JSON.stringify(mapped));
      });
    } catch {
      // Ignore listen errors during test environment teardown or mock absence
    }

    // 2. Fetch initial state from Rust (primary SSOT)
    try {
      const raw = await invoke<any>("get_print_settings");
      // Command returns { ok: true, data: {...} } envelope
      const data = raw?.data ?? raw;
      const mapped = mapFromRust(data);
      console.log(`[PRINT:${id}] Initial:`, JSON.stringify(mapped));
      setOptions(mapped);
    } catch (e) {
      console.error(`[PRINT:${id}] Fetch failed:`, e);
    } finally {
      setLoading(false);
    }
  });

  // MUST call unlisten() on cleanup to avoid memory leaks
  onCleanup(() => {
    unlisten?.();
  });

  /** Serialise IPC calls through _ipcQueue to guarantee response order.
   *  Each invoke is chained onto a global promise so only one IPC is
   *  in-flight at a time.  This prevents out-of-order responses when
   *  the user rapidly toggles state (spam-click scale-to-fit, etc.) —
   *  a response from an EARLIER command cannot overwrite the signal
   *  AFTER a LATER command's response. */
  async function invokeSet(command: string, args: Record<string, any>) {
    const task = async () => {
      console.log(`[PRINT:${id}] invoke -> ${command}`, JSON.stringify(args));
      // Track pending set_paper IPC — used by PrintInspector Effect 2 to
      // avoid auto-selecting with stale curPreset and overwriting the user's
      // paper change. See Effect 2 guard for details.
      const isSetPaper = command === "set_paper";
      if (isSetPaper) _hookPendingSetPaper = true;
      try {
        const raw = await invoke<Record<string, unknown>>(command, args);
        console.log(`[PRINT:${id}] invoke response raw keys:`, Object.keys(raw), "orientation:", (raw?.data as { orientation?: unknown } | undefined)?.orientation ?? (raw as { orientation?: unknown }).orientation);
        const data: unknown = raw?.data ?? raw;
        if (data) {
          const mapped = mapFromRust(data);
          console.log(`[PRINT:${id}] invoke response mapped — orientation:`, mapped.orientation, "paperWidthMm:", mapped.paperWidthMm, "paperHeightMm:", mapped.paperHeightMm);
          setOptions(mapped);
          console.log(`[PRINT:${id}] setOptions called — orientation set to:`, mapped.orientation, "— VERIFY options().orientation:", options().orientation);
        } else {
          console.log(`[PRINT:${id}] invoke response — no data field, raw keys:`, Object.keys(raw));
        }
      } catch (e) {
        console.error(`[PRINT:${id}] invoke failed ${command}:`, e);
      } finally {
        if (isSetPaper) _hookPendingSetPaper = false;
      }
    };
    // Chain onto the queue — previous task must finish before ours starts.
    // On rejection, the second `task` handler keeps the chain alive.
    _ipcQueue = _ipcQueue.then(task, task);
    await _ipcQueue;
  }

  return {
    options,
    setOptions,
    loading,
    isPendingSetPaper,
    // User actions — each invokes a Rust command
    setPaper: (name: string, paperIndex: number, widthMm: number, heightMm: number) => {
      console.log(`[PRINT:${id}] setPaper — name="${name}" index=${paperIndex} dims=(${widthMm}x${heightMm})`);
      return invokeSet("set_paper", { name, paperIndex, widthMm, heightMm });
    },
    toggleOrientation: () => invokeSet("toggle_orientation", {}),
    setOrientation: (o: string) => invokeSet("set_orientation", { orientation: o }),
    setMarginMm: (mm: number, hardwareMinMm?: number) => {
      const args: Record<string, unknown> = { marginMm: mm };
      if (hardwareMinMm !== undefined) args.hardwareMinMm = hardwareMinMm;
      return invokeSet("set_margin", args);
    },
    setScaleToFit: (enabled: boolean) => invokeSet("set_scale_to_fit", { enabled }),
    setScalePercent: (pct: number) => invokeSet("set_scale_percent", { percent: pct }),
    setCenterImage: (center: boolean) => invokeSet("set_center_image", { center }),
    setTopOffsetMm: (offset: number) => invokeSet("set_top_offset_mm", { offset }),
    setLeftOffsetMm: (offset: number) => invokeSet("set_left_offset_mm", { offset }),
    setCopies: (n: number) => invokeSet("set_copies", { copies: n }),
    setUnit: (u: string) => invokeSet("set_unit", { unit: u }),
    setShowPaperWhite: (show: boolean) => invokeSet("set_show_paper_white", { show }),
    setColorHandling: (handling: string) => invokeSet("set_color_handling", { handling }),
    setRenderingIntent: (intent: string) => invokeSet("set_rendering_intent", { intent }),
    setBlackPointCompensation: (enabled: boolean) => invokeSet("set_black_point_compensation", { enabled }),
    setPerSideMargins: (left: number, right: number, top: number, bottom: number) =>
      invokeSet("set_per_side_margins", { leftMm: left, rightMm: right, topMm: top, bottomMm: bottom }),
    setPrinter: (p: string) => invokeSet("set_printer", { printer: p }),
    openPrinterProperties: async () => {
      const res = await invoke<{
        data?: { applied?: boolean; settings?: unknown };
        applied?: boolean;
        settings?: unknown;
      }>("open_printer_properties_and_apply");
      console.log(`[PRINT:${id}] Properties result:`, JSON.stringify(res));
      // Update frontend state from the native dialog result
      const data = res?.data ?? res;
      if (data && data.applied && data.settings) {
        const mapped = mapFromRust(data.settings);
        setOptions(mapped);
      }
      return res;
    },
  };
}
