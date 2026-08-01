import { describe, it, expect, vi, beforeEach } from "vitest";

const writeFileBytes = vi.fn();
const readFileBytes = vi.fn();
const deleteAutosaveFile = vi.fn();
const appCacheDir = vi.fn(async () => "/cache/");
const serializeAndSaveProject = vi.fn(async (_engine: unknown, _path: string, _opts?: { signal?: AbortSignal }) => {});

vi.mock("@tauri-apps/api/path", () => ({ appCacheDir: () => appCacheDir() }));
vi.mock("@/tauri/native", () => ({ writeFileBytes, readFileBytes, deleteAutosaveFile }));
vi.mock("../projectSerialize", () => ({ serializeAndSaveProject }));

const { autosaveDirtyDocs, listAutosaves, clearAllAutosaves, setAutosaveStatus, autosaveStatus } = await import("../autoSave");

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

  it("passes the abort signal through to serializeAndSaveProject", async () => {
    const workspace = {
      getSessions: () => [makeSession("doc-1", true, "A.png")],
    } as never;
    const ctrl = new AbortController();

    await autosaveDirtyDocs(workspace, undefined, ctrl.signal);

    expect(serializeAndSaveProject).toHaveBeenCalledTimes(1);
    expect(serializeAndSaveProject.mock.calls[0][2]).toEqual({ signal: ctrl.signal });
  });

  it("skips saving entirely when the signal is already aborted", async () => {
    const workspace = {
      getSessions: () => [makeSession("doc-1", true, "A.png")],
    } as never;
    setAutosaveStatus("idle");
    const ctrl = new AbortController();
    ctrl.abort();

    await autosaveDirtyDocs(workspace, undefined, ctrl.signal);

    expect(serializeAndSaveProject).not.toHaveBeenCalled();
    expect(autosaveStatus()).toBe("idle");
  });

  it("does not report an error when aborted mid-save (manual save preemption)", async () => {
    const workspace = {
      getSessions: () => [makeSession("doc-1", true, "A.png")],
    } as never;
    setAutosaveStatus("idle");
    const onError = vi.fn();
    const ctrl = new AbortController();
    serializeAndSaveProject.mockRejectedValueOnce(new DOMException("The operation was aborted", "AbortError"));

    const promise = autosaveDirtyDocs(workspace, onError, ctrl.signal);
    ctrl.abort(); // manual save preempts — signal fires before the rejection settles
    await promise;

    expect(onError).not.toHaveBeenCalled();
    expect(autosaveStatus()).toBe("idle");
  });
});
