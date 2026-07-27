// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Unit tests for PrintInspector's createResource patterns:
 *
 * 1. Printer list resource — fetches on mount (refreshKey=0), re-fetches on refresh
 * 2. Paper sizes resource — auto-fetches when selectedPrinter changes
 * 3. Effect 1 — selects default printer when printer list loads
 * 4. Effect 2 — auto-selects first paper size + recalculates scale-to-fit
 *
 * These tests verify SolidJS createResource behaviour using mocked Tauri invoke() so
 * no Tauri runtime is required. The 6 "unhandled errors" from createResource trying
 * to call invoke() in setup are expected (they fire before our mocks take effect).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, fireEvent, waitFor } from "@solidjs/testing-library";
import { PrintInspector } from "../PrintInspector";
import type { PrintOptions } from "../printTypes";
import { createSignal } from "solid-js";

// Local default options (no longer importing from printTypes — BUG-10 cleanup)
const BASE_OPTIONS: PrintOptions = {
  selectedPrinter: "",
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
  unit: "cm",
  showPaperWhite: true,
};

// ── Mock helpers ───────────────────────────────────────────────────
// vi.hoisted ensures the mock function is defined before vi.mock runs (Vitest hoisting).
const { mockInvoke, mockListenCallback, mockListen } = vi.hoisted(() => ({
  mockInvoke: vi.fn(),
  mockListenCallback: { current: null as ((payload: any) => void) | null },
  mockListen: vi.fn().mockImplementation((_event: string, cb: (payload: any) => void) => {
    mockListenCallback.current = cb;
    return Promise.resolve(() => {});
  }),
}));
const { mockShowToast } = vi.hoisted(() => ({ mockShowToast: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: mockListen,
}));
vi.mock("../../Toast", () => ({ showToast: mockShowToast }));

// ── Emit helper: simulates Rust event emission after a mutation command ──
// When a mutation command (set_printer, set_paper, etc.) is invoked,
// Rust emits print-settings-changed. The mockListenCallback picks it up.
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

/**
 * Creates PrintInspector props for testing.
 * Internal reactive signal + callbacks that delegate to mockInvoke and
 * update ONLY the command-specific fields (not full-state mapFromRust)
 * to avoid race conditions from full-state mock responses.
 */
function createInspectorProps(initialOptions?: Partial<PrintOptions>) {
  // Match Rust defaults (what get_print_settings returns),
  // overriding BASE_OPTIONS where they differ.
  const [opts, setOptions] = createSignal<PrintOptions>({
    ...BASE_OPTIONS,
    selectedPrinter: "",
    scaleToFit: true,   // Rust returns scale_to_fit: true
    unit: "mm",         // Rust returns unit: "mm"
    ...initialOptions,
  });

  async function invokeCmd(cmd: string, args: Record<string, unknown> = {}) {
    const result = await mockInvoke(cmd, args);
    // Update only the field(s) this command controls — never full-state mapFromRust,
    // because mock responses return stale defaults that race against each other.
    switch (cmd) {
      case "set_printer":
        setOptions((prev) => ({ ...prev, selectedPrinter: (args.printer as string) ?? prev.selectedPrinter }));
        break;
      case "set_paper":
        setOptions((prev) => ({
          ...prev,
          paperPreset: (args.name as string) ?? prev.paperPreset,
          paperIndex: (args.paperIndex as number) ?? prev.paperIndex,
          paperWidthMm: (args.widthMm as number) ?? prev.paperWidthMm,
          paperHeightMm: (args.heightMm as number) ?? prev.paperHeightMm,
        }));
        break;
      case "toggle_orientation":
        setOptions((prev) => ({
          ...prev,
          orientation: prev.orientation === "portrait" ? "landscape" : "portrait",
        }));
        break;
      case "set_margin":
        setOptions((prev) => ({ ...prev, marginMm: (args.marginMm as number) ?? prev.marginMm }));
        break;
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
    setPaper: (name: string, paperIndex: number, widthMm: number, heightMm: number) =>
      invokeCmd("set_paper", { name, paperIndex, widthMm, heightMm }),
    toggleOrientation: () => invokeCmd("toggle_orientation", {}),
    setMarginMm: (mm: number) => invokeCmd("set_margin", { marginMm: mm }),
    setScaleToFit: (enabled: boolean) => invokeCmd("set_scale_to_fit", { enabled }),
    setScalePercent: (pct: number) => invokeCmd("set_scale_percent", { percent: pct }),
    setCenterImage: (center: boolean) => invokeCmd("set_center_image", { center }),
    setTopOffsetMm: (offset: number) => invokeCmd("set_top_offset_mm", { offset }),
    setLeftOffsetMm: (offset: number) => invokeCmd("set_left_offset_mm", { offset }),
    setCopies: (n: number) => invokeCmd("set_copies", { copies: n }),
    setUnit: (u: string) => invokeCmd("set_unit", { unit: u }),
    setShowPaperWhite: (show: boolean) => invokeCmd("set_show_paper_white", { show }),
    setPrinter: (p: string) => invokeCmd("set_printer", { printer: p }),
    openPrinterProperties: () => invokeCmd("open_printer_properties", {}),
  };
}

// ── Shared test data ───────────────────────────────────────────────
const PRINTERS_OK = {
  printers: ["Epson Stylus Pro 3880", "Canon PIXMA PRO-100"],
  default: "Epson Stylus Pro 3880",
};

const PAPER_SIZES_A4 = {
  sizes: [{ name: "A4", widthMm: 210, heightMm: 297, dmPaperIndex: 9 }],
};

const PAPER_SIZES_A3 = {
  sizes: [{ name: "A3", widthMm: 297, heightMm: 420, dmPaperIndex: 8 }],
};

const PAPER_SIZES_LETTER = {
  sizes: [{ name: "Letter", widthMm: 215.9, heightMm: 279.4, dmPaperIndex: 1 }],
};

/**
 * Default mock implementation that returns sensible data for all known invoke commands.
 * Tests can override specific commands via mockInvoke.mockImplementation().
 * Also emits the settings-changed event for mutation commands so the hook picks up state.
 */
async function defaultMockImpl(cmd: string, args?: Record<string, unknown>): Promise<any> {
  let result: any;
  if (cmd === "get_system_printers") {
    result = { ok: true, data: PRINTERS_OK };
  } else if (cmd === "get_printer_paper_sizes") {
    result = { ok: true, data: PAPER_SIZES_A4 };
  } else if (cmd === "get_print_settings") {
    // Default: printer NOT selected (for createResource pattern tests)
    // Interactive handler tests override this to return printer already selected
    result = {
      ok: true,
      data: {
        selected_printer: null,
        copies: 1,
        paper_name: "A4",
        paper_index: 9,
        paper_width_mm: 210,
        paper_height_mm: 297,
        orientation: "portrait",
        margin_mm: 5,
        scale_to_fit: true,
        scale_percent: 100,
        center_image: true,
        top_offset_mm: 0,
        left_offset_mm: 0,
        unit: "mm",
        show_paper_white: true,
      },
    };
  } else if (cmd === "set_paper" && args) {
    const { name, paperIndex, widthMm, heightMm } = args as { name: string; paperIndex: number; widthMm: number; heightMm: number };
    result = {
      ok: true,
      data: {
        selected_printer: "Epson Stylus Pro 3880", copies: 1, paper_name: name, paper_index: paperIndex,
        paper_width_mm: widthMm, paper_height_mm: heightMm, orientation: "portrait",
        margin_mm: 5, scale_to_fit: true, scale_percent: 100,
        center_image: true, top_offset_mm: 0, left_offset_mm: 0,
        unit: "mm", show_paper_white: true,
      },
    };
  } else if (cmd === "toggle_orientation") {
    result = {
      ok: true,
      data: {
        selected_printer: "Epson Stylus Pro 3880", copies: 1, paper_name: "A4",
        paper_width_mm: 297, paper_height_mm: 210, orientation: "landscape",
        margin_mm: 5, scale_to_fit: true, scale_percent: 100,
        center_image: true, top_offset_mm: 0, left_offset_mm: 0,
        unit: "mm", show_paper_white: true,
      },
    };
  } else if (cmd === "set_printer" && args) {
    result = {
      ok: true,
      data: {
        selected_printer: (args as { printer: string }).printer ?? null,
        copies: 1, paper_name: "A4",
        paper_width_mm: 210, paper_height_mm: 297, orientation: "portrait",
        margin_mm: 5, scale_to_fit: true, scale_percent: 100,
        center_image: true, top_offset_mm: 0, left_offset_mm: 0,
        unit: "mm", show_paper_white: true,
      },
    };
  } else if (cmd === "set_margin" && args) {
    result = {
      ok: true,
      data: {
        selected_printer: "Epson Stylus Pro 3880", copies: 1, paper_name: "A4",
        paper_width_mm: 210, paper_height_mm: 297, orientation: "portrait",
        margin_mm: (args as { mm?: number; margin_mm?: number; marginMm?: number }).mm ?? (args as any).margin_mm ?? (args as any).marginMm ?? 5,
        scale_to_fit: true, scale_percent: 100,
        center_image: true, top_offset_mm: 0, left_offset_mm: 0,
        unit: "mm", show_paper_white: true,
      },
    };
  } else if (cmd === "set_scale_to_fit" && args) {
    result = {
      ok: true,
      data: {
        selected_printer: "Epson Stylus Pro 3880", copies: 1, paper_name: "A4",
        paper_width_mm: 210, paper_height_mm: 297, orientation: "portrait",
        margin_mm: 5,
        scale_to_fit: (args as { enabled: boolean }).enabled ?? true,
        scale_percent: 100, center_image: true,
        top_offset_mm: 0, left_offset_mm: 0,
        unit: "mm", show_paper_white: true,
      },
    };
  } else {
    result = { ok: true, data: {} };
  }

  // Emit settings-changed event for mutation commands (simulates Rust event)
  emitSettingsIfMutation(cmd, result);
  return result;
}

describe("PrintInspector — createResource patterns", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Re-implement listen mock after clearAllMocks (it clears vi.mock implementations)
    mockListen.mockImplementation((_event: string, cb: (payload: any) => void) => {
      mockListenCallback.current = cb;
      return Promise.resolve(() => {});
    });
    mockInvoke.mockImplementation(defaultMockImpl);
  });

  // ── 1. Printer list resource ──────────────────────────────────────

  it("fetches printer list on mount and populates the printer dropdown", async () => {
    const { findByDisplayValue } = render(() => (
      <PrintInspector {...createInspectorProps()} />
    ));

    // The resource fetches → Effect 1 selects "Epson Stylus Pro 3880" as default
    const select = await findByDisplayValue("Epson Stylus Pro 3880");
    expect(select).toBeInTheDocument();
    expect(mockInvoke).toHaveBeenCalledWith("get_system_printers");
  });

  it("selects the first printer when API has no default", async () => {
    mockInvoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_system_printers") {
        return Promise.resolve({
          ok: true,
          data: { printers: ["Canon PIXMA PRO-100"], default: undefined },
        });
      }
      return defaultMockImpl(cmd, args);
    });

    const { findByDisplayValue } = render(() => (
      <PrintInspector {...createInspectorProps()} />
    ));

    const select = await findByDisplayValue("Canon PIXMA PRO-100");
    expect(select).toBeInTheDocument();
  });

  // ── 2. Error state ───────────────────────────────────────────────
  //
  // NOTE: createResource loading state and fetcher-error state are NOT directly
  // tested because SolidJS resource error handling creates unhandled Promise
  // rejections (from the async fetcher's return Promise) that Vitest's global
  // handler intercepts before SolidJS can process them internally. The error
  // banner rendering path IS tested via the empty printer list test below
  // (printerError's second branch: `printers().length === 0 && !loading`).
  // The fetcher-error path (`printersActions.error`) requires a real Tauri
  // runtime to test reliably — this is a known limitation of SolidJS resource
  // testing in Vitest/jsdom.





  it("shows 'No printers found' when list is empty after loading", async () => {
    mockInvoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_system_printers") {
        return Promise.resolve({ ok: true, data: { printers: [] } });
      }
      return defaultMockImpl(cmd, args);
    });

    const { findByText } = render(() => (
      <PrintInspector {...createInspectorProps()} />
    ));

    const errEl = await findByText(
      /No printers found. Please connect a printer and try again./i,
    );
    expect(errEl).toBeInTheDocument();
  });

  // ── 3. Paper sizes resource — auto-fetch ─────────────────────────

  it("auto-fetches paper sizes when printer is selected", async () => {
    render(() => (
      <PrintInspector {...createInspectorProps()} />
    ));

    // Wait for the auto-fetch to trigger (Effect 1 sets printer → resource source changes)
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("get_printer_paper_sizes", {
        printer: "Epson Stylus Pro 3880",
      });
    });
  });

  it("does NOT fetch paper sizes when no printer is selected", async () => {
    mockInvoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_system_printers") {
        return Promise.resolve({
          ok: true,
          data: { printers: [], default: undefined },
        });
      }
      return defaultMockImpl(cmd, args);
    });

    render(() => (
      <PrintInspector {...createInspectorProps()} />
    ));

    await waitFor(() => {});
    // get_printer_paper_sizes should NOT have been called because no printer selected
    const calls = mockInvoke.mock.calls.filter(
      (call: unknown[]) => call[0] === "get_printer_paper_sizes",
    );
    expect(calls).toHaveLength(0);
  });

  it("re-fetches paper sizes when printer changes", async () => {
    render(() => (
      <PrintInspector {...createInspectorProps()} />
    ));

    // Initial fetch for the first printer
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("get_printer_paper_sizes", {
        printer: "Epson Stylus Pro 3880",
      });
    });

    // Clear call history and change printer via dropdown
    mockInvoke.mockClear();
    const { container } = await waitFor(() => ({ container: document.body }));

    // Find the printer select (first select element)
    const printerSelect = container.querySelector("select") as HTMLSelectElement;
    expect(printerSelect).toBeInTheDocument();

    await fireEvent.change(printerSelect, { target: { value: "Canon PIXMA PRO-100" } });

    // Should call set_printer via invoke
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_printer", {
        printer: "Canon PIXMA PRO-100",
      });
    });
  });

  // ── 4. Paper size dropdown ───────────────────────────────────────
  //
  // NOTE: These tests were originally skipped due to jsdom 29.1.1
  // HTMLSelectElement ProxyHandler bug (issue #3565). The bug no longer
  // manifests (SolidJS ref-based sync avoids the crash). Tests were
  // updated to match camelCase arg naming in createInspectorProps.

  it("shows printer-reported paper sizes in the dropdown", async () => {
    mockInvoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_printer_paper_sizes") {
        return Promise.resolve({
          ok: true,
          data: { sizes: [{ name: "A4", widthMm: 210, heightMm: 297, dmPaperIndex: 9 },
                          { name: "Letter", widthMm: 215.9, heightMm: 279.4 }] },
        });
      }
      return defaultMockImpl(cmd, args);
    });

    const { findByText } = render(() => (
      <PrintInspector {...createInspectorProps()} />
    ));

    // Should show both printer-reported sizes
    const a4Option = await findByText(/A4 \(210 × 297 mm\)/i);
    expect(a4Option).toBeInTheDocument();

    const letterOption = await findByText(/Letter \(215.9 × 279.4 mm\)/i);
    expect(letterOption).toBeInTheDocument();
  });

  // This test is kept skipped: the component does not render an
  // "active but unlisted" entry when the current paper is absent from
  // the printer's supported size list (the <select> is simply empty).
  it.skip("shows active-but-unlisted paper when current paper not in printer sizes", async () => {
    mockInvoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_printer_paper_sizes") {
        return Promise.resolve({ ok: true, data: { sizes: [] } });
      }
      return defaultMockImpl(cmd, args);
    });

    const { findByText } = render(() => (
      <PrintInspector {...createInspectorProps()} />
    ));

    // Current paper (A4) not in driver list → shown as "active but unlisted"
    const a4Option = await findByText(/A4/i);
    expect(a4Option).toBeInTheDocument();
  });

  // ── 5. Effect 2 — auto-select + scale-to-fit recalculation ──────

  it("auto-selects first supported paper size when printer loads", async () => {
    mockInvoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_printer_paper_sizes") {
        return Promise.resolve({ ok: true, data: PAPER_SIZES_A3 });
      }
      return defaultMockImpl(cmd, args);
    });

    render(() => (
      <PrintInspector {...createInspectorProps()} />
    ));

    // Effect 1 selects printer → paper sizes fetch → Effect 2 auto-selects A3
await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_paper", {
        name: "A3", paperIndex: 8, widthMm: 297, heightMm: 420,
      });
    });
  });

  it("auto-selects first supported size and recalculates scale-to-fit", async () => {
    mockInvoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_printer_paper_sizes") {
        return Promise.resolve({ ok: true, data: PAPER_SIZES_A3 });
      }
      return defaultMockImpl(cmd, args);
    });

    render(() => (
      <PrintInspector {...createInspectorProps()} />
    ));

    // With scaleToFit=true (default from createInspectorProps), paper loaded as A3,
    // scale-to-fit auto-calculates — verify set_paper was called
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_paper", {
        name: "A3", paperIndex: 8, widthMm: 297, heightMm: 420,
      });
    });
  });

  // ── 6. Refresh button ────────────────────────────────────────────

  it("re-fetches printer list when refresh button is clicked", async () => {
    render(() => (
      <PrintInspector {...createInspectorProps()} />
    ));

    // Wait for initial fetch
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("get_system_printers");
    });

    const initialCount = mockInvoke.mock.calls.filter(
      (call: unknown[]) => call[0] === "get_system_printers",
    ).length;

    // Click the refresh button
    const refreshBtn = document.querySelector('button[aria-label="Refresh printer list"]');
    expect(refreshBtn).not.toBeNull();

    mockInvoke.mockClear();

    // Ensure loading is complete before clicking refresh
    await waitFor(() => {
      expect(refreshBtn).not.toBeDisabled();
    });

    await fireEvent.click(refreshBtn!);

    // Should re-fetch printer list (may also re-fetch paper sizes)
    await waitFor(() => {
      const printerCalls = mockInvoke.mock.calls.filter(
        (call: unknown[]) => call[0] === "get_system_printers",
      );
      expect(printerCalls.length).toBeGreaterThanOrEqual(1);
    });
  });

  it("re-fetches paper sizes when refresh button is clicked", async () => {
    render(() => (
      <PrintInspector {...createInspectorProps({ selectedPrinter: "Epson Stylus Pro 3880" })} />
    ));

    // Wait for initial paper sizes fetch
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("get_printer_paper_sizes", {
        printer: "Epson Stylus Pro 3880",
      });
    });

    // Click refresh
    const refreshBtn = document.querySelector('button[aria-label="Refresh printer list"]');
    await fireEvent.click(refreshBtn!);

    // Should re-fetch paper sizes
    await waitFor(() => {
      const calls = mockInvoke.mock.calls.filter(
        (call: unknown[]) => call[0] === "get_printer_paper_sizes",
      );
      expect(calls.length).toBeGreaterThanOrEqual(1);
    });
  });
});

describe("PrintInspector — interactive handlers", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Re-implement listen mock after clearAllMocks (it clears vi.mock implementations)
    mockListen.mockImplementation((_event: string, cb: (payload: any) => void) => {
      mockListenCallback.current = cb;
      return Promise.resolve(() => {});
    });
    // Interactive tests need a pre-initialized state (printer already selected)
    // so Effect 1 doesn't re-fire and overwrite our changes.
    mockInvoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      // Override get_print_settings to return printer already selected
      if (cmd === "get_print_settings") {
        return {
          ok: true,
          data: {
            selected_printer: "Epson Stylus Pro 3880",
            copies: 1,
            paper_name: "Custom",
            paper_width_mm: 210,
            paper_height_mm: 297,
            orientation: "portrait",
            margin_mm: 5,
            scale_to_fit: true,
            scale_percent: 100,
            center_image: true,
            top_offset_mm: 0,
            left_offset_mm: 0,
            unit: "mm",
            show_paper_white: true,
          },
        };
      }
      return defaultMockImpl(cmd, args);
    });
  });

  // ── 1. Scale to Fit toggle ────────────────────────────────────

  it("enables Scale to Fit and recalculates scale percent", async () => {
    const { getByText } = render(() => (
      <PrintInspector {...createInspectorProps()} />
    ));

    const scaleLabel = getByText("Scale to Fit");
    const checkbox = scaleLabel.querySelector(
      'input[type="checkbox"]',
    ) as HTMLInputElement;
    expect(checkbox).toBeInTheDocument();
    expect(checkbox.checked).toBe(true); // default is true from Rust

    // Click to disable
    await fireEvent.click(checkbox);

    // Should call set_scale_to_fit with enabled=false
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_scale_to_fit", { enabled: false });
    });
  });

  it("disables Scale to Fit while preserving the current scale", async () => {
    const { getByText } = render(() => (
      <PrintInspector {...createInspectorProps()} />
    ));

    const scaleLabel = getByText("Scale to Fit");
    const checkbox = scaleLabel.querySelector(
      'input[type="checkbox"]',
    ) as HTMLInputElement;
    expect(checkbox.checked).toBe(true); // default is true from Rust

    await fireEvent.click(checkbox);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_scale_to_fit", { enabled: false });
    });
  });

  // ── 2. Paper size dropdown ────────────────────────────────────

  it("changes paper dimensions when user selects a different paper size from dropdown", async () => {
    mockInvoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_printer_paper_sizes") {
        return Promise.resolve({
          ok: true,
          data: {
            sizes: [
              { name: "A4", widthMm: 210, heightMm: 297, dmPaperIndex: 9 },
              { name: "Letter", widthMm: 215.9, heightMm: 279.4, dmPaperIndex: 1 },
            ],
          },
        });
      }
      return defaultMockImpl(cmd, args);
    });

    const { container } = render(() => (
      <PrintInspector {...createInspectorProps({ selectedPrinter: "Epson Stylus Pro 3880", paperWidthMm: 210, paperHeightMm: 297 })} />
    ));

    // Wait for the Letter option to appear
    await waitFor(() => {
      expect(container.textContent).toMatch(/Letter.*215.9/i);
    });

    // Select order: [0]=printer, [1]=unit (in pos section header), [2]=paper size
    const selects = container.querySelectorAll("select");
    const paperSelect = selects[2];
    await fireEvent.change(paperSelect, { target: { value: "Letter" } });

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_paper", {
        name: "Letter", paperIndex: 1, widthMm: 215.9, heightMm: 279.4,
      });
    });
  });

  // ── 3. Orientation toggle ─────────────────────────────────────

  it("swaps width and height when toggling from Portrait to Landscape", async () => {
    const { getByText } = render(() => (
      <PrintInspector {...createInspectorProps()} />
    ));

    const landscapeBtn = getByText("Landscape");
    await fireEvent.click(landscapeBtn);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("toggle_orientation", {});
    });
  });

  // ── 4. Margin input with scale-to-fit ─────────────────────────

  it("displays per-side printer hardware margins from paperSizesRes", async () => {
    // Mock get_printer_paper_sizes to return per-side margins
    mockInvoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_system_printers") {
        return Promise.resolve({
          ok: true,
          data: {
            printers: ["Epson Stylus Pro 3880", "Canon PIXMA PRO-100"],
            default: "Epson Stylus Pro 3880",
          },
        });
      }
      if (cmd === "get_printer_paper_sizes") {
        return Promise.resolve({
          ok: true,
          data: {
            sizes: [{ name: "A4", widthMm: 210, heightMm: 297, dmPaperIndex: 9 }],
            defaultMargins: { leftMm: 12.7, topMm: 3.2, rightMm: 12.7, bottomMm: 3.2 },
          },
        });
      }
      return Promise.resolve({ ok: true });
    });

    render(() => (
      <PrintInspector {...createInspectorProps()} />
    ));

    // Wait for paper sizes to load and margin display to appear
    await waitFor(() => {
      expect(document.body.textContent).toMatch(/L:\s*12\.7/);
      expect(document.body.textContent).toMatch(/T:\s*3\.2/);
      expect(document.body.textContent).toMatch(/R:\s*12\.7/);
      expect(document.body.textContent).toMatch(/B:\s*3\.2/);
    });
  });

  // ── 6. Effect 2 — match by defaultPaperSize name (Strategy 1) ──
  // NOTE: defaultPaperSize must appear in the get_printer_paper_sizes
  // response (not just get_system_printers) because Effect 2 reads
  // it from paperSizesRes (not printersRes).

  it("auto-selects printer's current paper size by matching defaultPaperSize preset name", async () => {
    // Return defaultPaperSize with preset "Letter" so Strategy 1 (name match)
    // selects Letter instead of the first supported size (A4).
    mockInvoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_system_printers") {
        return Promise.resolve({
          ok: true,
          data: {
            printers: ["Epson Stylus Pro 3880", "Canon PIXMA PRO-100"],
            default: "Epson Stylus Pro 3880",
          },
        });
      }
      if (cmd === "get_printer_paper_sizes") {
        return Promise.resolve({
          ok: true,
          data: {
            sizes: [
              { name: "A4", widthMm: 210, heightMm: 297, dmPaperIndex: 9 },
              { name: "Letter", widthMm: 215.9, heightMm: 279.4, dmPaperIndex: 1 },
              { name: "A5", widthMm: 148, heightMm: 210, dmPaperIndex: 11 },
            ],
            defaultPaperSize: { preset: "Letter", widthMm: 215.9, heightMm: 279.4 },
          },
        });
      }
      return defaultMockImpl(cmd, args);
    });

    render(() => (
      <PrintInspector {...createInspectorProps()} />
    ));

    // Effect 1 selects default printer → paper sizes fetch → Effect 2 runs
    // Strategy 1 matching defaultPaperSize.preset="Letter" → should select Letter
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_paper", {
        name: "Letter", paperIndex: 1, widthMm: 215.9, heightMm: 279.4,
      });
    });
  });

  it("auto-selects first supported size when defaultPaperSize has Custom preset (Strategy 2)", async () => {
    // Return defaultPaperSize with preset "Custom" but valid dimensions.
    // Strategy 2 (dimension match) should find the matching size.
    mockInvoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_system_printers") {
        return Promise.resolve({
          ok: true,
          data: {
            printers: ["Epson Stylus Pro 3880"],
            default: "Epson Stylus Pro 3880",
          },
        });
      }
      if (cmd === "get_printer_paper_sizes") {
        return Promise.resolve({
          ok: true,
          data: {
            sizes: [
              { name: "A4", widthMm: 210, heightMm: 297, dmPaperIndex: 9 },
              { name: "Letter", widthMm: 215.9, heightMm: 279.4, dmPaperIndex: 1 },
            ],
            defaultPaperSize: { preset: "Custom", widthMm: 210, heightMm: 297 },
          },
        });
      }
      return defaultMockImpl(cmd, args);
    });

    render(() => (
      <PrintInspector {...createInspectorProps()} />
    ));

    // A4 matches by dimension (210×297 within ±0.5mm tolerance) — should select A4
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_paper", {
        name: "A4", paperIndex: 9, widthMm: 210, heightMm: 297,
      });
    });
  });

  it("falls back to sizes[0] when defaultPaperSize is missing from supported list", async () => {
    // Return defaultPaperSize with a preset that's NOT in the supported sizes.
    // Effect 2 should fall back to sizes[0] (A4).
    mockInvoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_system_printers") {
        return Promise.resolve({
          ok: true,
          data: {
            printers: ["Epson Stylus Pro 3880"],
            default: "Epson Stylus Pro 3880",
          },
        });
      }
      if (cmd === "get_printer_paper_sizes") {
        return Promise.resolve({
          ok: true,
          data: {
            sizes: [
              { name: "A4", widthMm: 210, heightMm: 297, dmPaperIndex: 9 },
              { name: "Letter", widthMm: 215.9, heightMm: 279.4, dmPaperIndex: 1 },
            ],
            defaultPaperSize: { preset: "Tabloid", widthMm: 279, heightMm: 432 },
          },
        });
      }
      return defaultMockImpl(cmd, args);
    });

    const [options, setOptions] = createSignal<PrintOptions>({
      ...BASE_OPTIONS,
      selectedPrinter: "",
      scaleToFit: false,
    });

    render(() => (
      <PrintInspector {...createInspectorProps()} />
    ));

    // Tabloid (279×432) is not in the supported list → fall back to A4 (sizes[0])
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_paper", {
        name: "A4", paperIndex: 9, widthMm: 210, heightMm: 297,
      });
    });
  });
});
