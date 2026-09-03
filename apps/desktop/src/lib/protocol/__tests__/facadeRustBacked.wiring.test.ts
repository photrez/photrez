// Structural-sharing wiring proof: the facade protocol is Rust-backed AFTER the wasm loads.
//
// getWasmExportModule() wires the bridge to the REAL Rust engine:
//   wasmExport.ts (getWasmExportModule) -> setProtocolWasm(mod)
//   bridge.applyCommand -> wasm.protocol_apply_command (serde JSON in/out)
// Until that load (or in a non-wasm env) the bridge falls back to the TS
// emulator (emulateApply). The facade becomes Rust-backed only once the wasm
// module is wired; the load-order robustness is a FLAG-ON acceptance criterion
// (see docs/AI_CURRENT_TASK.md FLAG-ON WASM-WIRING READINESS checklist).
//
// WHY THIS FILE IS DISCRIMINATING (vs the OLD tautological assertions):
// The previous version asserted `applyCommand({addLayer}).delta.resourceId>=1`,
// `deleteLayer` remove shape, and undo/redo delta presence — but the TS EMULATOR
// produces all of those too, so every test passed even with the wiring DELETED.
// This rewrite uses a bridge-side proof that only REAL Rust satisfies:
//   bridge.getSnapshot() is the live Rust engine snapshot when wired; when
//   UNWIRED bridge.ts hard-codes { version:0, layers:[] } (bridge.ts ~L60) and
//   the emulator NEVER populates it. So `getSnapshot().layers.length > 0` after
//   a wired command is TRUE only when setProtocolWasm actually armed the bridge
//   to the wasm engine. Delete the `setProtocolWasm(mod)` wiring in wasmExport.ts
//   and the first test below FAILS.
//
// Mock-fidelity: wasmTestShim loads the REAL .wasm bytes from disk (initSync),
// so this is the true production boundary (serde string in/out), not a fake wasm.

import { describe, it, expect, beforeAll, afterEach, vi } from "vitest";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import * as bridge from "@/lib/protocol/bridge";
import { __resetEmulatedForTests } from "@/lib/protocol/bridge";
import { CONTRACT_VERSION } from "../types";

type WasmModule = {
  protocol_apply_command: (json: string, docId: string) => string;
  protocol_snapshot_json: (docId: string) => string;
  protocol_reset: (docId: string) => void;
};

// The REAL wasm module (per-document engine map), loaded + wired by the
// production `getWasmExportModule()` path. We capture it once in beforeAll and
// NEVER call `setProtocolWasm` ourselves — so the only thing that can arm the
// bridge is the production wiring we are trying to prove.
let wasmModule: WasmModule | null = null;

beforeAll(async () => {
  // First getWasmExportModule() call in this test file triggers the production
  // wiring (wasmExport.ts -> setProtocolWasm(mod)). Do nothing else here.
  const m = await getWasmExportModule();
  expect(m).not.toBeNull();
  expect(typeof m.protocol_apply_command).toBe("function");
  wasmModule = m as WasmModule;
});

afterEach(() => {
  // Keep the bridge WIRED across tests (beforeAll armed it once). The reserved
  // "default" per-document engine is reset so tests don't leak state; the TS
  // emulator globals are cleared defensively too (though unused while wired).
  __resetEmulatedForTests();
  wasmModule?.protocol_reset("default");
  vi.restoreAllMocks();
});

describe("facade is Rust-backed once wasm loads (structural-sharing wiring)", () => {
  it("getWasmExportModule() wired bridge.applyCommand to the REAL Rust engine", () => {
    // Fresh engine: the wired bridge applies through the wasm boundary.
    const res = bridge.applyCommand({
      contractVersion: CONTRACT_VERSION,
      command: { type: "addLayer", name: "Rust" },
    }) as unknown as { documentVersion: number };

    // DISCRIMINATOR — only real Rust populates the bridge snapshot:
    //   * WIRED:    getSnapshot() -> wasm.protocol_snapshot_json() (engine state)
    //   * UNWIRED:  getSnapshot() -> { version:0, layers:[] } (bridge.ts) and the
    //              TS emulator (emulateApply) NEVER writes to it.
    // So a NON-EMPTY snapshot after a wired command is PROOF the real wasm ran.
    const snap = bridge.getSnapshot();
    expect(snap.layers.length).toBeGreaterThan(0);
    expect(snap.version).toBeGreaterThan(0);
    expect(snap.layers[0].name).toBe("Rust");
    expect(snap.layers[0].resourceId).toBeGreaterThanOrEqual(1);

    // The command delta is consistent with the real engine (fresh engine v0 -> v1).
    expect(res.documentVersion).toBe(1);
    const upsert = (res as unknown as { delta: { changes: Array<{ kind: string; layer: { resourceId: number } }> } }).delta.changes[0];
    expect(upsert.kind).toBe("upsert");
    expect(upsert.layer.resourceId).toBeGreaterThanOrEqual(1);
  });

  it("wired bridge deleteLayer returns the Rust Remove delta shape (kind + id + resourceId)", () => {
    const add = bridge.applyCommand({
      contractVersion: CONTRACT_VERSION,
      command: { type: "addLayer", name: "A" },
    }) as unknown as { delta: { changes: Array<{ layer: { id: string } }> } };
    const id = add.delta.changes[0].layer.id;
    // Real engine holds exactly the layer it just created.
    expect(bridge.getSnapshot().layers.length).toBe(1);

    const del = bridge.applyCommand({
      contractVersion: CONTRACT_VERSION,
      command: { type: "deleteLayer", id },
    }) as unknown as {
      documentVersion: number;
      delta: { baseVersion: number; version: number; changes: Array<{ kind: string; id?: string; resourceId?: number }> };
    };
    const rm = del.delta.changes.find((c) => c.kind === "remove");
    expect(rm?.kind).toBe("remove");
    expect(rm?.id).toBe(id);
    // PIN (outbound Remove wire key): the Rust `Remove` struct-variant of
    // RenderLayerChange carries `resourceId` (camelCase) via the
    // `#[serde(rename_all="camelCase")]` on the variant. These assertions FAIL
    // if that serde attr is removed (serde would emit snake_case
    // `resource_id`). This is the outbound mirror of the inbound snake_case
    // `layer_id` pin in the brush-stroke test below.
    expect(rm?.resourceId).toBeGreaterThanOrEqual(1);
    expect(rm).not.toHaveProperty("resource_id");
    expect(del.delta.version).toBe(del.documentVersion);
    // Discriminator: the real engine snapshot dropped the layer.
    expect(bridge.getSnapshot().layers.length).toBe(0);
  });

  it("wired bridge undo/redo walk the real Rust H0 stream", () => {
    bridge.applyCommand({ contractVersion: CONTRACT_VERSION, command: { type: "addLayer", name: "del" } }) as unknown as { documentVersion: number };
    bridge.applyCommand({ contractVersion: CONTRACT_VERSION, command: { type: "addLayer", name: "victim" } });
    expect(bridge.getSnapshot().layers.length).toBe(2);

    // Undo the last native entry -> the real engine rolls back to 1 layer.
    const undo = bridge.applyCommand({ contractVersion: CONTRACT_VERSION, command: { type: "undo" } }) as unknown as {
      documentVersion: number;
      delta: { changes: unknown[] };
    };
    expect(undo.delta.changes.length).toBeGreaterThan(0); // Rust diff produced a change
    expect(undo.documentVersion).toBeGreaterThan(0);
    expect(bridge.getSnapshot().layers.length).toBe(1);

    const redo = bridge.applyCommand({ contractVersion: CONTRACT_VERSION, command: { type: "redo" } }) as unknown as {
      documentVersion: number;
    };
    expect(redo.documentVersion).toBeGreaterThan(undo.documentVersion);
    expect(bridge.getSnapshot().layers.length).toBe(2);
  });

  it("wired bridge brushStroke uses the snake_case layer_id wire field (serde contract)", () => {
    // DISCRIMINATOR for the wire-format bug: `#[serde(rename_all="camelCase", tag="type")]`
    // on the Rust `Command` enum renames the VARIANT only, NOT the struct-variant
    // fields - so `BrushStroke` expects layer_id (snake_case). The pre-fix sender
    // emitted `layerId`, which the real Rust engine rejects with
    // E_ENVELOPE_PARSE (missing field `layer_id`). After the bridge.ts sender fix
    // (`layer_id`), this must NOT throw and must apply end-to-end.
    const add = bridge.applyCommand({
      contractVersion: CONTRACT_VERSION,
      command: { type: "addLayer", name: "stroke" },
    }) as unknown as { delta: { changes: Array<{ layer: { id: string } }> } };
    const id = add.delta.changes[0].layer.id;

    const res = bridge.applyCommand({
      contractVersion: CONTRACT_VERSION,
      command: {
        type: "brushStroke",
        layerId: id, // production command shape (editorFacade.commitStroke)
        points: [
          { x: 0, y: 0, pressure: 0.5 },
          { x: 10, y: 10, pressure: 0.8 },
        ],
        settings: { size: 20, hardness: 0.5, opacity: 1, flow: 1 },
      },
    }) as unknown as {
      documentVersion: number;
      delta: { changes: Array<{ kind: string; layer: { id: string; dirtyRect?: { width: number } } }> };
    };

    // NOT throwing is the discriminator - bridge.toRustEnvelope emitted `layer_id`
    // and the real Rust engine accepted it (wasm boundary, real serde JSON in/out).
    expect(res.documentVersion).toBeGreaterThan(0);
    const upsert = res.delta.changes.find((c) => c.kind === "upsert");
    expect(upsert?.layer.id).toBe(id);
    expect(upsert?.layer.dirtyRect?.width ?? 0).toBeGreaterThanOrEqual(30);
    // The real engine snapshot holds exactly the stroked layer.
    expect(bridge.getSnapshot().layers.length).toBe(1);
  });
});
