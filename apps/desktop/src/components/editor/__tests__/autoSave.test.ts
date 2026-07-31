import { describe, it, expect, vi, beforeEach } from "vitest";

const writeFileBytes = vi.fn();
const readFileBytes = vi.fn();
const deleteAutosaveFile = vi.fn();
const appCacheDir = vi.fn(async () => "/cache/");
const serializeAndSaveProject = vi.fn(async () => {});

vi.mock("@tauri-apps/api/path", () => ({ appCacheDir: () => appCacheDir() }));
vi.mock("@/tauri/native", () => ({ writeFileBytes, readFileBytes, deleteAutosaveFile }));
vi.mock("../projectSerialize", () => ({ serializeAndSaveProject }));

const { autosaveDirtyDocs, listAutosaves, clearAllAutosaves } = await import("../autoSave");

function makeSession(id: string, dirty: boolean, name: string) {
  return {
    engine: { getId: () => id, isDirty: () => dirty },
    displayName: name,
    dirty,
  } as never;
}

describe("autoSave", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("autosaveDirtyDocs persists only dirty sessions and writes a manifest", async () => {
    const workspace = {
      getSessions: () => [
        makeSession("doc-1", true, "A.png"),
        makeSession("doc-2", false, "B.png"),
        makeSession("doc-3", true, "C.png"),
      ],
    } as never;

    await autosaveDirtyDocs(workspace);

    // 2 dirty docs serialized (mock noop) + 1 manifest write
    expect(serializeAndSaveProject).toHaveBeenCalledTimes(2);
    expect(writeFileBytes).toHaveBeenCalledTimes(1);
    const manifestCall = writeFileBytes.mock.calls.find((c) => c[0].endsWith("manifest.json"));
    expect(manifestCall).toBeTruthy();
    const manifest = JSON.parse(Buffer.from(manifestCall![1]).toString());
    // Value format: `displayName|timestamp`
    expect(manifest["doc-1"]).toMatch(/^A\.png\|\d+$/);
    expect(manifest["doc-3"]).toMatch(/^C\.png\|\d+$/);
  });

  it("listAutosaves returns parsed entries from manifest", async () => {
    const manifest = JSON.stringify({ "doc-1": "A.png" });
    readFileBytes.mockResolvedValue(new TextEncoder().encode(manifest));

    const entries = await listAutosaves();
    expect(entries).toEqual([
      { docId: "doc-1", displayName: "A.png", path: "/cache/photrez/autosave/doc-1.ptz" },
    ]);
  });

  it("listAutosaves returns [] when manifest missing", async () => {
    readFileBytes.mockRejectedValue(new Error("not found"));
    expect(await listAutosaves()).toEqual([]);
  });

  it("clearAllAutosaves deletes doc files and manifest", async () => {
    readFileBytes.mockResolvedValue(new TextEncoder().encode(JSON.stringify({ "doc-1": "A.png" })));
    await clearAllAutosaves();
    expect(deleteAutosaveFile).toHaveBeenCalledWith("/cache/photrez/autosave/doc-1.ptz");
    expect(deleteAutosaveFile).toHaveBeenCalledWith("/cache/photrez/autosave/manifest.json");
  });
});
