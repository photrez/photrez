// Regression coverage for the async facade refreshSnapshot migration leak (#4).
// EditorFacade.addLayer/deleteLayer/setOpacity/commitTransform/undo/redo return
// `this.snapshot` AFTER `if (!applyDelta) this.refreshSnapshot()`.
// refreshSnapshot() is async; without the `await`, the method returns the STALE
// snapshot and the later microtask then silently replaces this.snapshot (the
// handed-back reference never heals). With the fix, the returned snapshot is the
// refreshed (authoritative) one. This test proves the contract per-branch.
import { afterEach, describe, expect, it, vi } from "vitest";
import { EditorFacade } from "../editorFacade";
import { __resetEmulatedForTests } from "../bridge";
import * as bridge from "../bridge";
import type { CommandResult, RenderSnapshot } from "../types";

const REFRESHED: RenderSnapshot = { version: 1000, layers: [{ id: "refreshed", name: "R", visible: true, opacity: 1, resourceId: 1, x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0 }] };

// A delta whose baseVersion never matches the facade's renderedVersion, so
// applyDelta() reports it inapplicable and the refreshSnapshot() path runs.
function inapplicableResult(): CommandResult {
  return {
    delta: { baseVersion: 9999, version: 9999, changes: [] },
    status: "ok",
    documentVersion: 1000,
  } as unknown as CommandResult;
}

describe("EditorFacade.refreshSnapshot await contract (async leak #4)", () => {
  afterEach(() => {
    __resetEmulatedForTests();
    vi.restoreAllMocks();
  });

  it("addLayer: returns the refreshed snapshot when applyDelta is inapplicable", async () => {
    const f = new EditorFacade();
    vi.spyOn(bridge, "applyCommand").mockResolvedValue(inapplicableResult());
    const getSnapSpy = vi.spyOn(bridge, "getSnapshot").mockResolvedValue(REFRESHED);

    const result = await f.addLayer("A");

    expect(getSnapSpy).toHaveBeenCalled();
    // The returned snapshot is the authoritative refreshed one, NOT stale.
    expect(result).toBe(REFRESHED);
    expect(f.snapshot).toBe(REFRESHED);
  });

  it("deleteLayer: returned snapshot heals via awaited refreshSnapshot", async () => {
    const f = new EditorFacade();
    vi.spyOn(bridge, "applyCommand").mockResolvedValue(inapplicableResult());
    vi.spyOn(bridge, "getSnapshot").mockResolvedValue(REFRESHED);

    const result = await f.deleteLayer("x");
    expect(result).toBe(REFRESHED);
  });

  it("undo: returned snapshot heals via awaited refreshSnapshot", async () => {
    const f = new EditorFacade();
    vi.spyOn(bridge, "applyCommand").mockResolvedValue(inapplicableResult());
    vi.spyOn(bridge, "getSnapshot").mockResolvedValue(REFRESHED);

    const result = await f.undo();
    expect(result).toBe(REFRESHED);
  });
});
