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

  it("uses printer DPI from settings when available", async () => {
    mockPrintSettings({ printer_dpi: 600 });
    const engine = createMockEngine();
    await printDocument(engine);
    // A4 portrait at 600 DPI:
    //   width  = (210 / 25.4) * 600 ≈ 4961
    //   height = (297 / 25.4) * 600 ≈ 7016
    expect(mockOffscreenCanvas).toHaveBeenCalledWith(
      expect.any(Number),
      expect.any(Number),
    );
    const calls = mockOffscreenCanvas.mock.calls;
    const lastCall = calls[calls.length - 1];
    expect(lastCall[0]).toBeGreaterThan(4000); // width > 4000
    expect(lastCall[1]).toBeGreaterThan(6000); // height > 6000
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
    // 1200 DPI on A4 = 9921×14031 → height exceeds MAX_PX=10000
    mockPrintSettings({ printer_dpi: 1200 });
    const engine = createMockEngine();
    await printDocument(engine);
    // DPI should be proportionally reduced:
    //   effectiveDpi = (10000 / 14031) * 1200 ≈ 855
    //   canvas height at 855 DPI ≈ (297/25.4) * 855 ≈ 9998 ≤ MAX_PX
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
    expect(invokeCall![2].headers).toMatchObject({
      printer: "Default Printer",
      copies: "1",
      orientation: "portrait",
      documentName: "Untitled",
      width: "2480",
      height: "3508",
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
      width: "3508",
      height: "4961",
    });
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
});

// ── Composition tests ───────────────────────────────────────────────
// The compositeForPrint function uses OffscreenCanvas (GPU-accelerated)
// for paper-sized compositing at target DPI. This runs in the webview's
// Canvas2D GPU backend — tested here via mock verification.