// Flip projection through the facade snapshot path.
//
// Why this exists: once flips route through the native arm, a routed flip sets
// flip_h in the engine but the projection has to carry it back into the TS
// model. Without that carry, the next numeric transform op would re-send
// flipH:false and silently undo the user's flip. The wire omits flipH/flipV
// when the native side holds None (Rust Option + skip_serializing), so an
// absent field must PRESERVE the model value, never clear it.

import { describe, it, expect } from "vitest";
import { DocumentEngine } from "@/engine/document";
import { toFacadeProjectionLayer } from "@/lib/protocol/facadeProjection";

function makeEngine(): DocumentEngine {
  return new DocumentEngine("flip-doc", "Flip Doc", 800, 600);
}

function layerDesc(id: string, extra: Record<string, unknown> = {}) {
  return {
    id,
    name: "L",
    visible: true,
    opacity: 1,
    x: 10,
    y: 20,
    scaleX: 1,
    scaleY: 1,
    rotation: 0,
    resourceId: 0,
    ...extra,
  };
}

function snap(version: number, layers: Record<string, unknown>[]): never {
  return { version, layers } as never;
}

describe("facade flip projection", () => {
  it("existing branch: present flipH/flipV reach the model", () => {
    const engine = makeEngine();
    const l = engine.addLayer("L");
    engine.applyFacadeSnapshot(snap(1, [layerDesc(l.id, { flipH: true, flipV: true })]));
    const t = engine.getLayer(l.id)!.transform;
    expect(t.flipH).toBe(true);
    expect(t.flipV).toBe(true);
    // Numeric fields still project alongside.
    expect(t.x).toBe(10);
  });

  it("existing branch: absent flip preserves the model value (anti-clobber)", () => {
    const engine = makeEngine();
    const l = engine.addLayer("L");
    engine.flipLayer(l.id, "h");
    expect(engine.getLayer(l.id)!.transform.flipH).toBe(true);
    // Snapshot omits flipH/flipV entirely (native None): must not clear.
    engine.applyFacadeSnapshot(snap(1, [layerDesc(l.id)]));
    expect(engine.getLayer(l.id)!.transform.flipH).toBe(true);
    expect(engine.getLayer(l.id)!.transform.flipV).toBe(false);
  });

  it("existing branch: present false overrides a set flip", () => {
    const engine = makeEngine();
    const l = engine.addLayer("L");
    engine.flipLayer(l.id, "h");
    engine.applyFacadeSnapshot(snap(1, [layerDesc(l.id, { flipH: false })]));
    expect(engine.getLayer(l.id)!.transform.flipH).toBe(false);
  });

  it("retained branch: absent keeps the retained flip, present overrides", () => {
    const engine = makeEngine();
    const l = engine.addLayer("L");
    engine.applyFacadeSnapshot(snap(1, [layerDesc(l.id, { flipH: true })]));
    expect(engine.getLayer(l.id)!.transform.flipH).toBe(true);
    // Layer vanishes: the projection retains the dropped node.
    engine.applyFacadeSnapshot(snap(2, []));
    expect(engine.getLayer(l.id)).toBeUndefined();
    // Reappears with flip absent: keeps the retained value.
    engine.applyFacadeSnapshot(snap(3, [layerDesc(l.id)]));
    expect(engine.getLayer(l.id)!.transform.flipH).toBe(true);
    // Reappears with flip restated: the restatement wins.
    engine.applyFacadeSnapshot(snap(4, [layerDesc(l.id, { flipH: false, flipV: true })]));
    expect(engine.getLayer(l.id)!.transform.flipH).toBe(false);
    expect(engine.getLayer(l.id)!.transform.flipV).toBe(true);
  });

  it("rebuild branch: absent means a fresh false, present carries through", () => {
    const engine = makeEngine();
    engine.applyFacadeSnapshot(snap(1, [layerDesc("fresh-absent")]));
    expect(engine.getLayer("fresh-absent")!.transform.flipH).toBe(false);
    expect(engine.getLayer("fresh-absent")!.transform.flipV).toBe(false);
    engine.applyFacadeSnapshot(
      snap(2, [layerDesc("fresh-absent"), layerDesc("fresh-present", { flipH: true })]),
    );
    expect(engine.getLayer("fresh-present")!.transform.flipH).toBe(true);
  });

  it("invalid input: a null flip flag is treated as absent, not written to the model", () => {
    // The wire type is Option<bool> with skip_serializing_if-none, so a real
    // engine sends the field absent or as a boolean - never null. A null can
    // only reach here from a hand-built snapshot or a legacy JSON document.
    // All three branches coalesce with ??, so null reads as absent: the
    // existing layer keeps its own value, a rebuilt layer starts at false, and
    // nothing writes null into a field typed boolean.
    const engine = makeEngine();
    const l = engine.addLayer("L");
    engine.flipLayer(l.id, "h");
    expect(() =>
      engine.applyFacadeSnapshot(snap(1, [layerDesc(l.id, { flipH: null })])),
    ).not.toThrow();
    expect(engine.getLayer(l.id)!.transform.flipH).toBe(true);
    engine.applyFacadeSnapshot(snap(2, [layerDesc("fresh-null", { flipH: null })]));
    expect(engine.getLayer("fresh-null")!.transform.flipH).toBe(false);
  });

  it("toFacadeProjectionLayer carries flipH/flipV out of the model", () => {
    const out = toFacadeProjectionLayer({
      id: "l",
      name: "L",
      visible: true,
      opacity: 1,
      transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: true, flipV: false },
    });
    expect(out.flipH).toBe(true);
    expect(out.flipV).toBe(false);
    const absent = toFacadeProjectionLayer({
      id: "l",
      name: "L",
      visible: true,
      opacity: 1,
      transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0 },
    });
    expect(absent.flipH).toBeUndefined();
    expect(absent.flipV).toBeUndefined();
  });
});
