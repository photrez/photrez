// Document-scoped engine isolation proof (photrez per-document engine).
//
// PROBLEM: a single shared module-lifetime ProtocolEngine means every document
// id routes to the SAME engine, so docX and docY share documentVersion + layer
// set + undo/redo history. `protocol_apply_command(json, docId)` MUST route by
// document id so the two documents are fully independent.
//
// DISCRIMINATOR: this test drives the REAL wasm (via wasmTestShim -> the same
// .wasm bytes the production getWasmExportModule arms). It asserts docX and docY
// produce independent, non-overlapping snapshots, and that undo/redo on docX
// leaves docY's documentVersion + layer set untouched. If the per-document
// routing is reverted to a single shared engine (or the bridge wires every
// command to the reserved "default" engine), these assertions FAIL:
//   * getSnapshot("docY").layers.length === 1 would be 2 (both docs on one engine)
//   * undo/redo on docX would also bump docY's version/layers.
// This is the ONLY committed test that pins the per-document contract at the
// real-wasm boundary. Reverting protocol.rs to a shared engine keeps every other
// test green, so this file is the decisive guard.
//
// LOAD-ORDER: same pattern as facadeRustBacked / facadeReadiness -- arm the
// bridge once via the production getWasmExportModule() in beforeAll, never call
// setProtocolWasm ourselves. afterEach resets docX/docY + "default" so this file
// is order-independent and does not leak engines into sibling test files.
//
// The wasmTestShim alias loads the REAL .wasm bytes, so this is the true
// production boundary (serde JSON in/out), not a fake.

import { describe, it, expect, beforeAll, afterEach } from "vitest";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import * as bridge from "@/lib/protocol/bridge";
import { __resetEmulatedForTests, resetWasmDoc } from "@/lib/protocol/bridge";
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
  // First getWasmExportModule() call in this file triggers the production
  // wiring (wasmExport.ts -> setProtocolWasm(mod)). Do nothing else here.
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
  resetWasmDoc(DOC_X);
  resetWasmDoc(DOC_Y);
  wasmModule?.protocol_reset("default");
});

async function addLayer(docId: string, name: string): Promise<{ documentVersion: number }> {
  return (await bridge.applyCommand({
    contractVersion: CONTRACT_VERSION,
    docId,
    command: { type: "addLayer", name },
  })) as unknown as { documentVersion: number };
}

describe("document-scoped engine isolation on the REAL wasm", () => {
  it("addLayer to docX and docY produce independent non-overlapping snapshots", async () => {
    await addLayer(DOC_X, "X-North");

    const snapX = await bridge.getSnapshot(DOC_X);
    expect(snapX.layers.length).toBe(1);
    expect(snapX.layers[0].name).toBe("X-North");
    expect(snapX.version).toBeGreaterThan(0);

    await addLayer(DOC_Y, "Y-South");

    const snapX2 = await bridge.getSnapshot(DOC_X);
    const snapY = await bridge.getSnapshot(DOC_Y);
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
    const beforeY = await bridge.getSnapshot(DOC_Y); // untouched: v0, empty
    expect(beforeY.layers.length).toBe(0);
    expect((await bridge.getSnapshot(DOC_X)).layers.length).toBe(2);

    const undo = await bridge.applyCommand({
      contractVersion: CONTRACT_VERSION,
      docId: DOC_X,
      command: { type: "undo" },
    }) as unknown as { documentVersion: number };
    expect(undo.documentVersion).toBeGreaterThan(0);
    expect((await bridge.getSnapshot(DOC_X)).layers.length).toBe(1);

    // docY completely unaffected by docX's undo.
    const afterYUndo = await bridge.getSnapshot(DOC_Y);
    expect(afterYUndo.version).toBe(beforeY.version);
    expect(afterYUndo.layers.length).toBe(beforeY.layers.length);

    const redo = await bridge.applyCommand({
      contractVersion: CONTRACT_VERSION,
      docId: DOC_X,
      command: { type: "redo" },
    }) as unknown as { documentVersion: number };
    expect(redo.documentVersion).toBeGreaterThan(undo.documentVersion);
    expect((await bridge.getSnapshot(DOC_X)).layers.length).toBe(2);

    const afterYRedo = await bridge.getSnapshot(DOC_Y);
    expect(afterYRedo.version).toBe(beforeY.version);
    expect(afterYRedo.layers.length).toBe(beforeY.layers.length);
  });
});
