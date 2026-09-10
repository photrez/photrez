// Wiring tests for the per-doc canonical-push barrier (HOLE-2 fix).
//
// Two production gaps: (a) the canonical OPEN-seed was not awaited by
// applyCommand's seed barrier (a facade command could apply natively before the
// canonical open-seed landed and overwrite doc_size with open-time dims), and (b)
// the two production re-pushes (repushCanonicalDocument) were fire-and-forget and
// not registered in the external-transition barrier, so a re-push for command 1
// could land after command 2 applied (unordered doc_size). Fix: route ALL
// canonical pushes through one per-doc barrier — awaitNativeSeed awaits BOTH the
// layer seed and the canonical seed, and repushCanonicalDocument registers its
// invoke in the external-transition barrier that flushExternalTransitions (called
// by syncFromEngine before every command) awaits.
//
// Mock fidelity: Tauri v2 `invoke()` rejects a Rust Err(String) with the bare
// string; the failing-re-push test rejects that way so it mirrors real Tauri.

import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  applyCommand,
  createNativeSeed,
  flushExternalTransitions,
  nativeCanonicalSeedPromiseByDoc,
  __resetNativeAuthorityForTests,
} from "../bridge";
import { getFacade, seedFacadeFromEngine } from "../facadeRegistry";
import { repushCanonicalDocument } from "../canonicalSeed";
import { WorkspaceManager } from "@/engine/workspace";
import { CONTRACT_VERSION } from "../types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as Mock<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>;

// Self-contained localStorage so this test runs in the node/jsdom project.
if (typeof (globalThis as { localStorage?: unknown }).localStorage === "undefined") {
  const __ls = new Map<string, string>();
  (globalThis as { localStorage: Storage }).localStorage = {
    getItem: (k: string) => (__ls.has(k) ? (__ls.get(k) as string) : null),
    setItem: (k: string, v: string) => {
      __ls.set(k, String(v));
    },
    removeItem: (k: string) => {
      __ls.delete(k);
    },
    clear: () => __ls.clear(),
    key: (i: number) => Array.from(__ls.keys())[i] ?? null,
    get length() {
      return __ls.size;
    },
  } as Storage;
}

const flush = () => new Promise<void>((r) => setTimeout(r, 0));

// Routes every native command this test exercises. The canonical-seed + apply
// commands succeed; the re-push test overrides only the seed to reject.
function routeNative(): void {
  const open = new Set<string>();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    const docId = (args.docId as string) ?? "default";
    switch (cmd) {
      case "rust_pixels_open_document":
        open.add(docId);
        return undefined;
      case "protocol_seed_native":
        if (!open.has(docId)) throw `document not open: ${docId}`;
        return JSON.stringify({ version: 0, layers: [] });
      case "protocol_seed_canonical_native":
        if (!open.has(docId)) throw `document not open: ${docId}`;
        return null;
      case "protocol_snapshot_native":
        return JSON.stringify({ version: 0, layers: [] });
      case "protocol_version_native":
        return 0;
      case "protocol_register_adapter_native":
        return null;
      case "protocol_apply_command_native": {
        const env = JSON.parse((args.envelopeJson as string) ?? "{}") as {
          command: { type: string };
        };
        if (env.command.type === "addLayer") {
          const layer = { id: "layer-minted", visible: true, opacity: 1, resourceId: 0, x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0 };
          return JSON.stringify({ documentVersion: 1, delta: { baseVersion: 0, version: 1, changes: [{ kind: "upsert", layer } as never] }, status: "ok" });
        }
        return JSON.stringify({ documentVersion: 1, delta: { baseVersion: 0, version: 1, changes: [] }, status: "ok" });
      }
      default:
        throw `E_UNKNOWN_COMMAND: ${cmd}`;
    }
  });
}

const engineStub = {
  getId: () => "docWedge",
  getName: () => "D",
  getWidth: () => 800,
  getHeight: () => 600,
  getLayers: () => [{ id: "bg" }],
  getSelection: () => null,
};

describe("canonical push barrier ordering", () => {
  beforeEach(() => {
    localStorage.clear();
    invokeMock.mockReset();
    __resetNativeAuthorityForTests();
    localStorage.setItem("photrez.facadeAuthority", "native");
    routeNative();
  });
  afterEach(() => {
    localStorage.clear();
    __resetNativeAuthorityForTests();
    vi.restoreAllMocks();
  });

  it("(a) applyCommand awaits the canonical open-seed before applying natively", async () => {
    const docId = "docBarrier";
    await createNativeSeed(docId, 0, []); // layer seed (resolves after a microtask)

    // A canonical-seed promise that fires its native invoke only when released.
    let release!: () => void;
    const trigger = new Promise<void>((r) => {
      release = r;
    });
    const seedP = trigger.then((): Promise<void> =>
      invoke("protocol_seed_canonical_native", { payloadJson: "{}", docId }) as Promise<void>,
    );
    nativeCanonicalSeedPromiseByDoc.set(docId, seedP);

    const cmd = {
      contractVersion: CONTRACT_VERSION,
      expectedVersion: undefined,
      docId,
      command: { type: "noop" },
    };
    const applyP = applyCommand(cmd as never);

    // Release the canonical seed: it invokes, then applyCommand proceeds.
    release();
    await applyP;

    const cmds = invokeMock.mock.calls.map((c) => c[0]);
    const idxSeed = cmds.indexOf("protocol_seed_canonical_native");
    const idxApply = cmds.indexOf("protocol_apply_command_native");
    expect(idxSeed).toBeGreaterThanOrEqual(0);
    expect(idxApply).toBeGreaterThanOrEqual(0);
    // The canonical open-seed must land BEFORE the native apply command on the wire.
    expect(idxSeed).toBeLessThan(idxApply);
  });

  it("(b) a second facade command flushes a pending re-push before dispatch", async () => {
    const docId = "docRepush";
    const wm = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument(docId, "D", 800, 600);
    wm.addDocument(session);
    await flush(); // open + layer seed + baseline canonical seed (fire-and-forget)
    invokeMock.mockClear(); // isolate the addLayer re-push traffic

    const facade = getFacade(docId);
    await seedFacadeFromEngine(session.engine as never, facade);
    await facade.addLayer("Layer 1");
    await flush(); // first addLayer + its barrier-registered re-push

    // Second addLayer: syncFromEngine -> flushExternalTransitions awaits the
    // pending re-push before the apply command dispatches.
    await facade.addLayer("Layer 2");
    await flush();

    const cmds = invokeMock.mock.calls.map((c) => c[0]);
    const applyIdxs = cmds
      .map((c, i) => (c === "protocol_apply_command_native" ? i : -1))
      .filter((i) => i >= 0);
    expect(applyIdxs.length).toBeGreaterThanOrEqual(2);
    const secondApply = applyIdxs[1];
    // The first addLayer's re-push seed must precede the second apply on the wire.
    expect(cmds.slice(0, secondApply).includes("protocol_seed_canonical_native")).toBe(true);
  });

  it("(c) a rejected re-push invoke does not wedge the external-transition barrier", async () => {
    const docId = "docWedge";
    await createNativeSeed(docId, 0, []);
    // Only the re-push's canonical seed rejects (real Tauri rejects with a bare string).
    const realImpl = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
      if (cmd === "protocol_seed_canonical_native") throw "E_CANON_PARSE: forced repush failure";
      return realImpl(cmd, args);
    });

    // Fire a re-push: its invoke rejects, but the barrier (setExternalTransitionPending)
    // swallows the rejection via a .catch, so flushExternalTransitions must still RESOLVE
    // and clear the pending re-push rather than wedging. Drain the re-push promise so no
    // unhandled rejection escapes the test.
    void repushCanonicalDocument(docId, engineStub as never).catch(() => {});

    // flushExternalTransitions must RESOLVE (the barrier swallows the rejection) ...
    await expect(flushExternalTransitions(docId)).resolves.toBeUndefined();
    // ... and a second flush is a harmless no-op (barrier cleared, not stuck on the rejection).
    await expect(flushExternalTransitions(docId)).resolves.toBeUndefined();
  });
});
