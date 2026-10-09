// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * Floor convention: panel rows, undo/redo counts, and the handoff refusal
 * gate all derive the document-open baseline from the STREAM CONTENT
 * (entries[0] shape), never from a host-side sticky boolean.
 *
 * Panel row alignment: panel row i must correspond to cursor i + floor
 * exactly - the stack skips sub-floor entries, the top row is clickable,
 * and the highlight (getUndoCount()) marks the true current row.
 * Empty-stack pops: undo()/redo() with a natively-authoritative cache and
 * an empty TS stack must return null, never pop a non-existent entry.
 * Eviction follows the stream: eviction drops entries[0], and the derived
 * floor follows the stream, so the oldest surviving USER entry stays
 * undoable. Duplicate baselines over-count in the sticky boolean; the
 * stream-derived count handles them.
 * Authority transitions are announced, never silent.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { CommandHistory } from "../history";
import { historyViewAuthority } from "../nativeHistoryView";
import { recordOpenBaselineEntry } from "@/lib/protocol/backgroundFlagRouting";

vi.mock("@/lib/protocol/bridge_emu", () => ({ getHistoryQuery: vi.fn() }));

import { getHistoryQuery } from "@/lib/protocol/bridge_emu";

const QUERY_MOCK = getHistoryQuery as unknown as ReturnType<typeof vi.fn>;

const baseline = (seq: number) => ({
  seq,
  groupId: seq,
  origin: "native",
  label: "Set Background Flag",
  affectedLayerIds: [],
  versionBefore: seq - 1,
  versionAfter: seq,
  memoryCostBytes: 0,
});

const user = (seq: number, label: string) => ({
  seq,
  groupId: seq,
  origin: "native",
  label,
  affectedLayerIds: [],
  versionBefore: seq - 1,
  versionAfter: seq,
  memoryCostBytes: 0,
});

function makeHistory(query: unknown) {
  QUERY_MOCK.mockResolvedValue(query);
  const history = new CommandHistory();
  history.attachDocIdGetter(() => "docFloor");
  return history;
}

beforeEach(() => {
  QUERY_MOCK.mockReset();
  vi.spyOn(console, "info").mockImplementation(() => {});
  vi.spyOn(console, "warn").mockImplementation(() => {});
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("the stack derives its rows from the same cursor arithmetic the jump uses", () => {
  it("floor=1: the baseline row is skipped and the highlight is the true current row", async () => {
    const history = makeHistory({
      cursor: 3,
      lastSeq: 4,
      degradedHint: false,
      pendingExternal: null,
      entries: [baseline(1), user(2, "Crop"), user(3, "Move"), user(4, "Fill")],
    });
    await history.resyncFromNativeQuery();

    expect(history.getUndoCount()).toBe(2);
    expect(history.getRedoCount()).toBe(1);
    // Row index == getUndoCount() marks the true current row; the baseline
    // is not a panel row.
    const rows = history.getHistoryStack();
    expect(rows.map((r) => r.label)).toEqual(["Open", "Crop", "Move", "Fill"]);
    expect(rows[0].isRedo).toBe(false);
    expect(rows[3].isRedo).toBe(true);
  });

  it("floor=0: no baseline, every row is user work", async () => {
    const history = makeHistory({
      cursor: 2,
      lastSeq: 2,
      degradedHint: false,
      pendingExternal: null,
      entries: [user(1, "Crop"), user(2, "Move")],
    });
    await history.resyncFromNativeQuery();

    expect(history.getUndoCount()).toBe(2);
    expect(history.getRedoCount()).toBe(0);
    expect(history.getHistoryStack().map((r) => r.label)).toEqual(["Open", "Crop", "Move"]);
  });
});

describe("the floor follows the stream, not a sticky boolean", () => {
  it("an evicted baseline under the depth cap keeps every surviving step undoable", async () => {
    // Post-eviction shape under max_depth 50: the baseline entry is gone, the
    // oldest survivor is a user-owned pixel entry (Rust labels pixel steps
    // "pixel"), the stream holds exactly 50 entries, and the cursor sits at
    // its end. 51 entries + a leading baseline is not a shape the stream can
    // actually hold at that cap.
    const pixels = user(1, "pixel");
    const entries = [pixels, ...Array.from({ length: 49 }, (_, i) => user(i + 2, `Stroke ${i + 1}`))];
    // The sticky flag says the baseline exists; the stream says otherwise.
    recordOpenBaselineEntry("docFloor");
    const history = makeHistory({
      cursor: 50,
      lastSeq: 50,
      degradedHint: false,
      pendingExternal: null,
      entries,
    });
    await history.resyncFromNativeQuery();

    expect(history.getUndoCount(), "no floor should be subtracted after eviction").toBe(50);
    expect(history.getHistoryStack().length).toBe(51); // Open + 50 rows
  });

  it("a duplicate baseline counts twice, so the first user step stays undoable", async () => {
    const history = makeHistory({
      cursor: 3,
      lastSeq: 3,
      degradedHint: false,
      pendingExternal: null,
      entries: [baseline(1), baseline(2), user(3, "Crop")],
    });
    await history.resyncFromNativeQuery();

    expect(history.getUndoCount()).toBe(1);
    expect(history.getHistoryStack().map((r) => r.label)).toEqual(["Open", "Crop"]);
  });
});

describe("undo()/redo() never pop past the end of a natively-authoritative empty stack", () => {
  it("undo returns null instead of throwing when the TS stack holds no payload", async () => {
    const history = makeHistory({
      cursor: 2,
      lastSeq: 2,
      degradedHint: false,
      pendingExternal: null,
      entries: [baseline(1), user(2, "Crop")],
    });
    await history.resyncFromNativeQuery();
    expect(history.canUndo()).toBe(true);

    let threw: unknown = null;
    let result: unknown = "unset";
    try {
      result = history.undo({ layers: [] } as never);
    } catch (e) {
      threw = e;
    }
    expect(threw).toBeNull();
    expect(result).toBeNull();
  });

  it("redo returns null instead of throwing when the TS stack holds no payload", async () => {
    const history = makeHistory({
      cursor: 1,
      lastSeq: 2,
      degradedHint: false,
      pendingExternal: null,
      entries: [baseline(1), user(2, "Crop")],
    });
    await history.resyncFromNativeQuery();
    expect(history.canRedo()).toBe(true);

    let threw: unknown = null;
    let result: unknown = "unset";
    try {
      result = history.redo({ layers: [] } as never);
    } catch (e) {
      threw = e;
    }
    expect(threw).toBeNull();
    expect(result).toBeNull();
  });
});

describe("commit refreshes the derived view (no dead undo button)", () => {
  it("a commit re-reads the native query so the cache stops serving pre-stroke counts", async () => {
    // First read: just the document-open baseline - nothing undoable.
    QUERY_MOCK.mockResolvedValueOnce({
      cursor: 1,
      lastSeq: 1,
      degradedHint: false,
      pendingExternal: null,
      entries: [baseline(1)],
    });
    const history = new CommandHistory();
    history.attachDocIdGetter(() => "docFloor");
    await history.resyncFromNativeQuery();
    expect(history.canUndo()).toBe(false);

    // A new entry arrives in the native stream, then the user commits locally:
    // the commit must trigger a re-read, or canUndo keeps saying no over live work.
    QUERY_MOCK.mockResolvedValue({
      cursor: 2,
      lastSeq: 2,
      degradedHint: false,
      pendingExternal: null,
      entries: [baseline(1), user(2, "Crop")],
    });
    history.commit({ layers: [] } as never, "Crop");
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));

    expect(history.canUndo(), "the cached stream no longer masks the committed work").toBe(true);
    expect(history.getUndoCount()).toBe(1);
  });
});

describe("authority transitions are announced, never silent", () => {
  it("adopting, then losing, the native view logs each transition exactly once", async () => {
    const history = makeHistory({
      cursor: 0,
      lastSeq: 0,
      degradedHint: false,
      pendingExternal: null,
      entries: [],
    });
    await history.resyncFromNativeQuery();
    history.getUndoCount();
    await history.resyncFromNativeQuery(); // empty again: no second announcement
    history.getUndoCount();
    expect(console.info).not.toHaveBeenCalled();
    expect(console.warn).not.toHaveBeenCalled();

    QUERY_MOCK.mockResolvedValue({
      cursor: 1,
      lastSeq: 1,
      degradedHint: false,
      pendingExternal: null,
      entries: [baseline(1)],
    });
    await history.resyncFromNativeQuery();
    history.getUndoCount();
    expect(console.info).toHaveBeenCalledTimes(1);
    // A real consumer, not a log-and-forget: the published signal flips with
    // the same transition the console reports.
    expect(historyViewAuthority()).toBe("native");

    QUERY_MOCK.mockResolvedValue({
      cursor: 0,
      lastSeq: 0,
      degradedHint: false,
      pendingExternal: null,
      entries: [],
    });
    await history.resyncFromNativeQuery();
    history.getUndoCount();
    expect(console.warn).toHaveBeenCalledTimes(1);
    expect(String(vi.mocked(console.warn).mock.calls[0][0])).toContain("no longer authoritative");
    expect(historyViewAuthority()).toBe("ts-fallback");
  });
});

describe("the cursor-parity probe's feed is the TS stack, not the query cache", () => {
  it("reports the host stack's own depth even when the cached query says otherwise", async () => {
    const history = makeHistory({
      cursor: 5,
      lastSeq: 5,
      degradedHint: false,
      pendingExternal: null,
      entries: [
        baseline(1),
        user(2, "Crop"),
        user(3, "Move"),
        user(4, "Fill"),
        user(5, "Brush"),
      ],
    });
    await history.resyncFromNativeQuery();

    // The derived counts read the cached native query...
    expect(history.getUndoCount()).toBe(4);
    // ...but the parity probe is fed the TS stack itself. Feeding it the query
    // cache would compare Rust against the same Rust reading and could only
    // ever report in-sync, whatever drift actually exists.
    expect(history.getTsUndoDepth()).toBe(0);

    history.commit({ layers: [] } as never, "Crop");
    expect(history.getTsUndoDepth()).toBe(1);
  });
});

describe("a malformed wire cursor cannot report work that does not exist", () => {
  it("cursor past the end of the stream clamps to the stream", async () => {
    const history = makeHistory({
      cursor: 7,
      lastSeq: 2,
      degradedHint: false,
      pendingExternal: null,
      entries: [baseline(1), user(2, "Crop")],
    });
    await history.resyncFromNativeQuery();

    // 7 clamps to entries.length (2), minus the baseline floor: one real step.
    expect(history.getUndoCount()).toBe(1);
    expect(history.getRedoCount()).toBe(0);
    expect(history.canUndo()).toBe(true);
    const rows = history.getHistoryStack();
    expect(rows.map((r) => r.label)).toEqual(["Open", "Crop"]);
    expect(rows.some((r) => r.isRedo), "no phantom redo rows past the stream").toBe(false);
  });

  it("a negative cursor reports nothing undoable and no phantom rows", async () => {
    const history = makeHistory({
      cursor: -3,
      lastSeq: 2,
      degradedHint: false,
      pendingExternal: null,
      entries: [baseline(1), user(2, "Crop")],
    });
    await history.resyncFromNativeQuery();

    expect(history.getUndoCount()).toBe(0);
    expect(history.canUndo()).toBe(false);
    expect(history.getRedoCount()).toBe(2);
    const rows = history.getHistoryStack();
    expect(rows.map((r) => r.label), "the stream, not index arithmetic off a bad cursor").toEqual([
      "Open",
      "Crop",
    ]);
  });
});
