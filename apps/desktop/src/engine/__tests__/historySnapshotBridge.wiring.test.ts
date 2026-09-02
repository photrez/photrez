import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { CommandHistory, restoreSnapshotBitmapsByToken } from "../history";
import type { SnapshotPayload } from "../history";
import type { DocumentModel } from "../types";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { invoke } from "@tauri-apps/api/core";
import { bitmapStoreFor, releaseBitmapStore, tokenForBitmap } from "../bitmapStore";

// Mock the Tauri-runtime detector so the history bridge gate can be forced on/off.
vi.mock("@/lib/desktop/tauriWindow", () => ({
  isTauriRuntime: vi.fn(),
}));

// history.ts dynamic-imports @tauri-apps/api/core inside the gate. vi.mock also
// intercepts dynamic imports, so `invoke` below is the real mock spy.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const GATE_KEY = "photrez.historyBridge";

function fakeBitmap(): ImageBitmap {
  return { width: 10, height: 10, close: vi.fn() } as unknown as ImageBitmap;
}

function makeLayer(id: string, bitmap: ImageBitmap | null) {
  return {
    id,
    name: id,
    type: "raster",
    visible: true,
    opacity: 1,
    locked: false,
    blendMode: "normal",
    transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    width: 10,
    height: 10,
    imageBitmap: bitmap,
  };
}

function makeModel(layers: ReturnType<typeof makeLayer>[]): DocumentModel {
  return {
    id: "doc-1",
    name: "Doc",
    width: 10,
    height: 10,
    layers: layers as unknown as DocumentModel["layers"],
    activeLayerId: layers[0]?.id ?? null,
    selection: null,
    viewport: { panX: 0, panY: 0, zoom: 1, rotation: 0 },
    dirty: false,
  };
}

const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

// `commit` fires the bridge invoke through a dynamic `import()` (fire-and-forget).
// Under parallel vitest workers a single macrotask is not enough for two calls,
// so poll until the scripted call count is observed.
const waitFor = async (pred: () => boolean, ms = 3000) => {
  const start = Date.now();
  while (!pred()) {
    if (Date.now() - start > ms) throw new Error("waitFor timeout");
    await new Promise((r) => setTimeout(r, 5));
  }
};

function gateOn(bridgeOn: boolean) {
  vi.mocked(isTauriRuntime).mockReturnValue(bridgeOn);
  localStorage.setItem(GATE_KEY, "1");
}

describe("snapshot history bridge (bitmap-token) wiring", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.mocked(invoke).mockReset();
    vi.mocked(isTauriRuntime).mockReset();
    vi.mocked(isTauriRuntime).mockReturnValue(false);
  });

  afterEach(() => {
    releaseBitmapStore("doc-1");
  });

  it("recordSnapshotHistory fires rust_pixels_record_snapshot with the after-layer bitmap token", async () => {
    gateOn(true);
    vi.mocked(invoke).mockResolvedValue({ version: 1, epoch: 0 });

    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc-1");
    const bm = fakeBitmap();
    // Pre-action state is empty (no layers); the action produces the bm layer.
    history.recordSnapshotHistory(makeModel([]), makeModel([makeLayer("l1", bm)]), "Action");
    await waitFor(() => vi.mocked(invoke).mock.calls.length >= 1);

    const rec = vi.mocked(invoke).mock.calls.filter(([c]) => c === "rust_pixels_record_snapshot");
    expect(rec.length).toBe(1);
    const args = rec[0][1] as { docId: string; before: SnapshotPayload; after: SnapshotPayload };
    expect(args.docId).toBe("doc-1");
    // The after snapshot carries the layer's bitmap token (the stable token for bm).
    expect(args.after.layers[0].bitmapToken).toEqual(tokenForBitmap(bm));
    // First commit has no prior state -> before is an empty baseline snapshot.
    expect(args.before.layers).toHaveLength(0);
    // The token resolves to the SAME bitmap object via the store.
    expect(bitmapStoreFor("doc-1").get(args.after.layers[0].bitmapToken!)).toBe(bm);
  });

  it("CONTRACT: recordSnapshotHistory sends the EXACT passed before/after (no stack / off-by-one) and undo returns the EXACT before", async () => {
    gateOn(true);
    const b0 = fakeBitmap();
    const b1 = fakeBitmap();
    const b2 = fakeBitmap();
    vi.mocked(invoke).mockResolvedValue({ version: 1, epoch: 0 });

    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc-1");
    // First action: b0 -> b1.
    history.recordSnapshotHistory(makeModel([makeLayer("l1", b0)]), makeModel([makeLayer("l1", b1)]), "A1");
    // Settle A1's fire before issuing A2 (two packed dynamic-imports under the
    // mock yield only ONE invoke — see the waitFor comment above).
    await flush();
    // Second action: b1 -> b2. A stack-derived/off-by-one `before` would wrongly
    // send the FIRST action's pre-state (b0) here — two commits back.
    history.recordSnapshotHistory(makeModel([makeLayer("l1", b1)]), makeModel([makeLayer("l1", b2)]), "A2");
    await waitFor(() => vi.mocked(invoke).mock.calls.filter(([c]) => c === "rust_pixels_record_snapshot").length >= 2);

    const rec = vi.mocked(invoke).mock.calls.filter(([c]) => c === "rust_pixels_record_snapshot");
    expect(rec.length).toBe(2);
    // A1: before=b0, after=b1.
    const first = rec[0][1] as { before: SnapshotPayload; after: SnapshotPayload };
    expect(first.before.layers[0].bitmapToken).toEqual(tokenForBitmap(b0));
    expect(first.after.layers[0].bitmapToken).toEqual(tokenForBitmap(b1));
    // A2: before=b1 (NOT b0), after=b2 (NOT b1) — the contract-correct derivation.
    const last = rec[1][1] as { before: SnapshotPayload; after: SnapshotPayload };
    expect(last.before.layers[0].bitmapToken).toEqual(tokenForBitmap(b1));
    expect(last.after.layers[0].bitmapToken).toEqual(tokenForBitmap(b2));

    // Undo after A2 restores the EXACT A2 pre-action state (b1), not a state
    // two-commits-back (b0). The undo-point pushed is the passed `before`.
    const undone = history.undo(makeModel([makeLayer("l1", b2)]));
    expect(undone?.layers[0]?.imageBitmap).toBe(b1);
    // Next undo returns the A1 pre-action state (b0), proving the stack holds the
    // two PRE-action states exactly — no extra stale entry.
    const undone2 = history.undo(makeModel([makeLayer("l1", b1)]));
    expect(undone2?.layers[0]?.imageBitmap).toBe(b0);
  });

  it("commit â†’ undo re-attaches the SAME ImageBitmap by token (never detached, never different)", async () => {
    gateOn(true);
    const baseBitmap = fakeBitmap();
    const afterBitmap = fakeBitmap();
    const baseToken = tokenForBitmap(baseBitmap);
    const afterToken = tokenForBitmap(afterBitmap);
    const beforeDto: SnapshotPayload = {
      docId: "doc-1",
      version: 0,
      layers: [{ layerId: "l1", width: 10, height: 10, bitmapToken: baseToken, epoch: 0, pixelVersion: 0 }],
    };
    const afterDto: SnapshotPayload = {
      docId: "doc-1",
      version: 0,
      layers: [{ layerId: "l1", width: 10, height: 10, bitmapToken: afterToken, epoch: 0, pixelVersion: 0 }],
    };
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "rust_pixels_record_snapshot") return Promise.resolve({ version: 1, epoch: 0 });
      if (cmd === "rust_pixels_undo_snapshot") return Promise.resolve(beforeDto);
      if (cmd === "rust_pixels_redo_snapshot") return Promise.resolve(afterDto);
      return Promise.resolve(null);
    });

    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc-1");
    history.recordSnapshotHistory(makeModel([]), makeModel([makeLayer("l1", baseBitmap)]), "Base");
    history.recordSnapshotHistory(makeModel([makeLayer("l1", baseBitmap)]), makeModel([makeLayer("l1", afterBitmap)]), "Action");
    await flush();

    // The per-doc store maps token -> the EXACT bitmap object both states used.
    const store = bitmapStoreFor("doc-1");
    expect(store.get(baseToken)).toBe(baseBitmap);
    expect(store.get(afterToken)).toBe(afterBitmap);

    // Undo re-attaches the BEFORE bitmap by token: same object, no detach, no close.
    const setUndo = vi.fn(() => true);
    const undoApplied = await restoreSnapshotBitmapsByToken("undo", "doc-1", (t) => store.get(t), setUndo);
    expect(undoApplied).toBe(true);
    expect(setUndo).toHaveBeenCalledWith("l1", baseBitmap);
    expect(baseBitmap.close).not.toHaveBeenCalled();
    expect(afterBitmap.close).not.toHaveBeenCalled();

    // Redo re-attaches the AFTER bitmap by token.
    const setRedo = vi.fn(() => true);
    const redoApplied = await restoreSnapshotBitmapsByToken("redo", "doc-1", (t) => store.get(t), setRedo);
    expect(redoApplied).toBe(true);
    expect(setRedo).toHaveBeenCalledWith("l1", afterBitmap);
    expect(baseBitmap.close).not.toHaveBeenCalled();
    expect(afterBitmap.close).not.toHaveBeenCalled();
  });

  it("no-detach: an unresolvable token leaves the existing bitmap untouched and surfaces the miss", async () => {
    gateOn(true);
    const dto: SnapshotPayload = {
      docId: "doc-1",
      version: 0,
      layers: [{ layerId: "l1", width: 10, height: 10, bitmapToken: "missing-token", epoch: 0, pixelVersion: 0 }],
    };
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "rust_pixels_undo_snapshot" ? Promise.resolve(dto) : Promise.resolve(null),
    );
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    try {
      const store = bitmapStoreFor("doc-1");
      // No bitmap registered under "missing-token".
      const set = vi.fn(() => true);
      const applied = await restoreSnapshotBitmapsByToken("undo", "doc-1", (t) => store.get(t), set);
      expect(applied).toBe(false);
      expect(set).not.toHaveBeenCalled();
      expect(warnSpy).toHaveBeenCalled();
    } finally {
      warnSpy.mockRestore();
    }
  });

  it("mock-fidelity: invoke REJECTS with a Tauri error envelope â†’ re-attach is safe (no detach)", async () => {
    gateOn(true);
    // Tauri v2 invoke() REJECTS with an error-envelope OBJECT on a Rust Err.
    vi.mocked(invoke).mockRejectedValue({ code: "E_RUST", message: "boom", details: null });
    const store = bitmapStoreFor("doc-1");
    const set = vi.fn(() => true);
    const applied = await restoreSnapshotBitmapsByToken("undo", "doc-1", (t) => store.get(t), set);
    expect(applied).toBe(false);
    expect(set).not.toHaveBeenCalled();
  });

  it("close-on-evict: an evicted unreferenced token bitmap closes once; a still-live one is not closed", () => {
    gateOn(true);
    vi.mocked(invoke).mockResolvedValue({ version: 1, epoch: 0 });
    const b1 = fakeBitmap();
    const b2 = fakeBitmap();
    const history = new CommandHistory(1);
    // Base action: b1 -> b2 (undo-point = b1 pre-state). Action: b2 -> b3
    // (undo-point = b2 post-of-base, which survives). The AFTER of the evicting
    // call (b3) must not reference b1, so b1 becomes unreferenced and closes once.
    const b3 = fakeBitmap();
    history.recordSnapshotHistory(makeModel([makeLayer("l1", b1)]), makeModel([makeLayer("l1", b2)]), "Base");
    history.recordSnapshotHistory(makeModel([makeLayer("l1", b2)]), makeModel([makeLayer("l1", b3)]), "Action");
    // Max depth 1 evicted the base snapshot; b1 is unreferenced -> closed once,
    // b2 (still in the undo stack, current snapshot) must NOT be closed.
    expect(b1.close).toHaveBeenCalledTimes(1);
    expect(b2.close).not.toHaveBeenCalled();
  });

  it("hazard#3: a base-only layer re-attach assigns baseImageBitmap (fieldFor), never imageBitmap", async () => {
    gateOn(true);
    const baseBm = fakeBitmap();
    const baseToken = tokenForBitmap(baseBm);
    const dto: SnapshotPayload = {
      docId: "doc-1",
      version: 0,
      layers: [{ layerId: "l1", width: 10, height: 10, bitmapToken: baseToken, epoch: 0, pixelVersion: 0 }],
    };
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "rust_pixels_undo_snapshot" ? Promise.resolve(dto) : Promise.resolve(null),
    );
    const store = bitmapStoreFor("doc-1");
    // A base-only layer registers its bitmap under the baseImageBitmap field.
    store.set(baseToken, baseBm, "baseImageBitmap");
    const set = vi.fn(() => true);
    const applied = await restoreSnapshotBitmapsByToken("undo", "doc-1", (t) => store.get(t), set, (t) => store.getField(t));
    expect(applied).toBe(true);
    // The set callback receives the field hint so it assigns baseImageBitmap,
    // never the imageBitmap it did not have pre-action.
    expect(set).toHaveBeenCalledWith("l1", baseBm, "baseImageBitmap");
    expect(baseBm.close).not.toHaveBeenCalled();
  });

  it("flag OFF is inert: a commit does NOT fire any invoke, and re-attach is a no-op", async () => {
    // No gate + non-Tauri runtime (the production default).
    vi.mocked(isTauriRuntime).mockReturnValue(false);
    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc-1");
    history.recordSnapshotHistory(makeModel([makeLayer("l1", fakeBitmap())]), makeModel([makeLayer("l1", fakeBitmap())]), "Meta");
    history.recordSnapshotHistory(makeModel([makeLayer("l1", fakeBitmap())]), makeModel([makeLayer("l1", fakeBitmap())]), "Meta2");
    await flush();
    expect(invoke).not.toHaveBeenCalled();

    const applied = await restoreSnapshotBitmapsByToken("undo", "doc-1", () => null, () => true);
    expect(applied).toBe(false);
  });

  it("flag OFF stays inert even in the Tauri runtime when the gate is unset", async () => {
    vi.mocked(isTauriRuntime).mockReturnValue(true); // Tauri runtime but NO gate
    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc-1");
    history.recordSnapshotHistory(makeModel([makeLayer("l1", fakeBitmap())]), makeModel([makeLayer("l1", fakeBitmap())]), "Meta");
    await flush();
    expect(invoke).not.toHaveBeenCalled();
  });

  // ── FINDING 1: a redo dispatch fires rust_pixels_redo_snapshot ──
  // Before the fix, the redo twin of a Snapshot entry lost its snapshotType on
  // the undo()→redo() stack hop, so useEditorCommands never reached the re-attach
  // and rust_pixels_redo_snapshot was dead. Driving the REAL helper in the redo
  // direction proves the redo command is actually dispatched when identity holds.
  it("finding1: a redo dispatch fires rust_pixels_redo_snapshot (re-attach is not dead)", async () => {
    gateOn(true);
    const afterBm = fakeBitmap();
    const afterToken = tokenForBitmap(afterBm);
    const afterDto: SnapshotPayload = {
      docId: "doc-1",
      version: 0,
      layers: [{ layerId: "l1", width: 10, height: 10, bitmapToken: afterToken, epoch: 0, pixelVersion: 0 }],
    };
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "rust_pixels_redo_snapshot" ? Promise.resolve(afterDto) : Promise.resolve(null),
    );
    const store = bitmapStoreFor("doc-1");
    store.set(afterToken, afterBm);
    const set = vi.fn(() => true);
    const applied = await restoreSnapshotBitmapsByToken("redo", "doc-1", (t) => store.get(t), set, undefined, () => ["l1"]);
    expect(applied).toBe(true);
    expect(set).toHaveBeenCalledWith("l1", afterBm);
    // The literal redo invoke FIRES.
    expect(invoke).toHaveBeenCalledWith("rust_pixels_redo_snapshot", { docId: "doc-1" });
  });

  // ── FINDING 2: identity assertion ──
  it("finding2: a payload whose docId differs SKIPS the re-attach (Model-A stays authoritative)", async () => {
    gateOn(true);
    const wrongBm = fakeBitmap();
    const wrongToken = tokenForBitmap(wrongBm);
    const wrongDto: SnapshotPayload = {
      docId: "other-doc", // mismatched docId
      version: 0,
      layers: [{ layerId: "l1", width: 10, height: 10, bitmapToken: wrongToken, epoch: 0, pixelVersion: 0 }],
    };
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "rust_pixels_undo_snapshot" ? Promise.resolve(wrongDto) : Promise.resolve(null),
    );
    const store = bitmapStoreFor("doc-1");
    store.set(wrongToken, wrongBm);
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    try {
      const set = vi.fn(() => true);
      const applied = await restoreSnapshotBitmapsByToken("undo", "doc-1", (t) => store.get(t), set, undefined, () => ["l1"]);
      expect(applied).toBe(false);
      expect(set).not.toHaveBeenCalled();
      expect(warnSpy).toHaveBeenCalled();
    } finally {
      warnSpy.mockRestore();
    }
  });

  it("finding2: a payload whose layer-set differs from the restored model SKIPS the re-attach", async () => {
    gateOn(true);
    const bm = fakeBitmap();
    const tk = tokenForBitmap(bm);
    const dto: SnapshotPayload = {
      docId: "doc-1",
      version: 0,
      layers: [
        { layerId: "l1", width: 10, height: 10, bitmapToken: tk, epoch: 0, pixelVersion: 0 },
        // payload includes l2 but the restored model only has l1 (cursor drift).
        { layerId: "l2", width: 10, height: 10, bitmapToken: tk, epoch: 0, pixelVersion: 0 },
      ],
    };
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "rust_pixels_undo_snapshot" ? Promise.resolve(dto) : Promise.resolve(null),
    );
    const store = bitmapStoreFor("doc-1");
    store.set(tk, bm);
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    try {
      const set = vi.fn(() => true);
      const applied = await restoreSnapshotBitmapsByToken("undo", "doc-1", (t) => store.get(t), set, undefined, () => ["l1"]);
      expect(applied).toBe(false);
      expect(set).not.toHaveBeenCalled();
      expect(warnSpy).toHaveBeenCalled();
    } finally {
      warnSpy.mockRestore();
    }
  });

  it("finding2: a MATCHING docId + layer-set still applies (no over-skip)", async () => {
    gateOn(true);
    const bm = fakeBitmap();
    const tk = tokenForBitmap(bm);
    const dto: SnapshotPayload = {
      docId: "doc-1",
      version: 0,
      layers: [{ layerId: "l1", width: 10, height: 10, bitmapToken: tk, epoch: 0, pixelVersion: 0 }],
    };
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "rust_pixels_undo_snapshot" ? Promise.resolve(dto) : Promise.resolve(null),
    );
    const store = bitmapStoreFor("doc-1");
    store.set(tk, bm);
    const set = vi.fn(() => true);
    const applied = await restoreSnapshotBitmapsByToken("undo", "doc-1", (t) => store.get(t), set, undefined, () => ["l1"]);
    expect(applied).toBe(true);
    expect(set).toHaveBeenCalledWith("l1", bm);
  });

  // ── FINDING B: per-layer epoch/pixelVersion/token identity ──
  it("findingB: a payload whose per-layer epoch/pixelVersion mismatches the restored model DROPS the re-attach (same docId + layer-set)", async () => {
    gateOn(true);
    const bm = fakeBitmap();
    const tk = tokenForBitmap(bm);
    const dto: SnapshotPayload = {
      docId: "doc-1", // correct docId
      version: 0,
      // Correct layer-set, but a DIFFERENT step's epoch/pixelVersion (5 vs 0).
      layers: [{ layerId: "l1", width: 10, height: 10, bitmapToken: tk, epoch: 5, pixelVersion: 5 }],
    };
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "rust_pixels_undo_snapshot" ? Promise.resolve(dto) : Promise.resolve(null),
    );
    const store = bitmapStoreFor("doc-1");
    store.set(tk, bm);
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    try {
      const set = vi.fn(() => true);
      // The restored model says l1's epoch is 0 (never painted); payload says 5.
      const applied = await restoreSnapshotBitmapsByToken(
        "undo", "doc-1", (t) => store.get(t), set,
        undefined, () => ["l1"],
        (layerId) =>
          layerId === "l1" ? { epoch: 0, pixelVersion: 0, imageBitmap: bm, baseImageBitmap: null } : null,
      );
      expect(applied).toBe(false);
      expect(set).not.toHaveBeenCalled();
      expect(warnSpy).toHaveBeenCalled();
    } finally {
      warnSpy.mockRestore();
    }
  });

  it("findingB: a payload whose per-layer epoch/pixelVersion MATCHES the restored model is applied", async () => {
    gateOn(true);
    const bm = fakeBitmap();
    const tk = tokenForBitmap(bm);
    const dto: SnapshotPayload = {
      docId: "doc-1",
      version: 0,
      layers: [{ layerId: "l1", width: 10, height: 10, bitmapToken: tk, epoch: 5, pixelVersion: 5 }],
    };
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "rust_pixels_undo_snapshot" ? Promise.resolve(dto) : Promise.resolve(null),
    );
    const store = bitmapStoreFor("doc-1");
    store.set(tk, bm);
    const set = vi.fn(() => true);
    const applied = await restoreSnapshotBitmapsByToken(
      "undo", "doc-1", (t) => store.get(t), set,
      undefined, () => ["l1"],
      (layerId) =>
        layerId === "l1" ? { epoch: 5, pixelVersion: 5, imageBitmap: bm, baseImageBitmap: null } : null,
    );
    expect(applied).toBe(true);
    expect(set).toHaveBeenCalledWith("l1", bm);
  });

  it("findingB: a payload whose token resolves to a DIFFERENT bitmap than the restored model DROPS the re-attach (epochs co-incide)", async () => {
    gateOn(true);
    const correctBm = fakeBitmap();
    const wrongBm = fakeBitmap();
    const wrongToken = tokenForBitmap(wrongBm);
    const dto: SnapshotPayload = {
      docId: "doc-1",
      version: 0,
      // Correct layer-set AND correct epoch/pixelVersion, but the token resolves
      // to a bitmap the restored model does NOT hold (a different step's bitmap).
      layers: [{ layerId: "l1", width: 10, height: 10, bitmapToken: wrongToken, epoch: 0, pixelVersion: 0 }],
    };
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "rust_pixels_undo_snapshot" ? Promise.resolve(dto) : Promise.resolve(null),
    );
    const store = bitmapStoreFor("doc-1");
    store.set(wrongToken, wrongBm); // token resolves to wrongBm
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    try {
      const set = vi.fn(() => true);
      const applied = await restoreSnapshotBitmapsByToken(
        "undo", "doc-1", (t) => store.get(t), set,
        undefined, () => ["l1"],
        // The restored model holds correctBm (NOT wrongBm).
        (layerId) =>
          layerId === "l1" ? { epoch: 0, pixelVersion: 0, imageBitmap: correctBm, baseImageBitmap: null } : null,
      );
      expect(applied).toBe(false);
      expect(set).not.toHaveBeenCalled();
      expect(warnSpy).toHaveBeenCalled();
    } finally {
      warnSpy.mockRestore();
    }
  });
});

