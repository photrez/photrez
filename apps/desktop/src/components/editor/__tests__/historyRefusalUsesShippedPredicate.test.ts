// The undo/redo REFUSAL, driven through the shipped predicate.
//
// The host refuses an undo/redo that has no user work behind it. It reads the
// shared stream's cursor through `getHistoryQuery` and compares it to a
// one-entry document-open baseline floor. The undo button is enabled
// unconditionally under facade ownership, so the refusal is the ONLY thing
// standing between an extra keypress and undoing the document-open entry -
// which is why it must be exercised against the real function.
//
// This test calls `rustStreamHoldsUserWork` itself. A test-local copy of
// `cursor > floor` would keep passing even if the shipped predicate changed, so
// it is deliberately not done here.
//
// What it proves about the host-owned document-size pair (the Phase 1 change):
// a crop's External step is a REAL handled step, so it must not be swallowed,
// and undoing down past every user entry must end in a refusal - not in a silent
// undo of the baseline.

import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { createNativeSeed, __resetNativeAuthorityForTests } from "@/lib/protocol/bridge";
import {
  getFacade,
  confirmExternalCursor,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";
import { recordOpenBaselineEntry } from "@/lib/protocol/backgroundFlagRouting";
import { rustStreamHoldsUserWork } from "../facadeHistoryHandoff";
import type { EditorContextValue } from "../shell/EditorContext";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as Mock<
  (cmd: string, args?: Record<string, unknown>) => Promise<unknown>
>;

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

/**
 * A stream whose cursor the refusal reads back, driven by a real
 * `RecordExternalTransition` / `Undo` / `CursorCommit` sequence. The cursor is
 * moved by the CursorCommit, exactly as the production two-phase handoff does,
 * so the value the refusal sees is produced by the same ordering the app uses.
 */
function routeStream(cursorAt: number[]): { cursors: number[] } {
  const cursors = cursorAt;
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
        if (env.command.type === "recordExternalTransition") {
          cursors.push(cursors[cursors.length - 1] + 1);
          return JSON.stringify({
            documentVersion: cursors.length,
            delta: { baseVersion: 0, version: cursors.length, changes: [] },
            status: "external-recorded",
            externalSeq: cursors.length,
          });
        }
        if (env.command.type === "undo" || env.command.type === "redo") {
          return JSON.stringify({
            documentVersion: cursors.length,
            delta: { baseVersion: 0, version: cursors.length, changes: [] },
            status: "external",
            externalSeq: cursors.length,
          });
        }
        return JSON.stringify({
          documentVersion: cursors.length,
          delta: { baseVersion: 0, version: cursors.length, changes: [] },
          status: "ok",
        });
      }
      case "protocol_history_cursor_commit_native": {
        const seq = Number(args.seq ?? 0);
        const direction = String(args.direction ?? "undo");
        const idx = cursors.indexOf(seq);
        if (idx < 0) throw "E_CURSOR_MISMATCH";
        if (direction === "undo") cursors.splice(idx, 1);
        else cursors.splice(idx + 1, 0, seq);
        return JSON.stringify({
          documentVersion: cursors.length,
          delta: { baseVersion: 0, version: cursors.length, changes: [] },
          status: "external-confirmed",
          externalSeq: seq,
        });
      }
      case "protocol_history_query_native":
        return JSON.stringify({
          cursor: cursors.length,
          lastSeq: cursors.length,
          degradedHint: false,
          pendingExternal: null,
          entries: cursors.map((s) => ({ seq: s, groupId: s, origin: "native", label: "e", affectedLayerIds: [], versionBefore: 0, versionAfter: 1, memoryCostBytes: 0 })),
        });
      default:
        throw `E_UNKNOWN_COMMAND: ${cmd}`;
    }
  });
  return { cursors };
}

function editorFor(docId: string): EditorContextValue {
  return {
    workspace: {
      getActiveDocumentId: () => docId,
      getActiveEngine: () => null,
    },
  } as unknown as EditorContextValue;
}

describe("undo/redo refusal: the shipped predicate, not a reimplementation", () => {
  beforeEach(() => {
    localStorage.clear();
    invokeMock.mockReset();
    __resetNativeAuthorityForTests();
    __resetFacadeRegistryForTests();
    localStorage.setItem("photrez.facadeAuthority", "native");
  });
  afterEach(() => {
    localStorage.clear();
    __resetNativeAuthorityForTests();
    __resetFacadeRegistryForTests();
    vi.restoreAllMocks();
  });

  it("refuses the press that would undo the document-open baseline", async () => {
    const docId = "docRefuse";
    // One entry: the document-open baseline. Nothing the user did.
    routeStream([1]);
    await createNativeSeed(docId, 0, []);
    recordOpenBaselineEntry(docId);

    const editor = editorFor(docId);
    expect(await rustStreamHoldsUserWork(editor, "undo")).toBe(false);
  });

  it("allows the press while a user entry sits below the cursor", async () => {
    const docId = "docAllow";
    // Baseline + one user entry (a crop).
    routeStream([1, 2]);
    await createNativeSeed(docId, 0, []);
    recordOpenBaselineEntry(docId);

    const editor = editorFor(docId);
    expect(await rustStreamHoldsUserWork(editor, "undo")).toBe(true);
  });

  it("walks a crop stream down to the floor, and the last press is refused", async () => {
    const docId = "docWalk";
    // Baseline + two user entries (crop, then crop again).
    const { cursors } = routeStream([1, 2, 3]);
    await createNativeSeed(docId, 0, []);
    recordOpenBaselineEntry(docId);
    const editor = editorFor(docId);

    expect(await rustStreamHoldsUserWork(editor, "undo")).toBe(true);

    // Undo the topmost entry through the real two-phase handoff: the walker
    // steps the cursor only once the host CONFIRMS, which is what the shipped
    // `runFacadeExternalHandoff` does.
    const facade = getFacade(docId);
    await facade.undo();
    expect(facade.lastExternalHandoff).not.toBeNull();
    const first = confirmExternalCursor(
      docId,
      facade.lastExternalHandoff!.seq,
      facade.lastExternalHandoff!.direction,
    );
    expect((await first).ok).toBe(true);
    expect(cursors.length).toBe(2);
    expect(await rustStreamHoldsUserWork(editor, "undo")).toBe(true);

    // Undo the second entry: only the baseline is left.
    await facade.undo();
    expect(facade.lastExternalHandoff).not.toBeNull();
    const second = confirmExternalCursor(
      docId,
      facade.lastExternalHandoff!.seq,
      facade.lastExternalHandoff!.direction,
    );
    expect((await second).ok).toBe(true);
    expect(cursors.length).toBe(1);
    // The press past the last user entry must be REFUSED: the engine would otherwise
    // undo the document-open entry.
    expect(await rustStreamHoldsUserWork(editor, "undo")).toBe(false);

    // Undo stays refused on a further press: the engine's cursor is already at the
    // floor, so no amount of pressing invents user work. This is the property a
    // frozen cursor would break, and it is why the refusal must read the real
    // cursor rather than a cached count.
    expect(await rustStreamHoldsUserWork(editor, "undo")).toBe(false);
  });

  it("assumes work when the baseline cannot be proven (never refuses wrongly)", async () => {
    const docId = "docNoBaseline";
    routeStream([1]);
    await createNativeSeed(docId, 0, []);
    // deliberately NOT recorded: exercises the "floor is 0" branch

    // floor 0 with cursor 1 -> one entry below, which IS user work here.
    expect(await rustStreamHoldsUserWork(editorFor(docId), "undo")).toBe(true);
  });
});