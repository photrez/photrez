// Document-scoped engine isolation proof (photrez per-document engine).
//
// PROBLEM: a single shared module-lifetime DocumentCore means every document
// id routes to the SAME engine, so docX and docY share documentVersion + layer
// set + undo/redo history. `protocol_apply_command(json, docId)` MUST route by
// document id so the two documents are fully independent.
//
// DISCRIMINATOR: this test drives the REAL wasm (via wasmTestShim -> the same
// .wasm bytes the pixel kernels ship). It asserts docX and docY produce
// independent, non-overlapping snapshots, and that undo/redo on docX leaves
// docY's documentVersion + layer set untouched. If the per-document routing is
// reverted to a single shared engine (or every command is sent to the reserved
// "default" engine), these assertions FAIL:
//   * getSnapshot("docY").layers.length === 1 would be 2 (both docs on one engine)
//   * undo/redo on docX would also bump docY's version/layers.
// This is the ONLY committed test that pins the per-document contract at the
// real-wasm boundary. Reverting protocol.rs to a shared engine keeps every other
// test green, so this file is the decisive guard.
//
// LOAD-ORDER: same pattern as the other direct-drive suites - capture the real
// wasm module once via getWasmExportModule() in beforeAll (it still returns the
// compiled module for the pixel kernels), then drive protocol_apply_command /
// protocol_snapshot_json directly with the production envelope serialization.
// afterEach resets docX/docY + "default" so this file is order-independent and
// does not leak engines into sibling test files.
//
// The wasmTestShim alias loads the REAL .wasm bytes, so this is the true
// production boundary (serde JSON in/out), not a fake.

import { describe, it, expect, beforeAll, beforeEach, afterEach } from "vitest";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import * as bridge from "@/lib/protocol/bridge";
import { __resetEmulatedForTests } from "@/lib/protocol/bridge";
import { CONTRACT_VERSION } from "../types";

type WasmModule = {
  protocol_apply_command: (json: string, docId: string) => string;
  protocol_snapshot_json: (docId: string) => string;
  protocol_reset: (docId: string) => void;
};

const DOC_X = "docX";
const DOC_Y = "docY";

let wasmModule: WasmModule | null = null;

beforeAll(async () => {
  // Capture the real module once. Production dispatch is native-only, so this
  // arming wires nothing - the module below is driven directly.
  const m = await getWasmExportModule();
  expect(m).not.toBeNull();
  expect(typeof m.protocol_apply_command).toBe("function");
  // The document-scoped engine contract requires a 2-arg protocol_apply_command
  // (json, docId). A stale 1-arg pkg (shared engine) would silently accept and
  // ignore docId, so this guard fails LOUDLY here.
  expect((m as WasmModule).protocol_apply_command.length).toBeGreaterThanOrEqual(2);
  wasmModule = m as WasmModule;
});

afterEach(() => {
  // Reset the TS emulator globals + the engines this file created so no state
  // leaks into sibling test files (regardless of interleaving).
  __resetEmulatedForTests();
  wasmModule?.protocol_reset(DOC_X);
  wasmModule?.protocol_reset(DOC_Y);
  wasmModule?.protocol_reset("default");
});

// The exact bytes the retired bridge wasm arm sent: toRustEnvelope
// serialization, protocol_apply_command JSON in/out, envelope-error
// normalization. Driving the module directly keeps this suite on the real
// engine without a bridge arm.
function rustApply(docId: string, command: any): { documentVersion: number } {
  try {
    const out = wasmModule!.protocol_apply_command(
      JSON.stringify(bridge.toRustEnvelope({ contractVersion: CONTRACT_VERSION, docId, command })),
      docId,
    );
    return JSON.parse(out) as { documentVersion: number };
  } catch (e) {
    throw bridge.normalizeProtocolError(e);
  }
}

function rustSnapshot(docId: string): { layers: Array<{ id: string; name: string }>; version: number } {
  return JSON.parse(wasmModule!.protocol_snapshot_json(docId)) as {
    layers: Array<{ id: string; name: string }>;
    version: number;
  };
}

async function addLayer(docId: string, name: string): Promise<{ documentVersion: number }> {
  return rustApply(docId, { type: "addLayer", id: `${name}-id`, name, width: 100, height: 100, index: 0 });
}

describe("document-scoped engine isolation on the REAL wasm", () => {
  it("addLayer to docX and docY produce independent non-overlapping snapshots", async () => {
    await addLayer(DOC_X, "X-North");

    const snapX = rustSnapshot(DOC_X);
    expect(snapX.layers.length).toBe(1);
    expect(snapX.layers[0].name).toBe("X-North");
    expect(snapX.version).toBeGreaterThan(0);

    await addLayer(DOC_Y, "Y-South");

    const snapX2 = rustSnapshot(DOC_X);
    const snapY = rustSnapshot(DOC_Y);
    expect(snapX2.layers.length).toBe(1); // docX unaffected by docY's add
    expect(snapX2.layers[0].name).toBe("X-North");
    expect(snapY.layers.length).toBe(1); // docY isolated from docX
    expect(snapY.layers[0].name).toBe("Y-South");

    // docX's layer set must NOT overlap docY's.
    const xIds = new Set(snapX2.layers.map((l) => l.id));
    expect(snapY.layers.some((l) => xIds.has(l.id))).toBe(false);
  });

  it("undo/redo on docX does NOT change docY's documentVersion or layers", async () => {
    await addLayer(DOC_X, "X-1");
    await addLayer(DOC_X, "X-2");
    const beforeY = rustSnapshot(DOC_Y); // untouched: v0, empty
    expect(beforeY.layers.length).toBe(0);
    expect(rustSnapshot(DOC_X).layers.length).toBe(2);

    const undo = rustApply(DOC_X, { type: "undo" });
    expect(undo.documentVersion).toBeGreaterThan(0);
    expect(rustSnapshot(DOC_X).layers.length).toBe(1);

    // docY completely unaffected by docX's undo.
    const afterYUndo = rustSnapshot(DOC_Y);
    expect(afterYUndo.version).toBe(beforeY.version);
    expect(afterYUndo.layers.length).toBe(beforeY.layers.length);

    const redo = rustApply(DOC_X, { type: "redo" });
    expect(redo.documentVersion).toBeGreaterThan(undo.documentVersion);
    expect(rustSnapshot(DOC_X).layers.length).toBe(2);

    const afterYRedo = rustSnapshot(DOC_Y);
    expect(afterYRedo.version).toBe(beforeY.version);
    expect(afterYRedo.layers.length).toBe(beforeY.layers.length);
  });
});
