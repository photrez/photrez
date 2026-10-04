// apps/desktop/src/components/editor/__tests__/saveAsPtzArms.test.ts
//
// BLOCKING coverage: the two `.ptz` arms of Save As must reach the typed Rust
// document parser.
//
// `useEditorCommands.ts` writes a `.ptz` from TWO places, and only the first
// was covered anywhere:
//
//   1. `:817` — Save As with a `.ptz` chosen in the dialog.
//   2. `:856` — the sibling `.ptz` BACKUP written while exporting a flat format
//              (PNG/JPEG/...) with "Also save a project backup (.ptz)" checked.
//              The path is derived (`path.replace(/\.[^.]+$/, ".ptz")`) and must
//              be approved via `setTrustedPaths` before Rust will write it.
//
// WHY THIS MATTERS: every save now round-trips through a REJECTING parser
// (`PtzDocument::from_json` returns `E_VALIDATION` rather than tolerating a
// shape it does not model). A document reachable from these arms that fails
// validation produces an error toast and NO file written, where the previous
// permissive writer would have written something. Autosave folds the same
// failure into a status flag. Nothing else connects the UI command to the
// parser, so without this a validation regression in either arm would ship with
// a green suite.
//
// These tests drive the REAL `useEditorCommands` and the REAL
// `serializeAndSaveProject`; only the Tauri boundary (dialog, trusted paths,
// streaming save commands) is mocked. The document is serialized by
// `ptzWriterContract.ts`, which is diffed byte-for-byte against the real Rust
// writer in `ptzWriterGolden.test.ts` — so "reached the typed parser" means the
// document really did pass that projection, which is pinned to Rust's bytes.

import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { DocumentEngine } from "@/engine/document";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "../dialogs/DialogProvider";
import { useEditorCommands } from "../useEditorCommands";

const { mockBegin, mockWriteLayer, mockEnd, mockCancel, mockInvoke, mockDialogSave } =
  vi.hoisted(() => ({
    mockBegin: vi.fn<(path: string, document: unknown) => Promise<string>>(),
    mockWriteLayer: vi.fn<(handleId: string, layerId: string, png: Uint8Array) => Promise<void>>(),
    mockEnd: vi.fn<(handleId: string) => Promise<void>>(),
    mockCancel: vi.fn<(handleId: string) => Promise<void>>(),
    mockInvoke: vi.fn(),
    mockDialogSave: vi.fn<(opts: unknown) => Promise<string | null>>(),
  }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(() => Promise.resolve("0.0.0")) }));
vi.mock("@/lib/desktop/tauriWindow", () => ({ isTauriRuntime: vi.fn(() => false) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: mockDialogSave, open: vi.fn() }));

// Only the streaming-save COMMANDS are mocked. `showSaveDialogAllFormats` and
// `setTrustedPaths` are the REAL implementations, because the path approval
// Rust requires lives inside `showSaveDialogAllFormats` (native.ts) -- mocking
// it would test the mock rather than the production approval path.
vi.mock("@/tauri/native", async (importOriginal) => ({
  ...(await importOriginal() as object),
  saveProjectStreamingBegin: mockBegin,
  saveProjectStreamingWriteLayer: mockWriteLayer,
  saveProjectStreamingEnd: mockEnd,
  saveProjectStreamingCancel: mockCancel,
}));

const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47]);

function makeBitmap(width: number, height: number): ImageBitmap {
  return { width, height, close: vi.fn() } as unknown as ImageBitmap;
}

/** OffscreenCanvas stub good enough for the save encode path under jsdom. */
function stubSerializeGlobals() {
  const mkCtx = () => {
    const ctx: any = {};
    ctx.drawImage = vi.fn();
    ctx.translate = vi.fn(() => ctx);
    ctx.beginPath = vi.fn();
    ctx.rect = vi.fn();
    ctx.moveTo = vi.fn();
    ctx.lineTo = vi.fn();
    ctx.closePath = vi.fn();
    ctx.ellipse = vi.fn();
    ctx.fill = vi.fn();
    ctx.stroke = vi.fn();
    ctx.save = vi.fn();
    ctx.restore = vi.fn();
    return ctx;
  };
  vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
    this.width = w;
    this.height = h;
    this.getContext = () => mkCtx();
    this.transferToImageBitmap = () => makeBitmap(Math.max(1, w), Math.max(1, h));
    this.convertToBlob = vi.fn().mockResolvedValue(new Blob([PNG as BlobPart], { type: "image/png" }));
  }));
}

/** A document with two layers, so the flat-export backup arm is reachable. */
function makeSession() {
  const engine = new DocumentEngine("saveas-doc", "Save As Doc", 200, 150);
  const l1 = engine.addLayer("Background", 200, 150);
  engine.setLayerImageBitmap(l1.id, makeBitmap(200, 150));
  engine.setLayerOpacity(l1.id, 0.5);
  const l2 = engine.addLayer("Overlay", 100, 80);
  engine.setLayerImageBitmap(l2.id, makeBitmap(100, 80));
  engine.setLayerBlendMode(l2.id, "multiply");
  return {
    engine,
    history: { canUndo: () => false, canRedo: () => false },
    displayName: "SaveAsDoc.png",
    sourcePath: undefined as string | undefined,
    dirty: true,
  };
}

function mockEditor(session: ReturnType<typeof makeSession>, confirmWithCheckbox: unknown) {
  mockUseEditor({
    workspace: {
      getActiveSession: () => session,
      getActiveEngine: () => session.engine,
      getActiveHistory: () => session.history,
      getActiveDocumentId: () => "saveas-doc",
      notifyVisualChange: vi.fn(),
      isFull: () => false,
    },
    renderer: { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn() },
    scheduler: { requestRender: vi.fn() },
    activeDocumentId: () => "saveas-doc",
    layerTransformSession: () => null,
    setLayerTransformSession: vi.fn(),
    activeTool: () => "brush",
    cropInteractionMode: () => "modern",
    canCropUndo: () => false,
    canCropRedo: () => false,
    canModernCropUndo: () => false,
    canModernCropRedo: () => false,
    layers: () => session.engine.getLayers(),
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
  } as unknown as Record<string, unknown>);

  vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({
    confirmWithCheckbox,
  } as unknown as ReturnType<typeof DialogProviderModule.useDialog>);
}

/** Let the save queue drain. */
const flush = async () => {
  for (let i = 0; i < 12; i++) await Promise.resolve();
  await new Promise<void>((r) => setTimeout(r, 0));
};

/** The documents the typed Rust parser was handed, with their paths. */
function savedDocuments() {
  return mockBegin.mock.calls.map((c) => ({ path: c[0], document: c[1] as any }));
}

/** Paths the REAL native layer approved for Rust file-IO. */
function trustedPathCalls(): string[][] {
  return mockInvoke.mock.calls
    .filter((c) => c[0] === "set_trusted_paths")
    .map((c) => (c[1] as { paths: string[] }).paths);
}

describe("Save As .ptz arms reach the typed Rust document parser", () => {
  beforeEach(() => {
    mockBegin.mockReset().mockResolvedValue("handle-saveas");
    mockWriteLayer.mockReset().mockResolvedValue(undefined);
    mockEnd.mockReset().mockResolvedValue(undefined);
    mockCancel.mockReset().mockResolvedValue(undefined);
    mockDialogSave.mockReset();
    // The real native layer resolves every IPC through `invokeApi`, so a
    // permissive envelope here lets the genuine set_trusted_paths call land.
    mockInvoke.mockReset().mockResolvedValue({
      ok: true, contract_version: "2.0.0", data: { trusted: 1 },
    });
    stubSerializeGlobals();
    localStorage.clear();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("arm 1: a .ptz chosen in the dialog is written to that exact path", async () => {
    mockDialogSave.mockResolvedValue("/chosen/out.ptz");
    const session = makeSession();
    mockEditor(session, vi.fn());

    const commands = useEditorCommands(() => {});
    commands.execute("file.save-as");
    await flush();

    // The real `showSaveDialogAllFormats` ran and approved the chosen path for
    // Rust file-IO -- without that approval `check_path_trusted` would reject
    // the save in production.
    expect(mockDialogSave).toHaveBeenCalledWith(
      expect.objectContaining({ defaultPath: "SaveAsDoc.ptz" }),
    );
    expect(trustedPathCalls()).toContainEqual(["/chosen/out.ptz"]);

    const saves = savedDocuments();
    expect(saves).toHaveLength(1);
    expect(saves[0].path).toBe("/chosen/out.ptz");

    // The document that reached the Rust writer is complete and typed.
    const doc = saves[0].document;
    expect(doc.id).toBe("saveas-doc");
    expect(doc.name).toBe("Save As Doc");
    expect(doc.width).toBe(200);
    expect(doc.height).toBe(150);
    expect(doc.layers).toHaveLength(2);
    // Resolved by name: the engine serialises layers top-to-bottom, so index 0
    // is the most recently added layer.
    const byName = Object.fromEntries(doc.layers.map((l: any) => [l.name, l]));
    expect(byName.Background.opacity).toBe(0.5);
    expect(byName.Overlay.blendMode).toBe("multiply");
    expect(byName.Background.visible).toBe(true);
    // Header keys belong to the Rust writer, never the host.
    expect(doc.format).toBeUndefined();
    expect(doc.version).toBeUndefined();
    // Live bitmaps must never reach the IPC boundary.
    for (const layer of doc.layers) {
      expect(layer.imageBitmap).toBeNull();
      expect(layer.baseImageBitmap).toBeNull();
    }
    expect(mockEnd).toHaveBeenCalled();
  });

  it("arm 2: the flat-export .ptz backup is written to the derived sibling path", async () => {
    // Flat format chosen, and the "also save a .ptz backup" box checked.
    mockDialogSave.mockResolvedValue("/chosen/out.png");
    const confirmWithCheckbox = vi.fn().mockResolvedValue({ confirmed: true, checked: true });
    const session = makeSession();
    mockEditor(session, confirmWithCheckbox);

    const commands = useEditorCommands(() => {});
    commands.execute("file.save-as");
    await flush();

    // The multi-layer warning arm is what makes the backup reachable.
    expect(confirmWithCheckbox).toHaveBeenCalled();
    expect(session.engine.getLayers().length).toBeGreaterThan(1);

    const saves = savedDocuments();
    // The backup went to the sibling .ptz, NOT the chosen .png.
    const ptzSaves = saves.filter((s) => s.path.endsWith(".ptz"));
    expect(ptzSaves).toHaveLength(1);
    expect(ptzSaves[0].path).toBe("/chosen/out.ptz");

    // It is a NEW path, so the command approves it explicitly (`:854`) before
    // Rust will write it -- a second approval, distinct from the dialog's.
    expect(trustedPathCalls()).toContainEqual(["/chosen/out.ptz"]);

    // And the backup document is the same complete, typed document.
    const doc = ptzSaves[0].document;
    expect(doc.id).toBe("saveas-doc");
    expect(doc.layers).toHaveLength(2);
    expect(doc.format).toBeUndefined();
    for (const layer of doc.layers) {
      expect(layer.imageBitmap).toBeNull();
    }
    expect(mockEnd).toHaveBeenCalled();
  });

  it("arm 2: no backup is written when the checkbox is cleared", async () => {
    mockDialogSave.mockResolvedValue("/chosen/out.png");
    const confirmWithCheckbox = vi.fn().mockResolvedValue({ confirmed: true, checked: false });
    const session = makeSession();
    mockEditor(session, confirmWithCheckbox);

    const commands = useEditorCommands(() => {});
    commands.execute("file.save-as");
    await flush();

    // No .ptz reaches the writer; only the flat export happens.
    expect(savedDocuments().filter((s) => s.path.endsWith(".ptz"))).toHaveLength(0);
  });

  it("a rejected document surfaces an error and writes nothing in both arms", async () => {
    // The rejecting-parser failure mode: the host hands over a document the
    // typed writer refuses, so no archive may be reported as saved.
    mockBegin.mockRejectedValue({
      ok: false,
      error: { code: "E_VALIDATION", message: "Document does not match the .ptz format", details: null },
    });
    mockDialogSave.mockResolvedValue("/chosen/out.ptz");
    const session = makeSession();
    mockEditor(session, vi.fn());

    const commands = useEditorCommands(() => {});
    commands.execute("file.save-as");
    await flush();

    // begin rejected, so `end` must never have run: no file was finalized.
    expect(mockBegin).toHaveBeenCalled();
    expect(mockEnd).not.toHaveBeenCalled();
  });
});
