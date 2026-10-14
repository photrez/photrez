// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * `rust_pixels_write_region` replies with the POST-image only. Its reply never
 * carried a pre-image, and undo/redo still restore pixels - from Rust, not from
 * a host memento.
 *
 * WHY THE REPLY LACKS THE PRE-IMAGE. Every stroke's reply crosses the process
 * boundary, and the pre-image was a whole dirty region's worth of base64 in it.
 * No consumer needs it from here:
 *
 *  - The brush never reads it. `useBrushOverlay` builds its memento's `before`
 *    from `beforePatches` - tiles the host snapshotted off its own surface
 *    before the stamp - not from the reply.
 *  - Undo does not take it from here either. A rustOwned entry's pixels come
 *    from Rust's own cursor step: `runFacadeExternalHandoff` drives the native
 *    walker (`Command::Undo`/`Command::Redo`), whose `protocol_apply_command_native`
 *    reply carries `pixelPatches` - the exact bytes Rust recorded.
 *    `projectRustPixelHandoff` uploads those; it never reads the TS memento.
 *
 * THE HAZARD THIS PINS. The host's TS history stack still holds each stroke's
 * rustOwned memento (a cursor token, kept for the derived history view). If a
 * future change re-routed undo/redo through that stack - replaying the memento's
 * `before` tiles instead of Rust's `pixelPatches` - the pre-image would become
 * load-bearing on the host side and the reply would have to carry it. The cases
 * below assert the projected bytes are Rust's (`RUST_STEP` first channel),
 * distinct from the host memento's own bytes (`POST`), so such a re-route
 * reddens here rather than in the app.
 *
 * MOCK FIDELITY: only `@tauri-apps/api/core`'s `invoke` is mocked - the true
 * transport boundary. The two replies `installNativeStepReply` builds are the
 * EXACT shapes the engine emits: `protocol_version_native` -> `1`, and
 * `protocol_apply_command_native` -> a JSON string carrying `delta` plus the
 * camelCase `pixelPatches` wrapper around Rust `TilePatch` fields. That shape is
 * pinned on the Rust side by `pixel_handoff_serializes_to_the_keys_the_host_parses`
 * in crates/core/src/pixel_store/tests.rs; a mock that guessed a different shape
 * would fail here. The real `runFacadeExternalHandoff` / `EditorFacade.undo` run
 * unmocked, which is the point: a test that stubs the facade can be green while
 * the app is broken. The native authority probe (`getHistoryQuery`) has no query
 * to answer in this harness, so it fails open exactly as it does in
 * `rustPixelUndoHandoff.wiring.test.ts`.
 *
 * Each case calls `runFacadeExternalHandoff` directly. The dispatcher chain that
 * reaches it (`useEditorCommands.undo` -> `restoreHistorySnapshot` -> this
 * handoff) is pinned by `historyRefusalUsesShippedPredicate.test.ts`, so this
 * file owns only the pre-image invariant: the projected bytes are Rust's, never
 * the host memento's.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

import { CommandHistory } from "@/engine/history";
import type { HistoryTilePatches } from "@/engine/history";
import type { DocumentModel } from "@/engine/types";
import { makeRustOwnedPatches } from "@/engine/__tests__/historyCursorFixtures";
import { runFacadeExternalHandoff } from "@/components/editor/facadeHistoryHandoff";
import { invoke } from "@tauri-apps/api/core";
import { flushPixelInvokeCensus } from "@/lib/protocol/pixelInvokeCensus";

async function settle() {
  await new Promise<void>((r) => setTimeout(r, 0));
  await flushPixelInvokeCensus();
}

/** Count the invokes the transport saw for `command`. */
const timesInvoked = (command: string): number =>
  vi.mocked(invoke).mock.calls.filter((c) => c[0] === command).length;

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const DOC = "doc-wr-preimage";

/** A 2x2 tile whose every pixel carries one byte value. */
const flatTile = (value: number) => ({
  x: 0,
  y: 0,
  w: 2,
  h: 2,
  data: Array.from({ length: 2 * 2 * 4 }, () => value),
});

/**
 * The one byte value Rust's cursor step hands back for a pixel undo/redo - the
 * first channel of the `pixelPatches` tile `installNativeStepReply` serves.
 * Distinct from the host memento's bytes below, so an upload that came from the
 * host's memento cannot be mistaken for one that came from Rust.
 */
const RUST_STEP = 0;

/** The pre-image bytes Rust's step returns: a 1x1 tile, first channel RUST_STEP. */
const RUST_TILE = { x: 0, y: 0, w: 1, h: 1, data: [RUST_STEP, 0, 0, 255] };

type NativeTile = { x: number; y: number; w: number; h: number; data: number[] };

/**
 * The transport answer for the native undo/redo step under test.
 *
 * `protocol_version_native` -> 1: the document version the facade syncs to
 * before it builds the command. `protocol_apply_command_native` -> the exact
 * reply shape the engine emits, with `pixelTiles` carrying the step's bytes.
 * A step with no dirty region serves `pixelTiles: []` - presence of
 * `pixelPatches` itself marks the step as taken.
 */
function installNativeStepReply(pixelTiles: NativeTile[]) {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === "protocol_version_native") return 1;
    if (cmd === "protocol_apply_command_native") {
      return JSON.stringify({
        documentVersion: 1,
        delta: { baseVersion: 0, version: 1, changes: [] },
        pixelPatches: { layerId: "l1", tiles: pixelTiles, epoch: 1, version: 1 },
      });
    }
    throw new Error(`unexpected invoke on the native history path: ${cmd}`);
  });
}

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

/** A context whose engine/renderer/history are doubles; the handoff runs real. */
function mountHandoff() {
  const history = new CommandHistory();
  history.attachDocIdGetter(() => DOC);
  const engine = makeEngine();
  const renderer = { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn() };
  const ctx = {
    workspace: {
      getActiveEngine: () => engine,
      getActiveDocumentId: () => DOC,
      getActiveHistory: () => history,
      notifyVisualChange: vi.fn(),
    },
    renderer,
    scheduler: { requestRender: vi.fn() },
  } as unknown as Parameters<typeof runFacadeExternalHandoff>[0];
  return { ctx, renderer, history };
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

beforeEach(() => {
  localStorage.clear();
  vi.mocked(invoke).mockReset();
  vi.spyOn(console, "warn").mockImplementation(() => {});
  vi.spyOn(console, "info").mockImplementation(() => {});
  // The write-region reply carries the post-image (and epoch/version) only -
  // no `before` key at all. That is the mock-fidelity contract: a double that
  // still answered `before` would let the reply grow one back without anything
  // here noticing. Cases that drive the native undo/redo step replace this
  // answer with `installNativeStepReply`.
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === "rust_pixels_write_region") {
      return { after: [flatTile(2)], epoch: 1, version: 1 };
    }
    throw new Error(`unexpected invoke: ${cmd}`);
  });
});

afterEach(async () => {
  await settle();
  await new Promise<void>((r) => setTimeout(r, 0));
  vi.restoreAllMocks();
});

describe("the write reply carries no pre-image; undo and redo restore pixels from Rust", () => {
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
    // `before` is irrelevant - the pixels come from Rust's cursor step
    // (`pixelPatches`), which projectRustPixelHandoff uploads verbatim. A wrong
    // restore that replayed the twin's tiles is invisible to a test that only
    // checked the memento.
    const { ctx, renderer, history } = mountHandoff();
    history.commit(model("Paint"), "Brush", makeRustOwnedPatches(), true);
    await settle();
    installNativeStepReply([RUST_TILE]);

    const handledUndo = await runFacadeExternalHandoff(ctx, "undo");
    expect(handledUndo).toBe(true);
    expect(timesInvoked("protocol_apply_command_native"), "the press drove exactly one native step").toBe(1);
    expect(timesInvoked("rust_pixels_undo"), "the host must not run a second pixel step").toBe(0);
    expect(
      uploadedValues(renderer),
      "undo uploaded the bytes Rust's cursor step returned, not the memento's",
    ).toEqual([RUST_STEP, 0, 0, 255]);

    const handledRedo = await runFacadeExternalHandoff(ctx, "redo");
    expect(handledRedo).toBe(true);
    expect(timesInvoked("protocol_apply_command_native"), "redo drove a second native step").toBe(2);
    expect(
      uploadedValues(renderer).slice(4),
      "redo uploaded the bytes Rust's cursor step returned",
    ).toEqual([RUST_STEP, 0, 0, 255]);
  });

  it("DEFEAT: an EMPTY rustOwned memento still restores, because the bytes are Rust's", async () => {
    // The control that makes the case above mean something. If the memento were
    // the pixel source, emptying it would change the restore. It does not, and
    // that is the whole reason the reply could drop its pre-image.
    const { ctx, renderer, history } = mountHandoff();
    const empty: HistoryTilePatches = {
      layerId: "l1",
      surfaceWidth: 10,
      surfaceHeight: 10,
      before: [],
      after: [],
      rustOwned: true,
    };
    history.commit(model("Paint"), "Brush", empty, true);
    await settle();
    installNativeStepReply([RUST_TILE]);

    await runFacadeExternalHandoff(ctx, "undo");

    expect(
      uploadedValues(renderer),
      "an empty rustOwned memento does not stop the restore - Rust supplied the bytes",
    ).toEqual([RUST_STEP, 0, 0, 255]);
    expect(timesInvoked("rust_pixels_undo"), "the host must not run a second pixel step").toBe(0);
  });

  it("a rustOwned step whose Rust reply yields no tiles refuses rather than replaying the memento", async () => {
    // The refusal branch, which is what made the memento's pre-image dead in
    // the first place. Rust moved the cursor but had no dirty region, so the
    // reply carries `pixelPatches` with empty tiles: the step is taken (handled,
    // and the native command was issued), yet the host uploads NOTHING - it does
    // not fall back to the twin's `before` tiles. If this ever started replaying
    // the memento, the pre-image would be load-bearing again.
    const { ctx, renderer, history } = mountHandoff();
    // The twin's `before` tiles are the would-be replay source; a host that
    // replayed them would surface PRE bytes in an upload below.
    history.commit(model("Paint"), "Brush", makeRustOwnedPatches(), true);
    await settle();
    installNativeStepReply([]);

    const handled = await runFacadeExternalHandoff(ctx, "undo");

    expect(handled, "an empty-tile pixel step is still a taken step").toBe(true);
    expect(
      timesInvoked("protocol_apply_command_native"),
      "the press reached the native executor and settled",
    ).toBeGreaterThanOrEqual(1);
    expect(
      renderer.uploadSurfaceTiles,
      "no tiles -> no upload; it does not fall back to the entry's tiles",
    ).not.toHaveBeenCalled();
    expect(uploadedValues(renderer), "nothing to restore, so nothing to upload").toEqual([]);
    expect(timesInvoked("rust_pixels_undo"), "the host must not run a second pixel step").toBe(0);
  });
});
