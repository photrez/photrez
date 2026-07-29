// apps/desktop/src/components/editor/__tests__/editorOpenImageWiring.test.ts
//
// Wiring contract tests for openImage() — File → Open dialog routing.
//
// What this catches: the "tests pass but app fails" pattern where .ptz project
// files are silently dropped because the dialog handler never routes them to
// loadProjectFile. See docs/AI_HISTORY.md §[2026-07-28] BUG FIX — .ptz File
// Tidak Bisa Dibuka via File → Open (Regresi).
//
// These tests verify the wiring that connects:
//   - showOpenImageDialog → openImage → decodeSessionFromFile (non-.ptz)
//   - showOpenImageDialog → openImage → loadProjectFile (.ptz)
//   - Empty/no-op cases → no toast shown

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import type { OpenImageParams } from "../editorOpenImage";

// ─── Hoisted state ───
const hoisted = vi.hoisted(() => ({
  // Native mocks
  mockShowOpenImageDialog: vi.fn<() => Promise<string[] | null>>(),
  mockLoadProject: vi.fn<() => Promise<{ document_json: string; layers: Record<string, string> }>>(),
  mockReadFileBytes: vi.fn<() => Promise<Uint8Array>>(),
  // Toast mock — capture calls for assertion
  showToastCalls: [] as Array<{ message: string; severity: string }>,
  mockShowToast: vi.fn((message: string, severity: string = "info") => {
    hoisted.showToastCalls.push({ message, severity });
  }),
  // Recent files
  mockAddRecentFile: vi.fn(),
  // Tauri runtime flag
  isTauri: true,
  mockIsTauriRuntime: vi.fn(() => hoisted.isTauri),
}));

// ─── Module mocks ───
vi.mock("@/tauri/native", () => ({
  showOpenImageDialog: hoisted.mockShowOpenImageDialog,
  loadProject: hoisted.mockLoadProject,
  readFileBytes: hoisted.mockReadFileBytes,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

vi.mock("@/lib/desktop/tauriWindow", () => ({
  isTauriRuntime: hoisted.mockIsTauriRuntime,
}));

vi.mock("@/lib/recentFiles", () => ({
  addRecentFile: hoisted.mockAddRecentFile,
}));

vi.mock("../Toast", () => ({
  showToast: hoisted.mockShowToast,
}));

// ─── Test helpers ───

const PNG_HEADER = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

// Centralized mock references so tests can assert on them without type casts.
const mockAddDocument = vi.fn();
const mockIsFull = vi.fn().mockReturnValue(false);
const mockUploadImage = vi.fn();
const mockRequestRender = vi.fn();

function makeParams(overrides?: { isFull?: boolean; onError?: (m: string) => void; onLoading?: (m: string | null) => void }): OpenImageParams {
  if (overrides?.isFull != null) mockIsFull.mockReturnValue(overrides.isFull);
  return {
    workspace: { isFull: mockIsFull, addDocument: mockAddDocument } as unknown as OpenImageParams["workspace"],
    renderer: { uploadImage: mockUploadImage } as unknown as OpenImageParams["renderer"],
    scheduler: { requestRender: mockRequestRender } as unknown as OpenImageParams["scheduler"],
    onError: overrides?.onError ?? vi.fn(),
    onLoading: overrides?.onLoading ?? vi.fn(),
  } as unknown as OpenImageParams;
}

let originalCreateImageBitmap: typeof globalThis.createImageBitmap;

beforeEach(() => {
  originalCreateImageBitmap = globalThis.createImageBitmap;
  globalThis.createImageBitmap = vi.fn().mockResolvedValue({
    width: 100,
    height: 100,
    close: () => {},
  } as ImageBitmap);
  hoisted.showToastCalls = [];
  hoisted.isTauri = true;
  vi.clearAllMocks();
  // Re-apply default mock behavior after clearAllMocks
  mockIsFull.mockReturnValue(false);
});

afterEach(() => {
  globalThis.createImageBitmap = originalCreateImageBitmap;
  vi.restoreAllMocks();
});

// ─── Tests ───

describe("openImage — .ptz project files", () => {
  it("routes .ptz paths to loadProjectFile", async () => {
    hoisted.mockShowOpenImageDialog.mockResolvedValue(["/path/project.ptz"]);
    hoisted.mockLoadProject.mockResolvedValue({
      document_json: JSON.stringify({
        id: "doc-1",
        name: "project",
        width: 800,
        height: 600,
        layers: [],
      }),
      layers: {},
    });

    const { openImage } = await import("../editorOpenImage");
    const p = makeParams();
    await openImage(p);

    expect(hoisted.mockLoadProject).toHaveBeenCalledWith("/path/project.ptz");
    expect(hoisted.mockReadFileBytes).not.toHaveBeenCalled();
    expect(mockAddDocument).toHaveBeenCalled();
    expect(hoisted.mockShowToast).toHaveBeenCalledWith("File(s) loaded", "info");
  });

  it("routes image files to decodeSessionFromFile (readFileBytes + createImageBitmap)", async () => {
    hoisted.mockShowOpenImageDialog.mockResolvedValue(["/path/photo.png"]);
    hoisted.mockReadFileBytes.mockResolvedValue(PNG_HEADER);

    const { openImage } = await import("../editorOpenImage");
    const p = makeParams();
    await openImage(p);

    expect(hoisted.mockReadFileBytes).toHaveBeenCalledWith("/path/photo.png");
    expect(hoisted.mockLoadProject).not.toHaveBeenCalled();
    expect(mockAddDocument).toHaveBeenCalled();
    expect(hoisted.mockShowToast).toHaveBeenCalledWith("File(s) loaded", "info");
  });

  it("handles mixed selection — images parallel, .ptz sequential", async () => {
    hoisted.mockShowOpenImageDialog.mockResolvedValue([
      "/path/photo.png",
      "/path/project.ptz",
    ]);
    hoisted.mockReadFileBytes.mockResolvedValue(PNG_HEADER);
    hoisted.mockLoadProject.mockResolvedValue({
      document_json: JSON.stringify({
        id: "doc-2", name: "project", width: 800, height: 600, layers: [],
      }),
      layers: {},
    });

    const { openImage } = await import("../editorOpenImage");
    const p = makeParams();
    await openImage(p);

    expect(hoisted.mockReadFileBytes).toHaveBeenCalledWith("/path/photo.png");
    expect(hoisted.mockLoadProject).toHaveBeenCalledWith("/path/project.ptz");
    expect(mockAddDocument).toHaveBeenCalledTimes(2);
    expect(hoisted.mockShowToast).toHaveBeenCalledWith("File(s) loaded", "info");
  });

  it("only .ptz files still triggers loadProjectFile", async () => {
    hoisted.mockShowOpenImageDialog.mockResolvedValue([
      "/path/a.ptz",
      "/path/b.ptz",
    ]);
    hoisted.mockLoadProject.mockResolvedValue({
      document_json: JSON.stringify({
        id: "doc", name: "proj", width: 800, height: 600, layers: [],
      }),
      layers: {},
    });

    const { openImage } = await import("../editorOpenImage");
    const p = makeParams();
    await openImage(p);

    expect(hoisted.mockLoadProject).toHaveBeenCalledTimes(2);
    expect(mockAddDocument).toHaveBeenCalledTimes(2);
    expect(hoisted.mockShowToast).toHaveBeenCalledWith("File(s) loaded", "info");
  });
});

describe("openImage — no-op / edge cases", () => {
  it("does nothing when dialog is cancelled (null)", async () => {
    hoisted.mockShowOpenImageDialog.mockResolvedValue(null);

    const { openImage } = await import("../editorOpenImage");
    const p = makeParams();
    await openImage(p);

    expect(mockAddDocument).not.toHaveBeenCalled();
    expect(hoisted.mockShowToast).not.toHaveBeenCalled();
  });

  it("does nothing when dialog returns empty array", async () => {
    hoisted.mockShowOpenImageDialog.mockResolvedValue([]);

    const { openImage } = await import("../editorOpenImage");
    const p = makeParams();
    await openImage(p);

    expect(mockAddDocument).not.toHaveBeenCalled();
    expect(hoisted.mockShowToast).not.toHaveBeenCalled();
  });

  it("does not show toast when files no-oped (full workspace)", async () => {
    hoisted.mockShowOpenImageDialog.mockResolvedValue(["/path/photo.png"]);

    const { openImage } = await import("../editorOpenImage");
    const p = makeParams({ isFull: true });
    await openImage(p);

    expect(mockAddDocument).not.toHaveBeenCalled();
    expect(hoisted.mockShowToast).not.toHaveBeenCalled();
  });

  it("calls onLoading with start/finish messages", async () => {
    hoisted.mockShowOpenImageDialog.mockResolvedValue(["/path/photo.png"]);
    hoisted.mockReadFileBytes.mockResolvedValue(PNG_HEADER);

    const { openImage } = await import("../editorOpenImage");
    const onLoading = vi.fn();
    const p = makeParams({ onLoading });
    await openImage(p);

    expect(onLoading).toHaveBeenCalledWith("Opening 1 file...");
    expect(onLoading).toHaveBeenCalledWith(null); // finish
  });

  it("reports error when dialog throws", async () => {
    hoisted.mockShowOpenImageDialog.mockRejectedValue(new Error("Dialog crashed"));

    const { openImage } = await import("../editorOpenImage");
    const onError = vi.fn();
    const p = makeParams({ onError });
    await openImage(p);

    expect(onError).toHaveBeenCalledWith(expect.stringContaining("Dialog crashed"));
    expect(hoisted.mockShowToast).not.toHaveBeenCalled();
  });
});

describe("openSingleFile — .ptz routing", () => {
  it("routes .ptz path to loadProjectFile", async () => {
    hoisted.mockLoadProject.mockResolvedValue({
      document_json: JSON.stringify({
        id: "doc-single", name: "single", width: 800, height: 600, layers: [],
      }),
      layers: {},
    });

    const { openSingleFile } = await import("../editorOpenImage");
    const p = makeParams();
    await openSingleFile("/path/single.ptz", p);

    expect(hoisted.mockLoadProject).toHaveBeenCalledWith("/path/single.ptz");
    expect(mockAddDocument).toHaveBeenCalled();
  });

  it("uses filename from path as displayName, not model.name", async () => {
    // Model.name is "photo.png" (old name from when it was saved), but the
    // actual file is "project.ptz" — displayName should reflect the file.
    hoisted.mockLoadProject.mockResolvedValue({
      document_json: JSON.stringify({
        id: "doc-old", name: "photo.png", width: 800, height: 600, layers: [],
      }),
      layers: {},
    });

    const { openSingleFile } = await import("../editorOpenImage");
    const p = makeParams();
    await openSingleFile("/path/project.ptz", p);

    const addedSession = mockAddDocument.mock.calls[0][0];
    expect(addedSession.displayName).toBe("project.ptz");
  });

  it("routes image path to decodeSessionFromFile", async () => {
    hoisted.mockReadFileBytes.mockResolvedValue(PNG_HEADER);

    const { openSingleFile } = await import("../editorOpenImage");
    const p = makeParams();
    await openSingleFile("/path/single.png", p);

    expect(hoisted.mockReadFileBytes).toHaveBeenCalledWith("/path/single.png");
    expect(hoisted.mockLoadProject).not.toHaveBeenCalled();
    expect(mockAddDocument).toHaveBeenCalled();
  });

  it("does nothing when workspace is full", async () => {
    const { openSingleFile } = await import("../editorOpenImage");
    const p = makeParams({ isFull: true });
    await openSingleFile("/path/single.png", p);

    expect(mockAddDocument).not.toHaveBeenCalled();
  });
});
