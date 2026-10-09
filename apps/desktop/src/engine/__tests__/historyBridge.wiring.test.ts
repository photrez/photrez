import { describe, it, expect, vi, beforeEach } from "vitest";
import { CommandHistory } from "../history";
import type { HistoryTilePatches } from "../history";
import type { DocumentModel } from "../types";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { invoke } from "@tauri-apps/api/core";
import { flushPixelInvokeCensus } from "@/lib/protocol/pixelInvokeCensus";

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

// Census read for one action: flush drains the fire-and-forget invokes first,
// then the slice gives exactly the entries this action recorded.
const censusCount = async () => (await flushPixelInvokeCensus()).entries.length;
const censusSince = async (before: number) => (await flushPixelInvokeCensus()).entries.slice(before);

/** The record/step commands the bridge gate suppresses; query reads are excluded. */
const bridgeRecordInvokes = () =>
  vi
    .mocked(invoke)
    .mock.calls.map((c) => c[0])
    .filter((cmd) =>
      ["rust_pixels_record_external", "apply_tile_patch", "rust_pixels_write_region"].includes(
        cmd as string,
      ),
    );

describe("history bridge gating", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.mocked(invoke).mockReset();
    vi.mocked(isTauriRuntime).mockReset();
    vi.mocked(isTauriRuntime).mockReturnValue(false);
  });

  it("fires no history-bridge record or step invoke in default production (no DEV gate)", async () => {
    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc-1");

    // Metadata commit -> would be rust_pixels_record_external if bridge on.
    history.commit(createMockModel("Meta"), "Add Layer");
    // Imperative commit -> would be apply_tile_patch if bridge on.
    history.commit(createMockModel("Paint"), "Brush", makePatches(), false);

    await flush();
    // The contract: no history-bridge RECORD/step invoke fires without the gate.
    // A read-back query (protocol_history_query_native) on commit is separate:
    // it exists so the derived view cannot go stale, and it proves nothing about
    // the bridge's record contract.
    expect(bridgeRecordInvokes()).toEqual([]);
  });

  it("fires no history-bridge record or step invoke even in the Tauri runtime when the DEV gate is unset (histories are independent by default)", async () => {
    // CRITICAL regression guard: previously bridgeEnabled() === isTauriRuntime()
    // fired in Tauri runtime with no gate. Now the gate alone must suppress it.
    vi.mocked(isTauriRuntime).mockReturnValue(true);

    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc-1");
    history.commit(createMockModel("Meta"), "Add Layer");

    await flush();
    expect(bridgeRecordInvokes()).toEqual([]);
  });

  it("fires no history-bridge record or step invoke while localStorage is set to a non-gate value", async () => {
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    localStorage.setItem(GATE_KEY, "0"); // present but not === "1"

    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc-1");
    history.commit(createMockModel("Meta"), "Add Layer");

    await flush();
    expect(bridgeRecordInvokes()).toEqual([]);
  });

  it("fires rust_pixels_record_external for a metadata commit when gate ON", async () => {
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    localStorage.setItem(GATE_KEY, "1");
    // Mock-fidelity: Tauri v2 invoke() RESOLVES on success with the command's
    // result (bridge returns {epoch,version} in production). Not {ok:false}.
    vi.mocked(invoke).mockResolvedValue({ ok: true, epoch: 1, version: 1 });

    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc-1");
    const before = await censusCount();
    history.commit(createMockModel("Meta"), "Add Layer");

    await flush();
    expect(invoke).toHaveBeenCalledWith("rust_pixels_record_external", {
      docId: "doc-1",
      label: "Add Layer",
      affected: [],
      adapterId: "ts",
      token: "Add Layer",
      // A metadata commit changes no document size, so it declares no size
      // halves. Rust reads the absent pair as "this transition changes no size"
      // and emits no size delta, keeping this a metadata step for the host.
      docSizeBefore: null,
      docSizeAfter: null,
    });
    expect(await censusSince(before)).toEqual([
      { order: expect.any(Number), command: "rust_pixels_record_external", phase: "resolved" },
    ]);
  });

  it("fires apply_tile_patch for an imperative commit when gate ON", async () => {
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    localStorage.setItem(GATE_KEY, "1");
    vi.mocked(invoke).mockResolvedValue({ ok: true, epoch: 1, version: 1 });

    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc-1");
    const before = await censusCount();
    history.commit(createMockModel("Paint"), "Brush", makePatches(), false);

    await flush();
    expect(invoke).toHaveBeenCalledWith("apply_tile_patch", {
      docId: "doc-1",
      layerId: "l1",
      before: [],
      after: [],
    });
    expect(await censusSince(before)).toEqual([
      { order: expect.any(Number), command: "apply_tile_patch", phase: "resolved" },
    ]);
  });

  it("does NOT fire any command when alreadyRecordedInRust=true (no double-count)", async () => {
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    localStorage.setItem(GATE_KEY, "1");
    vi.mocked(invoke).mockResolvedValue({ ok: true, epoch: 1, version: 1 });

    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc-1");
    history.commit(createMockModel("Brush"), "Brush", makePatches(), true);

    await flush();
    expect(bridgeRecordInvokes()).toEqual([]);
  });

  it("swallows a Tauri invoke rejection (best-effort bridge must not break TS history)", async () => {
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    localStorage.setItem(GATE_KEY, "1");
    // Mock-fidelity: Tauri v2 invoke() REJECTS with the bare `Err(String)`
    // payload ("E_RUST: boom"), not an object envelope. Exercise that real
    // rejection path.
    vi.mocked(invoke).mockRejectedValue("E_RUST: boom");

    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc-1");
    const before = await censusCount();
    history.commit(createMockModel("Meta"), "Add Layer");

    await flush();
    // The bridge is best-effort: the invoke fired (gate on) but the rejection
    // was swallowed. TS history is still the authority and committed the entry.
    expect(invoke).toHaveBeenCalledWith("rust_pixels_record_external", expect.anything());
    expect(history.getUndoCount()).toBe(1);
    // The census keeps the failed invoke visible instead of dropping it.
    expect(await censusSince(before)).toEqual([
      { order: expect.any(Number), command: "rust_pixels_record_external", phase: "rejected" },
    ]);
  });

  it("waits for this commit's own record to settle before reading the stream back", async () => {
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    localStorage.setItem(GATE_KEY, "1");

    // Settle-order log: each history-relevant invoke records its name only once
    // it has resolved. The record is given real flight time (10ms) so a read-back
    // that does not wait for it visibly lands first.
    const settled: string[] = [];
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "rust_pixels_record_external") {
        await new Promise((r) => setTimeout(r, 10));
        settled.push("record");
        return { ok: true, epoch: 1, version: 1 };
      }
      if (cmd === "protocol_history_query_native") {
        settled.push("query");
        return JSON.stringify({
          cursor: 1,
          lastSeq: 1,
          degradedHint: false,
          pendingExternal: null,
          entries: [],
        });
      }
      return { ok: true, epoch: 1, version: 1 };
    });

    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc-1");
    history.commit(createMockModel("Meta"), "Add Layer");

    // The read-back is scheduled by the commit itself; poll for it the way the
    // production poll would, then judge the order the two settled in.
    for (let i = 0; i < 100 && !settled.includes("query"); i++) {
      await new Promise((r) => setTimeout(r, 5));
    }
    expect(settled).toContain("record");
    expect(settled).toEqual(["record", "query"]);
  });
});
