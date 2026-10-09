// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * ONE press moves the Rust cursor EXACTLY ONCE, and the host moves it ZERO times.
 *
 * THE CONTRACT THIS PINS. The Rust cursor is moved ONLY inside Rust.
 * One undo press must therefore advance the canonical cursor by exactly one step
 * over the stroke's own `Pixel` entry, and no `rust_pixels_undo` /
 * `rust_pixels_redo` may be issued by the host - those commands are the host's
 * own cursor mover (`CommandHistory.stepRustCursor`), and while both exist a single
 * press has two mechanisms that can move one cursor.
 *
 * ONE DOCUMENT SHAPE, and the one that matters: FACADE-OWNED.
 * `hasFacadeOwnedLayers()` is true, so the dispatch runs `runFacadeExternalHandoff`,
 * whose `facade.undo()` issues the native `apply_command {undo}` walker - the cursor
 * moves inside Rust and the host never pops. This is the shape the real-app
 * default-path undo proof measured (native `apply_command {undo}` over the
 * stroke's own Pixel entry, zero host cursor commands); dates for that
 * measurement live in the internal ledger.
 *
 * THE OTHER SHAPE IS NOT PINNED HERE, and deliberately so. When the facade has never
 * projected a layer - `facadeOwnedIds` is written in exactly one place,
 * `markFacadeOwned` inside `applyFacadeSnapshot`, and `editorOpenImage.loadProjectFile`
 * builds a session without calling it - the dispatch skips the native dispatcher and
 * the host pop is the executor. Measured on the current tree: that pop
 * issues exactly one `rust_pixels_undo` and is the ONLY thing that moves the cursor
 * for that document, so it is the host's fallback executor, not dead symmetry.
 * Deleting it needs the pop removed with it, not just the step.
 *
 * MOCK FIDELITY. The native walker lives in Rust; here `nativeClient.nativeProtocol`
 * is stubbed to move the SAME emulated cursor `rust_pixels_*` moves
 * (`rustStreamEmulator`) and to answer with the `CommandResult` shape
 * `lib/protocol/types.ts:312-341` defines - `pixelPatches` present iff the step
 * landed on a `Pixel` entry, which is the flag the host's branch reads. The step is
 * applied DIRECTLY to the stream, never through the `invoke` spy, so the
 * "zero host cursor commands" assertion below cannot be satisfied by the model of
 * the walker itself. That separation is the point of the case: the walker is the
 * only thing allowed to move the cursor, so it must not be counted as a violation
 * of its own rule.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

import { CommandHistory } from "../history";
import type { DocumentModel } from "../types";
import { useEditorCommands } from "@/components/editor/useEditorCommands";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "@/components/editor/dialogs/DialogProvider";
import * as facadeRegistry from "@/lib/protocol/facadeRegistry";
import { hasFacadeOwnedLayers } from "@/engine/document";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { invoke } from "@tauri-apps/api/core";
import { answerInvoke, resetStreams, streamFor, tipFor } from "./rustStreamEmulator";
import { commitRustOwnedPaint } from "./historyCursorFixtures";
import { cursorStepInvokes, settle, waitForRust } from "./historyCursorHarness";

vi.mock("@/lib/desktop/tauriWindow", () => ({ isTauriRuntime: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(async () => "0.0.0") }));
vi.mock("@/engine/document", () => ({
  hasFacadeOwnedLayers: vi.fn(() => true),
  isFacadeOwnedLayer: vi.fn(() => false),
}));
// The native walker (`Command::Undo` / `Command::Redo`) drives the canonical
// cursor. It is Rust in production; the stub below moves the same emulated
// cursor so the host's own cursor commands stay countable.
vi.mock("@/lib/protocol/nativeClient", () => {
  // The walker must move the SAME stream `rust_pixels_write_region` recorded the
  // stroke into, so it reads through the shared emulator rather than a private map.
  const stream = (docId: string) => streamFor(docId);
  return {
    nativeProtocol: {
      protocol_seed_native: vi.fn(async () => JSON.stringify({ version: 0 })),
      protocol_seed_canonical_native: vi.fn(async () => JSON.stringify({ ok: true })),
      protocol_canonical_native: vi.fn(async () => JSON.stringify({ ok: true })),
      protocol_snapshot_native: vi.fn(async () => JSON.stringify({ version: 0, layers: [] })),
      protocol_layer_ids_native: vi.fn(async () => JSON.stringify({ ids: [] })),
      protocol_version_native: vi.fn(async (docId: string) => JSON.stringify(streamFor(docId).version)),
      protocol_register_adapter_native: vi.fn(async () => JSON.stringify({ ok: true })),
      protocol_history_query_native: vi.fn(async (docId: string) => JSON.stringify({ cursor: streamFor(docId).cursor, entries: [] })),
      protocol_history_cursor_commit_native: vi.fn(async () => JSON.stringify({ ok: true })),
      protocol_apply_command_native: vi.fn(async (envelopeJson: string, docId: string) => {
        const envelope = JSON.parse(envelopeJson) as {
          command?: { type?: string };
        };
        const type = envelope.command?.type;
        if (type !== "undo" && type !== "redo") {
          return JSON.stringify({
            documentVersion: stream(docId).version,
            delta: { changes: [], baseVersion: stream(docId).version, version: stream(docId).version },
          });
        }
        // The walker moves ITS OWN cursor over the tip; a `Pixel` tip also yields
        // that entry's tiles, an `External` tip yields none (crates/core
        // history.rs:219-233 - the cursor moves for both kinds).
        const s = stream(docId);
        const base = s.version;
        const idx = type === "undo" ? s.cursor - 1 : s.cursor;
        const yielded = idx >= 0 && idx < s.entries.length;
        let tiles: unknown[] | null = null;
        if (yielded) {
          s.cursor = type === "undo" ? s.cursor - 1 : s.cursor + 1;
          s.version += 1;
          const payload = s.payloads[idx] as { x: number; y: number; w: number; h: number; data: number[] } | null;
          tiles = s.entries[idx] === "pixel" && payload ? [payload] : [];
        }
        return JSON.stringify({
          documentVersion: s.version,
          delta: { changes: [], baseVersion: base, version: s.version },
          ...(tiles ? { pixelPatches: { layerId: "l1", tiles, epoch: 0, version: s.version } } : {}),
        });
      }),
    },
  };
});

// `rustStreamHoldsUserWork` and the derived view both read the query through this
// module; answer it from the same emulated stream the walker moves.
vi.mock("@/lib/protocol/bridge_emu", () => ({
  __resetEmulatedForTests: vi.fn(),
  resetWasmDoc: vi.fn(),
  getHistoryQuery: vi.fn(async (docId: string) => ({
    entries: streamFor(docId).entries.map((kind, i) => ({
      seq: i + 1,
      label: kind,
      payloadKind: kind,
      origin: "native",
      affectedLayerIds: ["l1"],
    })),
    cursor: streamFor(docId).cursor,
  })),
}));

const DOC = "doc-host-cursor-step";

function model(name: string): DocumentModel {
  return {
    id: DOC,
    name,
    width: 10,
    height: 10,
    layers: [],
    activeLayerId: null,
    selection: null,
    viewport: { panX: 0, panY: 0, zoom: 1, rotation: 0 },
    dirty: false,
  };
}

function makeEditorContext(history: CommandHistory) {
  const engine = {
    getId: () => DOC,
    getLayer: () => null,
    getLayers: () => [],
    snapshot: () => model("live"),
    restore: vi.fn(),
    applyFacadeSnapshot: vi.fn(),
    applyExternalRasterRestore: vi.fn(),
    getPaintSurface: () => null,
    ensureBitmapCurrent: vi.fn(async () => {}),
    getActiveLayerId: () => "l1",
  };
  return {
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => history,
      getActiveDocumentId: () => DOC,
      notifyVisualChange: vi.fn(),
    },
    renderer: { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn() },
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
    toggleLayerSelection: vi.fn(),
    rangeSelectLayers: vi.fn(),
    selectedLayerId: () => null,
    setSelectedLayerId: vi.fn(),
    textEditSession: () => null,
    setTextEditSession: vi.fn(),
    setStatusLoadingMessage: vi.fn(),
    setShowExportDialog: vi.fn(),
    setShowPrintDialog: vi.fn(),
    setShowResizeDialog: vi.fn(),
  };
}

/** One brush stroke, committed the way `useBrushOverlay` commits one. */
async function stroke() {
  const history = new CommandHistory();
  history.attachDocIdGetter(() => DOC);
  await commitRustOwnedPaint(history, model("Paint"));
  await waitForRust(DOC, 1);
  const editor = makeEditorContext(history);
  mockUseEditor(editor);
  return { history, editor, commands: useEditorCommands(() => {}) };
}

async function settlePress(): Promise<void> {
  for (let i = 0; i < 12; i++) await new Promise<void>((r) => setTimeout(r, 0));
  await settle();
}

describe("the Rust cursor is moved only inside Rust", () => {
  beforeEach(() => {
    localStorage.clear();
    // SHIPPING DEFAULTS, stated so a case cannot drift on them: no `photrez.*`
    // key is set by the app, so facade + native authority are both ON.
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    resetStreams();
    facadeRegistry.__resetFacadeRegistryForTests();
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({} as unknown as ReturnType<
      typeof DialogProviderModule.useDialog
    >);
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation((cmd: string, args: unknown) => answerInvoke(cmd, args));
    vi.spyOn(console, "warn").mockImplementation(() => {});
    vi.spyOn(console, "info").mockImplementation(() => {});
  });

  afterEach(async () => {
    await settle();
    vi.restoreAllMocks();
  });

  it("facade-owned document: the native walker moves the cursor, the host moves it zero times", async () => {
    vi.mocked(hasFacadeOwnedLayers).mockReturnValue(true);
    const { commands } = await stroke();
    expect(streamFor(DOC).cursor, "the stroke recorded its Pixel entry").toBe(1);

    commands.undo();
    await settlePress();

    expect(
      cursorStepInvokes(),
      "the host must issue no cursor command; the walker is the only mover",
    ).toBe(0);
    expect(streamFor(DOC).cursor, "the cursor advanced exactly one step").toBe(0);
    expect(tipFor(DOC).redo_tip_kind, "it advanced over the stroke's Pixel entry").toBe("pixel");
  });
});