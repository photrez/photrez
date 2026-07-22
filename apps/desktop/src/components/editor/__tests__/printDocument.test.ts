import { describe, it, expect, vi, beforeEach } from "vitest";
import { createMockEngine } from "@/__tests__/test-builders";

// Mock Tauri APIs
const mockInvoke = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

const mockTempDir = vi.fn().mockResolvedValue("/tmp/");
vi.mock("@tauri-apps/api/path", () => ({
  tempDir: mockTempDir,
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

const { printDocument } = await import("../printDocument");

describe("printDocument", () => {
  beforeEach(() => {
    vi.clearAllMocks();
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
    expect(path).toMatch(/^\/tmp\/photrez-print-\d+\.png$/);
    expect(bytes).toEqual(new Uint8Array([1, 2, 3]));
  });

  it("invokes print_image Rust command with the temp file path", async () => {
    const engine = createMockEngine();
    await printDocument(engine);
    expect(mockInvoke).toHaveBeenCalledWith("print_image", {
      path: expect.stringMatching(/^\/tmp\/photrez-print-\d+\.png$/),
      printer: null,
      copies: 1,
    });
  });
});
