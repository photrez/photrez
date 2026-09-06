// Regression coverage for the async facade commit migration leak (reviewer find).
// Before the fix, TransformOptionBar.apply() did `const snap = f.commitTransform()`
// (a Promise) and then `if (snap)` (always true) -> `applyFacadeSnapshot(Promise)`,
// which throws `snapshot.layers is not iterable` in production and never clears the
// session's preview. This test proves the handler now passes a REAL snapshot object
// (with an iterable `.layers`) to applyFacadeSnapshot and still clears the session.
import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { createSignal } from "solid-js";
import { TransformOptionBar } from "../TransformOptionBar";
import { mockUseEditor } from "@/__tests__/mockUseEditor";

const hoisted = vi.hoisted(() => ({
  snapshot: { version: 3, layers: [{ id: "layer-1", name: "Layer 1" }] },
}));

vi.mock("@/engine/document", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isFacadeOwnedLayer: () => true,
}));

vi.mock("@/lib/protocol/facadeRegistry", () => {
  const commitTransform = vi.fn(() => Promise.resolve(hoisted.snapshot));
  return {
    isFacadeEnabled: () => true,
    getFacade: () => ({ commitTransform }),
    setTransformPreview: vi.fn(),
    clearTransformPreview: vi.fn(),
  };
});

describe("TransformOptionBar facade-owned Apply (async leak regression)", () => {
  afterEach(() => {
    document.body.replaceChildren();
    vi.restoreAllMocks();
  });

  it("Apply on a facade-owned layer passes a real snapshot to applyFacadeSnapshot (not a Promise) and clears the session", async () => {
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
      { id: "layer-1", name: "Layer 1", transform: { x: 50, y: 80, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false }, width: 100, height: 100, visible: true, locked: false },
    ];

    const applySpy = vi.fn();

    const mockActiveEngine = {
      getId: () => "doc-1",
      getLayer: (id: string) => mockLayers().find((l: any) => l.id === id),
      applyFacadeSnapshot: applySpy,
      transformLayer: vi.fn(),
      restore: vi.fn(),
      snapshot: () => ({}),
    };

    const setSessionSpy = vi.fn((val: unknown) => setLayerTransformSession(val));

    const mockValue = {
      workspace: {
        getActiveEngine: () => mockActiveEngine,
        getActiveHistory: () => ({ commit: vi.fn() }),
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

    const applyBtn = Array.from(container.querySelectorAll("button")).find(
      (b) => b.textContent === "Apply",
    ) as HTMLButtonElement;
    expect(applyBtn).toBeTruthy();

    // The handler is async; let the commitTransform() promise resolve.
    applyBtn.click();
    await new Promise((r) => setTimeout(r, 0));

    expect(applySpy).toHaveBeenCalledTimes(1);
    const received = applySpy.mock.calls[0][0];
    expect(received).not.toBeInstanceOf(Promise);
    expect(received).toBe(hoisted.snapshot);
    expect(Array.isArray((received as any).layers)).toBe(true);
    expect((received as any).layers.length).toBe(1);
    expect((received as any).version).toBe(3);
    // Session is still cleared (preview torn down) on the facade path.
    expect(setSessionSpy).toHaveBeenCalledWith(null);

    dispose();
    container.parentNode?.removeChild(container);
  });
});
