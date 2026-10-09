// SPDX-License-Identifier: AGPL-3.0-or-later
//
// The open-from-disk path reaches the SAME ownership state as a factory document.
//
// WHY THIS FILE EXISTS. Undo/redo dispatches on `hasFacadeOwnedLayers()`: an owned
// document runs the native walker (`runFacadeExternalHandoff` -> `facade.undo()`),
// an unowned one pops the host stack and the host's own `stepRustCursor` issues
// `rust_pixels_undo`. A document FIRST opened from a `.ptz` was never facade-owned
// on the shipping profile, so it was the one production shape whose cursor had no
// native executor - and the endgame gate (delete the host step + the host pop)
// cannot land while that is true.
//
// THE PATH UNDER TEST IS THE REAL ONE. `loadProjectFile` runs: the Rust reader
// (`loadProject`), bounded parallel PNG decode, `engine.restore`, the workspace
// `addDocument` (native layer seed + canonical shadow seed), and the per-layer
// texture upload. Only the two IPC boundaries are stubbed (`@/tauri/native` and
// `invoke`), because the Rust reader and the Rust engine are not reachable from
// jsdom - every other module, including `@/engine/document`, is the real one.
// A fixture built by hand would prove nothing about this path: `loadProjectFile`
// reads `result.document_json` verbatim, so the fixture IS the document.
//
// THE DOCUMENT IS THE REAL WRITER'S OUTPUT. `ptzWriterGolden.json` holds the bytes
// `PtzDocument::to_json` produced for a real dumped v3 file; `realDumpedModel` is
// that fixture. It is read here, never regenerated.
//
// MOCK FIDELITY. `createImageBitmap` answers a real bitmap SHAPE (width, height,
// close) plus the decoded pixels, because the guard that matters is that the
// decoded object survives the open - a stub with no pixels could not tell a
// surviving bitmap from a blank one. The `protocol_*_native` stubs move the SAME
// emulated stream `rust_pixels_*` moves (`rustStreamEmulator`), which is what the
// real backend does: one cursor, two command families.

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

import { CommandHistory } from "@/engine/history";
import { hasFacadeOwnedLayers, isFacadeOwnedLayer } from "@/engine/document";
import { useEditorCommands } from "../useEditorCommands";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "@/components/editor/dialogs/DialogProvider";
import { WorkspaceManager } from "@/engine/workspace";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { invoke } from "@tauri-apps/api/core";
import { answerInvoke, resetStreams, streamFor, tipFor } from "@/engine/__tests__/rustStreamEmulator";
import { cursorStepInvokes, settle, waitForRust } from "@/engine/__tests__/historyCursorHarness";
import { loadWriterGolden } from "./ptzWriterContract";
import type { EditorContextValue } from "../shell/EditorContext";
import type { DocumentSession } from "@/engine/workspace";

// ── The real fixture: the Rust writer's bytes for a real dumped v3 project ──
const golden = loadWriterGolden();
const GOLDEN_CASE = "realDumpedModel";
const GOLDEN_INPUT = JSON.parse(golden[GOLDEN_CASE].input) as {
  id: string;
  name: string;
  width: number;
  height: number;
  activeLayerId: string;
  layers: Array<{ id: string; width: number; height: number }>;
};
const DOC = GOLDEN_INPUT.id;
const DOC_NAME = `${GOLDEN_CASE}.ptz`;
const LAYER_IDS = GOLDEN_INPUT.layers.map((l) => l.id);

// One distinct fill per layer id, so a crossed or blanked bitmap is detectable.
function pixelsFor(layerId: string): Uint8ClampedArray {
  const out = new Uint8ClampedArray(4);
  for (let i = 0; i < layerId.length; i++) out[i % 4] = layerId.charCodeAt(i);
  return out;
}

interface DecodedBitmap extends ImageBitmap {
  __pixels: Uint8ClampedArray;
  __source: string;
}

/**
 * A decoded bitmap in the shape `createImageBitmap` really returns, carrying the
 * PNG bytes it came from so the guard can compare CONTENT, not identity alone.
 */
const decodedBitmaps = new Map<string, DecodedBitmap>();

function stubCreateImageBitmap() {
  return (async (blob: Blob): Promise<DecodedBitmap> => {
    const source = await blob.text();
    const entry = decodedBitmaps.get(source);
    if (!entry) throw new Error(`emulator: no PNG registered for ${source}`);
    return entry;
  }) as unknown as typeof globalThis.createImageBitmap;
}

function registerPng(source: string, width: number, height: number, layerId: string) {
  const bitmap = {
    width,
    height,
    close: vi.fn(),
    __pixels: pixelsFor(layerId),
    __source: source,
  } as unknown as DecodedBitmap;
  decodedBitmaps.set(source, bitmap);
  return bitmap;
}

const hoisted = vi.hoisted(() => ({
  mockLoadProject: vi.fn<() => Promise<{ document_json: string; layers: Record<string, string> }>>(),
  mockShowToast: vi.fn(),
  mockAddRecentFile: vi.fn(),
}));

vi.mock("@/tauri/native", () => ({
  showOpenImageDialog: vi.fn(),
  loadProject: hoisted.mockLoadProject,
  readFileBytes: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@/lib/desktop/tauriWindow", () => ({ isTauriRuntime: vi.fn(() => true) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(async () => "0.0.0") }));
vi.mock("@/lib/recentFiles", () => ({ addRecentFile: hoisted.mockAddRecentFile }));
vi.mock("../Toast", () => ({ showToast: hoisted.mockShowToast }));

// The native engine is Rust in production. These stubs move the SAME emulated
// cursor `rust_pixels_*` moves, so the host's own cursor commands stay countable
// and the walker's movement is observable without going through `invoke`.
vi.mock("@/lib/protocol/nativeClient", () => {
  const stream = (docId: string) => streamFor(docId);
  return {
    nativeProtocol: {
      protocol_seed_native: vi.fn(async () => JSON.stringify({ version: 0 })),
      protocol_seed_canonical_native: vi.fn(async () => JSON.stringify({ ok: true })),
      protocol_canonical_native: vi.fn(async () => JSON.stringify({ ok: true })),
      protocol_snapshot_native: vi.fn(async () => JSON.stringify({ version: 0, layers: [] })),
      protocol_layer_ids_native: vi.fn(async () => JSON.stringify({ ids: [] })),
      protocol_version_native: vi.fn(async (docId: string) => JSON.stringify(stream(docId).version)),
      protocol_register_adapter_native: vi.fn(async () => JSON.stringify({ ok: true })),
      protocol_history_query_native: vi.fn(async (docId: string) =>
        JSON.stringify({ cursor: stream(docId).cursor, entries: [] }),
      ),
      protocol_history_cursor_commit_native: vi.fn(async () => JSON.stringify({ ok: true })),
      protocol_apply_command_native: vi.fn(async (envelopeJson: string, docId: string) => {
        const envelope = JSON.parse(envelopeJson) as { command?: { type?: string } };
        const type = envelope.command?.type;
        if (type !== "undo" && type !== "redo") {
          return JSON.stringify({
            documentVersion: stream(docId).version,
            delta: { changes: [], baseVersion: stream(docId).version, version: stream(docId).version },
          });
        }
        // `ProtocolEngine` moves its cursor over a Pixel AND an External tip
        // (crates/core/src/history.rs:219-233); only a Pixel tip yields tiles.
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
          ...(tiles ? { pixelPatches: { layerId: LAYER_IDS[0], tiles, epoch: 0, version: s.version } } : {}),
        });
      }),
    },
  };
});

// `rustStreamHoldsUserWork` and the derived view read the query through this
// module; answer it from the same stream the walker moves.
vi.mock("@/lib/protocol/bridge_emu", () => ({
  __resetEmulatedForTests: vi.fn(),
  resetWasmDoc: vi.fn(),
  getHistoryQuery: vi.fn(async (docId: string) => ({
    entries: streamFor(docId).entries.map((kind, i) => ({
      seq: i + 1,
      label: kind,
      payloadKind: kind,
      origin: "native",
      affectedLayerIds: [LAYER_IDS[0]],
    })),
    cursor: streamFor(docId).cursor,
  })),
}));

let originalCreateImageBitmap: typeof globalThis.createImageBitmap;

/** One layer PNG per fixture layer, distinct bytes so a crossed decode shows. */
function ptzFixture(): Record<string, string> {
  const out: Record<string, string> = {};
  for (const l of GOLDEN_INPUT.layers) {
    const source = `png:${l.id}`;
    registerPng(source, l.width, l.height, l.id);
    out[l.id] = btoa(source);
  }
  return out;
}

function makeParams(workspace: WorkspaceManager) {
  return {
    workspace,
    renderer: { uploadImage: vi.fn() } as unknown as OpenImageParams["renderer"],
    scheduler: { requestRender: vi.fn() } as unknown as OpenImageParams["scheduler"],
    onError: vi.fn(),
    onLoading: vi.fn(),
  } as unknown as OpenImageParams;
}

type OpenImageParams = import("../editorOpenImage").OpenImageParams;

function engineOf(workspace: WorkspaceManager) {
  const engine = workspace.getActiveEngine();
  if (!engine) throw new Error("no engine on the workspace");
  return engine;
}

function sessionOf(workspace: WorkspaceManager): DocumentSession {
  const sessions = (workspace as unknown as { sessions: Map<string, DocumentSession> }).sessions;
  const s = sessions.get(DOC);
  if (!s) throw new Error(`no session for ${DOC}`);
  return s;
}

async function openProject(workspace: WorkspaceManager): Promise<void> {
  hoisted.mockLoadProject.mockResolvedValue({
    document_json: golden[GOLDEN_CASE].input,
    layers: ptzFixture(),
  });
  const { loadProjectFile } = await import("../editorOpenImage");
  await loadProjectFile(`/projects/${DOC_NAME}`, makeParams(workspace), DOC_NAME);
  await settle();
}

/** A brush stroke committed the way `useBrushOverlay` commits one. */
async function strokeOn(session: DocumentSession, layerId: string) {
  const history = session.history as CommandHistory;
  history.attachDocIdGetter(() => session.engine.getId());
  const model = session.engine.snapshot();
  const patches = {
    layerId,
    surfaceWidth: 10,
    surfaceHeight: 10,
    before: [{ x: 0, y: 0, width: 2, height: 2, data: new Uint8ClampedArray([1, 1, 1, 255, 1, 1, 1, 255, 1, 1, 1, 255, 1, 1, 1, 255]) }],
    after: [{ x: 0, y: 0, width: 2, height: 2, data: new Uint8ClampedArray([2, 2, 2, 255, 2, 2, 2, 255, 2, 2, 2, 255, 2, 2, 2, 255]) }],
    rustOwned: true,
  };
  // The canonical, ungated writer records the Pixel entry; the host then commits
  // the twin with `alreadyRecordedInRust`, so nothing else records a second one.
  await vi.mocked(invoke)("rust_pixels_write_region", { docId: session.engine.getId(), layerId });
  history.commit(model, "Brush", patches as never, true);
  await waitForRust(session.engine.getId(), 1);
}

function editorContextFor(workspace: WorkspaceManager) {
  return {
    workspace: {
      getActiveEngine: () => workspace.getActiveEngine(),
      getActiveHistory: () => workspace.getActiveHistory(),
      getActiveDocumentId: () => workspace.getActiveDocumentId(),
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
    activeLayerId: () => LAYER_IDS[0],
    selectedLayerIds: () => [],
    setSelectedLayerIds: vi.fn(),
    setSelectedLayerId: vi.fn(),
    textEditSession: () => null,
    setTextEditSession: vi.fn(),
    setStatusLoadingMessage: vi.fn(),
    setShowExportDialog: vi.fn(),
    setShowPrintDialog: vi.fn(),
    setShowResizeDialog: vi.fn(),
  } as unknown as EditorContextValue;
}

async function settlePress(): Promise<void> {
  for (let i = 0; i < 12; i++) await new Promise<void>((r) => setTimeout(r, 0));
  await settle();
}

describe("a document opened from a .ptz is facade-owned like a factory document", () => {
  beforeEach(() => {
    localStorage.clear();
    // SHIPPING DEFAULTS: the app sets no `photrez.*` key, so facade + native
    // authority are both ON. Stated so a case cannot drift onto an opt-out.
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    resetStreams();
    decodedBitmaps.clear();
    originalCreateImageBitmap = globalThis.createImageBitmap;
    globalThis.createImageBitmap = stubCreateImageBitmap();
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
    globalThis.createImageBitmap = originalCreateImageBitmap;
    vi.restoreAllMocks();
    localStorage.clear();
  });

  it("ends the open facade-owned: the dispatch gate is on for every layer", async () => {
    const workspace = new WorkspaceManager();
    expect(hasFacadeOwnedLayers(), "premise: nothing is facade-owned before the open").toBe(false);

    await openProject(workspace);

    expect(hasFacadeOwnedLayers(), "an opened document must dispatch through the native walker").toBe(true);
    for (const id of LAYER_IDS) {
      expect(isFacadeOwnedLayer(id), `layer ${id} must be facade-owned`).toBe(true);
    }
  });

  it("ONE undo press moves the native cursor over the stroke's own entry, host moves it zero times", async () => {
    const workspace = new WorkspaceManager();
    await openProject(workspace);
    const session = sessionOf(workspace);
    await strokeOn(session, LAYER_IDS[0]);
    expect(streamFor(DOC).cursor, "the stroke recorded its Pixel entry").toBe(1);

    mockUseEditor(editorContextFor(workspace) as unknown as Record<string, unknown>);
    const commands = useEditorCommands(() => {});
    commands.undo();
    await settlePress();

    expect(
      cursorStepInvokes(),
      "the host must issue no cursor command; the walker is the only mover",
    ).toBe(0);
    expect(streamFor(DOC).cursor, "the cursor advanced exactly one step").toBe(0);
    expect(tipFor(DOC).redo_tip_kind, "it advanced over the stroke's Pixel entry").toBe("pixel");
  });

  it("keeps every decoded layer bitmap and its pixels across the open", async () => {
    const workspace = new WorkspaceManager();
    await openProject(workspace);

    const layers = engineOf(workspace).getLayers();
    expect(layers.map((l) => l.id), "no layer is dropped by the projection").toEqual(LAYER_IDS);
    for (const l of layers) {
      const bitmap = l.imageBitmap as unknown as DecodedBitmap | null;
      expect(bitmap, `layer ${l.id} kept its decoded bitmap`).not.toBeNull();
      expect(
        Array.from(bitmap!.__pixels),
        `layer ${l.id} kept ITS OWN pixels (a crossed or blanked decode shows here)`,
      ).toEqual(Array.from(pixelsFor(l.id)));
    }
  });

  it("restores the document size and viewport the file carried", async () => {
    const workspace = new WorkspaceManager();
    await openProject(workspace);
    const engine = engineOf(workspace);

    expect([engine.getWidth(), engine.getHeight()]).toEqual([GOLDEN_INPUT.width, GOLDEN_INPUT.height]);
    // The file's viewport is a float panY / zoom pair; a projection that wrote
    // the document size back would have to invent a viewport to do it.
    const viewport = engine.getModel().viewport as unknown as { panX: number; panY: number; zoom: number };
    const restored = JSON.parse(golden[GOLDEN_CASE].input).viewport as { panX: number; panY: number; zoom: number };
    expect([viewport.panX, viewport.panY, viewport.zoom]).toEqual([restored.panX, restored.panY, restored.zoom]);
  });

  it("emits NO native history entry for the open itself", async () => {
    const workspace = new WorkspaceManager();
    await openProject(workspace);

    expect(
      streamFor(DOC).entries,
      "seeding a loaded document is initialization, not a user edit (crates/core document_core.rs:143-147)",
    ).toEqual([]);
    expect(streamFor(DOC).cursor).toBe(0);
  });

  it("saves back to the byte-exact bytes the Rust writer produced for this document", async () => {
    const workspace = new WorkspaceManager();
    await openProject(workspace);

    const { emulateRustSerialization } = await import("./ptzWriterContract");
    const written = emulateRustSerialization(engineOf(workspace).snapshot());
    const expected = golden[GOLDEN_CASE].expected;

    // `loadProjectFile` calls `engine.clearDirty()`, so an opened document is
    // clean; the golden's `dirty` is the one field the open legitimately
    // normalizes. Everything else must be byte-identical.
    const normalizeDirty = (bytes: string) => bytes.replace('"dirty":true', '"dirty":false');
    expect(normalizeDirty(written)).toBe(normalizeDirty(expected));
  });

  it("reopening the same file in a fresh workspace is not blocked by stale ownership", async () => {
    const first = new WorkspaceManager();
    await openProject(first);

    // A second open of the same document id, after the first one was closed. The
    // owned-id set is module state, so this is where a stale marker would make
    // `engine.restore` refuse with E_FACADE_OWNED and the open throw.
    first.removeDocument(DOC);

    const second = new WorkspaceManager();
    await expect(openProject(second)).resolves.not.toThrow();
    expect(hasFacadeOwnedLayers()).toBe(true);
  });
});