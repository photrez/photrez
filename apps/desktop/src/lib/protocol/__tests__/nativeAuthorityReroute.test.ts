// Native-authority reroute tests: when native engine authority is active, the
// facade command/query path is rerouted to the native engine.
//
// The facade command/query path is rerouted to the native Rust ProtocolEngine
// (Tauri `invoke`) ONLY when `photrez.facadeAuthority === "native"`. Every native
// branch is gated: when the flag is unset (production default) the wasm path runs
// byte-identical and NO Tauri `invoke` for the native commands is ever issued.
//
// Mock fidelity (CRITICAL): Tauri v2 `invoke()` REJECTS with the bare
// `"CODE: message"` string on a Rust `Err(String)`. A mock that RESOLVES with
// `{ok:false}` is WRONG and would pass the wrong contract (this exact mistake
// shipped a bug before). These tests reject with the bare string.

import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  applyCommand,
  getSnapshot,
  getHistoryQuery,
  historyCursorCommit,
  getVersion,
  ensureNativeEngineSeeded,
  createNativeSeed,
  clearNativeSeed,
  normalizeProtocolError,
  setProtocolWasm,
  __resetNativeAuthorityForTests,
} from "../bridge";
import { seedFacadeFromEngine, getFacade, recordExternalTransitionFor, __resetFacadeRegistryForTests } from "../facadeRegistry";
import { WorkspaceManager } from "@/engine/workspace";
import { restoreSnapshotBitmapsByToken } from "@/engine/history";
import { EditorFacade } from "../editorFacade";
import { CONTRACT_VERSION } from "../types";
import type { CommandResult, RenderLayer } from "../types";

// Self-contained localStorage so this test runs in the node project (no jsdom).
// jsdom supplies its own; the shim is only installed when one is absent.
if (typeof (globalThis as { localStorage?: unknown }).localStorage === "undefined") {
  const __lsStore = new Map<string, string>();
  (globalThis as { localStorage: Storage }).localStorage = {
    getItem: (k: string) => (__lsStore.has(k) ? (__lsStore.get(k) as string) : null),
    setItem: (k: string, v: string) => {
      __lsStore.set(k, String(v));
    },
    removeItem: (k: string) => {
      __lsStore.delete(k);
    },
    clear: () => __lsStore.clear(),
    key: (i: number) => Array.from(__lsStore.keys())[i] ?? null,
    get length() {
      return __lsStore.size;
    },
  } as Storage;
}

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as Mock<(cmd: string, args: Record<string, unknown>) => Promise<unknown>>;
// Force the history-bridge gate on so the snapshot-command wiring (recordSnapshotHistory
// / restoreSnapshotBitmapsByToken) can be exercised. Native-authority tests set the
// photrez.historyBridge flag themselves; every other test leaves it unset.
vi.mock("@/lib/desktop/tauriWindow", () => ({ isTauriRuntime: vi.fn(() => true) }));

const layer: RenderLayer & { transform: { x: number; y: number; scaleX: number; scaleY: number; rotation: number } } = {
  id: "L1",
  name: "Base",
  visible: true,
  opacity: 1,
  resourceId: 1,
  x: 0,
  y: 0,
  scaleX: 1,
  scaleY: 1,
  rotation: 0,
  transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0 },
};

function cmdResultJson(docVersion: number): string {
  return JSON.stringify({
    documentVersion: docVersion,
    delta: {
      baseVersion: 0,
      version: docVersion,
      changes: [{ kind: "upsert", layer }],
    },
  } satisfies CommandResult);
}

// Faithful Tauri v2 mock backed by per-doc native state. Replicates the REAL Rust
// ProtocolEngine contract (verified against protocol_native_cmds.rs +
// document_core.rs):
//  - a missing doc (never opened) REJECTS with the bare "document not open: {key}"
//    string (NOT an invented E_MISSING_DOC code);
//  - protocol_apply_command_native runs engine.history.apply, which REJECTS a
//    mismatched expectedVersion with "E_VERSION_MISMATCH: expected version X got Y";
//  - protocol_history_query_native / protocol_history_cursor_commit_native reject a
//    missing doc like the siblings do;
//  - unknown command names THROW (they must never silently resolve undefined);
//  - the seed is only-when-empty; each successful command / snapshot cursor
//    advances and returns the live native document version.
// The rejection strings are EXACTLY what real Rust produces (Tauri v2 invoke
// rejects with that bare string), so these tests catch a reroute that silently
// diverges from the real contract (the faithful mock encodes what real Rust returns).
function routeNative(): void {
  const open = new Set<string>();
  const seeded = new Set<string>();
  const adapters = new Map<string, Set<string>>();
  const version = new Map<string, number>();
  const layers = new Map<string, unknown[]>();
  const snaps = new Set<string>(); // docs with a recorded Snapshot entry (tip is a Snapshot)
  const docIdOf = (args: Record<string, unknown> = {}) => (args.docId as string) ?? "default";
  // Reject with the EXACT bare string real Rust produces (no synthetic suffix):
  // Tauri v2 invoke() rejects with this string on a Rust Err(String).
  const reject = (code: string): never => {
    throw code;
  };
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    const docId = docIdOf(args);
    switch (cmd) {
      case "rust_pixels_open_document":
        open.add(docId);
        return undefined;
      case "protocol_register_adapter_native": {
        if (!open.has(docId)) reject(`document not open: ${docId}`);
        const aid = args.adapterId as string;
        if (!adapters.has(docId)) adapters.set(docId, new Set());
        adapters.get(docId)!.add(aid);
        return undefined;
      }
      case "protocol_seed_native": {
        if (!open.has(docId)) reject(`document not open: ${docId}`);
        // Only-when-empty: a second seed is a no-op (matches Rust seed idempotency).
        if (seeded.has(docId)) return JSON.stringify({ version: version.get(docId) ?? 0, layers: layers.get(docId) ?? [] });
        const payload = JSON.parse((args.payloadJson as string) ?? "{}");
        seeded.add(docId);
        version.set(docId, Number(payload.version ?? 0));
        layers.set(docId, payload.layers ?? []);
        return JSON.stringify({ version: version.get(docId) ?? 0, layers: payload.layers ?? [] });
      }
      case "protocol_seed_canonical_native": {
        if (!open.has(docId)) reject(`document not open: ${docId}`);
        // Mirrors protocol_seed_canonical_native (native client): returns the null
        // ack; the re-push caller ignores the result.
        return null;
      }
      case "protocol_apply_command_native": {
        if (!open.has(docId)) reject(`document not open: ${docId}`);
        const env = JSON.parse((args.envelopeJson as string) ?? "{}");
        // Mirror engine.history.apply: a mismatched expectedVersion is rejected with
        // the bare "E_VERSION_MISMATCH: expected version X got Y" string. This is the
        // exact bug class the reroute must NOT hide (the real Rust apply() does this).
        const cur = version.get(docId) ?? 0;
        if (env.expectedVersion !== undefined && env.expectedVersion !== cur) {
          reject(`E_VERSION_MISMATCH: expected version ${env.expectedVersion} got ${cur}`);
        }
        if (env.command?.type === "recordExternalTransition") {
          const aid = env.command.adapter_id;
          if (!adapters.get(docId)?.has(aid)) reject("E_UNKNOWN_ADAPTER: adapter not registered on native engine");
        }
        const next = cur + 1;
        version.set(docId, next);
        return cmdResultJson(next);
      }
      case "protocol_snapshot_native": {
        if (!open.has(docId)) reject(`document not open: ${docId}`);
        return JSON.stringify({ version: version.get(docId) ?? 0, layers: layers.get(docId) ?? [] });
      }
      case "protocol_version_native": {
        if (!open.has(docId)) reject(`document not open: ${docId}`);
        return version.get(docId) ?? 0;
      }
      case "rust_pixels_record_snapshot": {
        if (!open.has(docId)) reject(`document not open: ${docId}`);
        snaps.add(docId);
        const next = (version.get(docId) ?? 0) + 1;
        version.set(docId, next);
        return { version: next, epoch: 0 };
      }
      case "rust_pixels_undo_snapshot":
      case "rust_pixels_redo_snapshot": {
        if (!open.has(docId)) reject(`document not open: ${docId}`);
        // No Snapshot entry to step -> real Rust returns Ok(None) (null on the wire).
        if (!snaps.has(docId)) return null;
        const next = (version.get(docId) ?? 0) + 1;
        version.set(docId, next);
        // The returned DocumentSnapshot carries its STORED `version` field (TS records
        // version:0), NOT the engine document version. The engine DV advances
        // (self.version += 1) but the snapshot payload reflects what was recorded.
        return { version: 0, docId, layers: [] };
      }
      case "protocol_history_query_native": {
        if (!open.has(docId)) reject(`document not open: ${docId}`);
        return JSON.stringify({ cursor: 0, lastSeq: 0, degradedHint: false, entries: [] });
      }
      case "protocol_history_cursor_commit_native": {
        if (!open.has(docId)) reject(`document not open: ${docId}`);
        // Faithful note: real Rust history_cursor_commit (history.rs) rejects
        // E_CURSOR_MISMATCH unless a matching pending_external barrier is set by the
        // walker handoff. This mock does not model the barrier (out of scope for the
        // reroute change) and always succeeds; the reroute test covers the dispatch
        // seam, not the barrier contract.
        const next = (version.get(docId) ?? 0) + 1;
        version.set(docId, next);
        return cmdResultJson(next);
      }
      default:
        // Unknown command names must surface as errors, never silently resolve
        // undefined (which would hide a reroute to a non-existent native command).
        throw `E_UNKNOWN_COMMAND: ${cmd}`;
    }
  });
}

function makeWasm(applyResult: CommandResult) {
  return {
    protocol_contract_version: () => CONTRACT_VERSION,
    protocol_apply_command: (_json: string, _docId: string) => JSON.stringify(applyResult),
    protocol_snapshot_json: (_docId: string) => JSON.stringify({ version: 0, layers: [] }),
    protocol_history_query_json: (_docId: string) =>
      JSON.stringify({ cursor: 0, lastSeq: 0, degradedHint: false, entries: [] }),
    protocol_history_cursor_commit: (_json: string, _docId: string) => JSON.stringify(applyResult),
  };
}

beforeEach(() => {
  localStorage.clear();
  invokeMock.mockReset();
  __resetNativeAuthorityForTests();
  __resetFacadeRegistryForTests();
  setProtocolWasm(null as unknown as Parameters<typeof setProtocolWasm>[0]);
});

afterEach(() => {
  localStorage.clear();
  __resetNativeAuthorityForTests();
  __resetFacadeRegistryForTests();
  setProtocolWasm(null as unknown as Parameters<typeof setProtocolWasm>[0]);
});

describe("default (wasm) path is byte-identical", () => {
  it("uses the wasm engine and never issues a native invoke", async () => {
    localStorage.clear(); // facadeAuthority unset -> isNativeAuthority() false
    setProtocolWasm(makeWasm({ documentVersion: 7, delta: { baseVersion: 0, version: 7, changes: [] } }));
    const res = await applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: 0,
      command: { type: "ping", echo: "x" },
    });
    expect(res.documentVersion).toBe(7); // came from wasm
    expect(invokeMock).not.toHaveBeenCalledWith("protocol_apply_command_native", expect.anything());
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("getSnapshot/historyQuery/cursorCommit/getVersion stay on wasm when native authority is off", async () => {
    localStorage.clear();
    setProtocolWasm(makeWasm({ documentVersion: 3, delta: { baseVersion: 0, version: 3, changes: [] } }));
    await getSnapshot("docA");
    await getHistoryQuery("docA");
    await historyCursorCommit(1, "undo", "docA");
    await getVersion("docA");
    expect(invokeMock).not.toHaveBeenCalled();
  });
});

describe("native authority reroute", () => {
  beforeEach(() => {
    localStorage.setItem("photrez.facadeAuthority", "native");
    routeNative();
    // Arm wasm so the `!wasm` guards in getSnapshot/getHistoryQuery/
    // historyCursorCommit do not short-circuit before the native branch. The
    // native commands are dispatched first (isNativeAuthority wins), so the wasm
    // engine is only the fallback path here.
    setProtocolWasm(makeWasm({ documentVersion: 1, delta: { baseVersion: 0, version: 1, changes: [] } }));
  });

  it("applyCommand reroutes to protocol_apply_command_native with the rust envelope", async () => {
    const engineStub = { getId: () => "docA", getLayers: () => [layer], getName: () => "doc", getWidth: () => 800, getHeight: () => 600, getSelection: () => null };
    const facade = new EditorFacade(undefined, "docA");
    await seedFacadeFromEngine(engineStub as never, facade); // cutover seed (open + seed)
    const res = await applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: 0,
      docId: "docA",
      command: { type: "addLayer", id: "N-id", name: "N", width: 100, height: 100, index: 0 },
    });
    const call = invokeMock.mock.calls.find((c) => c[0] === "protocol_apply_command_native");
    expect(call).toBeDefined();
    expect(call![1].docId).toBe("docA");
    const env = JSON.parse(call![1].envelopeJson as string);
    expect(env.command.type).toBe("addLayer");
    expect(env.command.name).toBe("N");
    expect(res).toEqual(JSON.parse(cmdResultJson(1)) as CommandResult);
  });

  it("renderedVersion tracks the returned documentVersion through a facade command", async () => {
    const engineStub = { getId: () => "docB", getLayers: () => [layer], getName: () => "doc", getWidth: () => 800, getHeight: () => 600, getSelection: () => null };
    // Mirror production: obtain the facade via getFacade so it is registered in
    // the per-document registry (seedFacadeFromEngine does not register it).
    const facade = getFacade("docB");
    await seedFacadeFromEngine(engineStub as never, facade);
    await facade.addLayer("N");
    expect(facade.renderedVersion).toBe(1);
    expect(getFacade("docB").renderedVersion).toBe(1);
  });

  it("getSnapshot reroutes to protocol_snapshot_native", async () => {
    const engineStub = { getId: () => "docC", getLayers: () => [layer], getName: () => "doc", getWidth: () => 800, getHeight: () => 600, getSelection: () => null };
    await seedFacadeFromEngine(engineStub as never, new EditorFacade(undefined, "docC"));
    const snap = await getSnapshot("docC");
    expect(invokeMock).toHaveBeenCalledWith("protocol_snapshot_native", { docId: "docC" });
    expect(snap.version).toBe(0);
  });

  // PINS the sync wiring: a native-authority facade command (setOpacity) routes its
  // version-sync through protocol_version_native (the version-only probe), NOT
  // protocol_snapshot_native. If syncFromEngine is reverted from getVersion back to
  // getSnapshot, this breaks - proving the reroute is enforced by a real test.
  it("setOpacity sync path probes protocol_version_native (not the snapshot)", async () => {
    const engineStub = { getId: () => "docV", getLayers: () => [layer], getName: () => "doc", getWidth: () => 800, getHeight: () => 600, getSelection: () => null };
    const facade = getFacade("docV");
    await seedFacadeFromEngine(engineStub as never, facade);
    await facade.setOpacity("L1", 0.5);
    expect(invokeMock).toHaveBeenCalledWith("protocol_version_native", { docId: "docV" });
  });

  it("getHistoryQuery reroutes to protocol_history_query_native", async () => {
    const engineStub = { getId: () => "docD", getLayers: () => [layer], getName: () => "doc", getWidth: () => 800, getHeight: () => 600, getSelection: () => null };
    await seedFacadeFromEngine(engineStub as never, new EditorFacade(undefined, "docD"));
    const q = await getHistoryQuery("docD");
    expect(invokeMock).toHaveBeenCalledWith("protocol_history_query_native", { docId: "docD" });
    expect(q.cursor).toBe(0);
  });

  it("historyCursorCommit reroutes to protocol_history_cursor_commit_native with typed args", async () => {
    const engineStub = { getId: () => "docE", getLayers: () => [layer], getName: () => "doc", getWidth: () => 800, getHeight: () => 600, getSelection: () => null };
    await seedFacadeFromEngine(engineStub as never, new EditorFacade(undefined, "docE"));
    const res = await historyCursorCommit(2, "redo", "docE");
    expect(invokeMock).toHaveBeenCalledWith("protocol_history_cursor_commit_native", {
      docId: "docE",
      seq: 2,
      direction: "redo",
    });
    expect(res.documentVersion).toBe(1);
  });
});

describe("cutover seed (open-before-seed + idempotency)", () => {
  beforeEach(() => {
    localStorage.setItem("photrez.facadeAuthority", "native");
    routeNative();
    // Arm wasm so the `!wasm` guards in getSnapshot/getHistoryQuery/
    // historyCursorCommit do not short-circuit before the native branch. The
    // native commands are dispatched first (isNativeAuthority wins), so the wasm
    // engine is only the fallback path here.
    setProtocolWasm(makeWasm({ documentVersion: 1, delta: { baseVersion: 0, version: 1, changes: [] } }));
  });

  it("opens the document before seeding (open-before-seed ordering)", async () => {
    await ensureNativeEngineSeeded("docSeed", 3, [layer]);
    const cmds = invokeMock.mock.calls.map((c) => c[0]);
    expect(cmds[0]).toBe("rust_pixels_open_document");
    expect(cmds[1]).toBe("protocol_seed_native");
  });

  it("is idempotent: a second seed for the same doc issues no further seed invoke", async () => {
    await ensureNativeEngineSeeded("docSeed", 3, [layer]);
    invokeMock.mockClear();
    // Different version/layers must be ignored on the second (idempotent) call.
    await ensureNativeEngineSeeded("docSeed", 99, []);
    const seedCalls = invokeMock.mock.calls.filter((c) => c[0] === "protocol_seed_native");
    const openCalls = invokeMock.mock.calls.filter((c) => c[0] === "rust_pixels_open_document");
    expect(seedCalls.length).toBe(0);
    expect(openCalls.length).toBe(0);
  });

  it("seedFacadeFromEngine triggers the cutover seed when native authority is on", async () => {
    const engineStub = { getId: () => "docF", getLayers: () => [layer], getName: () => "doc", getWidth: () => 800, getHeight: () => 600, getSelection: () => null };
    await seedFacadeFromEngine(engineStub as never, new EditorFacade(undefined, "docF"));
    expect(invokeMock).toHaveBeenCalledWith("rust_pixels_open_document", { docId: "docF" });
    expect(invokeMock).toHaveBeenCalledWith("protocol_seed_native", expect.objectContaining({ docId: "docF" }));
  });
});

describe("native seed carries real layers; bridge commands never seed empty", () => {
  beforeEach(() => {
    localStorage.setItem("photrez.facadeAuthority", "native");
    routeNative();
    // Arm wasm so the `!wasm` guards do not short-circuit before the native branch.
    setProtocolWasm(makeWasm({ documentVersion: 1, delta: { baseVersion: 0, version: 1, changes: [] } }));
  });

  it("the open-path seed carries the doc's REAL layers, and a legacy commit awaits it (no empty seed)", async () => {
    // Simulate the document-open path (WorkspaceManager.addDocument) seeding the
    // native engine with the REAL layers + starting version BEFORE any command.
    await createNativeSeed("docRace", 0, [layer]);

    // The authoritative seed invoke must have carried the REAL layers (not []).
    const seedCall = invokeMock.mock.calls.find((c) => c[0] === "protocol_seed_native");
    expect(seedCall).toBeDefined();
    const seedArg = JSON.parse((seedCall![1] as { payloadJson: string }).payloadJson);
    expect(seedArg.layers.length).toBe(1);
    expect(seedArg.layers[0].id).toBe("L1");
    expect(seedArg.version).toBe(0);

    // Drive a REAL production entry point WITHOUT seedFacadeFromEngine: a legacy
    // commit mirrored through the facade commit shim -> recordExternalTransitionFor
    // -> applyCommand. On the buggy code this path is the FIRST seeder and seeds
    // the engine EMPTY (first-call-wins []), clobbering the model. Here the open
    // path already seeded real layers, so the bridge must only await that seed.
    invokeMock.mockClear();
    const res = await recordExternalTransitionFor("docRace", {
      label: "Legacy Edit",
      affectedLayerIds: ["L1"],
      snapshot: {},
    });
    expect(res.ok).toBe(true);

    // No second (empty) seed invoke - the bridge awaited the open-path seed.
    expect(invokeMock).not.toHaveBeenCalledWith("protocol_seed_native", expect.anything());
    // The command ran against the REAL seeded engine.
    const applyCall = invokeMock.mock.calls.find((c) => c[0] === "protocol_apply_command_native");
    expect(applyCall).toBeDefined();
    const env = JSON.parse(applyCall![1].envelopeJson as string);
    expect(env.command.type).toBe("recordExternalTransition");
  });

  it("getSnapshot on a freshly-opened doc awaits the open-path seed (real layers), not an empty seed", async () => {
    await createNativeSeed("docOpen", 0, [layer]);
    invokeMock.mockClear();
    const snap = await getSnapshot("docOpen");
    // No empty re-seed; snapshot comes from the REAL seeded engine.
    expect(invokeMock).not.toHaveBeenCalledWith("protocol_seed_native", expect.anything());
    expect(invokeMock).toHaveBeenCalledWith("protocol_snapshot_native", { docId: "docOpen" });
    expect(snap.version).toBe(0);
    expect(snap.layers[0].id).toBe("L1");
  });
});

describe("native-authority acceptance checks (each exercises a real Rust contract)", () => {
  beforeEach(() => {
    localStorage.setItem("photrez.facadeAuthority", "native");
    routeNative();
    setProtocolWasm(makeWasm({ documentVersion: 1, delta: { baseVersion: 0, version: 1, changes: [] } }));
  });

  // Reopening the SAME doc id must re-seed the native engine AND give a fresh
  // facade. This drives the REAL producers (WorkspaceManager.addDocument ->
  // removeDocument -> addDocument), not the internal clearNativeSeed/
  // createNativeSeed helpers, so it proves the close/reopen path actually invokes
  // them. Without the eviction on close (clearNativeSeed + removeFacade) the stale
  // resolved seed promise short-circuits and the reopened facade is stale.
  it("reopening the same doc id re-seeds the native engine and gives a fresh facade (WorkspaceManager close/reopen)", async () => {
    // addDocument seeds the native engine fire-and-forget (createNativeSeed(...).catch),
    // so flush microtasks before asserting the seed invokes landed.
    const flush = () => new Promise((r) => setTimeout(r, 0));
    const wm = new WorkspaceManager();
    wm.addDocument(WorkspaceManager.createBlankDocument("docReopen", "Reopen", 800, 600));
    await flush();
    // The document-open path seeded the native engine for this doc id.
    expect(invokeMock).toHaveBeenCalledWith("rust_pixels_open_document", { docId: "docReopen" });
    expect(invokeMock).toHaveBeenCalledWith("protocol_seed_native", expect.objectContaining({ docId: "docReopen" }));
    // The per-doc facade exists at the seeded version 0.
    expect(getFacade("docReopen").renderedVersion).toBe(0);
    // Advance the facade to a non-zero version so the reopen assertion proves the
    // facade was actually evicted on close. Without eviction (removeFacade) this
    // same instance would persist and still read 5 -> the assertion below fails.
    getFacade("docReopen").syncRenderedVersionTo(5);

    invokeMock.mockClear();
    // REAL close path: removeDocument evicts native seed/adapter state AND the
    // per-doc facade (clearNativeSeed + removeFacade), so a reopen starts clean.
    wm.removeDocument("docReopen");

    // REAL reopen path: addDocument again with the same id re-seeds the engine.
    wm.addDocument(WorkspaceManager.createBlankDocument("docReopen", "Reopen", 800, 600));
    await flush();
    expect(invokeMock).toHaveBeenCalledWith("rust_pixels_open_document", { docId: "docReopen" });
    expect(invokeMock).toHaveBeenCalledWith("protocol_seed_native", expect.objectContaining({ docId: "docReopen" }));

    // The facade for the reopened id is FRESH (evicted on close), not a stale one
    // left over from the first open.
    expect(getFacade("docReopen").renderedVersion).toBe(0);
  });

  // A bridge command on a doc whose native engine was never opened must reject
  // loudly with the REAL Rust string ("document not open: {key}"), NOT an invented
  // E_MISSING_DOC code. The earlier mock resolved ANY doc, so this path
  // never surfaced.
  it("a bridge command on an unseeded doc fails loud with the real Rust missing-doc error", async () => {
    await expect(
      applyCommand({
        contractVersion: CONTRACT_VERSION,
        expectedVersion: 0,
        docId: "docNeverOpened",
        command: { type: "addLayer", id: "N-id", name: "N", width: 100, height: 100, index: 0 },
      }),
    ).rejects.toThrow(/document not open/);
  });

  // protocol_apply_command_native must REJECT a mismatched expectedVersion exactly
  // like real Rust engine.history.apply (E_VERSION_MISMATCH). The earlier mock
  // ignored expectedVersion, so a reroute that desynced versions would pass green.
  it("protocol_apply_command_native rejects a mismatched expectedVersion (E_VERSION_MISMATCH)", async () => {
    const engineStub = { getId: () => "docEv", getLayers: () => [layer], getName: () => "doc", getWidth: () => 800, getHeight: () => 600, getSelection: () => null };
    await seedFacadeFromEngine(engineStub as never, new EditorFacade(undefined, "docEv"));
    // Engine seeded at version 0; a command claiming expectedVersion 5 must reject.
    await expect(
      applyCommand({
        contractVersion: CONTRACT_VERSION,
        expectedVersion: 5,
        docId: "docEv",
        command: { type: "addLayer", id: "N-id", name: "N", width: 100, height: 100, index: 0 },
      }),
    ).rejects.toThrow(/E_VERSION_MISMATCH/);
    // A command with the correct expectedVersion (0) succeeds and advances to 1.
    const res = await applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: 0,
      docId: "docEv",
      command: { type: "addLayer", id: "N-id", name: "N", width: 100, height: 100, index: 0 },
    });
    expect(res.documentVersion).toBe(1);
  });

  // protocol_history_query_native and protocol_history_cursor_commit_native must
  // enforce the open-doc check like the sibling native commands (reject if not
  // open). The earlier mock returned empty results for a never-opened doc.
  it("history query and cursor commit reject a doc that was never opened (mirrors real Rust)", async () => {
    await expect(getHistoryQuery("docNeverOpenedH")).rejects.toThrow(/document not open/);
    await expect(historyCursorCommit(1, "undo", "docNeverOpenedH")).rejects.toThrow(/document not open/);
  });

  // Unknown native command names must THROW, never silently resolve undefined
  // (which would hide a reroute to a non-existent native command).
  it("an unknown native command name throws instead of resolving undefined", async () => {
    await expect(
      invokeMock("protocol_does_not_exist_native", { docId: "docUnk" }),
    ).rejects.toThrow(/E_UNKNOWN_COMMAND/);
  });

  // The SOLE rust_pixels_record_snapshot entry point (history.ts recordSnapshotHistory)
  // must advance the native engine version and push it into the facade via
  // syncFacadeVersionFromPixel (the record-snapshot sync path; undo is the only one
  // previously exercised). Drives the real CommandHistory producer.
  it("a native snapshot record syncs the facade version from the native engine (recordSnapshotHistory)", async () => {
    localStorage.setItem("photrez.historyBridge", "1"); // enable the snapshot bridge
    const wm = new WorkspaceManager();
    wm.addDocument(WorkspaceManager.createBlankDocument("docRecord", "Record", 100, 100));
    const engine = wm.getEngine("docRecord")!;
    const history = wm.getHistory("docRecord")!;
    history.attachDocIdGetter(() => "docRecord");
    // The facade must exist so the sync has a target.
    expect(getFacade("docRecord").renderedVersion).toBe(0);
    // The SOLE rust_pixels_record_snapshot entry point (history.ts recordSnapshotHistory).
    history.recordSnapshotHistory(engine.snapshot(), engine.snapshot(), "Record");
    // The record fires async (fire-and-forget). Flush the microtask queue.
    await new Promise((r) => setTimeout(r, 0));
    expect(invokeMock).toHaveBeenCalledWith("rust_pixels_record_snapshot", expect.objectContaining({ docId: "docRecord" }));
    // Native record advanced the engine version to 1; the facade must track it.
    expect(getFacade("docRecord").renderedVersion).toBe(1);
  });

  // The legacy mirror (recordExternalTransitionFor) MUST register the ts-external
  // adapter on the native engine before the record command, else Rust
  // record_external rejects E_UNKNOWN_ADAPTER. Before this change, registerPayloadAdapter had no
  // native branch, so the adapter was registered only on the wasm/emulator engine.
  it("the legacy mirror registers ts-external on the native engine (no E_UNKNOWN_ADAPTER)", async () => {
    await createNativeSeed("docMirror", 0, [layer]);
    const res = await recordExternalTransitionFor("docMirror", {
      label: "Legacy Edit",
      affectedLayerIds: ["L1"],
      snapshot: {},
    });
    expect(res.ok).toBe(true);
    expect(invokeMock).toHaveBeenCalledWith("protocol_register_adapter_native", {
      docId: "docMirror",
      adapterId: "ts-external",
    });
  });

  // A snapshot undo advances the native engine version; the facade must track it
  // (syncFacadeVersionFromPixel) so the next facade command is not rejected with
  // E_VERSION_MISMATCH. Before this change, snapshot commands did not sync.
  it("an undo snapshot cursor syncs the facade version from the native engine", async () => {
    localStorage.setItem("photrez.historyBridge", "1"); // enable the snapshot bridge
    const facade = getFacade("docSnapSync");
    await seedFacadeFromEngine({ getId: () => "docSnapSync", getLayers: () => [layer], getName: () => "doc", getWidth: () => 800, getHeight: () => 600, getSelection: () => null } as never, facade);
    expect(getFacade("docSnapSync").renderedVersion).toBe(0); // seeded at version 0
    // Record a Snapshot entry (production always has one when restoring an undo).
    // This advances the native engine DV 0 -> 1.
    await invoke("rust_pixels_record_snapshot", {
      docId: "docSnapSync",
      before: { docId: "docSnapSync", version: 0, layers: [] },
      after: { docId: "docSnapSync", version: 0, layers: [] },
    });
    // Drive the production undo path. The snapshot cursor advances the engine DV,
    // but the returned payload's `version` field is the STORED snapshot version
    // (TS records version:0), NOT the engine DV. The facade must track the engine
    // DV (read from getSnapshot), not the snapshot's stored 0.
    await restoreSnapshotBitmapsByToken("undo", "docSnapSync", () => null, () => true);
    // The faithful mock returns the snapshot's STORED version (0), not the engine DV.
    const snapIdx = invokeMock.mock.calls.findIndex((c) => c[0] === "rust_pixels_undo_snapshot");
    expect(snapIdx).toBeGreaterThanOrEqual(0);
    const snapResult = (await (invokeMock.mock.results[snapIdx].value as Promise<{ version: number }>));
    expect(snapResult.version).toBe(0);
    // Engine DV after seed(0) + record(1) + undo(2) = 2, proving the sync reads
    // getSnapshot and not the snapshot's stored 0.
    expect(getFacade("docSnapSync").renderedVersion).toBe(2);
  });
});

describe("error path (native rejection normalizes to uniform CODE: message)", () => {
  beforeEach(() => {
    localStorage.setItem("photrez.facadeAuthority", "native");
    routeNative();
    // Arm wasm so the `!wasm` guards in getSnapshot/getHistoryQuery/
    // historyCursorCommit do not short-circuit before the native branch. The
    // native commands are dispatched first (isNativeAuthority wins), so the wasm
    // engine is only the fallback path here.
    setProtocolWasm(makeWasm({ documentVersion: 1, delta: { baseVersion: 0, version: 1, changes: [] } }));
  });

  it("a bare-string invoke rejection surfaces as CODE: message, not a resolved {ok:false}", async () => {
    const engineStub = { getId: () => "docG", getLayers: () => [layer], getName: () => "doc", getWidth: () => 800, getHeight: () => 600, getSelection: () => null };
    await seedFacadeFromEngine(engineStub as never, new EditorFacade(undefined, "docG"));
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "protocol_apply_command_native") {
        throw "E_VERSION_MISMATCH: expected version 2 got 1"; // bare Tauri v2 rejection
      }
      return undefined;
    });
    let caught: unknown;
    try {
      await applyCommand({
        contractVersion: CONTRACT_VERSION,
        expectedVersion: 1,
        docId: "docG",
        command: { type: "addLayer", id: "N-id", name: "N", width: 100, height: 100, index: 0 },
      });
    } catch (e) {
      caught = e;
    }
    expect(caught).toBeInstanceOf(Error);
    const msg = (caught as Error).message;
    expect(msg).toBe("E_VERSION_MISMATCH: expected version 2 got 1");
    // Same uniform shape the wasm envelope path produces via normalizeProtocolError.
    const wasmForm = normalizeProtocolError(
      new Error(JSON.stringify({ code: "E_VERSION_MISMATCH", message: "expected version 2 got 1" })),
    );
    expect(msg).toBe(wasmForm.message);
  });

  it("a malformed native result is surfaced as an Error, not an unhandled rejection", async () => {
    const engineStub = { getId: () => "docH", getLayers: () => [layer], getName: () => "doc", getWidth: () => 800, getHeight: () => 600, getSelection: () => null };
    await seedFacadeFromEngine(engineStub as never, new EditorFacade(undefined, "docH"));
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "protocol_apply_command_native") return "not-json{"; // invalid external input
      return undefined;
    });
    await expect(
      applyCommand({
        contractVersion: CONTRACT_VERSION,
        expectedVersion: 0,
        docId: "docH",
        command: { type: "addLayer", id: "N-id", name: "N", width: 100, height: 100, index: 0 },
      }),
    ).rejects.toBeInstanceOf(Error);
  });
});

describe("normalizeProtocolError", () => {
  it("normalizes a wasm JSON envelope to CODE: message", () => {
    const e = normalizeProtocolError(new Error(JSON.stringify({ code: "E_FOO", message: "bar" })));
    expect(e.message).toBe("E_FOO: bar");
  });
  it("passes a bare CODE: message string through unchanged", () => {
    const e = normalizeProtocolError("E_FOO: bar");
    expect(e.message).toBe("E_FOO: bar");
  });
});
