// apps/desktop/src/components/editor/__tests__/saveState.test.ts
//
// Contract tests for the shared save queue:
//   - saves run serially (one at a time)
//   - a queued manual save replaces the pending slot (rapid Ctrl+S)
//   - low-priority (autosave) jobs are skipped while any save is active

import { describe, expect, it, vi, beforeEach } from "vitest";

const { scheduleSave, isSaveRunning } = await import("../saveState");

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((r) => { resolve = r; });
  return { promise, resolve };
}

describe("saveState save queue", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("runs saves serially and drains the queue", async () => {
    const order: string[] = [];
    const first = deferred();
    const second = deferred();

    scheduleSave(async () => { order.push("a-start"); await first.promise; order.push("a-end"); });
    scheduleSave(async () => { order.push("b"); second.resolve(); });

    expect(order).toEqual(["a-start"]); // second job waits
    first.resolve();
    await second.promise;
    await vi.waitFor(() => expect(order).toEqual(["a-start", "a-end", "b"]));
    expect(isSaveRunning()).toBe(false);
  });

  it("a second manual save replaces the pending slot (no duplicate run)", async () => {
    const order: string[] = [];
    const first = deferred();

    scheduleSave(async () => { order.push("first"); await first.promise; });
    scheduleSave(async () => { order.push("second") });

    first.resolve();
    await vi.waitFor(() => expect(order).toEqual(["first", "second"]));
  });

  it("low-priority (autosave) jobs are skipped while a save is active", async () => {
    const order: string[] = [];
    const first = deferred();

    scheduleSave(async () => { order.push("manual"); await first.promise; });
    scheduleSave(async () => { order.push("autosave") }, true); // must be dropped

    first.resolve();
    await vi.waitFor(() => expect(order).toEqual(["manual"]));
  });

  it("a low-priority job runs when nothing else is active", async () => {
    const ran = vi.fn();
    await scheduleSave(async () => ran(), true);
    expect(ran).toHaveBeenCalledTimes(1);
  });
});
