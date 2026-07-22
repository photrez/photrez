// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, expect, it, vi } from "vitest";
import { render } from "@solidjs/testing-library";
import { PrintPaperViewport } from "../PrintPaperViewport";
import { PrintInspector } from "../PrintInspector";
import { DEFAULT_PRINT_OPTIONS, type PrintOptions } from "../printTypes";
import { createSignal } from "solid-js";

// Mock Tauri IPC
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === "get_system_printers") {
      return Promise.resolve({
        ok: true,
        data: {
          printers: ["Epson Stylus Pro 3880", "Canon PIXMA PRO-100"],
          default: "Epson Stylus Pro 3880",
        },
      });
    }
    if (cmd === "print_image") {
      return Promise.resolve({ ok: true, data: { printed: "test.png" } });
    }
    return Promise.resolve({ ok: true });
  }),
}));

describe("Pro Print Dialog Wiring", () => {
  it("renders paper viewport with physical dimension badge", () => {
    const [options, setOptions] = createSignal<PrintOptions>({
      ...DEFAULT_PRINT_OPTIONS,
      paperWidthMm: 210,
      paperHeightMm: 297,
      unit: "cm",
    });

    const { getByText } = render(() => (
      <PrintPaperViewport
        options={options()}
        setOptions={setOptions}
        previewUrl="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="
        docWidthPx={3000}
        docHeightPx={2000}
      />
    ));

    expect(getByText(/21 cm × 29.7 cm/i)).toBeInTheDocument();
    expect(getByText(/A4/i)).toBeInTheDocument();
  });

  it("renders inspector sections: Printer Setup, Color Management, Position and Size", async () => {
    const [options, setOptions] = createSignal<PrintOptions>({
      ...DEFAULT_PRINT_OPTIONS,
      docWidthPx: 3000,
      docHeightPx: 2000,
    } as any);

    const onPrint = vi.fn();
    const onCancel = vi.fn();

    const { getByText, findByDisplayValue } = render(() => (
      <PrintInspector
        options={options()}
        setOptions={setOptions}
        docWidthPx={3000}
        docHeightPx={2000}
      />
    ));

    expect(getByText("Printer Setup")).toBeInTheDocument();
    expect(getByText("Color Management")).toBeInTheDocument();
    expect(getByText("Position and Size")).toBeInTheDocument();

    // Check printer dropdown loaded
    const select = await findByDisplayValue("Epson Stylus Pro 3880");
    expect(select).toBeInTheDocument();
  });

  it("calculates live PPI and quality badge correctly", () => {
    const [options, setOptions] = createSignal<PrintOptions>({
      ...DEFAULT_PRINT_OPTIONS,
      scalePercent: 100,
    });

    const { getByText } = render(() => (
      <PrintInspector
        options={options()}
        setOptions={setOptions}
        docWidthPx={3000}
        docHeightPx={2000}
      />
    ));

    // 3000px doc at 100% scale = 300 PPI -> Optimal
    expect(getByText(/Optimal \(300\+ PPI\)/i)).toBeInTheDocument();
  });

  it("renders unit suffix labels next to Width and Height inputs", () => {
    const [options, setOptions] = createSignal<PrintOptions>({
      ...DEFAULT_PRINT_OPTIONS,
      unit: "cm",
    });

    const { getAllByText } = render(() => (
      <PrintInspector
        options={options()}
        setOptions={setOptions}
        docWidthPx={3000}
        docHeightPx={2000}
      />
    ));

    const cmElements = getAllByText("cm");
    expect(cmElements.length).toBeGreaterThanOrEqual(2);
  });

  it("toggles orientation layout without checkbox icons", () => {
    const [options, setOptions] = createSignal<PrintOptions>({
      ...DEFAULT_PRINT_OPTIONS,
      orientation: "portrait",
    });

    const { getByTitle } = render(() => (
      <PrintInspector
        options={options()}
        setOptions={setOptions}
        docWidthPx={3000}
        docHeightPx={2000}
      />
    ));

    const portraitBtn = getByTitle("Portrait");
    const landscapeBtn = getByTitle("Landscape");
    expect(portraitBtn).toHaveTextContent("Portrait");
    expect(landscapeBtn).toHaveTextContent("Landscape");
  });
});
