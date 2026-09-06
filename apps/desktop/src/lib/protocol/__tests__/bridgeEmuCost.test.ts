// Emulator memory-cost semantics (photrez-counter residual 2).
//
// `estimateEmuNativeBytes` mirrors Rust `estimate_native_entry_cost`: it counts
// UNIQUE layer references across before/after (an unchanged/shared layer is the
// SAME reference, so it counts once - NOT once per before+after). The byte VALUE
// is a documented approximation (TS hard-codes 128 vs Rust `size_of::<RenderLayer>()`),
// so the COUNT SEMANTICS are the contract, not the byte value. These tests pin the
// unique-count semantics so a future regression (e.g. a deep copy re-introducing
// a before+after double-count) is caught.
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import * as bridge from "@/lib/protocol/bridge";
import {
  __resetEmulatedForTests,
  estimateEmuNativeBytes,
} from "@/lib/protocol/bridge";
import { CONTRACT_VERSION } from "../types";
import type { RenderLayer } from "../types";

function mkLayer(id: string, name: string, opacity = 1): RenderLayer {
  return {
    id,
    name,
    visible: true,
    opacity,
    resourceId: 1,
    x: 0,
    y: 0,
    scaleX: 1,
    scaleY: 1,
    rotation: 0,
    dirtyRect: { x: 0, y: 0, width: 1, height: 1 },
  };
}

// Mirrors the bridge's documented approximations - pinned here ONLY so the test
// can compute the exact expected unique-count cost. The contract is the COUNT,
// not the bytes.
const baseBytes = 128;
const slackBytes = 64;
const arcPtr = 8;

function perLayer(l: RenderLayer): number {
  return baseBytes + l.id.length + l.name.length + slackBytes;
}

beforeEach(() => {
  __resetEmulatedForTests();
});
afterEach(() => {
  __resetEmulatedForTests();
  vi.restoreAllMocks();
});

describe("estimateEmuNativeBytes unique layer-reference counting", () => {
  it("counts a shared (unchanged) layer ONCE, not once per before/after", () => {
    const a = mkLayer("a", "A");
    const b = mkLayer("b", "B");
    const a2 = { ...a, opacity: 0.5 }; // COW-edit of A: new ref, same id/name
    const before = [a, b];
    const after = [a2, b]; // b is the SAME reference in both sets

    const cost = estimateEmuNativeBytes(before, after);

    // Unique count = 3 (a, a2, b); the shared b must NOT be double-counted.
    const expectedUniqueCountCost =
      perLayer(a) + perLayer(a2) + perLayer(b) + 3 * 2 * arcPtr + 2 * arcPtr;
    // Naive before+after double-count = 4 layer slots (a, b, a2, b).
    const expectedDoubleCountCost =
      perLayer(a) + perLayer(b) + perLayer(a2) + perLayer(b) + 4 * 2 * arcPtr + 2 * arcPtr;

    expect(cost).toBe(expectedUniqueCountCost); // exact unique-count semantics
    expect(cost).toBeLessThan(expectedDoubleCountCost); // strictly below double-count
    // The ONLY saving vs double-count is the shared b layer (its bytes + its 2 ptr slots).
    expect(expectedDoubleCountCost - cost).toBe(perLayer(b) + 2 * arcPtr);
  });

  it("counts an unchanged before/after set as the same unique entries, not doubled", () => {
    const a = mkLayer("a", "A");
    const b = mkLayer("b", "B");
    const before = [a, b];
    const after = [a, b]; // an op that leaves every layer reference identical

    const cost = estimateEmuNativeBytes(before, after);
    expect(cost).toBe(perLayer(a) + perLayer(b) + 2 * 2 * arcPtr + 2 * arcPtr);
  });
});

describe("emulator history memoryCostBytes is a plausible non-double-counted number", () => {
  it("a shared-layer op yields memoryCostBytes below the naive before+after sum", async () => {
    // Drive the emulator (no wasm wired in this file) through a shared-layer op:
    // addLayer A, addLayer B, then transform A (B is unchanged => shared ref).
    const addA = await bridge.applyCommand({
      contractVersion: CONTRACT_VERSION,
      command: { type: "addLayer", name: "A" },
    }) as unknown as { delta: { changes: Array<{ layer: RenderLayer }> } };
    const idA = addA.delta.changes[0].layer.id;
    const addB = await bridge.applyCommand({
      contractVersion: CONTRACT_VERSION,
      command: { type: "addLayer", name: "B" },
    }) as unknown as { delta: { changes: Array<{ layer: RenderLayer }> } };
    const idB = addB.delta.changes[0].layer.id;

    await bridge.applyCommand({
      contractVersion: CONTRACT_VERSION,
      command: { type: "transformLayer", id: idA, transform: { x: 5, y: 0, scaleX: 1, scaleY: 1, rotation: 0 } },
    });

    const q = await bridge.getHistoryQuery();
    const entry = q.entries[q.entries.length - 1]; // the transform entry
    expect(entry.memoryCostBytes).toBeGreaterThan(0);

    const helper = (id: string, name: string) => baseBytes + id.length + name.length + slackBytes;
    const perA = helper(idA, "A");
    const perB = helper(idB, "B");
    // Naive before+after double-count for a 2-layer set: A,B in before + A',B in after
    // (counts B twice) => 4 layer slots + 4*2 ptr slots + 2 wrappers.
    const naiveSum = perA + perB + perA + perB + 4 * 2 * arcPtr + 2 * arcPtr;
    expect(entry.memoryCostBytes).toBeLessThan(naiveSum);
  });
});
