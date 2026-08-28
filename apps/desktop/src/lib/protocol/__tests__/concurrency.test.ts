import { describe, it, expect } from "vitest";
import type { RenderDelta, RenderSnapshot } from "../types";

// Pure-logic harness — no facade import. Correctness derives from
// documentVersion + baseVersion + renderedVersion only.
// pending Map is diagnostic only and must not affect the decision.

type Applier = {
  renderedVersion: number;
  snapshot: RenderSnapshot | null;
  deltasApplied: RenderDelta[];
  snapshotsApplied: RenderSnapshot[];
  applyDelta(delta: RenderDelta): void;
  applySnapshot(snap: RenderSnapshot): void;
};

function createApplier(initialVersion: number): Applier {
  const a: Applier = {
    renderedVersion: initialVersion,
    snapshot: null,
    deltasApplied: [],
    snapshotsApplied: [],
    applyDelta(delta) {
      if (delta.baseVersion !== a.renderedVersion) return;
      a.deltasApplied.push(delta);
      a.renderedVersion = delta.version;
    },
    applySnapshot(snap) {
      if (snap.version <= a.renderedVersion) return;
      a.snapshotsApplied.push(snap);
      a.snapshot = snap;
      a.renderedVersion = snap.version;
    },
  };
  return a;
}

function delta(base: number, version: number): RenderDelta {
  return { baseVersion: base, version, changes: [] };
}
function snapshot(version: number): RenderSnapshot {
  return { version, layers: [] };
}

describe("concurrency — version correctness (facade-independent)", () => {
  it("stale delta: 42->43 applicable, 39->42 not applicable when rendered=42", () => {
    const applier = createApplier(42);
    expect(delta(42, 43).baseVersion === applier.renderedVersion).toBe(true);
    applier.applyDelta(delta(42, 43));
    expect(applier.renderedVersion).toBe(43);

    const stale = delta(39, 42);
    const before = applier.renderedVersion;
    applier.applyDelta(stale);
    expect(applier.renderedVersion).toBe(before); // not applied
    expect(applier.deltasApplied.length).toBe(1);
  });

  it("late response: snapshot@44 before delta 42->43, final must be 44", () => {
    const applier = createApplier(42);
    // A: 42->43 in flight, B completes as snapshot@44
    applier.applySnapshot(snapshot(44));
    expect(applier.renderedVersion).toBe(44);
    // late A arrives
    applier.applyDelta(delta(42, 43));
    expect(applier.renderedVersion).toBe(44); // A discarded
    expect(applier.deltasApplied.length).toBe(0);
    expect(applier.snapshotsApplied.length).toBe(1);
  });

  it("full snapshot superseding older delta: rendered 42, snapshot 44 supersedes delta 42->43", () => {
    const applier = createApplier(42);
    // delta arrives first would be applicable, but snapshot@44 arrives before it
    applier.applySnapshot(snapshot(44));
    applier.applyDelta(delta(42, 43));
    expect(applier.renderedVersion).toBe(44);
    expect(applier.snapshotsApplied[0].version).toBe(44);
    // never rewinds to 43
    expect(applier.renderedVersion).not.toBe(43);
  });

  it("two in-flight commands — B wins (42->43, 43->44, B before A)", () => {
    const applier = createApplier(42);
    const a = delta(42, 43);
    const b = delta(43, 44);

    // B / snapshot@44 arrives before A
    applier.applyDelta(b); // base 43 !== 42 -> discarded (demonstrates need for snapshot path)
    expect(applier.renderedVersion).toBe(42);
    // In real system B would arrive as snapshot@44 (not raw delta) after A's base assumption
    applier.applySnapshot(snapshot(44));
    expect(applier.renderedVersion).toBe(44);

    // late A
    applier.applyDelta(a);
    expect(applier.renderedVersion).toBe(44); // not 43
    expect(applier.deltasApplied.length).toBe(0);
    expect(applier.snapshotsApplied.length).toBe(1);

    // Alternate ordering where deltas arrive in order: both apply sequentially
    const applier2 = createApplier(42);
    applier2.applyDelta(a);
    expect(applier2.renderedVersion).toBe(43);
    applier2.applyDelta(b);
    expect(applier2.renderedVersion).toBe(44);
  });

  it("renderedVersion never decreases on older snapshot", () => {
    const applier = createApplier(44);
    applier.applySnapshot(snapshot(43));
    applier.applySnapshot(snapshot(42));
    expect(applier.renderedVersion).toBe(44);
    expect(applier.snapshotsApplied.length).toBe(0);
  });

  it("diagnostic pending map does not affect correctness", () => {
    // pending is intentionally not consulted — correctness is baseVersion check only
    const pending = new Map<number, number>([
      [1, 42],
      [2, 43],
    ]);
    const applier = createApplier(42);
    // Even if pending says seq 1 is 42->43, decision still uses delta.baseVersion
    const staleWithPending = delta(39, 44);
    expect(pending.has(1)).toBe(true);
    applier.applyDelta(staleWithPending);
    expect(applier.renderedVersion).toBe(42); // not applied despite pending entry
  });
});
