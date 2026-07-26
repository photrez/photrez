// SPDX-License-Identifier: AGPL-3.0-or-later
// Shared test helpers for print system tests

import { vi } from "vitest";

/** Default Rust print settings shape returned by get_print_settings */
export const defaultPrintSettings = {
  ok: true,
  data: {
    selected_printer: "Default Printer",
    copies: 1,
    paper_preset: "A4",
    paper_width_mm: 210,
    paper_height_mm: 297,
    orientation: "portrait",
    margin_mm: 5,
    scale_to_fit: false,
    scale_percent: 100,
    center_image: true,
    top_offset_mm: 0,
    left_offset_mm: 0,
    unit: "mm",
    show_paper_white: true,
  },
};

/** Create a mockInvoke that handles get_print_settings + other commands */
export function createPrintMock(overrides?: Record<string, unknown>) {
  const settings = overrides
    ? { ...defaultPrintSettings.data, ...overrides }
    : defaultPrintSettings.data;

  return vi.fn().mockImplementation(async (cmd: string, _args?: unknown) => {
    if (cmd === "get_print_settings") {
      return { ok: true, data: settings };
    }
    return { ok: true, data: { printed: "/tmp/photrez-print-12345.png", copies: 1 } };
  });
}
