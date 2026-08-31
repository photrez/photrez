import { describe, it, expect, vi, beforeEach } from "vitest";
import { CommandHistory } from "../history";
import type { HistoryTilePatches } from "../history";
import type { DocumentModel } from "../types";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { invoke } from "@tauri-apps/api/core";

// Mock the Tauri-runtime detector so the test can force the runtime on/off.
vi.mock("@/lib/desktop/tauriWindow", () => ({
  isTauriRuntime: vi.fn(),
}));

// history.ts dynamic-imports @tauri-apps/api/core inside the gate. vi.mock
// also intercepts dynamic imports, so `invoke` below is the real mock spy.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const GATE_KEY = "photrez.historyBridge";

const createMockModel = (name: string): DocumentModel => ({
  id: "doc-1",
  name,
  width: 10,
  height: 10,
  layers: [],
  activeLayerId: null,
  selection: null,
  viewport: { panX: 0, panY: 0, zoom: 1, rotation: 0 },
  dirty: false,
});

const makePatches = (): HistoryTilePatches => ({
  layerId: "l1",
  surfaceWidth: 10,
  surfaceHeight: 10,
  before: [],
  after: [],
});

// Let the dynamic-import + invoke microtask chain settle before asserting.
const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

describe("history bridge gating", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.mocked(invoke).mockReset();
    vi.mocked(isTauriRuntime).mockReset();
    vi.mocked(isTauriRuntime).mockReturnValue(false);
  });

  it("does NOT fire any Tauri invoke in default production (no DEV gate)", async () => {
    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc-1");

    // Metadata commit -> would be rust_pixels_record_external if bridge on.
    history.commit(createMockModel("Meta"), "Add Layer");
    // Imperative commit -> would be apply_tile_patch if bridge on.
    history.commit(createMockModel("Paint"), "Brush", makePatches(), false);

    await flush();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("does NOT fire any invoke even in the Tauri runtime when the DEV gate is unset (no split-brain at default)", async () => {
    // CRITICAL regression guard: previously bridgeEnabled() === isTauriRuntime()
    // fired in Tauri runtime with no gate. Now the gate alone must suppress it.
    vi.mocked(isTauriRuntime).mockReturnValue(true);

    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc-1");
    history.commit(createMockModel("Meta"), "Add Layer");

    await flush();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("does NOT fire any invoke while localStorage is set to a non-gate value", async () => {
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    localStorage.setItem(GATE_KEY, "0"); // present but not === "1"

    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc-1");
    history.commit(createMockModel("Meta"), "Add Layer");

    await flush();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("fires rust_pixels_record_external for a metadata commit when gate ON", async () => {
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    localStorage.setItem(GATE_KEY, "1");
    // Mock-fidelity: Tauri v2 invoke() RESOLVES on success with the command's
    // result (bridge returns {epoch,version} in production). Not {ok:false}.
    vi.mocked(invoke).mockResolvedValue({ ok: true, epoch: 1, version: 1 });

    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc-1");
    history.commit(createMockModel("Meta"), "Add Layer");

    await flush();
    expect(invoke).toHaveBeenCalledWith("rust_pixels_record_external", {
      docId: "doc-1",
      label: "Add Layer",
      affected: [],
      adapterId: "ts",
      token: "Add Layer",
    });
  });

  it("fires apply_tile_patch for an imperative commit when gate ON", async () => {
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    localStorage.setItem(GATE_KEY, "1");
    vi.mocked(invoke).mockResolvedValue({ ok: true, epoch: 1, version: 1 });

    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc-1");
    history.commit(createMockModel("Paint"), "Brush", makePatches(), false);

    await flush();
    expect(invoke).toHaveBeenCalledWith("apply_tile_patch", {
      docId: "doc-1",
      layerId: "l1",
      before: [],
      after: [],
    });
  });

  it("does NOT fire any command when alreadyRecordedInRust=true (no double-count)", async () => {
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    localStorage.setItem(GATE_KEY, "1");
    vi.mocked(invoke).mockResolvedValue({ ok: true, epoch: 1, version: 1 });

    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc-1");
    history.commit(createMockModel("Brush"), "Brush", makePatches(), true);

    await flush();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("swallows a Tauri invoke rejection (best-effort bridge must not break TS history)", async () => {
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    localStorage.setItem(GATE_KEY, "1");
    // Mock-fidelity: Tauri v2 invoke() REJECTS with an error-envelope OBJECT
    // when a Rust command returns Err(...). Exercise that real rejection path.
    const errorEnvelope = { code: "E_RUST", message: "boom", details: null };
    vi.mocked(invoke).mockRejectedValue(errorEnvelope);

    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc-1");
    history.commit(createMockModel("Meta"), "Add Layer");

    await flush();
    // The bridge is best-effort: the invoke fired (gate on) but the rejection
    // was swallowed. TS history is still the authority and committed the entry.
    expect(invoke).toHaveBeenCalledWith("rust_pixels_record_external", expect.anything());
    expect(history.getUndoCount()).toBe(1);
  });
});
