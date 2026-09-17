// Call-site wiring for the four selection ops routed through the native
// selection arms. The commit helpers are mocked spies, so these tests prove the
// PRODUCTION dispatch: with the facade flag ON the helper fires, and - crucially
// - the host engine mutation still runs (selection routing is mirror-shaped: the
// host stays the visual authority). Flag OFF must leave the legacy path
// untouched with zero facade calls.
//
// Coverage: SelectionOptionBar create/invert/deselect and the marquee
// (input-handler commitSelection).

import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { createSignal } from "solid-js";
import { SelectionOptionBar } from "../SelectionOptionBar";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { createMockEngine, createMockHistory, createToolContext } from "@/__tests__/test-builders";
import { handlePointerDown, handlePointerMove, handlePointerUp } from "@/viewport/input-handler";

const h = vi.hoisted(() => ({
  commitFacadeSetSelection: vi.fn(() => Promise.resolve({ status: "applied" })),
  commitFacadeClearSelection: vi.fn(() => Promise.resolve({ status: "applied" })),
  commitFacadeSelectAll: vi.fn(() => Promise.resolve({ status: "applied" })),
  commitFacadeInvertSelection: vi.fn(() => Promise.resolve({ status: "applied" })),
}));

vi.mock("@/lib/protocol/facadeRegistry", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  commitFacadeSetSelection: h.commitFacadeSetSelection,
  commitFacadeClearSelection: h.commitFacadeClearSelection,
  commitFacadeSelectAll: h.commitFacadeSelectAll,
  commitFacadeInvertSelection: h.commitFacadeInvertSelection,
}));

const tick = () => new Promise((r) => setTimeout(r, 0));

type HostSelection = { x: number; y: number; width: number; height: number; angle: number; shape?: "rect" | "ellipse"; inverted?: boolean } | null;

function makeEngine(initial: HostSelection) {
  const state = { sel: initial };
  const engine = {
    getSelection: () => state.sel,
    getActiveLayerId: () => null,
    getLayer: () => null,
    createSelection: vi.fn((x: number, y: number, w: number, height: number, angle = 0, shape?: "rect" | "ellipse") => {
      state.sel = shape === "ellipse"
        ? { x, y, width: w, height, angle, shape: "ellipse" }
        : { x, y, width: w, height, angle };
    }),
    clearSelection: vi.fn(() => { state.sel = null; }),
    invertSelection: vi.fn(() => { if (state.sel) state.sel = { ...state.sel, inverted: !state.sel.inverted }; }),
    selectAll: vi.fn(),
    getId: () => "doc-1",
  };
  return { engine, state };
}

function makeEditor(engine: unknown) {
  const defaults: Record<string, unknown> = {
    activeTool: "selection",
    selection: null,
    selectionEditMode: false,
    selectionShape: "rect",
    selectionConstraintMode: "normal",
    selectionRatioW: 1,
    selectionRatioH: 1,
    selectionSizeW: 100,
    selectionSizeH: 100,
    workspace: { getActiveEngine: () => engine, getActiveHistory: () => ({ commit: () => {} }) },
    renderer: { uploadImage: () => {} },
    scheduler: { requestRender: () => {} },
  };
  const signals: Record<string, unknown> = {};
  for (const [key, val] of Object.entries(defaults)) {
    if (typeof val === "function" || (val && typeof val === "object" && !("x" in val || "w" in val))) {
      signals[key] = val;
    } else {
      const [s, set] = createSignal(val);
      signals[key] = s;
      signals["set" + key.charAt(0).toUpperCase() + key.slice(1)] = set;
    }
  }
  return signals;
}

function mountOptionBar(engine: unknown) {
  mockUseEditor(makeEditor(engine));
  const root = document.createElement("div");
  document.body.appendChild(root);
  const dispose = render(() => <SelectionOptionBar />, root);
  return { root, dispose };
}

function findButton(root: HTMLElement, text: string): HTMLButtonElement {
  const btn = Array.from(root.querySelectorAll("button")).find((b) => b.textContent?.includes(text));
  if (!btn) throw new Error(`button not found: ${text}`);
  return btn;
}

afterEach(() => {
  document.body.replaceChildren();
  localStorage.removeItem("photrez.facade");
  vi.restoreAllMocks();
  h.commitFacadeSetSelection.mockClear();
  h.commitFacadeClearSelection.mockClear();
  h.commitFacadeSelectAll.mockClear();
  h.commitFacadeInvertSelection.mockClear();
});

describe("SelectionOptionBar selection routing (mirror dispatch)", () => {
  it("create (W submit): flag ON mirrors setSelection AND still mutates the host engine", async () => {
    localStorage.setItem("photrez.facade", "1");
    const { engine } = makeEngine({ x: 10, y: 20, width: 300, height: 200, angle: 0 });
    const { root, dispose } = mountOptionBar(engine);

    const w = root.querySelectorAll("input").item(2);
    w.dispatchEvent(new Event("focus", { bubbles: true }));
    w.value = "400";
    w.dispatchEvent(new Event("input", { bubbles: true }));
    w.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await tick();

    expect(engine.createSelection).toHaveBeenCalledTimes(1);
    expect(h.commitFacadeSetSelection).toHaveBeenCalledTimes(1);
    expect((h.commitFacadeSetSelection.mock.calls[0] as unknown[])[1]).toMatchObject({ x: 10, y: 20, width: 400, height: 200 });
    dispose();
  });

  it("create (W submit): photrez.facade=0 opt-out uses the host engine only, zero facade calls", async () => {
    localStorage.setItem("photrez.facade", "0");
    const { engine } = makeEngine({ x: 10, y: 20, width: 300, height: 200, angle: 0 });
    const { root, dispose } = mountOptionBar(engine);

    const w = root.querySelectorAll("input").item(2);
    w.dispatchEvent(new Event("focus", { bubbles: true }));
    w.value = "400";
    w.dispatchEvent(new Event("input", { bubbles: true }));
    w.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await tick();

    expect(engine.createSelection).toHaveBeenCalledTimes(1);
    expect(h.commitFacadeSetSelection).not.toHaveBeenCalled();
    dispose();
  });

  it("invert: flag ON mirrors invertSelection AND still mutates the host engine", async () => {
    localStorage.setItem("photrez.facade", "1");
    const { engine } = makeEngine({ x: 10, y: 20, width: 300, height: 200, angle: 0 });
    const { root, dispose } = mountOptionBar(engine);

    findButton(root, "Invert").dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await tick();

    expect(engine.invertSelection).toHaveBeenCalledTimes(1);
    expect(h.commitFacadeInvertSelection).toHaveBeenCalledTimes(1);
    dispose();
  });

  it("deselect: flag ON mirrors clearSelection AND still mutates the host engine", async () => {
    localStorage.setItem("photrez.facade", "1");
    const { engine } = makeEngine({ x: 10, y: 20, width: 300, height: 200, angle: 0 });
    const { root, dispose } = mountOptionBar(engine);

    findButton(root, "Deselect").dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await tick();

    expect(engine.clearSelection).toHaveBeenCalledTimes(1);
    expect(h.commitFacadeClearSelection).toHaveBeenCalledTimes(1);
    dispose();
  });

  it("deselect: photrez.facade=0 opt-out uses the host engine only, zero facade calls", async () => {
    localStorage.setItem("photrez.facade", "0");
    const { engine } = makeEngine({ x: 10, y: 20, width: 300, height: 200, angle: 0 });
    const { root, dispose } = mountOptionBar(engine);

    findButton(root, "Deselect").dispatchEvent(new MouseEvent("click", { bubbles: true }));

    expect(engine.clearSelection).toHaveBeenCalledTimes(1);
    expect(h.commitFacadeClearSelection).not.toHaveBeenCalled();
    dispose();
  });
});

describe("marquee selection routing (input-handler)", () => {
  it("flag ON: createSelection runs on the host AND mirrors setSelection", async () => {
    localStorage.setItem("photrez.facade", "1");
    const engine = createMockEngine(["createSelection", "clearSelection", "snapshot"]);
    const ctx = createToolContext({ selectedLayerId: null, onSelectionCreated: vi.fn() });

    handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
    handlePointerMove("selection", 200, 250, engine, vi.fn(), ctx);
    handlePointerUp("selection", 200, 250, engine, createMockHistory(), vi.fn(), ctx);
    await tick();

    expect(engine.createSelection).toHaveBeenCalledWith(100, 100, 100, 150);
    expect(h.commitFacadeSetSelection).toHaveBeenCalledTimes(1);
  });

  it("photrez.facade=0 opt-out: createSelection runs on the host with zero facade calls", () => {
    localStorage.setItem("photrez.facade", "0");
    const engine = createMockEngine(["createSelection", "clearSelection", "snapshot"]);
    const ctx = createToolContext({ selectedLayerId: null, onSelectionCreated: vi.fn() });

    handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
    handlePointerMove("selection", 200, 250, engine, vi.fn(), ctx);
    handlePointerUp("selection", 200, 250, engine, createMockHistory(), vi.fn(), ctx);

    expect(engine.createSelection).toHaveBeenCalledWith(100, 100, 100, 150);
    expect(h.commitFacadeSetSelection).not.toHaveBeenCalled();
  });

  it("move commit (pointerup): mirrors setSelection once with the final host geometry", async () => {
    localStorage.setItem("photrez.facade", "1");
    const engine = createMockEngine(["snapshot"]);
    (engine as unknown as { getSelection: unknown }).getSelection = vi.fn(() => ({ x: 150, y: 150, width: 200, height: 150, angle: 0 }));
    const ctx = createToolContext({
      selectionBounds: { x: 50, y: 50, width: 200, height: 150 },
      onSelectionMoved: vi.fn(),
    });

    handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
    handlePointerUp("selection", 200, 200, engine, createMockHistory(), vi.fn(), ctx);
    await tick();

    expect(ctx.onSelectionMoved).toHaveBeenCalled();
    expect(h.commitFacadeSetSelection).toHaveBeenCalledTimes(1);
    expect((h.commitFacadeSetSelection.mock.calls[0] as unknown[])[1]).toMatchObject({ x: 150, y: 150, width: 200, height: 150 });
  });
});
