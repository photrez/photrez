// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * Floor convention on the jump route: panel index i must land the cursor at
 * i + floor exactly, in BOTH directions. Before this convention was single,
 * the stack rendered the baseline as a past row while the jump target added
 * the floor again - highlight one row high, clicks one entry off, and the top
 * row refused by the context guard.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { navigateHistoryNative } from "../nativeHistoryJump";
import { CommandHistory } from "@/engine/history";

vi.mock("@/lib/protocol/bridge_emu", () => ({ getHistoryQuery: vi.fn() }));
vi.mock("../facadeHistoryHandoff", () => ({
  runFacadeExternalHandoff: vi.fn(async () => true),
  handoffMovedCursor: vi.fn(() => true),
}));

import { getHistoryQuery } from "@/lib/protocol/bridge_emu";
import { runFacadeExternalHandoff, handoffMovedCursor } from "../facadeHistoryHandoff";

const QUERY_MOCK = getHistoryQuery as unknown as ReturnType<typeof vi.fn>;
const HANDOFF_MOCK = runFacadeExternalHandoff as unknown as ReturnType<typeof vi.fn>;
const MOVED_MOCK = handoffMovedCursor as unknown as ReturnType<typeof vi.fn>;

const baseline = (seq: number) => ({
  seq, groupId: seq, origin: "native", label: "Set Background Flag",
  affectedLayerIds: [], versionBefore: seq - 1, versionAfter: seq, memoryCostBytes: 0,
});
const user = (seq: number, label: string) => ({
  seq, groupId: seq, origin: "native", label,
  affectedLayerIds: [], versionBefore: seq - 1, versionAfter: seq, memoryCostBytes: 0,
});

function makeHost() {
  const history = new CommandHistory();
  history.attachDocIdGetter(() => "docJumpFloor");
  return {
    workspace: {
      getActiveEngine: () => ({ snapshot: () => ({}), restore: vi.fn() }),
      getActiveHistory: () => history,
      getActiveDocumentId: () => "docJumpFloor",
      notifyVisualChange: vi.fn(),
    },
    renderer: {},
    scheduler: { requestRender: vi.fn() },
  } as never;
}

beforeEach(() => {
  QUERY_MOCK.mockReset();
  HANDOFF_MOCK.mockReset();
  HANDOFF_MOCK.mockImplementation(async () => true);
  MOVED_MOCK.mockReset();
  MOVED_MOCK.mockImplementation(() => true);
});

describe("navigateHistoryNative lands on cursor == row + floor", () => {
  it("cursor 3, floor 1: clicking row 0 steps undo twice (lands on cursor 1)", async () => {
    QUERY_MOCK.mockResolvedValue({
      cursor: 3,
      lastSeq: 4,
      degradedHint: false,
      pendingExternal: null,
      entries: [baseline(1), user(2, "Crop"), user(3, "Move"), user(4, "Fill")],
    });
    await navigateHistoryNative(makeHost(), 0);

    expect(HANDOFF_MOCK).toHaveBeenCalledTimes(2);
    expect(HANDOFF_MOCK.mock.calls.every((c) => c[1] === "undo")).toBe(true);
  });

  it("cursor 3, floor 1: the top row is clickable and steps redo once (lands on cursor 4)", async () => {
    QUERY_MOCK.mockResolvedValue({
      cursor: 3,
      lastSeq: 4,
      degradedHint: false,
      pendingExternal: null,
      entries: [baseline(1), user(2, "Crop"), user(3, "Move"), user(4, "Fill")],
    });
    await navigateHistoryNative(makeHost(), 3);

    expect(HANDOFF_MOCK).toHaveBeenCalledTimes(1);
    expect(HANDOFF_MOCK.mock.calls[0][1]).toBe("redo");
  });

  it("floor 0: row index equals cursor directly", async () => {
    QUERY_MOCK.mockResolvedValue({
      cursor: 2,
      lastSeq: 2,
      degradedHint: false,
      pendingExternal: null,
      entries: [user(1, "Crop"), user(2, "Move")],
    });
    await navigateHistoryNative(makeHost(), 0);

    expect(HANDOFF_MOCK).toHaveBeenCalledTimes(2);
    expect(HANDOFF_MOCK.mock.calls.every((c) => c[1] === "undo")).toBe(true);
  });
});
