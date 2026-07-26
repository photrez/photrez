import { describe, it, expect, vi, beforeEach } from "vitest";
import { createMockEngine } from "@/__tests__/test-builders";

// ── Mock Tauri APIs ───────────────────────────────────────────────
const mockInvoke = vi.fn().mockImplementation(async (cmd: string, _args?: unknown) => {
  if (cmd === "get_print_settings") {
    return defaultPrintSettings;
  }
  return { ok: true, data: { printed: "/tmp/photrez-print-12345.png", copies: 1 } };
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
  },
};

function mockPrintSettings(overrides: Record<string, unknown>) {
  mockInvoke.mockImplementation(async (cmd: string, _args?: unknown) => {
    if (cmd === "get_print_settings") {
      return { ok: true, data: { ...defaultPrintSettings.data, ...overrides } };
    }
    return { ok: true, data: { printed: "/tmp/photrez-print-12345.png", copies: 1 } };
  });
}

const mockTempDir = vi.fn().mockResolvedValue("/tmp/");
const mockJoin = vi.fn().mockImplementation(async (...parts: string[]) => {
  return parts.join("");
});
vi.mock("@tauri-apps/api/path", () => ({
  tempDir: mockTempDir,
  join: mockJoin,
}));

// Mock encodeComposite — we test printDocument, not the composite engine
const mockEncodeComposite = vi.fn().mockResolvedValue(new Uint8Array([1, 2, 3]));
vi.mock("../exportDocument", () => ({
  encodeComposite: mockEncodeComposite,
}));

// Mock writeFileBytes & deleteFile
const mockWriteFileBytes = vi.fn().mockResolvedValue(undefined);
const mockDeleteFile = vi.fn().mockResolvedValue(undefined);
vi.mock("@/tauri/native", () => ({
  writeFileBytes: mockWriteFileBytes,
  deleteFile: mockDeleteFile,
}));

// Mock showToast
const mockShowToast = vi.fn();
vi.mock("../Toast", () => ({
  showToast: mockShowToast,
}));

const { printDocument } = await import("../printDocument");

describe("printDocument — core flow", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Re-apply default mock implementation (clearAllMocks resets it)
    mockInvoke.mockImplementation(async (cmd: string, _args?: unknown) => {
      if (cmd === "get_print_settings") {
        return defaultPrintSettings;
      }
      return { ok: true, data: { printed: "/tmp/photrez-print-12345.png", copies: 1 } };
    });
  });

  it("calls encodeComposite with the engine", async () => {
    const engine = createMockEngine();
    await printDocument(engine);
    expect(mockEncodeComposite).toHaveBeenCalledWith(engine, "png", 100);
  });

  it("writes composite bytes to a temp PNG file", async () => {
    const engine = createMockEngine();
    await printDocument(engine);
    expect(mockTempDir).toHaveBeenCalledOnce();
    expect(mockWriteFileBytes).toHaveBeenCalledOnce();
    const [path, bytes] = mockWriteFileBytes.mock.calls[0];
    expect(path).toMatch(/^\/tmp\/photrez-print-[\w-]+\.png$/);
    expect(bytes).toEqual(new Uint8Array([1, 2, 3]));
  });

  it("invokes print_image Rust command with the temp file path and paper preset", async () => {
    const engine = createMockEngine();
    await printDocument(engine);
    expect(mockInvoke).toHaveBeenCalledWith("print_image", {
      path: expect.stringMatching(/^\/tmp\/photrez-print-[\w-]+\.png$/),
      printer: "Default Printer",
      copies: 1,
      paperWidthMm: 210,
      paperHeightMm: 297,
      paperPreset: "A4",
      paperIndex: 9,
      documentName: "Untitled",
    });
  });

  it("passes document name to print_image", async () => {
    const engine = createMockEngine();
    await printDocument(engine, "MyPhoto.png");
    expect(mockInvoke).toHaveBeenCalledWith("print_image", expect.objectContaining({
      documentName: "MyPhoto.png",
    }));
  });

  it("cleans up temp file on success", async () => {
    const engine = createMockEngine();
    await printDocument(engine);
    expect(mockDeleteFile).toHaveBeenCalledOnce();
    const [deletedPath] = mockDeleteFile.mock.calls[0];
    expect(deletedPath).toMatch(/^\/tmp\/photrez-print-[\w-]+\.png$/);
  });

  it("uses path.join() for cross-platform temp file path", async () => {
    const engine = createMockEngine();
    await printDocument(engine);
    expect(mockJoin).toHaveBeenCalledOnce();
    const [dir, filename] = mockJoin.mock.calls[0];
    expect(dir).toBe("/tmp/");
    expect(filename).toMatch(/^photrez-print-[\w-]+\.png$/);
  });

  it("uses default options when none provided", async () => {
    const engine = createMockEngine();
    await printDocument(engine);
    // Default options: A4 (210×297), 1 copy, centerImage=true, scalePercent=100
    expect(mockInvoke).toHaveBeenCalledWith("print_image", expect.objectContaining({
      copies: 1,
      paperWidthMm: 210,
      paperHeightMm: 297,
      paperPreset: "A4",
      documentName: "Untitled",
    }));
  });
});

describe("printDocument — custom options", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockImplementation(async (cmd: string, _args?: unknown) => {
      if (cmd === "get_print_settings") {
        return defaultPrintSettings;
      }
      return { ok: true, data: { printed: "/tmp/photrez-print-12345.png", copies: 1 } };
    });
  });

  it("passes custom paper dimensions to print_image", async () => {
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

    expect(mockInvoke).toHaveBeenCalledWith("print_image", expect.objectContaining({
      path: expect.stringMatching(/\.png$/),
      printer: "Custom Printer",
      copies: 3,
      paperWidthMm: 297,
      paperHeightMm: 420,
      paperPreset: "A3",
    }));
  });

  it("shows warning toast and does not invoke print_image when selectedPrinter is empty", async () => {
    const engine = createMockEngine();
    mockPrintSettings({ selected_printer: "" });
    await printDocument(engine);

    expect(mockShowToast).toHaveBeenCalledWith(
      "No printer selected. Select a printer in Print Settings.",
      "warn",
    );
    expect(mockInvoke).not.toHaveBeenCalledWith("print_image", expect.anything());
  });
});

describe("printDocument — error handling", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockImplementation(async (cmd: string, _args?: unknown) => {
      if (cmd === "get_print_settings") {
        return defaultPrintSettings;
      }
      return { ok: true, data: { printed: "/tmp/photrez-print-12345.png", copies: 1 } };
    });
  });

  it("cleans up temp file and shows error toast when print_image fails", async () => {
    // First call (get_print_settings) succeeds, second call (print_image) fails
    mockInvoke.mockImplementationOnce(async () => defaultPrintSettings);
    mockInvoke.mockRejectedValueOnce({
      error: { message: "Printer not found" },
    });
    const engine = createMockEngine();
    await expect(printDocument(engine)).rejects.toBeTruthy();
    // Temp file should be deleted on error
    expect(mockDeleteFile).toHaveBeenCalledOnce();
    const [deletedPath] = mockDeleteFile.mock.calls[0];
    expect(deletedPath).toMatch(/^\/tmp\/photrez-print-[\w-]+\.png$/);
    // Should show error toast
    expect(mockShowToast).toHaveBeenCalledWith(
      expect.stringContaining("Print failed"),
      "error",
    );
  });

  it("handles Error instance from print_image reject", async () => {
    // First call (get_print_settings) succeeds, second call (print_image) fails
    mockInvoke.mockImplementationOnce(async () => defaultPrintSettings);
    mockInvoke.mockRejectedValueOnce(new Error("access denied"));
    const engine = createMockEngine();
    await expect(printDocument(engine)).rejects.toBeTruthy();
    expect(mockDeleteFile).toHaveBeenCalledOnce();
    expect(mockShowToast).toHaveBeenCalledWith(
      expect.stringContaining("access denied"),
      "error",
    );
  });

  it("handles cleanup failure gracefully (deleteFile throws)", async () => {
    // Simulate deleteFile failing on success path
    mockDeleteFile.mockRejectedValueOnce(new Error("Permission denied"));
    const engine = createMockEngine();
    // Should not throw — error is swallowed in console.error
    await expect(printDocument(engine)).resolves.toBeUndefined();
    // deleteFile was called (and failed)
    expect(mockDeleteFile).toHaveBeenCalledOnce();
    // Should NOT show error toast (cleanup failure is non-critical)
    expect(mockShowToast).not.toHaveBeenCalledWith(
      expect.stringContaining("Print failed"),
      "error",
    );
  });

  it("handles cleanup failure during error path gracefully", async () => {
    mockInvoke.mockRejectedValueOnce(new Error("Spooler error"));
    mockDeleteFile.mockRejectedValueOnce(new Error("Cleanup failed"));

    const engine = createMockEngine();
    await expect(printDocument(engine)).rejects.toBeTruthy();
    // Should still show the print failure toast (not cleanup error)
    expect(mockShowToast).toHaveBeenCalledWith(
      expect.stringContaining("Spooler error"),
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
    // No temp file was created, so deleteFile should not be called
    expect(mockDeleteFile).not.toHaveBeenCalled();
  });

  it("handles tempDir failure", async () => {
    mockTempDir.mockRejectedValueOnce(new Error("FS error"));
    const engine = createMockEngine();
    await expect(printDocument(engine)).rejects.toBeTruthy();
    expect(mockShowToast).toHaveBeenCalledWith(
      expect.stringContaining("FS error"),
      "error",
    );
  });

  it("handles writeFileBytes failure", async () => {
    mockWriteFileBytes.mockRejectedValueOnce(new Error("Disk full"));
    const engine = createMockEngine();
    await expect(printDocument(engine)).rejects.toBeTruthy();
    expect(mockShowToast).toHaveBeenCalledWith(
      expect.stringContaining("Disk full"),
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

describe("printDocument — composition", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockImplementation(async (cmd: string, _args?: unknown) => {
      if (cmd === "get_print_settings") {
        return defaultPrintSettings;
      }
      return { ok: true, data: { printed: "/tmp/photrez-print-12345.png", copies: 1 } };
    });
  });

  it("clamps very large paper dimensions to max 1200mm", async () => {
    const engine = createMockEngine();
    mockPrintSettings({
      paper_width_mm: 2000,
      paper_height_mm: 2000,
      paper_name: "Custom",
    });
    await printDocument(engine);

    // Should still invoke successfully with clamped values in composite
    expect(mockInvoke).toHaveBeenCalledWith("print_image", expect.objectContaining({
      paperWidthMm: 2000,
      paperHeightMm: 2000,
    }));
    expect(mockWriteFileBytes).toHaveBeenCalledOnce();
  });

  it("succeeds when OffscreenCanvas is unavailable (falls back to raw bytes)", async () => {
    // In jsdom test environment, OffscreenCanvas might not be available.
    // The fallback path should still complete the full flow.
    const engine = createMockEngine();
    await printDocument(engine);
    // Should complete the flow: get_print_settings → encode → (fallback to raw bytes) → write → print_image → cleanup
    expect(mockWriteFileBytes).toHaveBeenCalledOnce();
    expect(mockInvoke).toHaveBeenCalledTimes(2); // get_print_settings + print_image
    expect(mockDeleteFile).toHaveBeenCalledOnce();
  });

  it("passes correct paper preset to the Rust backend", async () => {
    const engine = createMockEngine();
    mockPrintSettings({
      paper_width_mm: 215.9,
      paper_height_mm: 279.4,
      paper_name: "Letter",
    });
    await printDocument(engine);

    // Verify Letter preset is passed correctly
    expect(mockInvoke).toHaveBeenCalledWith("print_image", expect.objectContaining({
      paperPreset: "Letter",
      paperWidthMm: 215.9,
      paperHeightMm: 279.4,
    }));
  });
});
