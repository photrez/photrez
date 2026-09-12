// Delta-consumer contract (pure-TS, no wasm): applyDeltaToSnapshot adopts the
// walker's ordered upsert sequence so a restored/merged layer lands at its
// SNAPSHOT position, not the stack bottom. Covers the two-pass decision:
// superset-restatement adopt vs in-place+append fallback.

import { describe, it, expect } from "vitest";
import { applyDeltaToSnapshot } from "../editorFacade";
import type { RenderLayer, RenderSnapshot, RenderDelta } from "../types";

function mk(id: string, resourceId = 1): RenderLayer {
  return {
    id,
    name: id,
    visible: true,
    opacity: 1,
    resourceId,
    x: 0,
    y: 0,
    scaleX: 1,
    scaleY: 1,
    rotation: 0,
  };
}

describe("applyDeltaToSnapshot ordering (two-pass consumer)", () => {
  it("adopts the upsert sequence: a restored mid-stack layer returns to its snapshot position, not the bottom", () => {
    const snap: RenderSnapshot = { version: 5, layers: [mk("A"), mk("B"), mk("C")] };
    // Walker-style undo/redo delta: Remove B + ordered Upserts of EVERY merged layer.
    const delta: RenderDelta = {
      baseVersion: 5,
      version: 6,
      changes: [
        { kind: "remove", id: "B", resourceId: 2 },
        { kind: "upsert", layer: mk("A") },
        { kind: "upsert", layer: mk("B") },
        { kind: "upsert", layer: mk("C") },
      ],
    };
    const out = applyDeltaToSnapshot(snap, delta);
    expect(out.layers.map((l) => l.id)).toEqual(["A", "B", "C"]);
    expect(out.layers.findIndex((l) => l.id === "B")).toBe(1); // middle, not bottom
  });

  it("falls back to in-place when upserts do NOT cover every remaining id (partial restatement)", () => {
    const snap: RenderSnapshot = { version: 5, layers: [mk("A"), mk("B"), mk("C")] };
    const delta: RenderDelta = {
      baseVersion: 5,
      version: 6,
      changes: [
        { kind: "remove", id: "B", resourceId: 2 },
        { kind: "upsert", layer: mk("A") },
        // no upsert for C -> not a full restatement
      ],
    };
    const out = applyDeltaToSnapshot(snap, delta);
    expect(out.layers.map((l) => l.id)).toEqual(["A", "C"]);
  });

  it("appends a single new upsert (addLayer) instead of reordering", () => {
    const snap: RenderSnapshot = { version: 5, layers: [mk("A"), mk("B")] };
    const delta: RenderDelta = {
      baseVersion: 5,
      version: 6,
      changes: [{ kind: "upsert", layer: mk("C", 3) }],
    };
    const out = applyDeltaToSnapshot(snap, delta);
    expect(out.layers.map((l) => l.id)).toEqual(["A", "B", "C"]);
  });

  it("adopts the delta canvas size on the full-restatement branch", () => {
    const snap: RenderSnapshot = { version: 5, layers: [mk("A")], width: 100, height: 50 };
    const delta: RenderDelta = {
      baseVersion: 5,
      version: 6,
      changes: [{ kind: "upsert", layer: mk("A") }],
      width: 200,
      height: 300,
    };
    const out = applyDeltaToSnapshot(snap, delta);
    expect(out.width).toBe(200);
    expect(out.height).toBe(300);
  });

  it("adopts the delta canvas size on the fallback branch", () => {
    const snap: RenderSnapshot = { version: 5, layers: [mk("A"), mk("B")], width: 100, height: 50 };
    const delta: RenderDelta = {
      baseVersion: 5,
      version: 6,
      changes: [{ kind: "upsert", layer: mk("C", 3) }],
      width: 640,
      height: 480,
    };
    const out = applyDeltaToSnapshot(snap, delta);
    expect(out.width).toBe(640);
    expect(out.height).toBe(480);
  });

  it("carries the prior canvas size forward when the delta has no dims", () => {
    const snap: RenderSnapshot = { version: 5, layers: [mk("A")], width: 100, height: 50 };
    const delta: RenderDelta = {
      baseVersion: 5,
      version: 6,
      changes: [{ kind: "upsert", layer: mk("A") }],
    };
    const out = applyDeltaToSnapshot(snap, delta);
    expect(out.width).toBe(100);
    expect(out.height).toBe(50);
  });
});
