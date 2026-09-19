import { describe, it, expect, vi } from "vitest";
import { handlePointerDown, handlePointerMove, handlePointerUp } from "../viewport/input-handler";
import { createMockEngine, createMockHistory, createToolContext } from "./test-builders";

// Selection draw is transient: per-move only updates the preview signal
// (onSelectionCreated), and exactly one engine.createSelection commits at
// pointerup. A per-move commit would pay a Rust round-trip + JSON parse +
// full notifyChange at pointer rate.
describe("selection draw transient commit (interaction seam)", () => {
  it("N moves commit nothing; pointerup commits once with the final rect", () => {
    const engine = createMockEngine(["createSelection", "clearSelection", "snapshot"]);
    const history = createMockHistory();
    const preview = vi.fn();
    const ctx = createToolContext({ selectedLayerId: null, onSelectionCreated: preview });
    handlePointerDown("selection", 100, 100, engine, history, vi.fn(), ctx);
    expect(ctx.dragMode).toBe("draw");
    const N = 10;
    for (let i = 1; i <= N; i++) {
      handlePointerMove("selection", 100 + 20 * i, 100 + 10 * i, engine, vi.fn(), ctx);
    }
    // Down + N moves all stay on the transient channel.
    expect(preview).toHaveBeenCalledTimes(N + 1);
    expect(engine.createSelection).not.toHaveBeenCalled();
    handlePointerUp("selection", 300, 200, engine, history, vi.fn(), ctx);
    expect(engine.createSelection).toHaveBeenCalledTimes(1);
    expect(engine.createSelection).toHaveBeenCalledWith(100, 100, 200, 100);
    expect(ctx.isDragging).toBe(false);
  });

  it("tiny drag clears with no committed selection", () => {
    const engine = createMockEngine(["createSelection", "clearSelection", "snapshot"]);
    const history = createMockHistory();
    const ctx = createToolContext({ selectedLayerId: null, onSelectionCreated: vi.fn() });
    handlePointerDown("selection", 100, 100, engine, history, vi.fn(), ctx);
    handlePointerMove("selection", 101, 101, engine, vi.fn(), ctx);
    expect(engine.createSelection).not.toHaveBeenCalled();
    handlePointerUp("selection", 101, 101, engine, history, vi.fn(), ctx);
    expect(engine.createSelection).not.toHaveBeenCalled();
    expect(engine.clearSelection).toHaveBeenCalledTimes(1);
  });
});
