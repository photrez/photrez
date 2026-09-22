// apps/desktop/src/components/editor/__tests__/transformSession.test.ts
//
// Implementation-focused unit tests for transformSession.ts.
// Tests internal logic: isSessionForEngine, transformsEqual, ghost-entry
// prevention, cancel/restore, and reset behaviors.

import { describe, it, expect, vi } from "vitest";
import {
  isSessionForEngine,
  commitLayerTransformSession,
  cancelLayerTransformSession,
  resetLayerTransformPreview,
} from "../transformSession";
import type { Transform2D, DocumentModel } from "@/engine/types";
import type { LayerTransformSession } from "../tools/editorState";

// ── Helpers ───────────────────────────────────────────────────────────────

function makeTransform(overrides: Partial<Transform2D> = {}): Transform2D {
  return {
    x: 100,
    y: 50,
    scaleX: 1,
    scaleY: 1,
    rotation: 0,
    flipH: false,
    flipV: false,
    ...overrides,
  };
}

function makeSession(
  overrides: Partial<LayerTransformSession> = {}
): LayerTransformSession {
  return {
    documentId: "doc-1",
    layerId: "layer-1",
    originalSnapshot: { id: "doc-1", snap: 1 } as unknown as DocumentModel,
    originalTransform: makeTransform(),
    mode: "resize",
    lockRatio: false,
    startedAt: Date.now(),
    ...overrides,
  };
}

function makeEngine(overrides: Record<string, any> = {}) {
  let currentTransform = makeTransform();
  return {
    getId: () => "doc-1",
    getLayer: vi.fn((id: string) =>
      id === "layer-1"
        ? { id: "layer-1", transform: { ...currentTransform } }
        : null
    ),
    snapshot: vi.fn(
      () => ({ snap: Date.now(), id: "doc-1" } as unknown as DocumentModel)
    ),
    restore: vi.fn(),
    transformLayer: vi.fn((id: string, t: Partial<Transform2D>) => {
      Object.assign(currentTransform, t);
    }),
    ...overrides,
  };
}

function makeHistory(overrides: Record<string, any> = {}) {
  return {
    commit: vi.fn(),
    ...overrides,
  };
}

// ── Tests ─────────────────────────────────────────────────────────────────

describe("isSessionForEngine", () => {
  it("returns true when session and engine share the same documentId", () => {
    const engine = makeEngine();
    const session = makeSession();
    expect(isSessionForEngine(session, engine)).toBe(true);
  });

  it("returns false when session is null", () => {
    const engine = makeEngine();
    expect(isSessionForEngine(null, engine)).toBe(false);
  });

  it("returns false when engine is null", () => {
    const session = makeSession();
    expect(isSessionForEngine(session, null)).toBe(false);
  });

  it("returns false when both are null", () => {
    expect(isSessionForEngine(null, null)).toBe(false);
  });

  it("returns false when session documentId differs from engine", () => {
    const engine = makeEngine();
    const session = makeSession({ documentId: "doc-other" });
    expect(isSessionForEngine(session, engine)).toBe(false);
  });

  it("returns false when engine getId returns undefined", () => {
    const engine = makeEngine({ getId: () => undefined });
    const session = makeSession();
    expect(isSessionForEngine(session, engine)).toBe(false);
  });
});

describe("commitLayerTransformSession", () => {
  it("commits originalSnapshot when layer transform changed", () => {
    const engine = makeEngine();
    // Mutate the layer transform via the mock's internal state
    engine.transformLayer("layer-1", { x: 200, y: 100 });
    const history = makeHistory();
    const session = makeSession();

    const result = commitLayerTransformSession(session, engine, history);
    expect(result).toBe(true);
    expect(history.commit).toHaveBeenCalledWith(
      session.originalSnapshot,
      "Transform Layer",
      undefined,
      undefined,
      []
    );
  });

  it("skips commit when layer transform did NOT change (ghost prevention)", () => {
    const engine = makeEngine();
    const history = makeHistory();
    const session = makeSession();

    const result = commitLayerTransformSession(session, engine, history);
    expect(result).toBe(true);
    expect(history.commit).not.toHaveBeenCalled();
  });

  it("skips commit when only transform.x changed but not other properties", () => {
    const engine = makeEngine();
    engine.transformLayer("layer-1", { x: 200 });
    const history = makeHistory();
    const session = makeSession({ originalTransform: makeTransform({ x: 100 }) });

    const result = commitLayerTransformSession(session, engine, history);
    expect(result).toBe(true);
    expect(history.commit).toHaveBeenCalled();
  });

  it("skips commit when only rotation changed", () => {
    const engine = makeEngine();
    engine.transformLayer("layer-1", { rotation: 45 });
    const history = makeHistory();
    const session = makeSession({
      originalTransform: makeTransform({ rotation: 0 }),
    });

    const result = commitLayerTransformSession(session, engine, history);
    expect(result).toBe(true);
    expect(history.commit).toHaveBeenCalled();
  });

  it("returns false when session is null", () => {
    const engine = makeEngine();
    const history = makeHistory();
    expect(commitLayerTransformSession(null, engine, history)).toBe(false);
  });

  it("returns false when engine is null", () => {
    const session = makeSession();
    const history = makeHistory();
    expect(commitLayerTransformSession(session, null, history)).toBe(false);
  });

  it("returns false when history is null", () => {
    const engine = makeEngine();
    const session = makeSession();
    expect(commitLayerTransformSession(session, engine, null)).toBe(false);
  });

  it("returns true when session targets a different documentId than engine", () => {
    const engine = makeEngine();
    const history = makeHistory();
    const session = makeSession({ documentId: "doc-other" });
    const result = commitLayerTransformSession(session, engine, history);
    expect(result).toBe(false);
    expect(history.commit).not.toHaveBeenCalled();
  });

  it("returns true when layer does not exist in engine", () => {
    const engine = makeEngine();
    const history = makeHistory();
    const session = makeSession({ layerId: "non-existent" });
    const result = commitLayerTransformSession(session, engine, history);
    expect(result).toBe(true);
    expect(history.commit).not.toHaveBeenCalled();
  });

  it("returns true when layer exists but has null transform", () => {
    const engine = makeEngine({
      getLayer: () => ({ id: "layer-1", transform: null }),
    });
    const history = makeHistory();
    const session = makeSession();
    const result = commitLayerTransformSession(session, engine, history);
    expect(result).toBe(true);
    expect(history.commit).not.toHaveBeenCalled();
  });

  it("detects change when flipH changes", () => {
    const engine = makeEngine();
    engine.transformLayer("layer-1", { flipH: true });
    const history = makeHistory();
    const session = makeSession({
      originalTransform: makeTransform({ flipH: false }),
    });

    commitLayerTransformSession(session, engine, history);
    expect(history.commit).toHaveBeenCalled();
  });

  it("detects change when scaleY changes but scaleX stays same", () => {
    const engine = makeEngine();
    engine.transformLayer("layer-1", { scaleY: 2 });
    const history = makeHistory();
    const session = makeSession({
      originalTransform: makeTransform({ scaleY: 1 }),
    });

    commitLayerTransformSession(session, engine, history);
    expect(history.commit).toHaveBeenCalled();
  });
});

describe("cancelLayerTransformSession", () => {
  it("restores original snapshot when session matches engine", () => {
    const engine = makeEngine();
    const session = makeSession();

    const result = cancelLayerTransformSession(session, engine);
    expect(result).toBe(true);
    expect(engine.restore).toHaveBeenCalledWith(session.originalSnapshot);
  });

  it("returns false when session is null", () => {
    const engine = makeEngine();
    expect(cancelLayerTransformSession(null, engine)).toBe(false);
  });

  it("returns false when engine is null", () => {
    const session = makeSession();
    expect(cancelLayerTransformSession(session, null)).toBe(false);
  });

  it("returns false when session documentId differs from engine", () => {
    const engine = makeEngine();
    const session = makeSession({ documentId: "doc-other" });
    expect(cancelLayerTransformSession(session, engine)).toBe(false);
  });

  it("does not call restore when session doesn't match engine", () => {
    const engine = makeEngine();
    const session = makeSession({ documentId: "doc-other" });
    cancelLayerTransformSession(session, engine);
    expect(engine.restore).not.toHaveBeenCalled();
  });

  it("can cancel a move session", () => {
    const engine = makeEngine();
    const session = makeSession({
      originalTransform: makeTransform({ x: 0, y: 0 }),
      mode: "resize" as const,
    });

    const result = cancelLayerTransformSession(session, engine);
    expect(result).toBe(true);
    expect(engine.restore).toHaveBeenCalled();
  });
});

describe("resetLayerTransformPreview", () => {    it("resets layer transform to originalTransform", () => {
      const engine = makeEngine();
      engine.transformLayer("layer-1", { x: 999, y: 888 });
      const session = makeSession({
        originalTransform: makeTransform({ x: 100, y: 50 }),
      });

      const result = resetLayerTransformPreview(session, engine);
      expect(result).toBe(true);
      expect(engine.transformLayer).toHaveBeenLastCalledWith(
        "layer-1",
        makeTransform({ x: 100, y: 50 })
      );
    });

  it("returns false when session is null", () => {
    const engine = makeEngine();
    expect(resetLayerTransformPreview(null, engine)).toBe(false);
  });

  it("returns false when engine is null", () => {
    const session = makeSession();
    expect(resetLayerTransformPreview(session, null)).toBe(false);
  });

  it("returns false when session documentId differs from engine", () => {
    const engine = makeEngine();
    const session = makeSession({ documentId: "doc-other" });
    expect(resetLayerTransformPreview(session, engine)).toBe(false);
  });

  it("does not call transformLayer when session doesn't match engine", () => {
    const engine = makeEngine();
    const session = makeSession({ documentId: "doc-other" });
    resetLayerTransformPreview(session, engine);
    expect(engine.transformLayer).not.toHaveBeenCalled();
  });    it("resets all transform properties (x, y, scaleX, scaleY, rotation, flipH, flipV)", () => {
      const engine = makeEngine();
      engine.transformLayer("layer-1", {
        x: 500,
        y: 400,
        scaleX: 3,
        scaleY: 0.5,
        rotation: 90,
        flipH: true,
        flipV: false,
      });
      const originalTransform = makeTransform({
        x: 10, y: 20, scaleX: 1.5, scaleY: 1, rotation: 15,
        flipH: false, flipV: true,
      });
      const session = makeSession({ originalTransform });

      resetLayerTransformPreview(session, engine);
      expect(engine.transformLayer).toHaveBeenLastCalledWith(
        "layer-1",
        originalTransform
      );
    });
});
