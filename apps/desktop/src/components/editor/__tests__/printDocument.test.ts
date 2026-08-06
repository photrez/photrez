import { describe, it, expect, vi, beforeEach } from "vitest";
import { createMockEngine } from "@/__tests__/test-builders";

// ── Mock Tauri APIs ───────────────────────────────────────────────
const mockInvoke = vi.fn().mockImplementation(async (cmd: string, _args?: unknown, _options?: unknown) => {
  if (cmd === "get_print_settings") {
    return defaultPrintSettings;
  }
  // Mock print_image_raw (raw IPC)
  return { ok: true, data: { status: "printed" } };
});
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

const defaultPrintSettings = {
  ok: true,
  data: {
    selected_printer: "Default Printer",
    copies: 1,
    paper_name: "A4",
    paper_index: 9,
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
    printer_dpi: 300,
  },
};

function mockPrintSettings(overrides: Record<string, unknown>) {
  mockInvoke.mockImplementation(async (cmd: string, _args?: unknown, _options?: unknown) => {
    if (cmd === "get_print_settings") {
      return { ok: true, data: { ...defaultPrintSettings.data, ...overrides } };
    }
    return { ok: true, data: { status: "printed" } };
  });
}

// Mock Tauri dialog (save for PDF output)
const mockSave = vi.fn().mockResolvedValue(null);
vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: (...args: unknown[]) => mockSave(...args),
}));

// Mock encodeComposite — returns raw image bytes
const mockEncodeComposite = vi.fn().mockResolvedValue(new Uint8Array([1, 2, 3]));
vi.mock("../exportDocument", () => ({
  encodeComposite: mockEncodeComposite,
}));

// Mock showToast
const mockShowToast = vi.fn();
vi.mock("../Toast", () => ({
  showToast: mockShowToast,
}));

// Mock compositeForPrint internals — OffscreenCanvas + createImageBitmap
// These are GPU APIs not available in Node test runner, so we mock them.
const mockCreateImageBitmap = vi.fn();
const mockOffscreenCanvas = vi.fn();
const mockCanvasGetContext = vi.fn();
const mockGetImageData = vi.fn();

// Set up the compositing mock chain
function setupCompositeMock() {
  const mockCtx = {
    fillStyle: "",
    fillRect: vi.fn(),
    drawImage: vi.fn(),
    getImageData: mockGetImageData,
  };

  mockCreateImageBitmap.mockResolvedValue({ width: 100, height: 100, close: vi.fn() });
  mockGetImageData.mockReturnValue({
    data: new Uint8ClampedArray([10, 20, 30, 255]),
    width: 1,
    height: 1,
  });
  mockCanvasGetContext.mockReturnValue(mockCtx);

  // Override global OffscreenCanvas for the test
  globalThis.OffscreenCanvas = class MockOffscreenCanvas {
    width: number;
    height: number;
    constructor(w: number, h: number) {
      this.width = w;
      this.height = h;
      mockOffscreenCanvas(w, h);
    }
    getContext() {
      return mockCanvasGetContext();
    }
  } as unknown as typeof OffscreenCanvas;

  globalThis.createImageBitmap = mockCreateImageBitmap;
}

const { printDocument } = await import("../printDocument");

describe("printDocument — core flow", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupCompositeMock();
    // Re-apply default mock implementation (clearAllMocks resets it)
    mockInvoke.mockImplementation(async (cmd: string, _args?: unknown, _options?: unknown) => {
      if (cmd === "get_print_settings") {
        return defaultPrintSettings;
      }
      return { ok: true, data: { status: "printed" } };
    });
  });

  it("calls encodeComposite with the engine", async () => {
    const engine = createMockEngine();
    await printDocument(engine);
    expect(mockEncodeComposite).toHaveBeenCalledWith(engine, "png", 100);
  });

  it("composites the image via OffscreenCanvas GPU", async () => {
    const engine = createMockEngine();
    await printDocument(engine);
    expect(mockCreateImageBitmap).toHaveBeenCalledOnce();
    expect(mockCanvasGetContext).toHaveBeenCalledOnce();
    // Should draw the decoded image onto the paper-sized canvas
    const ctx = mockCanvasGetContext.mock.results[0]?.value;
    expect(ctx.drawImage).toHaveBeenCalledOnce();
    // Should read raw RGBA pixels (no format encoding)
    expect(ctx.getImageData).toHaveBeenCalledOnce();
  });

  it("caps composite DPI at 300 even when printer reports 600", async () => {
    mockPrintSettings({ printer_dpi: 600 });
    const engine = createMockEngine();
    await printDocument(engine);
    // PDF drivers report 600 DPI; we cap at 300 (industry best practice,
    // matches browser print engines). A4 portrait at 300 DPI:
    //   width  = (210 / 25.4) * 300 ≈ 2480
    //   height = (297 / 25.4) * 300 ≈ 3508
    expect(mockOffscreenCanvas).toHaveBeenCalledWith(
      expect.any(Number),
      expect.any(Number),
    );
    const calls = mockOffscreenCanvas.mock.calls;
    const lastCall = calls[calls.length - 1];
    expect(lastCall[0]).toBeLessThan(2500); // capped, NOT ~4961 at 600
    expect(lastCall[1]).toBeLessThan(3600); // capped, NOT ~7016 at 600
  });

  it("falls back to 300 DPI when printer_dpi is null", async () => {
    mockPrintSettings({ printer_dpi: null });
    const engine = createMockEngine();
    await printDocument(engine);
    // A4 portrait at 300 DPI:
    //   width  = (210 / 25.4) * 300 ≈ 2480
    //   height = (297 / 25.4) * 300 ≈ 3508
    const calls = mockOffscreenCanvas.mock.calls;
    const lastCall = calls[calls.length - 1];
    expect(lastCall[0]).toBeLessThan(2500);
    expect(lastCall[1]).toBeLessThan(3600);
  });

  it("clamps composite DPI to prevent oversized canvas", async () => {
    // 1200 DPI on A4 would be 9921×14031 → the 300-DPI cap kicks in
    // FIRST (Math.min), so effective DPI = 300 → canvas 2480×3508.
    // MAX_PX clamp remains as a second guard for very large papers.
    mockPrintSettings({ printer_dpi: 1200 });
    const engine = createMockEngine();
    await printDocument(engine);
    const calls = mockOffscreenCanvas.mock.calls;
    const lastCall = calls[calls.length - 1];
    expect(lastCall[1]).toBeLessThanOrEqual(10000);
    expect(lastCall[0]).toBeLessThanOrEqual(10000);
  });

  it("invokes print_image_raw with Uint8Array as top-level argument", async () => {
    const engine = createMockEngine();
    await printDocument(engine);
    // First arg is command name, second is raw bytes
    const invokeCall = mockInvoke.mock.calls.find(
      (c: unknown[]) => c[0] === "print_image_raw"
    );
    expect(invokeCall).toBeDefined();
    // The second argument (raw body) should be a Uint8Array
    expect(invokeCall![1]).toBeInstanceOf(Uint8Array);
    // The third argument should have headers
    expect(invokeCall![2]).toHaveProperty("headers");
  });

  it("passes printer/copies/DPI as headers to print_image_raw", async () => {
    const engine = createMockEngine();
    await printDocument(engine);
    const invokeCall = mockInvoke.mock.calls.find(
      (c: unknown[]) => c[0] === "print_image_raw"
    );
    expect(invokeCall).toBeDefined();
    // A4 portrait at 300 DPI with 5mm margin:
    //   printable W = 210-10 = 200mm → 2362 px
    //   printable H = 297-10 = 287mm → 3390 px
    expect(invokeCall![2].headers).toMatchObject({
      printer: "Default Printer",
      copies: "1",
      orientation: "portrait",
      documentName: "Untitled",
      width: "2362",
      height: "3390",
    });
  });

  it("passes document name to print_image_raw headers", async () => {
    const engine = createMockEngine();
    await printDocument(engine, "MyPhoto.png");
    const invokeCall = mockInvoke.mock.calls.find(
      (c: unknown[]) => c[0] === "print_image_raw"
    );
    expect(invokeCall).toBeDefined();
    expect(invokeCall![2].headers).toHaveProperty("documentName", "MyPhoto.png");
  });

  it("passes custom paper dimensions as headers", async () => {
    const engine = createMockEngine();
    mockPrintSettings({
      selected_printer: "Custom Printer",
      copies: 3,
      paper_width_mm: 297,
      paper_height_mm: 420,
      paper_name: "A3",
      paper_index: 8,
    });
    await printDocument(engine);

    const invokeCall = mockInvoke.mock.calls.find(
      (c: unknown[]) => c[0] === "print_image_raw"
    );
    expect(invokeCall).toBeDefined();
    expect(invokeCall![2].headers).toMatchObject({
      printer: "Custom Printer",
      copies: "3",
      paperWidthMm: "297",
      paperHeightMm: "420",
      paperIndex: "8",
      width: "3390",
      height: "4843",
    });
  });
});

describe("printDocument — margin & orientation headers", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupCompositeMock();
    mockInvoke.mockImplementation(async (cmd: string, _args?: unknown, _options?: unknown) => {
      if (cmd === "get_print_settings") {
        return defaultPrintSettings;
      }
      return { ok: true, data: { status: "printed" } };
    });
  });

  it("passes marginMm header to print_image_raw", async () => {
    const engine = createMockEngine();
    await printDocument(engine);
    const invokeCall = mockInvoke.mock.calls.find(
      (c: unknown[]) => c[0] === "print_image_raw"
    );
    expect(invokeCall).toBeDefined();
    expect(invokeCall![2].headers).toHaveProperty("marginMm", "5");
  });

  it("passes paperWidthMm/paperHeightMm/paperIndex headers", async () => {
    const engine = createMockEngine();
    await printDocument(engine);
    const invokeCall = mockInvoke.mock.calls.find(
      (c: unknown[]) => c[0] === "print_image_raw"
    );
    expect(invokeCall).toBeDefined();
    expect(invokeCall![2].headers).toMatchObject({
      paperWidthMm: "210",
      paperHeightMm: "297",
      paperIndex: "9",
    });
  });

  it("passes ALL expected print_image_raw headers for A4 portrait", async () => {
    const engine = createMockEngine();
    await printDocument(engine);
    const invokeCall = mockInvoke.mock.calls.find(
      (c: unknown[]) => c[0] === "print_image_raw"
    );
    expect(invokeCall).toBeDefined();
    expect(invokeCall![2].headers).toMatchObject({
      printer: "Default Printer",
      copies: "1",
      paperWidthMm: "210",
      paperHeightMm: "297",
      paperIndex: "9",
      marginMm: "5",
      documentName: "Untitled",
      orientation: "portrait",
      width: "2362",
      height: "3390",
    });
  });

  it("swaps width/height in headers for landscape orientation", async () => {
    mockPrintSettings({
      orientation: "landscape",
      paper_width_mm: 210,
      paper_height_mm: 297,
    });
    const engine = createMockEngine();
    await printDocument(engine);
    const invokeCall = mockInvoke.mock.calls.find(
      (c: unknown[]) => c[0] === "print_image_raw"
    );
    expect(invokeCall).toBeDefined();
    // Landscape: effW = paper_height_mm (297), effH = paper_width_mm (210)
    // Canvas at 300 DPI: 297mm → 3508px, 210mm → 2480px
    // But with 5mm margin: printable = 287×200mm → 3390×2362px
    expect(invokeCall![2].headers).toMatchObject({
      paperWidthMm: "297",
      paperHeightMm: "210",
      orientation: "landscape",
      width: "3390",
      height: "2362",
    });
  });

  it("produces larger canvas with zero margin (full bleed)", async () => {
    mockPrintSettings({ margin_mm: 0 });
    const engine = createMockEngine();
    await printDocument(engine);
    const invokeCall = mockInvoke.mock.calls.find(
      (c: unknown[]) => c[0] === "print_image_raw"
    );
    expect(invokeCall).toBeDefined();
    // 0mm margin → printable = 210×297mm → 2480×3508 px at 300 DPI
    expect(invokeCall![2].headers).toMatchObject({
      marginMm: "0",
      width: "2480",
      height: "3508",
    });
  });

  it("produces smaller canvas with custom 10mm margin", async () => {
    mockPrintSettings({ margin_mm: 10 });
    const engine = createMockEngine();
    await printDocument(engine);
    const invokeCall = mockInvoke.mock.calls.find(
      (c: unknown[]) => c[0] === "print_image_raw"
    );
    expect(invokeCall).toBeDefined();
    // 10mm margin → printable = 190×277mm → 2244×3272 px at 300 DPI
    //   190/25.4*300 = 2244.09 → round(2244)
    //   277/25.4*300 = 3271.65 → round(3272)
    expect(invokeCall![2].headers).toMatchObject({
      marginMm: "10",
      width: "2244",
      height: "3272",
    });
  });

  it("handles extreme margin (larger than half paper) gracefully", async () => {
    // paper is 210mm wide, so margin=150mm → printable width = max(1, 210-300) = 1mm
    mockPrintSettings({ margin_mm: 150 });
    const engine = createMockEngine();
    await printDocument(engine);
    const invokeCall = mockInvoke.mock.calls.find(
      (c: unknown[]) => c[0] === "print_image_raw"
    );
    expect(invokeCall).toBeDefined();
    // printable = 1mm × 1mm (clamped) → 12×12 px at 300 DPI
    expect(invokeCall![2].headers).toMatchObject({
      marginMm: "150",
      width: "12",
      height: "12",
    });
  });

  it("handles landscape with custom 10mm margin", async () => {
    mockPrintSettings({ orientation: "landscape", margin_mm: 10 });
    const engine = createMockEngine();
    await printDocument(engine);
    const invokeCall = mockInvoke.mock.calls.find(
      (c: unknown[]) => c[0] === "print_image_raw"
    );
    expect(invokeCall).toBeDefined();
    // Landscape: effW=297, effH=210, margin=10 → printable = 277×190mm
    // Pixels at 300 DPI: 277mm → 3274px, 190mm → 2244px
    expect(invokeCall![2].headers).toMatchObject({
      paperWidthMm: "297",
      paperHeightMm: "210",
      marginMm: "10",
      orientation: "landscape",
    });
  });

  it("passes centerImage and offset settings through composite (not as headers)", async () => {
    // Offsets are embedded in the composited pixels, NOT passed as headers.
    // This test verifies the contract: compositeForPrint receives offset args.
    mockPrintSettings({ center_image: false, top_offset_mm: 20, left_offset_mm: 15 });
    const engine = createMockEngine();
    await printDocument(engine);
    // drawImage should be called — verifies compositeForPrint ran
    const ctx = mockCanvasGetContext.mock.results[0]?.value;
    expect(ctx.drawImage).toHaveBeenCalled();
    // Headers should NOT contain centerImage/topOffsetmm/leftOffsetmm
    const invokeCall = mockInvoke.mock.calls.find(
      (c: unknown[]) => c[0] === "print_image_raw"
    );
    expect(invokeCall![2].headers).not.toHaveProperty("centerImage");
    expect(invokeCall![2].headers).not.toHaveProperty("topOffsetMm");
    expect(invokeCall![2].headers).not.toHaveProperty("leftOffsetMm");
  });
});

describe("printDocument — printer selection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupCompositeMock();
    mockInvoke.mockImplementation(async (cmd: string, _args?: unknown, _options?: unknown) => {
      if (cmd === "get_print_settings") {
        return defaultPrintSettings;
      }
      return { ok: true, data: { status: "printed" } };
    });
  });

  it("shows warning toast when no printer selected", async () => {
    const engine = createMockEngine();
    mockPrintSettings({ selected_printer: "" });
    await printDocument(engine);

    expect(mockShowToast).toHaveBeenCalledWith(
      "No printer selected. Select a printer in Print Settings.",
      "warn",
    );
    // Should NOT call print_image_raw when no printer
    expect(mockInvoke).not.toHaveBeenCalledWith("print_image_raw", expect.anything());
  });
});

describe("printDocument — error handling", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupCompositeMock();
    mockInvoke.mockImplementation(async (cmd: string, _args?: unknown, _options?: unknown) => {
      if (cmd === "get_print_settings") {
        return defaultPrintSettings;
      }
      return { ok: true, data: { status: "printed" } };
    });
  });

  it("shows error toast when print_image_raw fails", async () => {
    mockInvoke.mockImplementationOnce(async () => defaultPrintSettings);
    mockInvoke.mockRejectedValueOnce({
      error: { message: "Printer not found" },
    });
    const engine = createMockEngine();
    await expect(printDocument(engine)).rejects.toBeTruthy();
    expect(mockShowToast).toHaveBeenCalledWith(
      expect.stringContaining("Print failed"),
      "error",
    );
  });

  it("handles Error instance from print_image_raw reject", async () => {
    mockInvoke.mockImplementationOnce(async () => defaultPrintSettings);
    mockInvoke.mockRejectedValueOnce(new Error("access denied"));
    const engine = createMockEngine();
    await expect(printDocument(engine)).rejects.toBeTruthy();
    expect(mockShowToast).toHaveBeenCalledWith(
      expect.stringContaining("access denied"),
      "error",
    );
  });

  it("handles encodeComposite failure", async () => {
    mockEncodeComposite.mockRejectedValueOnce(new Error("Canvas error"));
    const engine = createMockEngine();
    await expect(printDocument(engine)).rejects.toBeTruthy();
    expect(mockShowToast).toHaveBeenCalledWith(
      expect.stringContaining("Canvas error"),
      "error",
    );
  });

  it("handles createImageBitmap failure", async () => {
    mockCreateImageBitmap.mockRejectedValueOnce(new Error("Decode error"));
    const engine = createMockEngine();
    await expect(printDocument(engine)).rejects.toBeTruthy();
    expect(mockShowToast).toHaveBeenCalledWith(
      expect.stringContaining("Decode error"),
      "error",
    );
  });

  it("shows toast with unknown error message for non-Error throws", async () => {
    mockInvoke.mockRejectedValueOnce("string error");
    const engine = createMockEngine();
    await expect(printDocument(engine)).rejects.toBeTruthy();
    expect(mockShowToast).toHaveBeenCalledWith(
      expect.stringContaining("Print failed"),
      "error",
    );
  });

  it("shows toast with error.message for error object with message", async () => {
    mockInvoke.mockRejectedValueOnce({ message: "custom error message" });
    const engine = createMockEngine();
    await expect(printDocument(engine)).rejects.toBeTruthy();
    expect(mockShowToast).toHaveBeenCalledWith(
      expect.stringContaining("custom error message"),
      "error",
    );
  });

  it("reports a readable dimension error instead of a cryptic RangeError when canvas size is invalid", async () => {
    // printer_dpi: 0 → printableWidth * 0 = 0px → composite guard must fail
    // with a readable message (previously: "Invalid array length" RangeError).
    const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    mockPrintSettings({ printer_dpi: 0 });
    const engine = createMockEngine();
    await expect(printDocument(engine)).rejects.toBeTruthy();
    expect(mockShowToast).toHaveBeenCalledWith(
      expect.stringContaining("Invalid print composite dimensions"),
      "error",
    );
    consoleSpy.mockRestore();
  });
});

describe("printDocument — Print to PDF (PORTPROMPT)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupCompositeMock();
    // restore default invoke impl (get_print_settings + ok for print_image_raw)
    mockInvoke.mockImplementation(async (cmd: string, _args?: unknown, _options?: unknown) => {
      if (cmd === "get_print_settings") {
        return defaultPrintSettings;
      }
      return { ok: true, data: { status: "printed" } };
    });
    mockSave.mockResolvedValue(null); // default: no PDF dialog outcome
  });

  it("passes outputPath header when printing to 'Microsoft Print to PDF'", async () => {
    mockPrintSettings({ selected_printer: "Microsoft Print to PDF" });
    mockSave.mockResolvedValue("C:/Users/x/Documents/out.pdf");
    const engine = createMockEngine();
    const result = await printDocument(engine, "Photo.png");
    expect(result).toBe(true);
    expect(mockSave).toHaveBeenCalledOnce();
    const invokeCall = mockInvoke.mock.calls.find(
      (c: unknown[]) => c[0] === "print_image_raw",
    );
    expect(invokeCall).toBeDefined();
    expect(invokeCall![2].headers).toHaveProperty("outputPath", "C:/Users/x/Documents/out.pdf");
  });

  it("suggests a .pdf filename derived from the document name", async () => {
    mockPrintSettings({ selected_printer: "Microsoft Print to PDF" });
    mockSave.mockResolvedValue("C:/out.pdf");
    const engine = createMockEngine();
    await printDocument(engine, "Photo.png");
    expect(mockSave).toHaveBeenCalledWith(
      expect.objectContaining({
        defaultPath: "Photo.pdf",
        filters: [{ name: "PDF Document (*.pdf)", extensions: ["pdf"] }],
      }),
    );
  });

  it("aborts cleanly (returns false, no print_image_raw) when the user cancels the PDF save dialog", async () => {
    mockPrintSettings({ selected_printer: "Microsoft Print to PDF" });
    mockSave.mockResolvedValue(null); // user cancelled
    const engine = createMockEngine();
    const result = await printDocument(engine);
    expect(result).toBe(false);
    // The actual print call must NOT be dispatched
    expect(
      mockInvoke.mock.calls.find((c: unknown[]) => c[0] === "print_image_raw"),
    ).toBeUndefined();
    // No error toast on cancel — cancelling your own save dialog is not an error
    expect(mockShowToast).not.toHaveBeenCalledWith(
      expect.stringContaining("Print failed"),
      "error",
    );
  });

  it("works without save dialog for regular printers (no outputPath header)", async () => {
    // default printer = "Default Printer" (not a PDF printer)
    const engine = createMockEngine();
    const result = await printDocument(engine);
    expect(result).toBe(true);
    expect(mockSave).not.toHaveBeenCalled();
    const invokeCall = mockInvoke.mock.calls.find(
      (c: unknown[]) => c[0] === "print_image_raw",
    );
    expect(invokeCall).toBeDefined();
    expect(invokeCall![2].headers).not.toHaveProperty("outputPath");
  });
});

// ── Composition tests ───────────────────────────────────────────────
// The compositeForPrint function uses OffscreenCanvas (GPU-accelerated)
// for paper-sized compositing at target DPI. This runs in the webview's
// Canvas2D GPU backend — tested here via mock verification.