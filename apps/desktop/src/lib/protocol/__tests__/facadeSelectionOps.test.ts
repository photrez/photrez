// Selection op routing (setSelection / clearSelection / selectAll /
// invertSelection) through the native selection arms.
//
// Selection is engine-local UI state, so routing here is MIRROR-shaped: the host
// engine mutation is the visual authority and always runs, and the native
// dispatch is an additional shadow sync. These tests pin:
//  (1) the facade methods issue the right command envelope;
//  (2) flag/authority gating returns "legacy" and touches no facade method;
//  (3) the funnel never projects the (selection-less) facade snapshot back onto
//      the engine, so a routed op cannot clobber the host's model.selection.

import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { DocumentEngine } from "@/engine/document";
import * as bridge from "@/lib/protocol/bridge";
import { EditorFacade } from "@/lib/protocol/editorFacade";
import {
  commitFacadeSetSelection,
  commitFacadeClearSelection,
  commitFacadeSelectAll,
  commitFacadeInvertSelection,
} from "@/lib/protocol/facadeRegistry";
import type { SelectionState } from "@/lib/protocol/types";

function setFlags(opts: { facade: boolean; native: boolean }): void {
  if (opts.facade) localStorage.setItem("photrez.facade", "1");
  else localStorage.setItem("photrez.facade", "0");
  if (opts.native) localStorage.setItem("photrez.facadeAuthority", "native");
  else localStorage.setItem("photrez.facadeAuthority", "wasm");
}

afterEach(() => {
  localStorage.removeItem("photrez.facade");
  localStorage.removeItem("photrez.facadeAuthority");
  vi.restoreAllMocks();
});

const OK_RESULT = { delta: { baseVersion: 0, version: 1, changes: [] }, status: "ok" };

describe("EditorFacade selection arms issue the right command envelope", () => {
  beforeEach(() => {
    setFlags({ facade: true, native: true });
    vi.spyOn(bridge, "getVersion").mockResolvedValue(0);
    vi.spyOn(bridge, "flushExternalTransitions").mockResolvedValue();
  });

  it("setSelection -> setSelection with the camelCase SelectionState passthrough", async () => {
    const spy = vi.spyOn(bridge, "applyCommand").mockResolvedValue(OK_RESULT as never);
    const facade = new EditorFacade(undefined, "sel-doc");
    const sel: SelectionState = { x: 1, y: 2, width: 3, height: 4, angle: 5, shape: "ellipse" };

    await facade.setSelection(sel);

    const env = spy.mock.calls[0][0] as { expectedVersion: number; command: unknown };
    expect(env.expectedVersion).toBe(0);
    expect(env.command).toEqual({ type: "setSelection", selection: sel });
  });

  it("clearSelection / selectAll / invertSelection -> their arm types", async () => {
    const spy = vi.spyOn(bridge, "applyCommand").mockResolvedValue(OK_RESULT as never);
    const facade = new EditorFacade(undefined, "sel-doc");

    await facade.clearSelection();
    await facade.selectAll();
    await facade.invertSelection();

    expect(spy.mock.calls.map((c) => (c[0] as { command: { type: string } }).command.type)).toEqual([
      "clearSelection",
      "selectAll",
      "invertSelection",
    ]);
  });
});

describe("selection commit mirrors: gate + no projection write-back", () => {
  it("flag OFF -> legacy; the facade method is never called", async () => {
    setFlags({ facade: false, native: true });
    const engine = new DocumentEngine("d1", "D", 800, 600);
    engine.createSelection(10, 20, 30, 40, 0);
    const f = { setSelection: vi.fn() } as unknown as EditorFacade;

    const r = await commitFacadeSetSelection(engine as never, engine.getSelection()!, f);

    expect(r.status).toBe("legacy");
    expect(f.setSelection).not.toHaveBeenCalled();
  });

  it("flag ON but wasm authority -> legacy; the facade method is never called", async () => {
    setFlags({ facade: true, native: false });
    const engine = new DocumentEngine("d1b", "D", 800, 600);
    const f = { clearSelection: vi.fn() } as unknown as EditorFacade;

    const r = await commitFacadeClearSelection(engine as never, f);

    expect(r.status).toBe("legacy");
    expect(f.clearSelection).not.toHaveBeenCalled();
  });

  it("flag ON + native authority -> dispatches to the facade and does NOT project the snapshot back", async () => {
    setFlags({ facade: true, native: true });
    const engine = new DocumentEngine("d2", "D", 800, 600);
    engine.createSelection(10, 20, 30, 40, 0);
    const hostSelection = engine.getSelection();
    const projectSpy = vi.spyOn(engine, "applyFacadeSnapshot");
    const f = { setSelection: vi.fn().mockResolvedValue({}) } as unknown as EditorFacade;

    const r = await commitFacadeSetSelection(engine as never, hostSelection!, f);

    expect(r.status).toBe("applied");
    expect(f.setSelection).toHaveBeenCalledWith(hostSelection);
    // The host stays the visual authority: no snapshot write-back, selection intact.
    expect(projectSpy).not.toHaveBeenCalled();
    expect(engine.getSelection()).toEqual(hostSelection);
  });

  it("each op routes to its own facade method", async () => {
    setFlags({ facade: true, native: true });
    const engine = new DocumentEngine("d3", "D", 800, 600);
    const f = {
      clearSelection: vi.fn().mockResolvedValue({}),
      selectAll: vi.fn().mockResolvedValue({}),
      invertSelection: vi.fn().mockResolvedValue({}),
    } as unknown as EditorFacade;

    await commitFacadeClearSelection(engine as never, f);
    await commitFacadeSelectAll(engine as never, f);
    await commitFacadeInvertSelection(engine as never, f);

    expect(f.clearSelection).toHaveBeenCalledTimes(1);
    expect(f.selectAll).toHaveBeenCalledTimes(1);
    expect(f.invertSelection).toHaveBeenCalledTimes(1);
  });

  it("host selection survives a routed invert (mirror does not suppress/clobber it)", async () => {
    setFlags({ facade: true, native: true });
    const engine = new DocumentEngine("d4", "D", 800, 600);
    // Simulate the production order: host mutation first, then the mirror.
    engine.createSelection(10, 20, 30, 40, 0);
    engine.invertSelection();
    const hostAfter = engine.getSelection();
    expect(hostAfter?.inverted).toBe(true);
    const f = { invertSelection: vi.fn().mockResolvedValue({}) } as unknown as EditorFacade;

    await commitFacadeInvertSelection(engine as never, f);

    expect(engine.getSelection()).toEqual(hostAfter);
  });
});
