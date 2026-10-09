// SPDX-License-Identifier: AGPL-3.0-or-later
//
// The open-from-disk path reaches the SAME ownership state as a factory document.
//
// WHY THIS FILE EXISTS. Undo/redo dispatches on `hasFacadeOwnedLayers()`: an owned
// document runs the native walker (`runFacadeExternalHandoff` -> `facade.undo()`),
// an unowned one pops the host stack and the host's own `stepRustCursor` issues
// `rust_pixels_undo`. A document FIRST opened from a `.ptz` was never facade-owned
// on the shipping profile, so for that shape the host's own cursor step was the
// only thing that moved the cursor - and that host step and the host stack pop it
// serves can only be deleted once EVERY document shape that can still reach them
// is gone. MEASURED, so the bar is stated exactly: on the shipping profile real
// documents still reach the host executor in two more shapes - (a) a
// `loadProjectFile` whose facade seed fails hits the catch-and-continue path and
// opens with zero claims while a sibling owns layers, and (b) a two-document
// shared-lineage session before its sibling closes. This file's cases cover a
// THIRD, narrower one: a lone opened document, which after this path runs fully
// native.
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
import { DocumentEngine, hasFacadeOwnedLayers, isFacadeOwnedLayer, releaseFacadeOwned } from "@/engine/document";
import { cancelLayerTransformSession } from "../transformSession";
import type { LayerTransformSession } from "../tools/editorState";
import { useEditorCommands } from "../useEditorCommands";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "@/components/editor/dialogs/DialogProvider";
import { WorkspaceManager } from "@/engine/workspace";
import { nativeProtocol } from "@/lib/protocol/nativeClient";
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
// Two more documents opened from the SAME project bytes. Their layer ids are the
// file's own, so all three documents carry identical layer ids and differ only in
// the document id - which is what makes facade ownership a per-document question.
const DOC_A = `${DOC}-lineage-a`;
const DOC_B = `${DOC}-lineage-b`;
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
//
// WHAT THIS TRANSPORT DOES NOT MODEL: the document-open baseline entry. The real
// `Command::SetBackgroundFlag` arm brackets its apply with `begin_forward` /
// `finish_forward` (crates/core/src/document_core_apply.rs:377-397), so a
// factory-built document's stream opens holding one "Set Background Flag" entry
// at position 0 and `deriveOpenBaselineFloor` reads 1. The apply arm below
// answers a non-cursor command without recording anything, so that floor reads
// 0 here for the same document. MEASURED CONSEQUENCE: `getNativeUndoCount` and
// `rustStreamHoldsUserWork` both subtract the floor, so a factory document's
// count and refusal verdict would sit one step lower here than in production.
// No assertion in this file depends on the production value: this file drives
// the OPENED-FROM-DISK shape, which issues no background-flag apply at all and
// so carries no baseline entry in production either (`loadProjectFile` seeds
// and adopts ownership; it never flags a layer) - production and this transport
// agree at floor 0 here. The floor of 1 is pinned where it is read from a query
// no test constructed, in `openBaselineLabelBoundary.test.ts`. A transport that
// DOES need the production baseline must record the entry in its own apply arm;
// `nativeProtocolEmulator.ts` is the reference for the seed/membership half of
// this boundary, and it does not record a baseline either.
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

function sessionOf(workspace: WorkspaceManager, docId: string = DOC): DocumentSession {
  const sessions = (workspace as unknown as { sessions: Map<string, DocumentSession> }).sessions;
  const s = sessions.get(docId);
  if (!s) throw new Error(`no session for ${docId}`);
  return s;
}

/**
 * The project file's own bytes with ONE field rewritten: the document id.
 *
 * A project read from disk restores the FILE's layer ids verbatim, so two
 * documents opened from this file carry the same layer ids by construction -
 * that is the shape the ownership cases below need, and it is the shape that
 * makes ownership a per-document question rather than a per-layer-id one.
 */
function projectBytesFor(docId: string): string {
  const model = JSON.parse(golden[GOLDEN_CASE].input) as Record<string, unknown>;
  model.id = docId;
  return JSON.stringify(model);
}

async function openProjectAs(workspace: WorkspaceManager, docId: string): Promise<void> {
  hoisted.mockLoadProject.mockResolvedValue({
    document_json: projectBytesFor(docId),
    layers: ptzFixture(),
  });
  const { loadProjectFile } = await import("../editorOpenImage");
  await loadProjectFile(`/projects/${docId}.ptz`, makeParams(workspace), `${docId}.ptz`);
  await settle();
}

function openProject(workspace: WorkspaceManager): Promise<void> {
  return openProjectAs(workspace, DOC);
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
    // Follows the workspace's focused document, the way the real context does,
    // so a case that switches documents drives the press on the one it focused.
    activeDocumentId: () => workspace.getActiveDocumentId() ?? "",
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

/**
 * How many times the NATIVE walker was asked to undo or redo.
 *
 * The host-side census (`cursorStepInvokes`) cannot tell "the floor refused"
 * from "the floor refused and then something else popped": a press that reaches
 * neither the walker nor the host pop is still a correct outcome. This counts the
 * walker's own `undo`/`redo` commands, so a case can assert the walker was not
 * driven at all - which is what "the refusal happened before any executor" means.
 */
function nativeWalkerSteps(): number {
  return vi
    .mocked(nativeProtocol.protocol_apply_command_native)
    .mock.calls.filter((c) => {
      const envelope = JSON.parse(c[0] as string) as { command?: { type?: string } };
      return envelope.command?.type === "undo" || envelope.command?.type === "redo";
    }).length;
}

describe("a document opened from a .ptz is facade-owned like a factory document", () => {
  beforeEach(() => {
    localStorage.clear();
    // Ownership is process-global module state (`@/engine/document`), so a case
    // that inherits it inherits the code under test to clear it. Reset it here so
    // every case below proves its OWN open; without this the multi-document cases
    // would pass on markers an earlier case left behind.
    (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
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

  it("opening the SAME file twice without closing the first is not blocked by the superseded session's claims", async () => {
    const workspace = new WorkspaceManager();
    await openProject(workspace);
    expect(hasFacadeOwnedLayers(), "the first open claims the file's layer ids").toBe(true);

    // The workspace holds one session per document id, so this second open
    // SUPERSEDES the first session - same document id, same layer ids - and its
    // `engine.restore` reads that document's own claims. Without the open-path
    // release the superseded session's claims are still there and the open throws
    // E_FACADE_OWNED against itself. Nothing is closed in between, so the
    // document-close path cannot be what unblocks this one.
    await expect(openProject(workspace)).resolves.not.toThrow();
    expect(hasFacadeOwnedLayers(), "the reopened document owns its layers again").toBe(true);
  });

  it("a projection through a CLOSED engine cannot release the reopened document's claims", async () => {
    const workspace = new WorkspaceManager();
    await openProject(workspace);
    const closed = sessionOf(workspace).engine;

    // Close, then reopen the same file: the reopen mints a NEW engine under the
    // SAME document id, so both engines answer to `DOC` and carry the file's
    // layer ids verbatim.
    workspace.removeDocument(DOC);
    await openProject(workspace);
    const live = sessionOf(workspace).engine;
    expect(live, "the reopen installed a new engine").not.toBe(closed);
    expect(live.getId(), "both engines answer to the one document id").toBe(closed.getId());
    for (const id of LAYER_IDS) {
      expect(isFacadeOwnedLayer(id), `premise: the reopened document owns ${id}`).toBe(true);
    }

    // The closed engine is still reachable by anything that captured it before
    // the close - an in-flight funnel holds the engine it was driving, and the
    // close detaches its callbacks without invalidating it. Its projection
    // reconciles facade ownership under `getId()`, which is now the LIVE
    // document's id, so a layer dropped here would strip the reopened session's
    // claim over that id and disarm its undo.
    closed.applyFacadeSnapshot({
      version: 1,
      layers: [
        {
          id: LAYER_IDS[0],
          name: LAYER_IDS[0],
          visible: true,
          opacity: 1,
          x: 0,
          y: 0,
          scaleX: 1,
          scaleY: 1,
          rotation: 0,
          resourceId: 1,
        },
      ],
    } as never);

    for (const id of LAYER_IDS) {
      expect(isFacadeOwnedLayer(id), `the reopened document still owns ${id}`).toBe(true);
    }
    expect(hasFacadeOwnedLayers(), "its undo still dispatches through the native walker").toBe(true);
  });

  it("closing the last facade-owning document releases every claim it held", async () => {
    const workspace = new WorkspaceManager();
    await openProject(workspace);
    expect(hasFacadeOwnedLayers(), "premise: the open claimed the file's layer ids").toBe(true);

    // This is the document-CLOSE path's own work, and nothing here re-opens a
    // project afterwards, so no open-path release can stand in for it. `restore`
    // reads the closed document's claims, so a claim left behind would block the
    // next open of the same file outright - and while it survives it also keeps
    // the undo dispatch gate (`useEditorCommands.restoreHistorySnapshot`) pointed
    // at the native walker for a document nobody has open.
    workspace.removeDocument(DOC);

    expect(hasFacadeOwnedLayers(), "no open document owns any layer any more").toBe(false);
    for (const id of LAYER_IDS) {
      expect(isFacadeOwnedLayer(id), `layer ${id} lost its claim with its document`).toBe(false);
    }

    const reopened = new WorkspaceManager();
    await expect(openProject(reopened)).resolves.not.toThrow();
    expect(hasFacadeOwnedLayers(), "the reopened document adopts ownership again").toBe(true);
  });

  it("closing one of two documents opened from the same file leaves the other's undo on the native walker", async () => {
    const workspace = new WorkspaceManager();
    expect(hasFacadeOwnedLayers(), "premise: nothing is owned before the opens").toBe(false);

    // The real defect shape: two documents from ONE project, live at the same
    // time. A project read from disk restores the FILE's layer ids verbatim, so
    // both documents carry the same layer ids and ownership cannot be tracked by
    // layer id alone.
    await openProjectAs(workspace, DOC_A);
    await openProjectAs(workspace, DOC_B);
    for (const id of LAYER_IDS) {
      expect(isFacadeOwnedLayer(id), `layer ${id} is owned while both documents are open`).toBe(true);
    }

    mockUseEditor(editorContextFor(workspace) as unknown as Record<string, unknown>);
    const commands = useEditorCommands(() => {});

    // While BOTH are open, each document's press runs the native walker.
    for (const docId of [DOC_A, DOC_B]) {
      workspace.switchDocument(docId);
      await strokeOn(sessionOf(workspace, docId), LAYER_IDS[0]);
      const hostStepsBefore = cursorStepInvokes();
      commands.undo();
      await settlePress();
      expect(
        cursorStepInvokes() - hostStepsBefore,
        `${docId}: the host issued no cursor command; the native walker is the only mover`,
      ).toBe(0);
      expect(streamFor(docId).cursor, `${docId}: the native walker moved its cursor one step`).toBe(0);
    }

    workspace.removeDocument(DOC_B);

    for (const id of LAYER_IDS) {
      expect(
        isFacadeOwnedLayer(id),
        `closing ${DOC_B} must not strip ${DOC_A} of layer ${id} - it is still open`,
      ).toBe(true);
    }
    expect(hasFacadeOwnedLayers(), `${DOC_A} still dispatches through the native walker`).toBe(true);

    // And the survivor's press is still the native walker's, not the host's.
    workspace.switchDocument(DOC_A);
    await strokeOn(sessionOf(workspace, DOC_A), LAYER_IDS[0]);
    expect(streamFor(DOC_A).cursor, "the second stroke recorded its own entry").toBe(1);
    const hostStepsAfterClose = cursorStepInvokes();
    commands.undo();
    await settlePress();
    expect(
      cursorStepInvokes() - hostStepsAfterClose,
      `${DOC_A}: after its sibling closed, the host still issues no cursor command`,
    ).toBe(0);
    expect(streamFor(DOC_A).cursor, `${DOC_A}: the native walker moved its cursor one step`).toBe(0);
    expect(tipFor(DOC_A).redo_tip_kind, "it advanced over the stroke's own Pixel entry").toBe("pixel");
  });

  it("undoing past the open state of a document read from disk drives the cursor nowhere", async () => {
    // The opened-document shape has NO open-baseline entry: seeding a loaded
    // document is initialization, not a user edit, so `deriveOpenBaselineFloor`
    // reads 0 here where a factory document reads 1. That makes "the floor" mean
    // "cursor 0" and the case below is the only evidence that the floor refuses
    // at 0 for THIS shape - the factory-document floor case cannot speak for it.
    const workspace = new WorkspaceManager();
    await openProject(workspace);
    await strokeOn(sessionOf(workspace), LAYER_IDS[0]);
    expect(streamFor(DOC).entries, "the stroke is the first entry the stream holds").toHaveLength(1);

    const history = sessionOf(workspace).history as CommandHistory;
    const popSpy = vi.spyOn(history, "undo");

    mockUseEditor(editorContextFor(workspace) as unknown as Record<string, unknown>);
    const commands = useEditorCommands(() => {});

    commands.undo();
    await settlePress();
    expect(streamFor(DOC).cursor, "one undo brings the cursor to the floor").toBe(0);

    // One step FURTHER. The native cursor must not move: there is nothing behind
    // the open state to undo, and a cursor that walks below the floor is a step
    // the user can never get back. Asserting the cursor alone would prove
    // nothing - the emulator cannot step below zero - so this pins that NEITHER
    // executor ran: the walker was never driven, and the host issued nothing.
    const hostStepsAtFloor = cursorStepInvokes();
    const walkerStepsAtFloor = nativeWalkerSteps();
    commands.undo();
    await settlePress();
    expect(streamFor(DOC).cursor, "the second undo did not walk the cursor below the floor").toBe(0);
    expect(
      nativeWalkerSteps() - walkerStepsAtFloor,
      "the native walker was not driven past the floor",
    ).toBe(0);
    expect(
      cursorStepInvokes() - hostStepsAtFloor,
      "and no host cursor command was issued either",
    ).toBe(0);

    // WHICH GATE produced that host zero, because the two censuses are not the
    // same kind of claim. The walker arm is a live guard: the press reached it and
    // it refused. The host arm never ran at all - `restoreHistorySnapshot` asks
    // `history.canUndo()` first, and for a stream the engine knows, that answer
    // comes from the NATIVE cursor, not from the derived stack. So the host twin
    // is still sitting there, un-popped, and the pop is refused anyway. Pinning
    // the gate is what makes the host census above mean something: it is a census
    // of an arm that cannot execute here, and this is the reason why.
    expect(
      popSpy,
      "the derived stack was never popped: the press never reached the host pop",
    ).not.toHaveBeenCalled();
    expect(
      history.canUndo(),
      "the pop's own gate refuses at the floor, because it reads the native cursor",
    ).toBe(false);

    // Mirror at the top: redo back up, then one step further.
    commands.redo();
    await settlePress();
    expect(streamFor(DOC).cursor, "one redo returns to the top of the stream").toBe(1);
    const hostStepsAtTop = cursorStepInvokes();
    const walkerStepsAtTop = nativeWalkerSteps();
    commands.redo();
    await settlePress();
    expect(streamFor(DOC).cursor, "the second redo did not walk past the top").toBe(1);
    expect(nativeWalkerSteps() - walkerStepsAtTop, "the native walker was not driven past the top").toBe(0);
    expect(cursorStepInvokes() - hostStepsAtTop, "and no host cursor command was issued either").toBe(0);
  });
});

/**
 * The two guard structures, pinned as a PAIR.
 *
 * `DocumentEngine.restore`'s mixed-history guard reads this document's OWN claims
 * (`facadeOwnedByDoc`); the twelve legacy mutator guards read the process-wide
 * union (`facadeOwnedIds`). One layer id therefore gets two different answers,
 * and the case below fixes WHICH answer each site owes, for a FOREIGN id - one
 * another open document owns:
 *
 *  - `restore` must LAND. It protects one engine's own history, so a sibling's
 *    claim over a shared id is not this engine's business. Layer ids are only
 *    unique inside a document and a project read from disk restores the file's
 *    ids verbatim, so two documents opened from one file share every id; reading
 *    the union here makes the second open throw E_FACADE_OWNED against its own
 *    sibling. That is the measured reason for the narrowing, not a preference.
 *  - the legacy mutators must be REFUSED. Rust owns that layer exclusively
 *    somewhere, so no host write may go through it, whichever document asked.
 *
 * Both halves are load-bearing in opposite directions, and one case carries them
 * because one case fails if EITHER site switches structure: read the union in
 * `restore` and the first half throws; read the ledger in a mutator and the
 * second half stops throwing.
 *
 * Real engines, real projection seam, shipping defaults (no `photrez.*` key set).
 * The divergence the split buys is asserted, not hidden: after the restore, the
 * borrowed layer IS in this engine's model and IS blocked from the legacy path -
 * Rust stays the single writer for an id any document holds.
 */
describe("the per-document and process-wide guard structures answer differently for a foreign id", () => {
  /** One layer per id, in the shape `applyFacadeSnapshot` reads. */
  function projection(ids: string[]) {
    return {
      version: 1,
      layers: ids.map((id, i) => ({
        id,
        name: id,
        visible: true,
        opacity: 1,
        x: 0,
        y: 0,
        scaleX: 1,
        scaleY: 1,
        rotation: 0,
        resourceId: i + 1,
      })),
    };
  }

  const DOC_OWNER = "guard-structure-owner";
  const DOC_BORROWER = "guard-structure-borrower";
  const OWNED_ELSEWHERE = "owned-by-the-other-document";

  beforeEach(() => {
    localStorage.clear();
    (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation((cmd: string, args: unknown) => answerInvoke(cmd, args));
  });

  afterEach(async () => {
    await settle();
    vi.restoreAllMocks();
    localStorage.clear();
  });

  it("restore lands on a foreign id while every legacy mutator is refused it", () => {
    // The owning document, marked through the real ownership seam. Only ONE
    // document holds the id from here on.
    const owner = new DocumentEngine(DOC_OWNER, "Owner", 32, 32);
    owner.applyFacadeSnapshot(projection([OWNED_ELSEWHERE]) as never);
    expect(isFacadeOwnedLayer(OWNED_ELSEWHERE), "premise: the union says the id is owned").toBe(true);

    // The borrowing document, in the shape a project open starts in: a fresh
    // engine with an empty model and NO claims of its own. This is the production
    // two-document open - the second document from a file whose ids the first one
    // already owns - and its ledger and the union disagree about the snapshot.
    const borrower = new DocumentEngine(DOC_BORROWER, "Borrower", 32, 32);

    // `restore` reads this document's claims only, so the owner's claim over the
    // borrowed id cannot block this engine's restore. The owner's snapshot is a
    // real `DocumentModel`; only its document id is rewritten, which is what the
    // shared-lineage open does.
    const borrowed = { ...owner.snapshot(), id: DOC_BORROWER };
    expect(() => borrower.restore(borrowed), "a foreign claim must not block this restore").not.toThrow();
    expect(
      borrower.getLayers().map((l) => l.id),
      "the borrowed id really is in this engine's model now",
    ).toContain(OWNED_ELSEWHERE);

    // The mutator guards read the union, so the borrowed layer is refused: it is
    // Rust-owned, and the facade stays its single writer.
    expect(
      () => borrower.deleteLayer(OWNED_ELSEWHERE),
      "the union's answer is authoritative for a foreign id: delete is refused",
    ).toThrow(/E_FACADE_OWNED/);
    expect(
      () => borrower.setLayerOpacity(OWNED_ELSEWHERE, 0.5),
      "and so is every metadata mutator in the same family",
    ).toThrow(/E_FACADE_OWNED/);
  });
});

/**
 * A `restore()` that lands outside the facade path leaves the engine's projection
 * baseline stale, and the next projection reconciles against that stale baseline.
 *
 * The question this answers is whether that staleness can RELEASE a claim for a
 * layer the model still holds. It cannot, and the reason is structural rather
 * than lucky: `restore` refuses whenever this document claims an id that appears
 * in the snapshot or in the current model, so a restore that LANDS does so only
 * while this document claims nothing - and the release set of the next
 * projection is (stale baseline minus the ids that projection carries), every
 * one of which has already left this engine's model, because the projection
 * REPLACES the model with exactly the ids it carries. So the release set and the
 * model are disjoint by construction.
 *
 * `restore` is driven through a real production caller - the transform-session
 * cancel, `cancelLayerTransformSession` - not called directly. The same engine
 * call is made by the Escape arm of a legacy selection drag
 * (`useSelectionTransformDrag`, the `engine.restore(session.originalSnapshot)`
 * on the non-facade drag branch), so the shape under test is the shape both
 * callers have.
 *
 * The claim this guards is a NEGATIVE ("no live claim is released"), so it is
 * pinned from both ends: the landed restore must leave the model holding layers
 * the owned set does not, and the projection after it must leave every layer the
 * model still holds claimed again. The invariant is unconditional given one
 * production fact - a projection REPLACES the model with exactly the ids it
 * carries - so a case that stops asserting the model's contents after the
 * projection would pass vacuously. That is why the shape is asserted last and
 * the ownership of the surviving layers first.
 */
describe("a restore outside the facade path cannot strand a live claim through the stale projection baseline", () => {
  const DOC_ID = "stale-baseline-document";
  const KEPT = "kept-through-the-projection";
  const GONE = "deleted-on-the-rust-side";

  function projection(ids: string[]) {
    return {
      version: 1,
      layers: ids.map((id, i) => ({
        id,
        name: id,
        visible: true,
        opacity: 1,
        x: 0,
        y: 0,
        scaleX: 1,
        scaleY: 1,
        rotation: 0,
        resourceId: i + 1,
      })),
    };
  }

  beforeEach(() => {
    localStorage.clear();
    (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation((cmd: string, args: unknown) => answerInvoke(cmd, args));
  });

  afterEach(async () => {
    await settle();
    vi.restoreAllMocks();
    localStorage.clear();
  });

  it("releases nothing the model still holds, through the real transform-session cancel", () => {
    const engine = new DocumentEngine(DOC_ID, "Stale", 32, 32);
    engine.applyFacadeSnapshot(projection([KEPT, GONE]) as never);
    expect(
      isFacadeOwnedLayer(GONE),
      "premise: the projection claimed both ids and set the baseline",
    ).toBe(true);

    // A real session snapshot, taken the way a transform session takes one.
    const full = engine.snapshot();

    // The production open-site release: `loadProjectFile` gives up this document's
    // claims over the file's ids before it restores them. It does NOT touch the
    // engine's projection baseline, which is what makes the baseline stale below.
    releaseFacadeOwned(DOC_ID, [KEPT, GONE]);
    expect(hasFacadeOwnedLayers(), "premise: the claims are gone, the baseline is not").toBe(false);

    // Drive the real caller. Its snapshot is the session's own real
    // `DocumentModel`, so the model's layers are untouched by the restore - which
    // is the point: the stale baseline, not the model, is what drifted.
    const session: LayerTransformSession = {
      documentId: DOC_ID,
      layerId: KEPT,
      originalSnapshot: full,
      originalTransform: engine.getLayer(KEPT)!.transform,
      mode: "resize",
      lockRatio: false,
      startedAt: 0,
    };
    expect(
      () => cancelLayerTransformSession(session, engine as never),
      "the cancel's restore lands: nothing this document claims is in the way",
    ).not.toThrow();
    expect(engine.getLayers().map((l) => l.id), "the model still holds both layers").toEqual([KEPT, GONE]);
    expect(
      isFacadeOwnedLayer(KEPT),
      "the model now holds layers the owned set does not - this is the stale state",
    ).toBe(false);

    // The next projection carries only KEPT: GONE was deleted on the Rust side. Its
    // release set is the stale baseline minus what it carries, so GONE is released -
    // and it has left the model in the same step, because the projection REPLACES
    // the model with exactly the ids it carries. That replacement is what keeps the
    // release set and the model disjoint, and it is the load-bearing fact here.
    engine.applyFacadeSnapshot(projection([KEPT]) as never);
    for (const id of engine.getLayers().map((l) => l.id)) {
      expect(isFacadeOwnedLayer(id), `layer ${id} the model still holds is claimed again`).toBe(true);
    }
    expect(
      engine.getLayers().map((l) => l.id),
      "the projection's ids are the model's ids",
    ).toEqual([KEPT]);
    expect(
      isFacadeOwnedLayer(GONE),
      "the released id is one the model no longer holds, so it stays released",
    ).toBe(false);
  });
});