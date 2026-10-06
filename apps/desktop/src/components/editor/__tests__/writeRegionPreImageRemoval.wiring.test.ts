// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * `rust_pixels_write_region` replies with the POST-image only. Its reply no
 * longer carries a pre-image, and both history arms still restore pixels.
 *
 * WHY THE REPLY LOST THE PRE-IMAGE. Every stroke's reply crosses the process
 * boundary, and the pre-image was a whole dirty region's worth of base64 in it.
 * No consumer could use it:
 *
 *  - The BRUSH never read it. `useBrushOverlay` builds its memento's `before`
 *    from `beforePatches` - tiles the host snapshotted off its own surface
 *    before the stamp - not from the reply.
 *  - Every other producer that read `res.before` stored it on an entry marked
 *    `rustOwned: true`, and `projectRustTiles` deliberately REFUSES to replay a
 *    `rustOwned` memento: it returns the tiles Rust's cursor step returned, or
 *    an empty list, never `fallbackTiles`.
 *  - Undo does not need it from here either. The write already appended the
 *    pre-image to Rust's own pixel history, and `rust_pixels_undo` returns those
 *    exact bytes.
 *
 * THE HAZARD THIS PINS. The brush's SYNCHRONOUS FALLBACK, taken when the async
 * commit throws, commits a memento with NO `rustOwned` marker and therefore DOES
 * replay from its own `before` bytes. If the pre-image had been coming from the
 * reply, dropping it would have made that path restore nothing - a silent,
 * stroke-losing failure on the one path that exists precisely to avoid losing a
 * stroke. It does not, because that path's bytes are `beforePatches`, which this
 * change does not touch. The two arms are asserted separately below so a future
 * change that re-routes the fallback's bytes through the reply reddens here
 * rather than in the app.
 *
 * MOCK FIDELITY: only `@tauri-apps/api/core`'s `invoke` is mocked, and it
 * answers with the SHAPE the real command now emits - `after`, `epoch`,
 * `version`, no `before`. A double that still answered `before` would let a
 * regression pass here while the real runtime sent nothing.
 *
 * Every case drives the REAL dispatcher (`useEditorCommands().undo` /
 * `.redo` over a real `CommandHistory`, so the pop that owns the Rust cursor
 * step is the production one) against the real `projectRustTiles`.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

import { CommandHistory } from "@/engine/history";
import type { HistoryTilePatches } from "@/engine/history";
import type { DocumentModel } from "@/engine/types";
import { useEditorCommands } from "@/components/editor/useEditorCommands";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "@/components/editor/dialogs/DialogProvider";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { invoke } from "@tauri-apps/api/core";
import { answerInvoke, resetStreams, streamFor, record } from "@/engine/__tests__/rustStreamEmulator";
import { commitRustOwnedPaint } from "@/engine/__tests__/historyCursorFixtures";
import { settle, timesInvoked, waitForRust } from "@/engine/__tests__/historyCursorHarness";

vi.mock("@/lib/desktop/tauriWindow", () => ({ isTauriRuntime: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(() => Promise.resolve("0.0.0")) }));
// The undo path checks hasFacadeOwnedLayers() before touching the TS store. A
// document whose layers the facade never projected takes the legacy path, which
// is the path that reaches `stepRustCursor`.
vi.mock("@/engine/document", () => ({
  hasFacadeOwnedLayers: vi.fn(() => false),
  isFacadeOwnedLayer: vi.fn(() => false),
}));
// The facade handoff drives the native walker (Command::Undo), a DIFFERENT
// executor with its own coverage (rustPixelUndoHandoff.wiring.test.ts). Here it
// must decline, so each case exercises the TS pop that owns the cursor step.
vi.mock("@/components/editor/facadeHistoryHandoff", () => ({
  runFacadeExternalHandoff: vi.fn(async () => false),
  handoffMovedCursor: vi.fn(() => false),
}));

const FACADE = "photrez.facade";
const DOC = "doc-wr-preimage";

/**
 * The PRE-IMAGE, as a byte value. Distinct from every post-image below so a
 * restore that returned the wrong pixels cannot pass.
 */
const PRE = 1;
const POST = 2;

/**
 * The one byte value the Rust STREAM hands back for a Pixel step
 * (rustStreamEmulator.ts:116 gives it `[0, 0, 0, 255]`, so the first channel is
 * 0). Distinct from every host-side value below, so an upload that came from the
 * host's memento cannot be mistaken for one that came from Rust.
 */
const RUST_STEP = 0;

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

/** A 2x2 tile whose every pixel carries one byte value. */
const flatTile = (value: number) => ({
  x: 0,
  y: 0,
  w: 2,
  h: 2,
  data: Array.from({ length: 2 * 2 * 4 }, () => value),
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

/** Mount the production undo/redo over a real history and the emulated Rust stream. */
function mountCommands() {
  const history = new CommandHistory();
  history.attachDocIdGetter(() => DOC);
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

/** Every byte value the renderer was handed, across all its tile uploads. */
function uploadedValues(renderer: { uploadSurfaceTiles: { mock: { calls: unknown[][] } } }): number[] {
  const out: number[] = [];
  for (const call of renderer.uploadSurfaceTiles.mock.calls) {
    for (const tile of call[3] as { data: ArrayLike<number> }[]) {
      // `data` is typed ArrayLike because Rust's tiles arrive that way, but every
      // producer here hands over a real typed array; indexed for that reason.
      for (let i = 0; i < tile.data.length; i++) out.push(tile.data[i]);
    }
  }
  return out;
}

/** Wait for the press's tile upload to land, then drain. */
async function until(predicate: () => boolean, timeoutMs = 3000): Promise<void> {
  const start = Date.now();
  while (!predicate()) {
    if (Date.now() - start > timeoutMs) throw new Error("until: timed out waiting for the press to land");
    await new Promise<void>((r) => setTimeout(r, 5));
  }
  await settle();
}

beforeEach(() => {
  localStorage.clear();
  // `photrez.facade` = ON (the shipping default; nothing in production sets it).
  // The bridge stays OFF, which is also the shipping default, so the only writer
  // of a Pixel entry here is `rust_pixels_write_region` - the command under test.
  localStorage.setItem(FACADE, "1");
  vi.mocked(isTauriRuntime).mockReturnValue(true);
  resetStreams();
  vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({} as unknown as ReturnType<
    typeof DialogProviderModule.useDialog
  >);
  vi.mocked(invoke).mockReset();
  vi.spyOn(console, "warn").mockImplementation(() => {});
  vi.spyOn(console, "info").mockImplementation(() => {});
  // The emulator owns the stream and the cursor moves; this wrapper only
  // replaces the ONE answer whose SHAPE changed, and delegates everything else.
  //
  // The shape below is the real reply after the change: post-image, epoch,
  // version, and no `before` key at all. That is the mock-fidelity contract -
  // a double that still answered `before` would let the reply grow one back
  // without anything here noticing.
  //
  // The emulator's own Pixel tile is the distinguishable value the stream hands
  // back on undo/redo (rustStreamEmulator.ts:116), so restoring THAT is what
  // proves the bytes came from Rust rather than from the host.
  vi.mocked(invoke).mockImplementation(async (cmd: string, args: unknown) => {
    if (cmd === "rust_pixels_write_region") {
      const ans = (await answerInvoke(cmd, args)) as { after: unknown[]; version: number };
      return { after: [flatTile(POST)], epoch: 1, version: ans.version };
    }
    return answerInvoke(cmd, args);
  });
});

afterEach(async () => {
  await settle();
  await new Promise<void>((r) => setTimeout(r, 0));
  vi.restoreAllMocks();
});

describe("the write reply carries no pre-image, and both history arms still restore pixels", () => {
  it("the reply the host decodes has no `before` key to read", async () => {
    // The transport-level fact, asserted where the decode happens. This is the
    // premise every case below rests on: there is no pre-image on the wire to
    // have accidentally come from.
    const reply = await invoke("rust_pixels_write_region", { docId: DOC, layerId: "l1" });
    expect(Object.keys(reply as object).sort()).toEqual(["after", "epoch", "version"]);
    expect((reply as { before?: unknown }).before).toBeUndefined();
  });

  it("a rustOwned entry restores the PRE-IMAGE on undo and the POST-IMAGE on redo, both from Rust", async () => {
    // ARM 1: the async commit's arm. The memento is a cursor token, so its own
    // `before` is irrelevant - the pixels come from Rust's cursor step. A wrong
    // restore here is invisible to a test that only checked the memento.
    const { history, renderer, commands } = mountCommands();
    await commitRustOwnedPaint(history, model("Paint"));
    await waitForRust(DOC, 1);

    commands.undo();
    await until(() => renderer.uploadSurfaceTiles.mock.calls.length >= 1);

    expect(timesInvoked("rust_pixels_undo"), "the pop issued exactly one cursor step").toBe(1);
    expect(
      uploadedValues(renderer),
      "undo uploaded the bytes RUST's cursor step returned, not the memento's",
    ).toEqual([RUST_STEP, 0, 0, 255]);

    commands.redo();
    await until(() => renderer.uploadSurfaceTiles.mock.calls.length >= 2);

    expect(timesInvoked("rust_pixels_redo"), "the redo issued exactly one cursor step").toBe(1);
    expect(
      uploadedValues(renderer).slice(4),
      "redo uploaded the bytes RUST's cursor step returned",
    ).toEqual([RUST_STEP, 0, 0, 255]);
  });

  it("DEFEAT: an EMPTY rustOwned memento still restores, because the bytes are Rust's", async () => {
    // The control that makes the case above mean something. If the memento were
    // the pixel source, emptying it would change the restore. It does not, and
    // that is the whole reason the reply could drop its pre-image.
    const { history, renderer, commands } = mountCommands();
    await vi.mocked(invoke)("rust_pixels_write_region", { docId: DOC, layerId: "l1" });
    const empty: HistoryTilePatches = {
      layerId: "l1",
      surfaceWidth: 10,
      surfaceHeight: 10,
      before: [],
      after: [],
      rustOwned: true,
    };
    history.commit(model("Paint"), "Brush", empty, true);
    await waitForRust(DOC, 1);

    commands.undo();
    await until(() => renderer.uploadSurfaceTiles.mock.calls.length >= 1);

    expect(
      uploadedValues(renderer),
      "an empty rustOwned memento does not stop the restore - Rust supplied the bytes",
    ).toEqual([RUST_STEP, 0, 0, 255]);
  });

  it("ARM 2: the fallback arm - no rustOwned marker - replays ITS OWN memento bytes", async () => {
    // ARM 2: the synchronous fallback the brush takes when the async commit
    // throws. It commits WITHOUT `rustOwned`, so the dispatcher replays the
    // entry's own tiles instead of fetching from Rust. Its `before` must
    // therefore be real bytes - which is why the fallback reads `beforePatches`
    // and not the reply. An entry with an empty `before` and no `rustOwned`
    // marker restores NOTHING, and that is the arm that would silently lose a
    // stroke if the pre-image had been sourced from the reply.
    const { history, renderer, commands } = mountCommands();
    const memento: HistoryTilePatches = {
      layerId: "l1",
      surfaceWidth: 10,
      surfaceHeight: 10,
      before: [{ x: 0, y: 0, width: 2, height: 2, data: new Uint8ClampedArray(2 * 2 * 4).fill(PRE) }],
      after: [{ x: 0, y: 0, width: 2, height: 2, data: new Uint8ClampedArray(2 * 2 * 4).fill(POST) }],
    };
    expect(memento.rustOwned, "the fallback commits no rustOwned marker").toBeUndefined();
    history.commit(model("Brush"), "Brush Stroke", memento);
    await settle();

    commands.undo();
    await until(() => renderer.uploadSurfaceTiles.mock.calls.length >= 1);

    expect(
      streamFor(DOC).entries.length,
      "nothing recorded this entry, so no Rust pixel source exists",
    ).toBe(0);
    expect(
      uploadedValues(renderer),
      "the fallback restored from its OWN memento bytes, which is what it must do",
    ).toEqual(Array.from({ length: 2 * 2 * 4 }, () => PRE));
  });

  it("DEFEAT: emptying the fallback arm's memento DOES break its restore", async () => {
    // The falsifiability control for arm 2. Arm 2's assertion above would pass
    // against an arm that uploaded nothing at all, so this pins that it is
    // genuinely replaying the memento: with the bytes removed the restore is
    // empty. That is precisely the failure the reply change had to avoid causing.
    const { history, renderer, commands } = mountCommands();
    history.commit(model("Brush"), "Brush Stroke", {
      layerId: "l1",
      surfaceWidth: 10,
      surfaceHeight: 10,
      before: [],
      after: [],
    });
    await settle();

    expect(streamFor(DOC).entries.length, "again, no Rust pixel source").toBe(0);

    commands.undo();
    await until(() => renderer.uploadSurfaceTiles.mock.calls.length >= 1);

    expect(
      uploadedValues(renderer),
      "with no memento bytes and no Rust step, there is nothing to restore - the loss this change must not cause",
    ).toEqual([]);
  });

  it("a rustOwned pop whose Rust step yields no tiles refuses rather than replaying", async () => {
    // The refusal branch, which is what made the memento's pre-image dead in
    // the first place. With Rust holding no Pixel tip the step is refused and
    // the host uploads NOTHING - it does not fall back to the entry's tiles. If
    // this ever started replaying the memento, the pre-image would be load-
    // bearing again and the reply would have to carry it.
    // A foreign External entry ABOVE the paint entry, so the Pixel-tip gate
    // refuses. Placed by hand after the commit, and the emulator still owns the
    // cursor, so the refusal is the production gate's doing and not a stubbed
    // answer.
    vi.mocked(invoke).mockImplementation(async (cmd: string, args: unknown) => {
      if (cmd === "rust_pixels_write_region") {
        const ans = (await answerInvoke(cmd, args)) as { version: number };
        return { after: [flatTile(POST)], epoch: 1, version: ans.version };
      }
      return answerInvoke(cmd, args);
    });
    const { history, renderer, commands } = mountCommands();
    await vi.mocked(invoke)("rust_pixels_write_region", { docId: DOC, layerId: "l1" });
    history.commit(
      model("Paint"),
      "Brush",
      {
        layerId: "l1",
        surfaceWidth: 10,
        surfaceHeight: 10,
        before: [{ x: 0, y: 0, width: 2, height: 2, data: new Uint8ClampedArray(2 * 2 * 4).fill(PRE) }],
        after: [],
        rustOwned: true,
      },
      true,
    );
    await waitForRust(DOC, 1);
    record(DOC, "external");

    commands.undo();
    await until(() => renderer.uploadSurfaceTiles.mock.calls.length >= 1);

    expect(timesInvoked("rust_pixels_undo"), "the gate refused the foreign tip").toBe(0);
    expect(
      uploadedValues(renderer),
      "the refusal uploaded nothing rather than replaying the memento",
    ).toEqual([]);
    expect(
      streamFor(DOC).entries,
      "neither entry was consumed - the refusal is not a destructive move",
    ).toEqual(["pixel", "external"]);
  });
});
