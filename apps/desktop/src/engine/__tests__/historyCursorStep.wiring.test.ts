// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * The Rust cursor step's OUTCOMES: serialisation, never-an-unhandled-rejection,
 * and "it did not move the cursor" being reported.
 *
 * These live apart from `historyCursorParity.wiring.test.ts` because they are
 * about the step itself, not about the TS/Rust depth comparison - and they drive
 * `CommandHistory.undo()` directly rather than through the undo dispatcher, since
 * the dispatcher is the wrong seam for a pop site that consumes nothing.
 *
 * Two silent failure shapes, both observed in the real app:
 *  - `Ok` with an UNMOVED cursor. `undo_pixel` refuses to move it for a
 *    Snapshot/Native tip (crates/core/src/history.rs:239), and at cursor 0 the
 *    command answers `Ok` with an empty result
 *    (apps/desktop/src-tauri/src/paint_parity_cmds.rs:186-198).
 *  - a rejection: `Err(E_EXTERNAL_PENDING)` while a pending-external barrier is set
 *    (history.rs:209-214). The only SETTER of that barrier is the native walker's
 *    external-handoff arm (crates/core/src/document_core_apply.rs:975);
 *    `record_external` only CHECKS it (history.rs:290).
 *
 * Either way the host stack has already popped, so the NEXT step proceeds from a
 * stale cursor and consumes an entry the user never asked to undo. Both must be
 * reported, from the step, whether or not the pop site consumes it.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

import { CommandHistory } from "../history";
import type { DocumentModel } from "../types";
import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { flushPixelInvokeCensus } from "@/lib/protocol/pixelInvokeCensus";
import { answerInvoke, resetStreams, streamFor } from "./rustStreamEmulator";

vi.mock("@/lib/desktop/tauriWindow", () => ({ isTauriRuntime: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const GATE_KEY = "photrez.historyBridge";
const DOC = "doc-step";

const model = (name: string): DocumentModel => ({
  id: DOC,
  name,
  width: 10,
  height: 10,
  layers: [],
  activeLayerId: null,
  selection: null,
  viewport: { panX: 0, panY: 0, zoom: 1, rotation: 0 },
  dirty: false,
});

const stepWarns = (): string[] =>
  vi
    .mocked(console.warn)
    .mock.calls.map((c) => String(c[0]))
    .filter((m) => m.includes("history-cursor-step"));

const timesInvoked = (command: string): number =>
  vi.mocked(invoke).mock.calls.filter((c) => c[0] === command).length;

const settle = async () => {
  await new Promise<void>((r) => setTimeout(r, 0));
  await flushPixelInvokeCensus();
};

const waitFor = async (pred: () => boolean, ms = 3000) => {
  const start = Date.now();
  while (!pred()) {
    if (Date.now() - start > ms) throw new Error("waitFor timeout");
    await new Promise((r) => setTimeout(r, 5));
  }
};

/** A metadata commit under the bridge: Rust really records an External entry. */
async function commitRecordedMetadata(history: CommandHistory) {
  history.commit(model("Meta"), "Add Layer");
  await settle();
}

describe("the Rust cursor step reports what it did", () => {
  let history: CommandHistory;

  beforeEach(() => {
    resetStreams();
    localStorage.clear();
    localStorage.setItem(GATE_KEY, "1");
    localStorage.setItem("photrez.facade", "0");
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    vi.spyOn(console, "warn").mockImplementation(() => {});
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation((cmd: string, args: unknown) => answerInvoke(cmd, args));
    history = new CommandHistory();
    history.attachDocIdGetter(() => DOC);
  });

  afterEach(() => {
    vi.restoreAllMocks();
    localStorage.clear();
  });

  it("a step that does NOT move the cursor is reported, with the numbers behind it", async () => {
    // Two entries, so there is a second pop to make after the baseline step.
    await commitRecordedMetadata(history);
    await commitRecordedMetadata(history);
    // An `Ok` whose version does not advance IS a step that did not move the
    // cursor: `undo_pixel` bumps version exactly when it moves
    // (history.rs:221 / :231) and returns the current one otherwise.
    vi.mocked(invoke).mockImplementation((cmd: string, args: unknown) =>
      cmd === "rust_pixels_undo"
        ? Promise.resolve({ layer_id: "l1", tiles: [], epoch: 0, version: 0 })
        : answerInvoke(cmd, args),
    );
    history.undo(model("live"));
    await settle();
    expect(stepWarns(), "the first step only establishes the baseline").toEqual([]);
    vi.mocked(console.warn).mockClear();

    history.undo(model("live"));
    await waitFor(() => stepWarns().length === 1);
    expect(stepWarns()[0]).toContain("did not move the cursor");
    expect(vi.mocked(console.warn).mock.calls.at(-1)?.[1]).toMatchObject({
      docId: DOC,
      direction: "undo",
      version: 0,
    });
    // Non-fatal: the pop still returned its snapshot and the queue still drains.
    expect(history.getRedoCount()).toBe(2);
    expect(await flushPixelInvokeCensus()).toBeTruthy();
  });

  it("a REJECTING step is reported, and is never an unhandled rejection", async () => {
    await commitRecordedMetadata(history);
    vi.mocked(invoke).mockImplementation((cmd: string, args: unknown) =>
      cmd === "rust_pixels_undo"
        ? Promise.reject(new Error("E_EXTERNAL_PENDING: external history transition pending"))
        : answerInvoke(cmd, args),
    );
    const unhandled: unknown[] = [];
    const onUnhandled = (reason: unknown) => unhandled.push(reason);
    process.on("unhandledRejection", onUnhandled);
    try {
      // A pop that does NOT consume the step: the shape of every model-restore
      // pop site. Nothing calls takeLastCursorStep() afterwards.
      history.undo(model("live"));
      await settle();
      await new Promise<void>((r) => setTimeout(r, 20));
    } finally {
      process.off("unhandledRejection", onUnhandled);
    }
    expect(timesInvoked("rust_pixels_undo"), "the pop did fire its step").toBe(1);
    expect(unhandled, "the step carries its own handler").toEqual([]);
    expect(stepWarns()[0]).toContain("was rejected");
    expect(vi.mocked(console.warn).mock.calls.at(-1)?.[1]).toMatchObject({
      docId: DOC,
      direction: "undo",
    });
  });

  it("a step that DOES move the cursor says nothing", async () => {
    await commitRecordedMetadata(history);
    await commitRecordedMetadata(history);
    expect(streamFor(DOC).entries).toEqual(["external", "external"]);

    history.undo(model("live"));
    await settle();
    history.undo(model("live"));
    await settle();

    expect(streamFor(DOC).cursor, "both steps moved the cursor").toBe(0);
    expect(stepWarns(), "a working step is not a diagnostic").toEqual([]);
  });

  it("steps stay in POP order, and a failed one does not stall the ones behind it", async () => {
    await commitRecordedMetadata(history);
    await commitRecordedMetadata(history);
    const order: string[] = [];
    vi.mocked(invoke).mockImplementation(async (cmd: string, args: unknown) => {
      if (cmd !== "rust_pixels_undo") return answerInvoke(cmd, args);
      const n = order.length;
      order.push(`issued-${n}`);
      if (n === 0) throw new Error("first step fails");
      return answerInvoke(cmd, args);
    });
    history.undo(model("live"));
    history.undo(model("live"));
    await settle();

    expect(order, "issued in pop order, not interleaved").toEqual(["issued-0", "issued-1"]);
    // The first step REJECTED, so it consumed nothing; the second moved the cursor
    // exactly once - a failed step does not stall or double-count the ones behind it.
    expect(streamFor(DOC).cursor).toBe(1);
    expect(timesInvoked("rust_pixels_undo")).toBe(2);
  });
});
