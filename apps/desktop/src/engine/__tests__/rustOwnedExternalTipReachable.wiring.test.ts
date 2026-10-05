// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * The External-tip cursor step: a `rustOwned` pop must not move the Rust cursor
 * over an entry that is not its own.
 *
 * THE DEFECT THIS PINS. Two sides of one stream were gated differently.
 * `installFacadeCommitShim` (lib/protocol/facadeRegistry.ts) mirrors every commit
 * into an `External` entry under `isFacadeEnabled()` - which reads
 * `photrez.facade !== "0"`, so ON unless explicitly opted out - while
 * `CommandHistory.stepRustCursor` decided whether to step under
 * `bridgeRecordsFor()` -> `historyBridgeEnabled()`, which needs
 * `photrez.historyBridge === "1"` AND the Tauri runtime, so OFF unless explicitly
 * opted in. At those two shipping defaults a metadata commit appended an
 * `External` entry that no pop would step, and the next `rustOwned` pop inherited
 * it as its tip. Stepping there is destructive rather than merely useless:
 * `ProtocolEngine::undo_pixel` moves the cursor over an `External` tip and yields
 * `(None, None, None)` (crates/core/src/history.rs:229-233), which
 * `PixelStoreRegistry::undo_pixel` collapses to `None` AFTER the move
 * (crates/core/src/pixel_store.rs:745). The paint entry was never consumed, the
 * host uploaded nothing, and the cursor had moved - which is what
 * `undo_over_an_external_tip_consumes_it_and_returns_no_tiles`
 * (apps/desktop/src-tauri/src/pixel_history_depth.rs) pins on the Rust side.
 *
 * THE FIX. `RustCursorStepper.fireGatedOnPixelTip` reads the tip's payload kind
 * through `rust_pixels_history_tip` BEFORE issuing the step, and issues nothing
 * unless the tip is a `Pixel` entry. It fails OPEN: an unreadable, malformed or
 * unrecognised answer fires the step, because the host stack has already popped
 * and a skipped step is a silently lost undo, whereas the bounded cost of firing
 * is the already-warned no-tiles path. The `bridgeRecordsFor()` arm is
 * deliberately NOT gated: the bridge recorded that entry's own counterpart, so an
 * `External` tip there is the correct entry to step over.
 *
 * Every case drives the REAL production undo path (`useEditorCommands().undo` over
 * a real `CommandHistory`, so the pop that owns the step is the real one) against
 * the emulated Rust stream, which models `undo_pixel`'s cursor move for both
 * payload kinds exactly as the real registry does. Each asserts the cursor and the
 * tip AFTER the press, so re-widening the gate reddens a case.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

import { CommandHistory } from "../history";
import type { HistoryTilePatches } from "../history";
import type { DocumentModel } from "../types";
import { useEditorCommands } from "@/components/editor/useEditorCommands";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "@/components/editor/dialogs/DialogProvider";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { invoke } from "@tauri-apps/api/core";
import {
  answerInvoke,
  record,
  resetStreams,
  streamFor,
} from "./rustStreamEmulator";
import { commitRustOwnedPaint, makeRustOwnedPatches } from "./historyCursorFixtures";
import { cursorStepInvokes, settle, timesInvoked, waitForRust } from "./historyCursorHarness";

vi.mock("@/lib/desktop/tauriWindow", () => ({ isTauriRuntime: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(() => Promise.resolve("0.0.0")) }));
// The undo path checks hasFacadeOwnedLayers() before touching the TS store. A
// document whose layers the facade never projected takes the legacy path, which
// is the path that reaches `stepRustCursor`.
vi.mock("@/engine/document", () => ({
  hasFacadeOwnedLayers: vi.fn(() => false),
  isFacadeOwnedLayer: vi.fn(() => false),
}));
// The facade handoff drives the native walker (`Command::Undo`), a DIFFERENT
// executor with its own coverage (rustPixelUndoHandoff.wiring.test.ts). Here it
// must decline, so the case exercises the TS pop that owns the cursor step.
vi.mock("@/components/editor/facadeHistoryHandoff", () => ({
  runFacadeExternalHandoff: vi.fn(async () => false),
  // This mock's handoff never drives the native walker, so it never moved the
  // cursor - which is exactly what the dispatcher reads here. The real
  // fall-through-AFTER-moving path is driven in facadeHandoffDoubleStep.wiring.test.ts.
  handoffMovedCursor: vi.fn(() => false),
}));

const BRIDGE_GATE = "photrez.historyBridge";
const RUST_PIXELS = "photrez.rustPixels";
const FACADE = "photrez.facade";
const DOC = "doc-1";

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

function makeEngine() {
  let current = model("live");
  return {
    getId: () => DOC,
    getActiveLayerId: () => "l1",
    getLayer: () => null,
    getLayers: () => current.layers,
    snapshot: (): DocumentModel => ({ ...current, layers: [...current.layers] }),
    restore: (snap: DocumentModel) => {
      current = { ...snap, layers: [...snap.layers] };
    },
    getPaintSurface: () => null,
    getModel: () => current,
    ensureBitmapCurrent: vi.fn(),
    invalidatePaintSurface: vi.fn(),
    transformLayer: vi.fn(),
  };
}

/** Mount the production undo/redo over a real history + the emulated Rust stream. */
function mountCommands(attachGetter = true) {
  const history = new CommandHistory();
  if (attachGetter) history.attachDocIdGetter(() => DOC);
  const engine = makeEngine();
  const renderer = { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn() };
  mockUseEditor({
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => history,
      getActiveDocumentId: () => DOC,
      notifyVisualChange: vi.fn(),
    },
    renderer,
    scheduler: { requestRender: vi.fn() },
    activeDocumentId: () => DOC,
    layerTransformSession: () => null,
    setLayerTransformSession: vi.fn(),
    activeTool: () => "brush",
    cropInteractionMode: () => "modern",
    canCropUndo: () => false,
    canCropRedo: () => false,
    canModernCropUndo: () => false,
    canModernCropRedo: () => false,
    layers: () => [],
    activeLayerId: () => null,
    selectedLayerIds: () => [],
    setSelectedLayerIds: vi.fn(),
    setSelectedLayerId: vi.fn(),
    textEditSession: () => null,
    setTextEditSession: vi.fn(),
    setStatusLoadingMessage: vi.fn(),
    setShowExportDialog: vi.fn(),
    setShowPrintDialog: vi.fn(),
    setShowResizeDialog: vi.fn(),
  });
  return { history, engine, renderer, commands: useEditorCommands(() => {}) };
}

/**
 * Drain a press that may have issued a gated step.
 *
 * `settle()` alone is one macrotask, which used to be enough because a press issued
 * at most one step. The tip-kind gate puts a READ in front of the step, and the
 * read is itself a round trip through the same registry mutex, so a gated press now
 * spans more than one. `until` waits for the observable effect of the press and
 * THEN drains, so a case cannot pass or fail merely because it guessed the number
 * of macrotasks correctly.
 *
 * `predicate` is the effect: for a tile pop, the tile path's upload; for a metadata
 * pop, which issues no step at all and is finished as soon as `settle` returns.
 */
async function until(predicate: () => boolean, timeoutMs = 3000): Promise<void> {
  const start = Date.now();
  while (!predicate()) {
    if (Date.now() - start > timeoutMs) {
      throw new Error("until: timed out waiting for the press to land");
    }
    await new Promise<void>((r) => setTimeout(r, 5));
  }
  await settle();
}

/** Run one undo press and wait for its tile-path upload (a tile entry's effect). */
async function pressUndo(
  commands: { undo: () => void },
  renderer: { uploadSurfaceTiles: { mock: { calls: unknown[][] } } },
  expectedUploads: number,
): Promise<void> {
  commands.undo();
  await until(() => renderer.uploadSurfaceTiles.mock.calls.length >= expectedUploads);
}

describe("an External tip inherited by a rustOwned pop", () => {
  beforeEach(() => {
    localStorage.clear();
    // The SHIPPING-DEFAULT flags, stated explicitly so a case cannot drift on them:
    // `photrez.facade` = ON, `photrez.historyBridge` = OFF. Nothing in production sets
    // either key.
    //
    // What the facade key controls HERE is only the shim arm's predicate in
    // `stepRustCursor` (`isFacadeEnabled() && !historyDegraded()`), which decides
    // whether a metadata pop may step ungated. This file does NOT install
    // `installFacadeCommitShim`, so no `External` entry is ever recorded by a commit
    // in these cases - `record(DOC, "external")` places them by hand, which is what
    // makes the inherited tip deterministic. The real recorder over the real
    // `CommandHistory` is proved in historyCursorDriftClosure.wiring.test.ts.
    localStorage.setItem(FACADE, "1");
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    resetStreams();
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({} as unknown as ReturnType<
      typeof DialogProviderModule.useDialog
    >);
    vi.mocked(invoke).mockReset();
    vi.spyOn(console, "warn").mockImplementation(() => {});
    vi.spyOn(console, "info").mockImplementation(() => {});
    vi.mocked(invoke).mockImplementation((cmd: string, args: unknown) => answerInvoke(cmd, args));
  });

  afterEach(async () => {
    // Drain in-flight work BEFORE the next case rebuilds the emulator and the
    // renderer mock. A press that has issued its step but not yet had it claimed
    // would otherwise land inside the next case and be counted there - which is
    // how a case can assert on another case's upload. `flushPixelInvokeCensus`
    // awaits every routed pixel invoke; the gate's tip read is not census-routed,
    // so a fixed macrotask yield covers it.
    await settle();
    await new Promise<void>((r) => setTimeout(r, 0));
    vi.restoreAllMocks();
  });

  it("no recorder at all: the paint pop refuses to step onto a foreign External tip", async () => {
    // The state the gate still exists for, and the ONLY configuration that can
    // reach it: the facade shim AND the bridge both off, so nothing records a
    // counterpart for the metadata step, and a foreign `External` entry sits above
    // the paint entry's `Pixel` one.
    //
    // This case USED to be the shipping-default one, and it asserted that the
    // metadata pop steps nothing - which was the drift, pinned as if it were the
    // design. The drift is now closed (see historyCursorDriftClosure.wiring.test.ts:
    // with the facade ON the shim arm steps the metadata entry, so the paint pop
    // finds its own `Pixel` tip and reverts). What remains is the genuinely
    // counterpart-less case, and the gate's refusal is still real there.
    localStorage.setItem(FACADE, "0");
    const { history, renderer, commands } = mountCommands();
    await commitRustOwnedPaint(history, model("Paint"));
    await waitForRust(DOC, 1);
    expect(streamFor(DOC).entries).toEqual(["pixel"]);

    // A metadata step that NOTHING records: both recorders are off. The `External`
    // entry below it belongs to nobody - it is the closest surviving route to the
    // inherited-tip state, planted explicitly.
    record(DOC, "external");
    history.commit(model("Meta"), "Add Layer");
    await settle();
    expect(streamFor(DOC).entries).toEqual(["pixel", "external"]);

    // The metadata pop: no recorder armed it, so no arm fires and the cursor stays.
    commands.undo();
    await settle();
    expect(
      cursorStepInvokes(),
      "with neither recorder armed, the metadata pop has no counterpart to step",
    ).toBe(0);
    expect(streamFor(DOC).cursor, "the Rust cursor did not move").toBe(2);

    // The paint pop: `rustOwned` is true, so `stepRustCursor` reaches the gate -
    // and the gate reads the tip, finds the foreign `External` entry, and refuses.
    await pressUndo(commands, renderer, 1);

    expect(
      cursorStepInvokes(),
      "the gate refused: no cursor step was issued onto the foreign External tip",
    ).toBe(0);
    expect(
      timesInvoked("rust_pixels_undo"),
      "and specifically no destructive undo",
    ).toBe(0);

    // The destructive outcome the gate exists to prevent: before the fix this step
    // moved the cursor over the External entry (history.rs:229-233), yielded no
    // tiles, and `PixelStoreRegistry::undo_pixel` collapsed that to `None` AFTER
    // the move (pixel_store.rs:745) - consuming the External entry while the paint
    // entry was never reached.
    const after = streamFor(DOC);
    expect(after.cursor, "the Rust cursor did not move at all").toBe(2);
    expect(
      after.entries[after.cursor - 1],
      "the External entry is untouched - nothing consumed it",
    ).toBe("external");
    expect(
      after.entries[after.cursor - 2],
      "and the paint step's Pixel entry is still there, un-consumed",
    ).toBe("pixel");

    // The gate refused, so the tile path got no step: `projectRustTiles` returned
    // its refusal branch and uploaded NOTHING rather than replaying a memento no
    // store holds (rustTileProjection.ts:85-88). The refusal is what keeps the
    // surface from being repainted from stale bytes.
    const uploads = renderer.uploadSurfaceTiles.mock.calls;
    expect(uploads.length, "the tile path ran once").toBe(1);
    expect(
      // `uploadSurfaceTiles(layerId, surfaceWidth, surfaceHeight, tiles)`.
      (uploads[0][3] as unknown[]).length,
      "and uploaded no tiles, because Rust had no entry for this pop",
    ).toBe(0);

    // The gate is loud, so this is diagnosable rather than silent.
    expect(
      vi.mocked(console.warn).mock.calls.map((c) => String(c[0])).join("\n"),
      "the refusal is warned",
    ).toContain("[history-cursor-step] skipping the cursor step");
  });

  it("the control: a Pixel tip DOES yield tiles, so the empty yield above is the External arm", async () => {
    // Without this the case above could be passing because the emulator never
    // yields tiles at all, which would make its central assertion unfalsifiable.
    const { history, renderer, commands } = mountCommands();
    await commitRustOwnedPaint(history, model("Paint"));
    await waitForRust(DOC, 1);
    expect(streamFor(DOC).entries).toEqual(["pixel"]);

    await pressUndo(commands, renderer, 1);

    expect(cursorStepInvokes()).toBe(1);
    expect(streamFor(DOC).cursor, "the Pixel entry was consumed").toBe(0);
    const uploads = renderer.uploadSurfaceTiles.mock.calls;
    expect(uploads.length).toBe(1);
    expect(
      (uploads[0][3] as unknown[]).length,
      "a Pixel tip yields its tiles, so the empty yield above is the External arm",
    ).toBeGreaterThan(0);
  });

  it("the gate reads the tip exactly once per press, and never two reads race", async () => {
    // PERFORMANCE. The fix adds an IPC read to the undo path, so its cost has to
    // be pinned rather than assumed: one `rust_pixels_history_tip` per gated pop,
    // issued at most once. A gate that read per arm, retried, or re-read after the
    // step would pass every behavioural case above and still be wrong here.
    const { history, renderer, commands } = mountCommands();
    await commitRustOwnedPaint(history, model("Paint"));
    await settle();

    const before = timesInvoked("rust_pixels_history_tip");
    await pressUndo(commands, renderer, 1);
    expect(
      timesInvoked("rust_pixels_history_tip") - before,
      "exactly one tip read for one press",
    ).toBe(1);

    // Two rapid presses, the shape a held Ctrl+Z produces: two reads, serialised
    // on the stepper's chain, never interleaved against each other's step. Two
    // entries are committed first, because a press with nothing left to pop
    // returns before the gate and would not read at all.
    await commitRustOwnedPaint(history, model("Paint2"));
    await commitRustOwnedPaint(history, model("Paint3"));
    await settle();
    const before2 = timesInvoked("rust_pixels_history_tip");
    const stepsBefore2 = cursorStepInvokes();
    commands.undo();
    commands.undo();
    // Wait for the READS and then the STEPS: waiting only on the reads would let
    // this case finish with both steps still in flight, and they would land in the
    // next case's freshly-built emulator.
    await until(() => timesInvoked("rust_pixels_history_tip") - before2 >= 2);
    await until(() => cursorStepInvokes() - stepsBefore2 >= 2);
    expect(
      timesInvoked("rust_pixels_history_tip") - before2,
      "two presses, two reads - one each, no retry",
    ).toBe(2);
    expect(
      cursorStepInvokes() - stepsBefore2,
      "and two steps, so neither press lost its undo to the other's handle",
    ).toBe(2);
  });

  it("FAILS OPEN: a rejected tip read still issues the step, so no undo is silently lost", async () => {
    // The fail-safe direction, pinned. The host stack has ALREADY popped by the
    // time the gate reads, so refusing on an unknown answer would drop a
    // legitimate pixel undo with no error at all. An unreadable tip must therefore
    // fire, reproducing the pre-gate behaviour rather than inventing a new loss.
    const { history, renderer, commands } = mountCommands();
    await commitRustOwnedPaint(history, model("Paint"));
    await waitForRust(DOC, 1);

    const answering = vi.mocked(invoke).getMockImplementation()!;
    vi.mocked(invoke).mockImplementation((cmd: string, args: unknown) => {
      if (cmd === "rust_pixels_history_tip") {
        // Exactly how the real transport fails: a rejected promise carrying the
        // bare string Tauri surfaces for `Err(String)` (protocol_native_cmds.rs).
        return Promise.reject("document not open: doc-1");
      }
      return answering(cmd, args as never);
    });

    await pressUndo(commands, renderer, 1);

    expect(
      timesInvoked("rust_pixels_history_tip"),
      "the gate did try to read",
    ).toBe(1);
    expect(
      cursorStepInvokes(),
      "and fired anyway - an unknown tip must not become a lost undo",
    ).toBe(1);
    expect(
      streamFor(DOC).cursor,
      "so the Pixel entry was consumed normally",
    ).toBe(0);
    const uploads = renderer.uploadSurfaceTiles.mock.calls;
    expect(uploads.length).toBe(1);
    expect(
      (uploads[0][3] as unknown[]).length,
      "and the paint step reverted with its own tiles",
    ).toBeGreaterThan(0);
  });

  it("FAILS OPEN: a malformed tip answer is treated as unknown, not as a non-Pixel tip", async () => {
    // A tip kind this build does not recognise must not be read as "not a Pixel
    // entry" and refused. `readTipKind` returns undefined for anything that is not
    // a string, and undefined means fire - so a future `PayloadKind` variant
    // degrades to today's behaviour instead of silently swallowing undos.
    const { history, renderer, commands } = mountCommands();
    await commitRustOwnedPaint(history, model("Paint"));
    await waitForRust(DOC, 1);

    const answering = vi.mocked(invoke).getMockImplementation()!;
    vi.mocked(invoke).mockImplementation((cmd: string, args: unknown) => {
      if (cmd === "rust_pixels_history_tip") {
        // Depths well-formed, kind absent - the shape a partially-corrupt reply
        // takes. `requireHistoryDepthNumbers` accepts it, so the gate must decide.
        return Promise.resolve({ total_depth: 1, undo_depth: 1, redo_depth: 0 });
      }
      return answering(cmd, args as never);
    });

    await pressUndo(commands, renderer, 1);
    expect(cursorStepInvokes(), "an absent kind is unknown, so the step fires").toBe(1);
    expect(streamFor(DOC).cursor).toBe(0);
  });

  it("the bridgeRecordsFor() metadata arm is NOT gated: it still steps onto an External tip", async () => {
    // THE OVER-APPLICATION GUARD. The gate applies to the two entry-shaped arms
    // only. The bridge recorded this entry's OWN counterpart, so an `External` tip
    // is precisely the entry the pop must consume - refusing it would strand every
    // bridge-ON metadata undo and leave the cursor permanently behind. If a future
    // change widens the gate to cover this arm, this case reddens.
    localStorage.setItem(BRIDGE_GATE, "1");
    const { history, renderer, commands } = mountCommands();

    // A paint entry (Pixel) and a metadata entry (External), both recorded by the
    // bridge, so the stream is [pixel, external] and the cursor sits after both.
    await commitRustOwnedPaint(history, model("Paint"));
    await waitForRust(DOC, 1);
    history.commit(model("Meta"), "Add Layer");
    await settle();
    expect(streamFor(DOC).entries).toEqual(["pixel", "external"]);

    // The metadata pop. Its arm is the bridge, so the gate is bypassed entirely -
    // no tip read is issued for it, and the External tip is stepped over. It takes
    // the snapshot/metadata restore path, so it is finished once the step settles.
    const readsBefore = timesInvoked("rust_pixels_history_tip");
    commands.undo();
    await until(() => cursorStepInvokes() >= 1);

    expect(
      timesInvoked("rust_pixels_history_tip") - readsBefore,
      "the bridge arm issues no tip read - it needs no gate",
    ).toBe(0);
    expect(
      cursorStepInvokes(),
      "and it stepped onto the External tip, which is correct there",
    ).toBe(1);
    expect(
      streamFor(DOC).cursor,
      "the External entry was consumed, so the Pixel entry is the tip again",
    ).toBe(1);
    expect(streamFor(DOC).entries[0]).toBe("pixel");

    // And the gated arm still works immediately afterwards: a Pixel tip, so the
    // paint pop reads once and steps.
    await pressUndo(commands, renderer, 1);
    expect(
      timesInvoked("rust_pixels_history_tip") - readsBefore,
      "the entry-shaped arm does read",
    ).toBe(1);
    expect(cursorStepInvokes(), "and steps onto its own Pixel entry").toBe(2);
    expect(streamFor(DOC).cursor, "both consumed").toBe(0);
  });

  /*
   * DEFEAT RECIPE - the falsifiability half, executed rather than asserted.
   * `fireGatedOnPixelTip` is driven through the REAL production undo path by the
   * first case above, so defeating the gate in production code and re-running reddens
   * it. The exact edit, applied and reverted while proving this fix:
   *
   *   in apps/desktop/src/engine/historyCursorStep.ts, inside `fireGatedOnPixelTip`,
   *   change
   *     `if (tipKind !== undefined && tipKind !== "pixel") {`
   *   to
   *     `if ((false as boolean) && tipKind !== undefined && tipKind !== "pixel") {`
   *
   * To re-run:
   *   bun run --filter photrez-desktop test --run \
   *     src/engine/__tests__/rustOwnedExternalTipReachable.wiring.test.ts
   * and expect the four named cases below to redden.
   *
   * Observed RED with the gate defeated (4 failed / 5 passed). Four INDEPENDENT cases
   * reddened, so none of them can be passing for a reason unrelated to the gate:
   *   "the gate refused: no cursor step was issued onto the foreign External
   *    tip: expected 1 to be +0"                (no-recorder case)
   *   "the gate still read the tip and refused, so nothing was consumed:
   *    expected 1 to be +0"                    (getterless history case)
   *   "the gate refused: a counterpart-less tile pop must not consume the
   *    External entry: expected 1 to be +0"    (rustPixels flag case)
   *   "with the gate in place nothing is issued - defeat it and this becomes 1:
   *    expected 1 to be +0"                    (the first case, re-run)
   *
   * All four are configurations where NOTHING recorded a counterpart for the popped
   * entry. The shipping-default drift those cases used to cover is closed at its root
   * in historyCursorDriftClosure.wiring.test.ts, which is why they now opt the facade
   * out: with it on, every one of them has a counterpart and the gate correctly never
   * refuses.
   *
   * There is deliberately no `it()` here. The recipe is the evidence; an assertion
   * duplicating the first case would add a second test passing for the same reason.
   */
  it("a getterless history is gated exactly like a getter-ful one, because the snapshot names the document", async () => {
    // `editorOpenImage.loadProjectFile` builds `new CommandHistory()` with no
    // `attachDocIdGetter`, which is the File>Open shape. The missing getter does
    // NOT make such a pop inert: `stepRustCursor` falls back to the popped
    // snapshot's own id before it consults the getter (history.ts:489), and
    // `DocumentModel.id` is the document id. So the gate runs for this shape too,
    // which is what stops the File>Open path from inheriting an External tip.
    //
    // What the missing getter DOES kill is the bridge arm, because
    // `bridgeRecordsFor()` requires one (history.ts:172-179).
    const { history, renderer, commands } = mountCommands(false);
    await commitRustOwnedPaint(history, model("Paint"));
    record(DOC, "external");
    await settle();

    await pressUndo(commands, renderer, 1);
    expect(
      cursorStepInvokes(),
      "the gate still read the tip and refused, so nothing was consumed",
    ).toBe(0);
    expect(streamFor(DOC).cursor, "the Rust cursor did not move").toBe(2);
    expect(
      streamFor(DOC).entries[0],
      "and the paint entry's Pixel entry is still un-consumed",
    ).toBe("pixel");
  });

  it("TRANSITIONAL (photrez.rustPixels gating; delete with the flag): flag ON is gated too, so a counterpart-less tile pop cannot consume an External entry", async () => {
    // The tile arm `stepRustCursor` documents as "the one arm that can step
    // without a recorded counterpart" (history.ts:453-455). It is NOT exempt from
    // the gate: lacking a counterpart is precisely when the stream's tip is
    // whatever someone else left there, which is the case the gate exists for.
    // Pinned so the flag's retirement cannot silently change this file's verdicts.
    //
    // The facade is opted OUT here, which is what makes the tile entry genuinely
    // counterpart-less: with it ON the shim mirrors tile commits too, so the pop has
    // a real `External` counterpart and the shim arm steps it (asserted in
    // historyCursorDriftClosure.wiring.test.ts). This is the residual transitional
    // configuration, where the flag can still aim a step at an entry nobody owns.
    localStorage.setItem(FACADE, "0");
    localStorage.setItem(RUST_PIXELS, "1");
    const { history, renderer, commands } = mountCommands();

    record(DOC, "external");
    // A TS-owned tile entry: recorded by NEITHER the bridge (off) NOR the writer,
    // so the Rust stream holds only the External entry.
    const patches: HistoryTilePatches = makeRustOwnedPatches();
    history.commit(model("Tile"), "Text", { ...patches, rustOwned: false });
    await settle();
    expect(streamFor(DOC).entries, "Rust holds only the External entry").toEqual(["external"]);

    await pressUndo(commands, renderer, 1);
    expect(
      cursorStepInvokes(),
      "the gate refused: a counterpart-less tile pop must not consume the External entry",
    ).toBe(0);
    expect(streamFor(DOC).cursor, "the Rust cursor did not move").toBe(1);
  });
});
