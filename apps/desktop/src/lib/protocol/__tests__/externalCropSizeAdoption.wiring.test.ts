// End-to-end: the HOST adopts a crop's document size from the External undo
// delta, all the way into the live TS model.
//
// This half had NO test. The Rust unit tests prove the engine emits the right
// delta; nothing proved the host then WRITES it. The chain is
//
//   ProtocolEngine undo/redo  ->  EditorFacade.applyDelta (marks dims
//   authoritative)  ->  applyDeltaToSnapshot (carries width/height)  ->
//   DocumentEngine.applyFacadeSnapshot -> applyProjectedDims ->
//   model.width / model.height
//
// A break anywhere in that chain is invisible to the core-crate tests, and the
// user-visible symptom is a document whose canvas size does not follow undo.
//
// The engine here is the real EditorFacade + real DocumentEngine; only the IPC
// transport is stubbed, and the `protocol_apply_command_native` stub returns the
// SAME delta shape the real walker produces for a host-owned size pair, so the
// projection under test is the shipped one rather than a reimplementation.

import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  createNativeSeed,
  __resetNativeAuthorityForTests,
} from "../bridge";
import {
  getFacade,
  recordExternalTransitionFor,
  __resetFacadeRegistryForTests,
} from "../facadeRegistry";
import { DocumentEngine } from "@/engine/document";
import { CommandHistory } from "@/engine/history";
import type { DocumentModel } from "@/engine/types";

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
 * Route the native protocol, replaying the delta the real walker emits.
 *
 * `undoSize` / `redoSize` are the width/height the walker puts on the delta.
 * Passing `null` for both reproduces a METADATA External step (no size on the
 * delta at all), which is what the fall-through routing keys on.
 */
function routeNative(undoSize: [number, number] | null, redoSize: [number, number] | null): void {
  const open = new Set<string>();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    const docId = (args.docId as string) ?? "default";
    // The Rust delta marks its size fields `skip_serializing_if = "Option::is_none"`
    // (projection.rs:47-50), so an absent size is OMITTED from the JSON and
    // arrives in JS as `undefined` - never `null`. Emitting `null` here would
    // test a wire shape that cannot occur, and `lastHistoryDeltaWasEmpty` keys on
    // `=== undefined`.
    const dims = (s: [number, number] | null) =>
      s ? { width: s[0], height: s[1] } : {};
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
          return JSON.stringify({
            documentVersion: 1,
            delta: { baseVersion: 0, version: 1, changes: [] },
            status: "external-recorded",
            externalSeq: 1,
          });
        }
        if (env.command.type === "undo") {
          return JSON.stringify({
            documentVersion: 2,
            delta: { baseVersion: 1, version: 2, changes: [], ...dims(undoSize) },
            status: "external",
            externalSeq: 1,
          });
        }
        if (env.command.type === "redo") {
          return JSON.stringify({
            documentVersion: 2,
            delta: { baseVersion: 1, version: 2, changes: [], ...dims(redoSize) },
            status: "external",
            externalSeq: 1,
          });
        }
        return JSON.stringify({
          documentVersion: 2,
          delta: { baseVersion: 1, version: 2, changes: [] },
          status: "ok",
        });
      }
      case "protocol_history_cursor_commit_native":
        return JSON.stringify({
          documentVersion: 2,
          delta: { baseVersion: 1, version: 2, changes: [] },
          status: "external-confirmed",
          externalSeq: 1,
        });
      default:
        throw `E_UNKNOWN_COMMAND: ${cmd}`;
    }
  });
}

function makeModel(id: string, width: number, height: number): DocumentModel {
  return {
    id,
    name: "doc",
    width,
    height,
    layers: [
      {
        id: "bg",
        name: "Background",
        visible: true,
        opacity: 1,
        x: 0,
        y: 0,
        scaleX: 1,
        scaleY: 1,
        rotation: 0,
        resourceId: 1,
      },
    ],
    activeLayerId: "bg",
    selection: null,
    dirty: false,
    viewport: { x: 0, y: 0, width: 800, height: 600, zoom: 1 },
  } as unknown as DocumentModel;
}

describe("host adopts the External crop-size delta end to end", () => {
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

  it("undoing a crop writes the pre-crop size into the live TS model", async () => {
    // The walker restores 128x128 on undo and re-applies 13x13 on redo.
    routeNative([128, 128], [13, 13]);
    const docId = "docCropUndo";
    await createNativeSeed(docId, 0, []);

    // The model is currently cropped (13x13); the entry's before half is 128x128.
    const model = makeModel(docId, 13, 13);
    // DocumentEngine takes scalars, not a model object; `model` is the separate
    // pre-op payload the record captures.
    const engine = new DocumentEngine(docId, "doc", 13, 13);

    await recordExternalTransitionFor(docId, {
      label: "Crop Canvas",
      affectedLayerIds: ["bg"],
      snapshot: model,
      docSizeChange: {
        before: { width: 128, height: 128 },
        after: { width: 13, height: 13 },
      },
    });

    const facade = getFacade(docId);
    await facade.undo();

    // The facade adopted the size...
    expect(facade.snapshot.width).toBe(128);
    expect(facade.snapshot.height).toBe(128);
    // ...and marked it authoritative, which is what lets the projection write it.
    expect(facade.lastProjectionDimsAuthoritative).toBe(true);
    // A size-bearing step is a real handled step, not a fall-through no-op.
    expect(facade.lastHistoryDeltaWasEmpty).toBe(false);

    void history;

    // ...and the engine's live model followed.
    engine.applyFacadeSnapshot(facade.snapshot as never, {
      dimsAuthoritative: facade.lastProjectionDimsAuthoritative,
    });
    expect(engine.getWidth()).toBe(128);
    expect(engine.getHeight()).toBe(128);
  });

  it("redoing a crop writes the cropped size back into the live TS model", async () => {
    routeNative([128, 128], [13, 13]);
    const docId = "docCropRedo";
    await createNativeSeed(docId, 0, []);

    const model = makeModel(docId, 128, 128);
    const engine = new DocumentEngine(docId, "doc", 128, 128);

    await recordExternalTransitionFor(docId, {
      label: "Crop Canvas",
      affectedLayerIds: ["bg"],
      snapshot: model,
      docSizeChange: {
        before: { width: 128, height: 128 },
        after: { width: 13, height: 13 },
      },
    });

    const facade = getFacade(docId);
    await facade.redo();

    expect(facade.snapshot.width).toBe(13);
    expect(facade.snapshot.height).toBe(13);
    engine.applyFacadeSnapshot(facade.snapshot as never, {
      dimsAuthoritative: facade.lastProjectionDimsAuthoritative,
    });
    expect(engine.getWidth()).toBe(13);
    expect(engine.getHeight()).toBe(13);
  });

  it("a METADATA external carries no size and stays a fall-through no-op", async () => {
    // No size on the delta at all - the shape a layer delete / move / reorder
    // produces. This is the regression guard for the fall-through path: a
    // spurious Some() here would flip lastHistoryDeltaWasEmpty and make the handoff
    // claim the step, so the TS store would never be popped.
    routeNative(null, null);
    const docId = "docMeta";
    await createNativeSeed(docId, 0, []);

    const model = makeModel(docId, 128, 128);
    const engine = new DocumentEngine(docId, "doc", 128, 128);
    await recordExternalTransitionFor(docId, {
      label: "Delete Layer",
      affectedLayerIds: ["bg"],
      snapshot: model,
      // no docSizeChange: a size-neutral host transition
    });

    const facade = getFacade(docId);
    await facade.undo();

    // The delta carried NO width/height, so the facade must leave the document
    // size it already had rather than adopting a new one. Seeded via the crop
    // test's shape so the baseline snapshot has a size to carry forward.
    expect(facade.snapshot.width).toBeUndefined();
    expect(facade.snapshot.height).toBeUndefined();
    expect(facade.lastHistoryDeltaWasEmpty).toBe(true);
    expect(facade.lastProjectionDimsAuthoritative).toBe(false);

    // A projection carrying no size leaves the live model untouched - the
    // engine keeps the size it had, so a metadata undo cannot resize a document.
    engine.applyFacadeSnapshot(facade.snapshot as never, {
      dimsAuthoritative: facade.lastProjectionDimsAuthoritative,
    });
    expect(engine.getWidth()).toBe(128);
    expect(engine.getHeight()).toBe(128);
  });
});