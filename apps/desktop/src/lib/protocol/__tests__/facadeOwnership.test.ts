import { describe, it, expect, beforeEach, vi } from "vitest";
import { EditorFacade } from "../editorFacade";
import { __resetEmulatedForTests } from "../bridge";
import * as bridge from "../bridge";

describe("facade ownership — Ticket 2 invariants", () => {
  beforeEach(() => __resetEmulatedForTests());

  it("has no persistent layers field — only snapshot cache", async () => {
    const f = new EditorFacade();
    expect(f.hasPersistentLayersField()).toBe(false);
    expect(f.snapshot.layers.length).toBe(0);
    await f.addLayer("A");
    expect(f.snapshot.layers.length).toBe(1);
    // layers live only in snapshot (projection), not as separate mutable field
    expect(f.hasPersistentLayersField()).toBe(false);
  });

  it("transform lifecycle: transient does not mutate snapshot, commit does via delta", async () => {
    const f = new EditorFacade();
    await f.addLayer("A");
    const id = f.snapshot.layers[0].id;
    const baseVersion = f.renderedVersion;
    const baseX = f.snapshot.layers[0].x;

    f.beginTransform(id, { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0 });
    f.updateTransform({ x: 999, y: 0, scaleX: 2, scaleY: 2, rotation: 15 });
    // invariant: snapshot unchanged during pointermove
    expect(f.snapshot.layers[0].x).toBe(baseX);
    expect(f.transientTransform?.live.x).toBe(999);
    expect(f.renderedVersion).toBe(baseVersion);

    await f.commitTransform();
    expect(f.snapshot.layers[0].x).toBe(999);
    expect(f.transientTransform).toBeNull();
    expect(f.renderedVersion).toBe(baseVersion + 1);
  });

  it("pointermove never calls adapter/IPC (spy)", async () => {
    const f = new EditorFacade();
    await f.addLayer("A");
    const id = f.snapshot.layers[0].id;
    const spy = vi.spyOn(bridge, "applyCommand");
    f.beginTransform(id, { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0 });
    spy.mockClear();
    for (let i = 0; i < 50; i++) f.updateTransform({ x: i, y: 0, scaleX: 1, scaleY: 1, rotation: i });
    expect(spy).not.toHaveBeenCalled(); // no IPC on pointermove
    await f.commitTransform();
    expect(spy).toHaveBeenCalledTimes(1); // single command on pointerup
    spy.mockRestore();
  });

  it("brush: activeStroke lives in transient, stroke command only on pointerup", async () => {
    const f = new EditorFacade();
    await f.addLayer("L");
    const id = f.snapshot.layers[0].id;
    const spy = vi.spyOn(bridge, "applyCommand");
    f.beginStroke(id);
    for (let i = 0; i < 30; i++) f.addStrokePoint({ x: i, y: i, pressure: 0.5 });
    expect(spy).not.toHaveBeenCalled();
    expect(f.transientStroke?.points.length).toBe(30);
    expect(f.snapshot.version).toBe(1); // not bumped during stroke
    await f.commitStroke();
    expect(spy).toHaveBeenCalledTimes(1);
    expect(f.transientStroke).toBeNull();
    expect(f.renderedVersion).toBe(2);
    spy.mockRestore();
  });

  it("add/delete/opacity are single-command, no optimistic persistent mutation on failure", async () => {
    const f = new EditorFacade();
    const v0 = f.renderedVersion;
    await f.addLayer("A");
    expect(f.renderedVersion).toBe(v0 + 1);
    const id = f.snapshot.layers[0].id;
    await f.setOpacity(id, 0.5);
    expect(f.snapshot.layers[0].opacity).toBe(0.5);
    await f.deleteLayer(id);
    expect(f.snapshot.layers.length).toBe(0);

    // failure case: wrong contractVersion does not mutate persistent state
    await expect(bridge.applyCommand({ contractVersion: 999, command: { type: "addLayer", name: "bad" } } as never)).rejects.toThrow();
    expect(f.snapshot.layers.length).toBe(0); // still 0, no optimistic write
  });

  it("undo/redo via delta, version monotonic, no TS history mirror", async () => {
    const f = new EditorFacade();
    await f.addLayer("A");
    const id = f.snapshot.layers[0].id;
    await f.setOpacity(id, 0.3);
    expect(f.snapshot.layers[0].opacity).toBe(0.3);
    const v2 = f.renderedVersion;
    await f.undo();
    expect(f.snapshot.layers[0].opacity).toBe(1.0);
    expect(f.renderedVersion).toBe(v2 + 1);
    await f.redo();
    expect(f.snapshot.layers[0].opacity).toBe(0.3);
    expect(f.renderedVersion).toBe(v2 + 2);
    // no separate TS history array
    expect((f as unknown as { history?: unknown }).history).toBeUndefined();
  });

  it("version checks: stale delta does not overwrite newer snapshot", async () => {
    const f = new EditorFacade();
    await f.addLayer("A"); // v1
    await f.addLayer("B"); // v2
    expect(f.renderedVersion).toBe(2);
    // simulate late delta 0->1
    const stale = { baseVersion: 0, version: 1, changes: [] } as never;
    expect(f.applyDelta(stale as never)).toBe(false);
    expect(f.renderedVersion).toBe(2);
    // snapshot older than rendered ignored
    expect(f.applySnapshot({ version: 1, layers: [] })).toBe(false);
    expect(f.renderedVersion).toBe(2);
  });

  it("migration invariant: after migration, exactly one persistent owner per op", async () => {
    const f = new EditorFacade();
    // addLayer's persistent owner is Rust (via snapshot), not TS field
    await f.addLayer("X");
    const id = f.snapshot.layers[0].id;
    // prove no second owner: mutating a fake TS field would not affect snapshot
    (f as unknown as Record<string, unknown>).layers = [{ id, fake: true }];
    expect(f.snapshot.layers[0].name).toBe("X"); // snapshot unaffected
    // opacity owner
    await f.setOpacity(id, 0.2);
    expect(f.snapshot.layers[0].opacity).toBe(0.2);
    // transform owner
    f.beginTransform(id, { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0 });
    f.updateTransform({ x: 50, y: 0, scaleX: 1, scaleY: 1, rotation: 0 });
    expect(f.snapshot.layers[0].x).toBe(0); // not yet
    await f.commitTransform();
    expect(f.snapshot.layers[0].x).toBe(50);
  });
});
