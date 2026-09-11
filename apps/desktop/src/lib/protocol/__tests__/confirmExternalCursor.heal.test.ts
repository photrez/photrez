// Wiring test for the external-handoff heal contract.
//
// After an external-handoff undo (the walker lands on a legacy/external entry and
// confirmExternalCursor clears the pending-external barrier), the native engine's
// doc_size can stay stale relative to the restored TS state. The heal path is
// intentionally NOT inside confirmExternalCursor: the canonical re-push now runs
// AFTER the legacy TS restore completes (useEditorCommands.restoreHistorySnapshot,
// handoff-fallthrough branch), so the re-pushed payload reflects the post-restore
// state. This test pins the INVERTED contract of confirmExternalCursor itself: it
// clears the barrier and must NOT fire the re-push, regardless of native authority
// or whether an engine is supplied. The actual re-push is covered by the
// restore-history wiring test.

import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  createNativeSeed,
  __resetNativeAuthorityForTests,
  __resetEmulatedForTests,
} from "../bridge";
import {
  getFacade,
  confirmExternalCursor,
  recordExternalTransitionFor,
  __resetFacadeRegistryForTests,
} from "../facadeRegistry";
import * as canonicalSeed from "../canonicalSeed";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as Mock<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>;

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
      // Order/routing-contract harness only: this stub does NOT apply the pushed
      // layer vector - the push-applies contract is pinned Rust-side (canonical seed
      // and reorder test modules).
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
        if (env.command.type === "recordExternalTransition")
          return JSON.stringify({ documentVersion: 1, delta: { baseVersion: 0, version: 1, changes: [] }, status: "external-recorded", externalSeq: 1 });
        if (env.command.type === "undo")
          return JSON.stringify({ documentVersion: 2, delta: { baseVersion: 1, version: 2, changes: [] }, status: "external", externalSeq: 1 });
        return JSON.stringify({ documentVersion: 2, delta: { baseVersion: 1, version: 2, changes: [] }, status: "ok" });
      }
      case "protocol_history_cursor_commit_native":
        return JSON.stringify({ documentVersion: 2, delta: { baseVersion: 1, version: 2, changes: [] }, status: "external-confirmed", externalSeq: 1 });
      default:
        throw `E_UNKNOWN_COMMAND: ${cmd}`;
    }
  });
}

describe("external-handoff confirmExternalCursor must NOT re-push (heal moved to restore path)", () => {
  beforeEach(() => {
    localStorage.clear();
    invokeMock.mockReset();
    __resetNativeAuthorityForTests();
    __resetFacadeRegistryForTests();
    routeNative();
  });
  afterEach(() => {
    localStorage.clear();
    __resetNativeAuthorityForTests();
    __resetFacadeRegistryForTests();
    vi.restoreAllMocks();
  });

  it("native-authority path: clears the barrier but does NOT fire a canonical re-push", async () => {
    localStorage.setItem("photrez.facadeAuthority", "native");
    const docId = "docHeal";
    await createNativeSeed(docId, 0, []);

    // Build an external (legacy) entry, then undo to land on it and set the barrier.
    await recordExternalTransitionFor(docId, { label: "Legacy", affectedLayerIds: [], snapshot: null });
    const facade = getFacade(docId);
    await facade.undo();
    expect(facade.lastExternalHandoff).not.toBeNull();
    const seq = facade.lastExternalHandoff!.seq;

    const repushSpy = vi
      .spyOn(canonicalSeed, "repushCanonicalDocument")
      .mockResolvedValue(undefined as never);

    // No engine argument: confirmExternalCursor no longer takes one.
    const res = await confirmExternalCursor(docId, seq, "undo");

    expect(res.ok).toBe(true);
    expect(repushSpy).not.toHaveBeenCalled();
  });

  it("flag-off path: also no re-push (symmetry — re-push is on the restore path, not here)", async () => {
    localStorage.clear(); // photrez.facadeAuthority unset => isNativeAuthority() false
    const docId = "docHealOff";

    await recordExternalTransitionFor(docId, { label: "Legacy", affectedLayerIds: [], snapshot: null });
    const facade = getFacade(docId);
    await facade.undo();
    expect(facade.lastExternalHandoff).not.toBeNull();
    const seq = facade.lastExternalHandoff!.seq;

    const repushSpy = vi
      .spyOn(canonicalSeed, "repushCanonicalDocument")
      .mockResolvedValue(undefined as never);

    const res = await confirmExternalCursor(docId, seq, "undo");

    expect(res.ok).toBe(true);
    expect(repushSpy).not.toHaveBeenCalled();
  });
});
