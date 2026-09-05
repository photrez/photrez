// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Arm the Rust graph engine before constructing DocumentEngine so these tests
// exercise the Rust mirror, not the TS fallback. The live Rust SSOT path was
// never exercised by document.test.ts, because that suite constructed
// `DocumentEngine` WITHOUT awaiting `getWasmExportModule()` first, so
// `this.rustEngine === null` and every graph-op test silently ran the TS
// fallback. That hiding gap is exactly why the just-fixed `duplicate_layer`
// position/name/lock divergences shipped.
//
// This harness arms the wasm module in beforeAll (mirroring the production
// loader used by facadeDelete.test.ts:26-40) BEFORE constructing the TS
// DocumentEngine, so `rustEngine` is armed and graph ops delegate to the REAL
// Rust mirror. It then asserts Rust-path ORDER / POSITION / NAME / LOCK
// behavior - including the two regressions that the TS-fallback tests missed.

import { describe, it, expect, beforeAll, afterEach, vi, beforeEach } from "vitest";
import { DocumentEngine } from "../document";
import { getWasmExportModule } from "@/components/editor/wasmExport";

let wasmMod: any = null;

beforeAll(async () => {
  wasmMod = await getWasmExportModule();
});

beforeEach(() => {
  // Photrez.facade stays OFF - these tests are about the always-on Rust SSOT
  // graph mirror (USE_RUST_SSOT=true), not the facade-gated ownership stream.
  localStorage.removeItem("photrez.facade");
});

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.removeItem("photrez.facade");
});

// Proves the Rust path is actually wired: constructs the TS DocumentEngine with
// the wasm module already loaded, then asserts the duplicate op is delegated to
// the Rust `duplicate_layer` (not the TS fallback).
it("arms the Rust engine and delegates duplicateLayer to it", () => {
  expect(wasmMod?.DocumentEngine).toBeTruthy();
  const duplicateSpy = vi.spyOn(wasmMod.DocumentEngine.prototype, "duplicate_layer");
  const engine = new DocumentEngine("doc-rust-armed", "Rust Armed", 800, 600);
  const l1 = engine.addLayer("Layer 1");
  const dup = engine.duplicateLayer(l1.id);
  expect(duplicateSpy).toHaveBeenCalledTimes(1);
  // The "-copy" id shape is minted only by the Rust duplicate_layer
  // (document.rs:249); the TS fallback mints "layer-<uuid>". Asserting this
  // proves the Rust result was actually used, not just attempted.
  expect(dup.id).toBe(l1.id + "-copy");
  expect(dup.id).toBeTruthy();
  expect(engine.getActiveLayerId()).toBe(dup.id);
});

// FIX #1 (Rust mirror): duplicate of a locked layer is unlocked; duplicate of
// the Background is NOT a background and exactly one background remains.
it("Rust path: duplicate is unlocked and does not inherit background", () => {
  const engine = new DocumentEngine("doc-rust-locks", "Rust Locks", 800, 600);
  const bg = engine.addLayer("Background");
  engine.markLayerAsBackground(bg.id); // flags isBackground + position/rotation locks (Rust)
  const l1 = engine.addLayer("Layer 1");
  engine.setLayerLocked(l1.id, true);

  const dupBg = engine.duplicateLayer(bg.id);
  const dupBgNode = engine.getLayer(dupBg.id)!;
  expect(dupBgNode.isBackground).toBeUndefined();
  expect(dupBgNode.locked).toBe(false);

  const dupL1 = engine.duplicateLayer(l1.id);
  const dupL1Node = engine.getLayer(dupL1.id)!;
  expect(dupL1Node.locked).toBe(false);

  // Exactly one background must remain in the document.
  const bgCount = engine.getLayers().filter((l) => l.isBackground).length;
  expect(bgCount).toBe(1);
});

// FIX #2 (Rust mirror): duplicate name follows the TS numeric-suffix
// convention ("Layer 1" -> "Layer 2" -> "Layer 3"), not a literal " copy".
it("Rust path: duplicate name uses numeric suffix", () => {
  const engine = new DocumentEngine("doc-rust-name", "Rust Name", 800, 600);
  const l1 = engine.addLayer("Layer 1");
  const d1 = engine.duplicateLayer(l1.id);
  const d2 = engine.duplicateLayer(l1.id);
  expect(d1.name).toBe("Layer 2");
  expect(d2.name).toBe("Layer 3");
  const names = engine.getLayers().map((l) => l.name);
  expect(names).not.toContain("Layer 1 copy");
});

// Rust-path ORDER assertion: duplicate sits immediately ABOVE the source in the
// top-indexed stack (index 0 = top), matching addLayer / duplicateLayer order.
it("Rust path: duplicate sits immediately above the source", () => {
  const engine = new DocumentEngine("doc-rust-order", "Rust Order", 800, 600);
  engine.addLayer("Layer 1");
  const l2 = engine.addLayer("Layer 2");
  const l3 = engine.addLayer("Layer 3");
  // Stack (top -> bottom): Layer 3, Layer 2, Layer 1
  const dup = engine.duplicateLayer(l2.id);
  const order = engine.getLayers().map((l) => l.id);
  const srcIdx = order.indexOf(l2.id);
  const dupIdx = order.indexOf(dup.id);
  expect(dupIdx).toBe(srcIdx - 1);
});

// Rust-path POSITION/ORDER assertion for reorder: reordering is delegated to the
// Rust engine and the resulting order is observable from the TS model.
it("Rust path: reorder delegates and reorders the stack", () => {
  expect(wasmMod?.DocumentEngine).toBeTruthy();
  const reorderSpy = vi.spyOn(wasmMod.DocumentEngine.prototype, "reorder_layer");
  const engine = new DocumentEngine("doc-rust-reorder", "Rust Reorder", 800, 600);
  engine.addLayer("Layer 1");
  engine.addLayer("Layer 2");
  engine.addLayer("Layer 3");
  // Stack: [Layer 3, Layer 2, Layer 1]; move top (idx 0) to bottom (idx 2).
  engine.reorderLayer(0, 2);
  expect(reorderSpy).toHaveBeenCalledTimes(1);
  const order = engine.getLayers().map((l) => l.name);
  expect(order).toEqual(["Layer 2", "Layer 1", "Layer 3"]);
});
