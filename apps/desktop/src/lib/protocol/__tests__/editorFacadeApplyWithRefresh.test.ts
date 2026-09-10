// Coverage for EditorFacade.applyCommandWithRefresh: a refresh-based command path
// for commands whose Rust arms emit a full ordered layer restatement plus Removes
// (the structural/canvas arms) that the production delta consumer cannot reconstruct.
// Instead of applying the delta, the method issues the command once and then
// UNCONDITIONALLY re-reads the full authoritative snapshot (layer ORDER and canvas
// DIMS come from the engine, NOT carried forward from previous local state), failing
// closed (no refresh) when applyCommand rejects.
import { afterEach, describe, expect, it, vi } from "vitest";
import { EditorFacade } from "../editorFacade";
import * as bridge from "../bridge";
import { __resetEmulatedForTests } from "../bridge";
import type { CommandResult, RenderSnapshot } from "../types";
import { CONTRACT_VERSION } from "../types";

// Local state BEFORE the command: a different layer ORDER and different canvas
// DIMS than the engine will report. The refresh path must NOT carry these forward.
const LOCAL: RenderSnapshot = {
  version: 1,
  layers: [
    { id: "localA", name: "A", visible: true, opacity: 1, resourceId: 1, x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0 },
    { id: "localB", name: "B", visible: true, opacity: 1, resourceId: 2, x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0 },
  ],
  width: 800,
  height: 600,
};

// Engine report AFTER the command: reordered layers (localB first), an extra
// engine-owned layer, and new canvas dims.
const ENGINE: RenderSnapshot = {
  version: 5,
  layers: [
    { id: "localB", name: "B", visible: true, opacity: 1, resourceId: 2, x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0 },
    { id: "localA", name: "A", visible: true, opacity: 1, resourceId: 1, x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0 },
    { id: "engineC", name: "C", visible: true, opacity: 1, resourceId: 3, x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0 },
  ],
  width: 1920,
  height: 1080,
};

// A structural-shaped command input (flatten). The bridge maps it to its rust
// wire form; here we assert only the TS envelope the facade builds.
const STRUCTURAL_COMMAND = { type: "flatten", mergedId: "merged-1" } as const;

function okResult(): CommandResult {
  return {
    delta: { baseVersion: 5, version: 5, changes: [] },
    status: "ok",
    documentVersion: 5,
  } as unknown as CommandResult;
}

describe("EditorFacade.applyCommandWithRefresh", () => {
  afterEach(() => {
    __resetEmulatedForTests();
    bridge.__resetNativeAuthorityForTests();
    try { localStorage.clear(); } catch {}
    vi.restoreAllMocks();
  });

  it("invokes applyCommand exactly once with the correct envelope for a structural command", async () => {
    const f = new EditorFacade(LOCAL);
    const applySpy = vi.spyOn(bridge, "applyCommand").mockResolvedValue(okResult());
    vi.spyOn(bridge, "getSnapshot").mockResolvedValue(ENGINE);

    await f.applyCommandWithRefresh({ ...STRUCTURAL_COMMAND });

    expect(applySpy).toHaveBeenCalledTimes(1);
    expect(applySpy).toHaveBeenCalledWith({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: 1,
      docId: "default",
      command: { type: "flatten", mergedId: "merged-1" },
    });
  });

  it("refreshes to the engine snapshot: order and dims come from the engine, not previous local", async () => {
    const f = new EditorFacade(LOCAL);
    vi.spyOn(bridge, "applyCommand").mockResolvedValue(okResult());
    const getSnapSpy = vi.spyOn(bridge, "getSnapshot").mockResolvedValue(ENGINE);

    const result = await f.applyCommandWithRefresh({ ...STRUCTURAL_COMMAND });

    expect(getSnapSpy).toHaveBeenCalledTimes(1);
    // Engine layer ORDER (localB, localA, engineC) wins, not the local (localA, localB) order.
    expect(f.snapshot.layers.map((l) => l.id)).toEqual(["localB", "localA", "engineC"]);
    expect(result.layers.map((l) => l.id)).toEqual(["localB", "localA", "engineC"]);
    // Engine DIMS win, not the previous local 800x600.
    expect(f.snapshot.width).toBe(1920);
    expect(f.snapshot.height).toBe(1080);
    expect(f.snapshot.width).not.toBe(LOCAL.width);
    expect(f.snapshot.height).not.toBe(LOCAL.height);
  });

  it("does not refresh when applyCommand rejects, and propagates the error", async () => {
    const f = new EditorFacade(LOCAL);
    const applySpy = vi.spyOn(bridge, "applyCommand").mockRejectedValue(new Error("E_BOOM: command failed"));
    const getSnapSpy = vi.spyOn(bridge, "getSnapshot").mockResolvedValue(ENGINE);

    await expect(f.applyCommandWithRefresh({ ...STRUCTURAL_COMMAND })).rejects.toThrow("E_BOOM: command failed");

    expect(applySpy).toHaveBeenCalledTimes(1);
    expect(getSnapSpy).not.toHaveBeenCalled();
    // Local state is untouched: the refresh never ran.
    expect(f.snapshot.layers.map((l) => l.id)).toEqual(["localA", "localB"]);
    expect(f.snapshot.width).toBe(800);
  });

  it("throws REFRESH_FAILED (not a stale-success) when getSnapshot rejects, leaving local snapshot untouched", async () => {
    const f = new EditorFacade(LOCAL);
    const applySpy = vi.spyOn(bridge, "applyCommand").mockResolvedValue(okResult());
    const getSnapSpy = vi.spyOn(bridge, "getSnapshot").mockRejectedValue(new Error("E_SNAP: boom"));

    // Command applied successfully, but the authoritative refresh failed. This must
    // reject loudly (not silently return the pre-command snapshot as if success).
    await expect(f.applyCommandWithRefresh({ ...STRUCTURAL_COMMAND })).rejects.toThrow(
      "REFRESH_FAILED:",
    );

    expect(applySpy).toHaveBeenCalledTimes(1);
    expect(getSnapSpy).toHaveBeenCalledTimes(1);
    // Local state is untouched: the failed refresh must not have been swallowed.
    expect(f.snapshot.layers.map((l) => l.id)).toEqual(["localA", "localB"]);
    expect(f.snapshot.width).toBe(800);
    expect(f.snapshot.height).toBe(600);
  });

  it("native-authority addLayer projects native snapshot order and re-pushes native-order canonical payload", async () => {
    // Native-authority path: the Rust engine mints the layer at its OWN host index,
    // so the bridged delta order can disagree with TS tail-append. addLayer must route
    // through the authoritative-snapshot path and re-push the canonical (native-order)
    // state, so the projection AND the canonical payload match native order.
    localStorage.setItem("photrez.facadeAuthority", "native");
    const f = new EditorFacade(LOCAL, "docA");
    const engine = {
      getId: () => "docA",
      getName: () => "D",
      getWidth: () => 800,
      getHeight: () => 600,
      getLayers: () => [{ id: "L1" }, { id: "bg" }],
      getSelection: () => null,
      applyFacadeSnapshot: vi.fn(),
    } as unknown as import("@/engine/document").DocumentEngine;
    f.bindEngine(engine);

    const applySpy = vi.spyOn(bridge, "applyCommand").mockResolvedValue({
      delta: { baseVersion: 1, version: 1, changes: [] },
      status: "ok",
      documentVersion: 1,
    } as unknown as CommandResult);
    // Native snapshot returns its OWN order (L1 first, then bg) - a TS tail-append
    // would have produced [bg, L1]; native order is [L1, bg].
    vi.spyOn(bridge, "getSnapshot").mockResolvedValue({
      version: 5,
      layers: [
        { id: "L1", name: "L1", visible: true, opacity: 1, resourceId: 1, x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0 },
        { id: "bg", name: "bg", visible: true, opacity: 1, resourceId: 2, x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0 },
      ],
      width: 800,
      height: 600,
    });
    vi.spyOn(bridge, "getVersion").mockResolvedValue(0);
    // Spy seedNativeCanonical (NOT repushCanonicalDocument) so the payload CONTENT is
    // exercised - the audit rejected spy-only because it hid a wrong payload.
    const seedSpy = vi.spyOn(bridge, "seedNativeCanonical").mockResolvedValue(undefined as never);

    await f.addLayer("New");

    // Wire order: applyCommand -> full snapshot refresh -> re-push canonical.
    expect(applySpy).toHaveBeenCalledTimes(1);
    // Facade projection matches native snapshot order, not a TS tail-append.
    expect(f.snapshot.layers.map((l) => l.id)).toEqual(["L1", "bg"]);
    expect(seedSpy).toHaveBeenCalledTimes(1);
    const payload = JSON.parse(seedSpy.mock.calls[0][1] as string);
    // Canonical payload carries native order (L1 first) - payload content asserted.
    expect(payload.layers.map((l: { id: string }) => l.id)).toEqual(["L1", "bg"]);
  });
});
