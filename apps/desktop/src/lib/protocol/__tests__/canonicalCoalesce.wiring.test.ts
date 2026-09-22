// Wiring tests for the coalesced canonical re-push slot.
//
// Two rapid re-pushes with a byte-identical payload must produce exactly ONE
// `protocol_seed_canonical_native` invoke (payload-compare guard skips the
// duplicate invoke + slot write), while the next command still observes the
// complete shadow (flushExternalTransitions awaits the slot) and the
// external-mirror barrier keeps chaining (version bumps never coalesce).
//
// Mock fidelity: mirrors the canonicalSeedBarrier harness - Tauri v2 `invoke()`
// rejects a Rust Err(String) with the bare string; the native routes below
// resolve the same way production does.

import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  canonicalPendingByDoc,
  flushExternalTransitions,
  setExternalTransitionPending,
  __resetNativeAuthorityForTests,
} from "../bridge";
import { repushCanonicalDocument, __resetCanonicalRepushForTests } from "../canonicalSeed";
import { WorkspaceManager } from "@/engine/workspace";

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
const tick = () => new Promise<void>((r) => setTimeout(r, 0));
const seedInvokeCount = () =>
  invokeMock.mock.calls.filter((c) => c[0] === "protocol_seed_canonical_native").length;

// Routes every native command this test exercises. The open + layer seed +
// canonical seed + apply commands succeed; individual tests override entries.
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
      case "protocol_apply_command_native":
        return JSON.stringify({ documentVersion: 1, delta: { baseVersion: 0, version: 1, changes: [] }, status: "ok" });
      default:
        throw `E_UNKNOWN_COMMAND: ${cmd}`;
    }
  });
}

// Opens a real blank document (open-path layer + canonical seeds) and isolates
// the re-push traffic under test.
async function openDoc(docId: string) {
  const wm = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument(docId, "D", 800, 600);
  wm.addDocument(session);
  await flush();
  invokeMock.mockClear();
  return session;
}

describe("coalesced canonical re-push slot", () => {
  beforeEach(() => {
    localStorage.clear();
    invokeMock.mockReset();
    __resetNativeAuthorityForTests();
    __resetCanonicalRepushForTests();
    localStorage.setItem("photrez.facadeAuthority", "native");
    routeNative();
  });
  afterEach(() => {
    localStorage.clear();
    __resetNativeAuthorityForTests();
    __resetCanonicalRepushForTests();
    vi.restoreAllMocks();
  });

  it("two rapid identical re-pushes produce exactly ONE canonical seed invoke", async () => {
    const docId = "docCoalesce";
    const session = await openDoc(docId);
    const engine = session.engine as never;

    // No mutation between the calls: both build a byte-identical payload.
    const first = repushCanonicalDocument(docId, engine);
    const second = repushCanonicalDocument(docId, engine);
    await Promise.all([first, second]);

    expect(seedInvokeCount()).toBe(1);
  });

  it("flush awaits the coalesced slot before the next command dispatches", async () => {
    const docId = "docCoalesceFlush";
    const session = await openDoc(docId);
    const engine = session.engine as never;

    // Gate the seed invoke so the test can observe the flush waiting on it.
    let release!: () => void;
    const gate = new Promise<void>((r) => {
      release = r;
    });
    const realImpl = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
      if (cmd === "protocol_seed_canonical_native") await gate;
      return realImpl(cmd, args);
    });

    const repush = repushCanonicalDocument(docId, engine);
    await tick();
    await tick();

    let flushed = false;
    const flushedP = flushExternalTransitions(docId).then(() => {
      flushed = true;
    });
    await tick();
    await tick();
    // The slot holds the gated push, so the flush must still be waiting.
    expect(flushed).toBe(false);

    release();
    await flushedP;
    await repush;
    expect(flushed).toBe(true);
    // The flush consumed the slot; nothing stale remains for the next command.
    expect(canonicalPendingByDoc.has(docId)).toBe(false);
    expect(seedInvokeCount()).toBe(1);
  });

  it("rejected push still resolves flush and an identical repush retries", async () => {
    const docId = "docCoalesceReject";
    const session = await openDoc(docId);
    const engine = session.engine as never;

    // Only the first re-push invoke rejects (real Tauri rejects a Rust
    // Err(String) with the bare string); the retry must invoke again.
    let failOnce = true;
    const realImpl = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
      if (cmd === "protocol_seed_canonical_native" && failOnce) {
        failOnce = false;
        throw "E_CANON_PARSE: forced repush failure";
      }
      return realImpl(cmd, args);
    });

    await expect(repushCanonicalDocument(docId, engine)).rejects.toBe(
      "E_CANON_PARSE: forced repush failure",
    );
    // The slot holds a guarded copy, so the flush still resolves and drains.
    await expect(flushExternalTransitions(docId)).resolves.toBeUndefined();

    // The failure evicted the payload record, so the identical re-push is not
    // skipped as a duplicate: it re-invokes instead of skipping forever.
    const before = seedInvokeCount();
    await repushCanonicalDocument(docId, engine);
    expect(seedInvokeCount()).toBe(before + 1);
  });

  it("rapid distinct payloads invoke twice and the slot keeps the latest", async () => {
    const docId = "docCoalesceDistinct";
    const session = await openDoc(docId);
    const engine = session.engine as unknown as { addLayer(name: string): void };

    // Gate ONLY the first canonical invoke: if the slot kept the first push,
    // the flush below would wedge on the gate; overwrite lets it drain on the
    // ungated second push while the gate is still closed.
    let release!: () => void;
    const gate = new Promise<void>((r) => {
      release = r;
    });
    let seedCalls = 0;
    const realImpl = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
      if (cmd === "protocol_seed_canonical_native") {
        seedCalls += 1;
        if (seedCalls === 1) await gate;
      }
      return realImpl(cmd, args);
    });

    // Real mutation between the calls: the second payload is genuinely newer.
    const first = repushCanonicalDocument(docId, engine as never);
    engine.addLayer("Extra");
    const second = repushCanonicalDocument(docId, engine as never);
    await tick();
    await tick();
    await tick();

    let flushed = false;
    const flushedP = flushExternalTransitions(docId).then(() => {
      flushed = true;
    });
    await tick();
    await tick();
    await tick();
    // The slot holds the latest (ungated) push, so the flush drains now.
    expect(flushed).toBe(true);

    release();
    await flushedP;
    await Promise.all([first, second]);
    expect(seedInvokeCount()).toBe(2);
    expect(canonicalPendingByDoc.has(docId)).toBe(false);
  });

  it("external-mirror entries still chain and never touch the canonical slot", async () => {
    const docId = "docMirrorChain";
    await openDoc(docId);

    const order: string[] = [];
    setExternalTransitionPending(docId, (async () => { await tick(); order.push("m1"); })());
    setExternalTransitionPending(docId, (async () => { await tick(); order.push("m2"); })());
    await flushExternalTransitions(docId);

    // Both mirrors ran in registration order: chaining preserved, no coalescing.
    expect(order).toEqual(["m1", "m2"]);
    expect(canonicalPendingByDoc.has(docId)).toBe(false);
  });
});
