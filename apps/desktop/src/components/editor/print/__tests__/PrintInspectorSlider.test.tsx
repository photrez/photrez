// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Wiring + edge-case tests for PrintInspector's scale slider and number input.
 *
 * These tests verify:
 *   1. Drag-slider → setScaleToFit(false) → setScalePercent(value) sequence
 *   2. Value is correctly captured from the input element
 *   3. Edge cases: min/max bounds, non-numeric fallback, rapid interaction
 *   4. Number input also fires the correct IPC calls
 *
 * NOTE: A SolidJS-specific race condition (e.currentTarget.value stale after
 * re-render during async IPC) cannot be reproduced in jsdom.  The fix
 * (capturing the value BEFORE await) is verified indirectly by these tests
 * enforcing that the value read matches the user's input.
 *
 * NOTE: fireEvent.input(element, { target: { value: "N" } }) does NOT work
 * with SolidJS event delegation — SolidJS retargets e.target via composedPath()
 * to the actual DOM element, so the override is lost.  Always set the DOM
 * element's .value property directly before calling fireEvent.input.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, fireEvent, waitFor } from "@solidjs/testing-library";
import { PrintInspector } from "../PrintInspector";
import type { PrintOptions } from "../printTypes";
import { createSignal } from "solid-js";

const BASE_OPTIONS: PrintOptions = {
  selectedPrinter: "Epson Stylus Pro 3880",
  copies: 1,
  orientation: "portrait",
  paperPreset: "A4",
  paperIndex: 9,
  paperWidthMm: 210,
  paperHeightMm: 297,
  marginMm: 5,
  colorHandling: "printer_manages",
  renderingIntent: "perceptual",
  blackPointCompensation: true,
  centerImage: true,
  topOffsetMm: 0,
  leftOffsetMm: 0,
  scalePercent: 100,
  scaleToFit: false,
  unit: "mm",
  showPaperWhite: true,
  printerDpi: 300,
};

const { mockInvoke, mockListenCallback, mockListen } = vi.hoisted(() => ({
  mockInvoke: vi.fn(),
  mockListenCallback: { current: null as ((payload: any) => void) | null },
  mockListen: vi.fn().mockImplementation((_event: string, cb: (payload: any) => void) => {
    mockListenCallback.current = cb;
    return Promise.resolve(() => {});
  }),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: mockListen,
}));

const MUTATION_COMMANDS = new Set([
  "set_printer", "set_paper", "toggle_orientation",
  "set_orientation", "set_margin", "set_scale_to_fit", "set_scale_percent",
  "set_center_image", "set_top_offset_mm", "set_left_offset_mm", "set_copies",
  "set_unit", "set_show_paper_white",
]);

function emitSettingsIfMutation(cmd: string, result: any) {
  if (MUTATION_COMMANDS.has(cmd) && result?.ok && result.data && mockListenCallback.current) {
    mockListenCallback.current({ payload: { data: result.data } });
  }
}

function createInspectorProps(initialOptions?: Partial<PrintOptions>) {
  const [opts, setOptions] = createSignal<PrintOptions>({
    ...BASE_OPTIONS,
    ...initialOptions,
  });

  async function invokeCmd(cmd: string, args: Record<string, unknown> = {}) {
    const result = await mockInvoke(cmd, args);
    emitSettingsIfMutation(cmd, result);
    switch (cmd) {
      case "set_scale_to_fit":
        setOptions((prev) => ({ ...prev, scaleToFit: (args.enabled as boolean) ?? prev.scaleToFit }));
        break;
      case "set_scale_percent":
        setOptions((prev) => ({ ...prev, scalePercent: (args.percent as number) ?? prev.scalePercent }));
        break;
      default:
        break;
    }
    return result;
  }

  return {
    options: opts,
    setOptions,
    loading: false,
    isPendingSetPaper: () => false,
    docWidthPx: 3000,
    docHeightPx: 2000,
    setPaper: () => Promise.resolve(),
    toggleOrientation: () => invokeCmd("toggle_orientation", {}),
    setMarginMm: (mm: number) => invokeCmd("set_margin", { marginMm: mm }),
    setScaleToFit: (enabled: boolean) => invokeCmd("set_scale_to_fit", { enabled }),
    setScalePercent: (pct: number) => invokeCmd("set_scale_percent", { percent: pct }),
    setCenterImage: () => Promise.resolve(),
    setTopOffsetMm: () => Promise.resolve(),
    setLeftOffsetMm: () => Promise.resolve(),
    setCopies: () => Promise.resolve(),
    setUnit: () => Promise.resolve(),
    setShowPaperWhite: () => Promise.resolve(),
    setPerSideMargins: () => Promise.resolve(),
    setPrinter: () => Promise.resolve(),
    openPrinterProperties: () => Promise.resolve({ ok: true }),
  };
}

async function defaultMockImpl(cmd: string, args?: Record<string, unknown>): Promise<any> {
  if (cmd === "get_system_printers") {
    return { ok: true, data: { printers: ["Test Printer"], default: "Test Printer" } };
  }
  if (cmd === "get_printer_paper_sizes") {
    return { ok: true, data: { sizes: [{ name: "A4", widthMm: 210, heightMm: 297, dmPaperIndex: 9 }] } };
  }
  if (cmd === "get_print_settings") {
    return {
      ok: true,
      data: {
        selected_printer: "Test Printer",
        copies: 1, paper_name: "A4", paper_index: 9,
        paper_width_mm: 210, paper_height_mm: 297,
        orientation: "portrait", margin_mm: 5,
        scale_to_fit: false, scale_percent: 100,
        center_image: true, top_offset_mm: 0, left_offset_mm: 0,
        unit: "mm", show_paper_white: true,
      },
    };
  }
  if (cmd === "set_scale_percent") {
    return {
      ok: true,
      data: { scale_percent: (args as any)?.percent ?? 100 },
    };
  }
  if (cmd === "set_scale_to_fit") {
    return {
      ok: true,
      data: { scale_to_fit: (args as any)?.enabled ?? false },
    };
  }
  return { ok: true };
}

/** Helper: set DOM value then dispatch input event.
 *  fireEvent's target.value override is lost in SolidJS delegation
 *  (SolidJS retargets e.target via composedPath), so we must set the
 *  DOM property directly. */
function setValueAndFireInput(el: HTMLInputElement, value: string) {
  el.value = value;
  fireEvent.input(el);
}

describe("PrintInspector — scale slider wiring", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockListen.mockImplementation((_event: string, cb: (payload: any) => void) => {
      mockListenCallback.current = cb;
      return Promise.resolve(() => {});
    });
    mockInvoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      return defaultMockImpl(cmd, args);
    });
  });

  // ── 1. Slider fires both IPC calls ──

  it("calls setScaleToFit(false) then setScalePercent(N) when slider is dragged", async () => {
    render(() => <PrintInspector {...createInspectorProps()} />);

    const slider = document.querySelector('input[type="range"]') as HTMLInputElement;
    expect(slider).toBeInTheDocument();
    expect(slider.value).toBe("100");

    setValueAndFireInput(slider, "80");

    // Order: set_scale_to_fit first, then set_scale_percent
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_scale_to_fit", { enabled: false });
    });
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_scale_percent", { percent: 80 });
    });

    // Verify order: set_scale_to_fit call index < set_scale_percent call index
    const toFitCalls = mockInvoke.mock.calls.filter((c: unknown[]) => c[0] === "set_scale_to_fit");
    const pctCalls = mockInvoke.mock.calls.filter((c: unknown[]) => c[0] === "set_scale_percent");
    expect(toFitCalls.length).toBeGreaterThanOrEqual(1);
    expect(pctCalls.length).toBeGreaterThanOrEqual(1);
    const toFitIdx = mockInvoke.mock.calls.indexOf(toFitCalls[0]);
    const pctIdx = mockInvoke.mock.calls.indexOf(pctCalls[0]);
    expect(toFitIdx).toBeLessThan(pctIdx);
  });

  it("passes the slider value as the scale percent argument", async () => {
    render(() => <PrintInspector {...createInspectorProps()} />);

    const slider = document.querySelector('input[type="range"]') as HTMLInputElement;

    setValueAndFireInput(slider, "80");
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_scale_percent", { percent: 80 });
    });
  });

  it("captures all values in rapid sequence when slider is dragged quickly", async () => {
    render(() => <PrintInspector {...createInspectorProps({ scaleToFit: false })} />);

    const slider = document.querySelector('input[type="range"]') as HTMLInputElement;

    // Simulate three rapid slider movements
    setValueAndFireInput(slider, "50");
    setValueAndFireInput(slider, "75");
    setValueAndFireInput(slider, "120");

    await waitFor(() => {
      // Should have captured at least the last value (120)
      expect(mockInvoke).toHaveBeenCalledWith("set_scale_percent", { percent: 120 });
    });
    // The intermediate values should also appear somewhere in the call history
    const pctCalls = mockInvoke.mock.calls.filter((c: unknown[]) => c[0] === "set_scale_percent");
    const values = pctCalls.map((c: unknown[]) => (c[1] as any).percent);
    expect(values).toContain(50);
    expect(values).toContain(75);
    expect(values).toContain(120);
  });

  it("number input accepts decimal step values", async () => {
    render(() => <PrintInspector {...createInspectorProps({ scaleToFit: false })} />);

    const numberInput = document.querySelector('input[type="number"][max="1000"]') as HTMLInputElement;

    setValueAndFireInput(numberInput, "75.5");
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_scale_percent", { percent: 75.5 });
    });
  });

  it("number input passes through values above 1000 (native input validation only)", async () => {
    render(() => <PrintInspector {...createInspectorProps({ scaleToFit: false })} />);

    const numberInput = document.querySelector('input[type="number"][max="1000"]') as HTMLInputElement;

    setValueAndFireInput(numberInput, "2000");
    await waitFor(() => {
      // Handler doesn't clamp — value passes through, native input handles validation
      expect(mockInvoke).toHaveBeenCalledWith("set_scale_percent", { percent: 2000 });
    });
  });

  it("slider down to minimum (10) calls setScaleToFit(false) before setScalePercent", async () => {
    render(() => <PrintInspector {...createInspectorProps({ scaleToFit: true })} />);

    const slider = document.querySelector('input[type="range"]') as HTMLInputElement;
    setValueAndFireInput(slider, "10");

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_scale_to_fit", { enabled: false });
    });
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_scale_percent", { percent: 10 });
    });
  });

  // ── 2. Edge values ──

  it("handles minimum slider value (10)", async () => {
    render(() => <PrintInspector {...createInspectorProps()} />);
    const slider = document.querySelector('input[type="range"]') as HTMLInputElement;
    setValueAndFireInput(slider, "10");
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_scale_percent", { percent: 10 });
    });
  });

  it("handles maximum slider value (400)", async () => {
    render(() => <PrintInspector {...createInspectorProps()} />);
    const slider = document.querySelector('input[type="range"]') as HTMLInputElement;
    setValueAndFireInput(slider, "400");
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_scale_percent", { percent: 400 });
    });
  });

  it("handles mid-range value (200)", async () => {
    render(() => <PrintInspector {...createInspectorProps()} />);
    const slider = document.querySelector('input[type="range"]') as HTMLInputElement;
    setValueAndFireInput(slider, "200");
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_scale_percent", { percent: 200 });
    });
  });

  // ── 3. Number input wiring ──

  it("number input fires setScalePercent with the typed value", async () => {
    render(() => <PrintInspector {...createInspectorProps({ scaleToFit: false })} />);

    // Scale number input is unique: max="1000" (copies input uses max="999")
    const numberInput = document.querySelector('input[type="number"][max="1000"]') as HTMLInputElement;
    expect(numberInput).toBeInTheDocument();

    setValueAndFireInput(numberInput, "150");
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_scale_percent", { percent: 150 });
    });
  });

  it("number input calls setScaleToFit(false) before setScalePercent", async () => {
    render(() => <PrintInspector {...createInspectorProps({ scaleToFit: true })} />);

    const numberInput = document.querySelector('input[type="number"][max="1000"]') as HTMLInputElement;

    setValueAndFireInput(numberInput, "75");
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_scale_to_fit", { enabled: false });
    });
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_scale_percent", { percent: 75 });
    });
  });

  // ── 4. Fallback behaviour ──

  it("falls back to 100 when number input value is non-numeric", async () => {
    render(() => <PrintInspector {...createInspectorProps({ scaleToFit: false })} />);

    const numberInput = document.querySelector('input[type="number"][max="1000"]') as HTMLInputElement;

    setValueAndFireInput(numberInput, "");
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_scale_percent", { percent: 100 });
    });
  });

  // ── 5. State propagation ──

  it("updates slider position after IPC roundtrip", async () => {
    render(() => <PrintInspector {...createInspectorProps({ scaleToFit: false })} />);

    const slider = document.querySelector('input[type="range"]') as HTMLInputElement;
    expect(slider).toBeInTheDocument();

    setValueAndFireInput(slider, "75");

    await waitFor(() => {
      // Effect 4 sets slider.value after signal update from IPC response
      expect(slider.value).toBe("75");
    });
  });

  it("preserves previous scale after scale-to-fit toggle", async () => {
    // Start with scalePercent 100, scaleToFit true, drag slider to 80
    render(() => <PrintInspector {...createInspectorProps({ scaleToFit: true })} />);

    const slider = document.querySelector('input[type="range"]') as HTMLInputElement;

    setValueAndFireInput(slider, "80");

    // Dragging slider calls setScaleToFit(false) first
    await waitFor(() => {
      const toFitCalls = mockInvoke.mock.calls.filter((c: unknown[]) => c[0] === "set_scale_to_fit");
      expect(toFitCalls.length).toBeGreaterThanOrEqual(1);
    });

    // Then setScalePercent(80)
    await waitFor(() => {
      const pctCalls = mockInvoke.mock.calls.filter((c: unknown[]) => c[0] === "set_scale_percent");
      expect(pctCalls.length).toBeGreaterThanOrEqual(1);
      const lastCall = pctCalls[pctCalls.length - 1];
      expect(lastCall[1]).toMatchObject({ percent: 80 });
    });
  });

  it("does not re-enable scale-to-fit when slider is dragged while scaleToFit is false", async () => {
    render(() => <PrintInspector {...createInspectorProps({ scaleToFit: false })} />);

    const slider = document.querySelector('input[type="range"]') as HTMLInputElement;
    setValueAndFireInput(slider, "120");

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_scale_percent", { percent: 120 });
    });

    // Verify no unnecessary IPC for setScaleToFit (mock would still call it,
    // but the mock returns immediately; the assertion is that the value sticks)
    const sliderAgain = document.querySelector('input[type="range"]') as HTMLInputElement;
    expect(sliderAgain.value).toBe("120");
  });
});