import { describe, it, expect, vi, afterEach } from "vitest";
import { render } from "solid-js/web";
import { createSignal } from "solid-js";
import { TransformOptionBar } from "../TransformOptionBar";
import { mockUseEditor } from "@/__tests__/mockUseEditor";

function qs<T extends HTMLElement>(root: HTMLElement, sel: string): T | null {
  return root.querySelector(sel) as T | null;
}

function mockSession(overrides: Record<string, unknown> = {}) {
  return {
    documentId: "doc-1",
    layerId: "layer-1",
    originalSnapshot: {},
    originalTransform: { x: 10, y: 20, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    mode: "resize",
    lockRatio: false,
    startedAt: Date.now(),
    ...overrides,
  };
}

function makeMockValue(overrides: Record<string, unknown> = {}) {
  const [layerTransformSession, setLayerTransformSession] = createSignal<any>(mockSession());
  const mockLayers = () => [
    {
      id: "layer-1",
      name: "Layer 1",
      transform: { x: 10, y: 20, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      width: 100,
      height: 100,
      visible: true,
      locked: false,
    },
  ];
  const mockActiveEngine = {
    getId: () => "doc-1",
    getLayer: (id: string) => mockLayers().find((l: any) => l.id === id),
    transformLayer: vi.fn(),
    restore: vi.fn(),
    snapshot: () => ({}),
  };
  const mockActiveHistory = { commit: vi.fn() };
  return {
    workspace: {
      getActiveEngine: () => mockActiveEngine,
      getActiveHistory: () => mockActiveHistory,
    },
    scheduler: { requestRender: vi.fn() },
    activeLayerId: () => "layer-1",
    layerTransformSession,
    setLayerTransformSession,
    constrainRatio: () => false,
    setConstrainRatio: vi.fn(),
    ...overrides,
  };
}

/** Simulate typing a value into an EditableNumField and pressing Enter */
function typeAndSubmit(input: HTMLInputElement, val: string) {
  input.focus();
  input.dispatchEvent(new Event("focus", { bubbles: true }));
  (input as HTMLInputElement).value = val;
  input.dispatchEvent(new InputEvent("input", { bubbles: true }));
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
}

describe("TransformOptionBar", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("renders Transform pill, mode badge, Apply, Cancel when session exists", () => {
    const [layerTransformSession, setLayerTransformSession] = createSignal<any>({
      documentId: "doc-1",
      layerId: "layer-1",
      originalSnapshot: {},
      originalTransform: { x: 10, y: 20, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      mode: "resize",
      lockRatio: false,
      startedAt: Date.now(),
    });

    const mockLayers = () => [
      { id: "layer-1", name: "Layer 1", transform: { x: 10, y: 20, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false }, width: 100, height: 100, visible: true, locked: false }
    ];

    const mockActiveEngine = {
      getId: () => "doc-1",
      getLayer: (id: string) => mockLayers().find(l => l.id === id),
      transformLayer: vi.fn(),
      restore: vi.fn(),
      snapshot: () => ({}),
    };

    const mockActiveHistory = {
      commit: vi.fn(),
    };

    const mockValue = {
      workspace: {
        getActiveEngine: () => mockActiveEngine,
        getActiveHistory: () => mockActiveHistory,
      },
      scheduler: { requestRender: vi.fn() },
      activeLayerId: () => "layer-1",
      layerTransformSession,
      setLayerTransformSession,
      constrainRatio: () => false,
      setConstrainRatio: vi.fn(),
    };

    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(() => <TransformOptionBar />, container);

    expect(container.textContent).toContain("Transform");

    const buttons = container.querySelectorAll("button");
    const applyBtn = Array.from(buttons).find(b => b.textContent === "Apply");
    const cancelBtn = Array.from(buttons).find(b => b.textContent === "Cancel");

    expect(applyBtn).toBeDefined();
    expect(cancelBtn).toBeDefined();

    dispose();
    container.parentNode?.removeChild(container);
  });

  it("Apply calls commit and clears session", () => {
    const [layerTransformSession, setLayerTransformSession] = createSignal<any>({
      documentId: "doc-1",
      layerId: "layer-1",
      originalSnapshot: { id: "original" },
      // originalTransform is what the layer WAS at session start; the current
      // layer transform (below) is what it is NOW (after user edited). The
      // session commits only when these differ — which mirrors the real
      // production guard we added 2026-06-18 to prevent ghost undo entries
      // from "Apply" on an unchanged session.
      originalTransform: { x: 10, y: 20, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      mode: "resize",
      lockRatio: false,
      startedAt: Date.now(),
    });

    const mockLayers = () => [
      { id: "layer-1", name: "Layer 1", transform: { x: 50, y: 80, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false }, width: 100, height: 100, visible: true, locked: false }
    ];

    const mockActiveEngine = {
      getId: () => "doc-1",
      getLayer: (id: string) => mockLayers().find(l => l.id === id),
      transformLayer: vi.fn(),
      restore: vi.fn(),
      snapshot: () => ({}),
    };

    const mockActiveHistory = {
      commit: vi.fn(),
    };

    const setSessionSpy = vi.fn((val) => setLayerTransformSession(val));

    const mockValue = {
      workspace: {
        getActiveEngine: () => mockActiveEngine,
        getActiveHistory: () => mockActiveHistory,
      },
      scheduler: { requestRender: vi.fn() },
      activeLayerId: () => "layer-1",
      layerTransformSession,
      setLayerTransformSession: setSessionSpy,
      constrainRatio: () => false,
      setConstrainRatio: vi.fn(),
    };

    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(() => <TransformOptionBar />, container);

    const buttons = container.querySelectorAll("button");
    const applyBtn = Array.from(buttons).find(b => b.textContent === "Apply") as HTMLButtonElement;
    applyBtn.click();

    expect(mockActiveHistory.commit).toHaveBeenCalledWith({ id: "original" }, "Transform Layer");
    expect(setSessionSpy).toHaveBeenCalledWith(null);

    dispose();
    container.parentNode?.removeChild(container);
  });

  it("Cancel calls restore and clears session", () => {
    const [layerTransformSession, setLayerTransformSession] = createSignal<any>({
      documentId: "doc-1",
      layerId: "layer-1",
      originalSnapshot: { id: "original" },
      originalTransform: { x: 10, y: 20, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      mode: "resize",
      lockRatio: false,
      startedAt: Date.now(),
    });

    const mockLayers = () => [
      { id: "layer-1", name: "Layer 1", transform: { x: 10, y: 20, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false }, width: 100, height: 100, visible: true, locked: false }
    ];

    const mockActiveEngine = {
      getId: () => "doc-1",
      getLayer: (id: string) => mockLayers().find(l => l.id === id),
      transformLayer: vi.fn(),
      restore: vi.fn(),
      snapshot: () => ({}),
    };

    const setSessionSpy = vi.fn((val) => setLayerTransformSession(val));

    const mockValue = {
      workspace: {
        getActiveEngine: () => mockActiveEngine,
        getActiveHistory: () => vi.fn() as any,
      },
      scheduler: { requestRender: vi.fn() },
      activeLayerId: () => "layer-1",
      layerTransformSession,
      setLayerTransformSession: setSessionSpy,
      constrainRatio: () => false,
      setConstrainRatio: vi.fn(),
    };

    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(() => <TransformOptionBar />, container);

    const buttons = container.querySelectorAll("button");
    const cancelBtn = Array.from(buttons).find(b => b.textContent === "Cancel") as HTMLButtonElement;
    cancelBtn.click();

    expect(mockActiveEngine.restore).toHaveBeenCalledWith({ id: "original" });
    expect(setSessionSpy).toHaveBeenCalledWith(null);

    dispose();
    container.parentNode?.removeChild(container);
  });

  it("Apply with UNCHANGED transform skips commit (regression: ghost undo entry)", () => {
    // Reproduces regression 2026-06-18: pressing Apply on a transform session
    // that the user never modified used to push a ghost entry to the undo
    // stack — undoing it would do nothing visible, making the history feel
    // unreliable. The fix in transformSession.ts skips the commit when the
    // layer transform matches the originalTransform.
    const sameTransform = { x: 10, y: 20, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false };
    const [layerTransformSession, setLayerTransformSession] = createSignal<any>({
      documentId: "doc-1",
      layerId: "layer-1",
      originalSnapshot: { id: "original" },
      originalTransform: sameTransform,
      mode: "resize",
      lockRatio: false,
      startedAt: Date.now(),
    });

    const mockLayers = () => [
      { id: "layer-1", name: "Layer 1", transform: sameTransform, width: 100, height: 100, visible: true, locked: false }
    ];

    const mockActiveEngine = {
      getId: () => "doc-1",
      getLayer: (id: string) => mockLayers().find(l => l.id === id),
      transformLayer: vi.fn(),
      restore: vi.fn(),
      snapshot: () => ({}),
    };

    const mockActiveHistory = { commit: vi.fn() };
    const setSessionSpy = vi.fn((val) => setLayerTransformSession(val));

    const mockValue = {
      workspace: {
        getActiveEngine: () => mockActiveEngine,
        getActiveHistory: () => mockActiveHistory,
      },
      scheduler: { requestRender: vi.fn() },
      activeLayerId: () => "layer-1",
      layerTransformSession,
      setLayerTransformSession: setSessionSpy,
      constrainRatio: () => false,
      setConstrainRatio: vi.fn(),
    };

    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <TransformOptionBar />, container);

    const buttons = container.querySelectorAll("button");
    const applyBtn = Array.from(buttons).find(b => b.textContent === "Apply") as HTMLButtonElement;
    applyBtn.click();

    // Session is cleared (so the user can exit transform mode), but NO ghost
    // entry was pushed to history.
    expect(mockActiveHistory.commit).not.toHaveBeenCalled();
    expect(setSessionSpy).toHaveBeenCalledWith(null);

    dispose();
    container.parentNode?.removeChild(container);
  });

  // ─── Field rendering ─────────────────────────────────────────────────

  it("renders X, Y, W, H, R input fields with layer values", () => {
    mockUseEditor(makeMockValue());
    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <TransformOptionBar />, container);

    expect(container.textContent).toContain("X");
    expect(container.textContent).toContain("Y");
    expect(container.textContent).toContain("W");
    expect(container.textContent).toContain("H");
    expect(container.textContent).toContain("R");
    expect(container.textContent).toContain("°");

    dispose();
    container.parentNode?.removeChild(container);
  });

  it("renders Lock Ratio toggle when session exists", () => {
    const [, setLayerTransformSession] = createSignal<any>(mockSession());
    const mv = makeMockValue({ setLayerTransformSession });
    mockUseEditor(mv);
    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <TransformOptionBar />, container);

    expect(container.textContent).toContain("Ratio");

    dispose();
    container.parentNode?.removeChild(container);
  });

  it("renders Reset Preview button", () => {
    mockUseEditor(makeMockValue());
    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <TransformOptionBar />, container);

    expect(container.textContent).toContain("Reset Preview");

    dispose();
    container.parentNode?.removeChild(container);
  });

  // ─── Input field submit ───────────────────────────────────────────────

  it("X field submit calls transformLayer with updated x", async () => {
    const mv = makeMockValue();
    mockUseEditor(mv);
    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <TransformOptionBar />, container);

    const inputs = container.querySelectorAll<HTMLInputElement>('input[type="text"]');
    const xInput = inputs[0];
    typeAndSubmit(xInput, "150");

    const engine = (mv.workspace as any).getActiveEngine();
    expect(engine.transformLayer).toHaveBeenCalledWith("layer-1",
      expect.objectContaining({ x: 150, y: 20 })
    );
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("Y field submit calls transformLayer with updated y", async () => {
    const mv = makeMockValue();
    mockUseEditor(mv);
    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <TransformOptionBar />, container);

    const inputs = container.querySelectorAll<HTMLInputElement>('input[type="text"]');
    const yInput = inputs[1];
    typeAndSubmit(yInput, "80");

    const engine = (mv.workspace as any).getActiveEngine();
    expect(engine.transformLayer).toHaveBeenCalledWith("layer-1",
      expect.objectContaining({ x: 10, y: 80 })
    );
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("R field submit calls transformLayer with updated rotation", async () => {
    const mv = makeMockValue();
    mockUseEditor(mv);
    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <TransformOptionBar />, container);

    const inputs = container.querySelectorAll<HTMLInputElement>('input[type="text"]');
    const rInput = inputs[4]; // 5th field (X, Y, W, H, R)
    typeAndSubmit(rInput, "45");

    const engine = (mv.workspace as any).getActiveEngine();
    expect(engine.transformLayer).toHaveBeenCalledWith("layer-1",
      expect.objectContaining({ x: 10, y: 20, rotation: 45 })
    );
    dispose();
    container.parentNode?.removeChild(container);
  });

  // ─── Locked layer ─────────────────────────────────────────────────────

  it("disables inputs when layer is locked", () => {
    const [layerTransformSession] = createSignal<any>(mockSession());
    const mockLayers = () => [
      {
        id: "layer-1", name: "Layer 1",
        transform: { x: 10, y: 20, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
        width: 100, height: 100, visible: true, locked: true,
      },
    ];
    const mockActiveEngine = {
      getId: () => "doc-1",
      getLayer: (id: string) => mockLayers().find((l: any) => l.id === id),
      transformLayer: vi.fn(),
      restore: vi.fn(),
      snapshot: () => ({}),
    };
    const mv = {
      workspace: { getActiveEngine: () => mockActiveEngine, getActiveHistory: () => ({ commit: vi.fn() }) },
      scheduler: { requestRender: vi.fn() },
      activeLayerId: () => "layer-1",
      layerTransformSession,
      setLayerTransformSession: vi.fn(),
      constrainRatio: () => false,
      setConstrainRatio: vi.fn(),
    };
    mockUseEditor(mv);
    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <TransformOptionBar />, container);

    const inputs = container.querySelectorAll<HTMLInputElement>('input[type="text"]');
    for (const input of Array.from(inputs)) {
      expect(input.closest('[class*="pointer-events-none"]')).not.toBeNull();
    }
    dispose();
    container.parentNode?.removeChild(container);
  });

  // ─── Arrow key nudge ──────────────────────────────────────────────────

  it("ArrowUp on X field nudges value up by 1 and calls transformLayer", async () => {
    const mv = makeMockValue();
    mockUseEditor(mv);
    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <TransformOptionBar />, container);

    const inputs = container.querySelectorAll<HTMLInputElement>('input[type="text"]');
    const xInput = inputs[0];
    xInput.focus();
    xInput.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp", bubbles: true }));
    // ArrowUp formats: 10 + 1 = 11, displayed as "11"
    expect(xInput.value).toBe("11");
    const engine = (mv.workspace as any).getActiveEngine();
    expect(engine.transformLayer).toHaveBeenCalledWith("layer-1",
      expect.objectContaining({ x: 11 })
    );
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("Shift+ArrowUp on X field nudges value up by 10", async () => {
    const mv = makeMockValue();
    mockUseEditor(mv);
    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <TransformOptionBar />, container);

    const inputs = container.querySelectorAll<HTMLInputElement>('input[type="text"]');
    const xInput = inputs[0];
    xInput.focus();
    xInput.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp", shiftKey: true, bubbles: true }));
    expect(xInput.value).toBe("20");
    const engine = (mv.workspace as any).getActiveEngine();
    expect(engine.transformLayer).toHaveBeenCalledWith("layer-1",
      expect.objectContaining({ x: 20 })
    );
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("ArrowDown on Y field nudges value down by 1", async () => {
    const mv = makeMockValue();
    mockUseEditor(mv);
    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <TransformOptionBar />, container);

    const inputs = container.querySelectorAll<HTMLInputElement>('input[type="text"]');
    const yInput = inputs[1];
    yInput.focus();
    yInput.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    expect(yInput.value).toBe("19");
    const engine = (mv.workspace as any).getActiveEngine();
    expect(engine.transformLayer).toHaveBeenCalledWith("layer-1",
      expect.objectContaining({ y: 19 })
    );
    dispose();
    container.parentNode?.removeChild(container);
  });
});
