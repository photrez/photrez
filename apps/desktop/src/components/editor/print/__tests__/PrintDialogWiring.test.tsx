// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, expect, it, vi } from "vitest";
import { render } from "@solidjs/testing-library";
import { PrintPaperViewport } from "../PrintPaperViewport";
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
  printerDpi: 300,
};

// Mock Tauri IPC
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string, args?: any) => {
    if (cmd === "get_system_printers") {
      return Promise.resolve({
        ok: true,
        data: {
          printers: ["Epson Stylus Pro 3880", "Canon PIXMA PRO-100"],
          default: "Epson Stylus Pro 3880",
        },
      });
    }
    if (cmd === "get_print_settings") {
      return Promise.resolve({
        ok: true,
        data: {
          selected_printer: "",
          copies: 1,
          paper_preset: "A4",
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
          printer_dpi: 300,
        },
      });
    }
    if (cmd === "print_image") {
      return Promise.resolve({ ok: true, data: { printed: "test.png" } });
    }
    return Promise.resolve({ ok: true });
  }),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

/**
 * Creates basic PrintInspector props for wiring tests.
 * Pre-seeds selectedPrinter so the dropdown displays the default.
 */
function createInspectorProps() {
  // Match Rust defaults (what get_print_settings returns),
  // overriding BASE_OPTIONS where they differ.
  const [opts, setOptions] = createSignal<PrintOptions>({
    ...BASE_OPTIONS,
    selectedPrinter: "Epson Stylus Pro 3880",
    scaleToFit: true,   // Rust returns scale_to_fit: true
    unit: "mm",         // Rust returns unit: "mm"
  });

  const stub = vi.fn(async () => {});

  return {
    options: opts,
    setOptions,
    loading: false,
    isPendingSetPaper: () => false,
    docWidthPx: 3000,
    docHeightPx: 2000,
    setPaper: stub, toggleOrientation: stub, setMarginMm: stub,
    setScaleToFit: stub, setScalePercent: stub, setCenterImage: stub,
    setTopOffsetMm: stub, setLeftOffsetMm: stub, setCopies: stub,
    setUnit: stub, setShowPaperWhite: stub, setPerSideMargins: stub, setPrinter: stub,
    openPrinterProperties: stub,
  };
}

describe("Pro Print Dialog Wiring", () => {
  it("renders paper viewport with physical dimension badge", () => {
    const [options, setOptions] = createSignal<PrintOptions>({
      ...BASE_OPTIONS,
      paperWidthMm: 210,
      paperHeightMm: 297,
      unit: "cm",
    });

    const { getByText } = render(() => (
      <PrintPaperViewport
        options={options}
        previewUrl="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="
        docWidthPx={3000}
        docHeightPx={2000}
      />
    ));

    expect(getByText(/21 cm × 29.7 cm/i)).toBeInTheDocument();
    expect(getByText(/A4/i)).toBeInTheDocument();
  });

  it("renders inspector sections: Printer Setup, Color Management, Position and Size", async () => {
    const { getByText, findByDisplayValue } = render(() => (
      <PrintInspector {...createInspectorProps()} />
    ));

    expect(getByText("Printer Setup")).toBeInTheDocument();
    expect(getByText("Color Management")).toBeInTheDocument();
    expect(getByText("Position and Size")).toBeInTheDocument();

    // Check printer dropdown loaded
    const select = await findByDisplayValue("Epson Stylus Pro 3880");
    expect(select).toBeInTheDocument();
  });

  it("calculates live PPI and quality badge correctly", () => {
    const { getByText } = render(() => (
      <PrintInspector {...createInspectorProps()} />
    ));

    // 3000px doc at 100% scale = 300 PPI -> Optimal
    expect(getByText(/Optimal \(300\+ PPI\)/i)).toBeInTheDocument();
  });

  it("renders unit suffix labels next to Width and Height inputs", () => {
    const { getAllByText } = render(() => (
      <PrintInspector {...createInspectorProps()} />
    ));

    // Default unit from Rust is "mm"
    const mmElements = getAllByText("mm");
    expect(mmElements.length).toBeGreaterThanOrEqual(2);
  });

  it("toggles orientation layout without checkbox icons", () => {
    const { getByTitle } = render(() => (
      <PrintInspector {...createInspectorProps()} />
    ));

    const portraitBtn = getByTitle("Portrait");
    const landscapeBtn = getByTitle("Landscape");
    expect(portraitBtn).toHaveTextContent("Portrait");
    expect(landscapeBtn).toHaveTextContent("Landscape");
  });
});
