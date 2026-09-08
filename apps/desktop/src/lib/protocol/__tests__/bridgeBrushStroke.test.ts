// Emulator-side brushStroke consistency (photrez-counter residual 1).
//
// The wasm-backed discriminating test in facadeRustBacked.wiring.test.ts proves
// the wire format matches Rust (`toRustEnvelope` emits snake_case `layer_id`).
// This file runs the SAME production TS command through the TS emulator fallback
// (no wasm wired here) and asserts it works too - the emulator reads `cmd.layerId`
// from the ORIGINAL envelope, so the command contract must be consistent across
// both backends. A pure-function/serde test alone would NOT prove the emulator
// and Rust agree on the command shape; this does.
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import * as bridge from "@/lib/protocol/bridge";
import { __resetEmulatedForTests } from "@/lib/protocol/bridge";
import { CONTRACT_VERSION } from "../types";

beforeEach(() => {
  __resetEmulatedForTests();
});
afterEach(() => {
  __resetEmulatedForTests();
  vi.restoreAllMocks();
});

describe("emulator brushStroke command consistency (no wasm wired)", () => {
  it("applies the same brushStroke command the production commitStroke() sends", async () => {
    const add = await bridge.applyCommand({
      contractVersion: CONTRACT_VERSION,
      command: { type: "addLayer", id: "L-id", name: "L", width: 100, height: 100, index: 0 },
    }) as unknown as { delta: { changes: Array<{ layer: { id: string } }> } };
    const id = add.delta.changes[0].layer.id;

    // Same command shape as editorFacade.commitStroke() (layerId / points / settings).
    const res = await bridge.applyCommand({
      contractVersion: CONTRACT_VERSION,
      command: {
        type: "brushStroke",
        layerId: id,
        points: [
          { x: 0, y: 0, pressure: 0.5 },
          { x: 10, y: 10, pressure: 0.8 },
        ],
        settings: { size: 20, hardness: 0.5, opacity: 1, flow: 1 },
      },
    });
    expect(res.documentVersion).toBeGreaterThan(0);
    const upsert = res.delta.changes[0] as { kind: string; layer: { id: string; dirtyRect?: { width: number } } };
    expect(upsert.kind).toBe("upsert");
    expect(upsert.layer.id).toBe(id);
    // dirtyRect reflects the brush footprint (points 0,0..10,10 + size 20 => >=30).
    expect(upsert.layer.dirtyRect?.width ?? 0).toBeGreaterThanOrEqual(30);
  });

  it("brushStroke on a missing layer produces an empty delta, not a throw", async () => {
    const res = await bridge.applyCommand({
      contractVersion: CONTRACT_VERSION,
      command: {
        type: "brushStroke",
        layerId: "nope",
        points: [{ x: 0, y: 0, pressure: 0.5 }],
        settings: { size: 20, hardness: 0.5, opacity: 1, flow: 1 },
      },
    });
    expect(res.delta.changes).toHaveLength(0);
  });
});
