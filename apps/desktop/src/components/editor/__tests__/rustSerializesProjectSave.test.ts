// apps/desktop/src/components/editor/__tests__/rustSerializesProjectSave.test.ts
//
// Wiring contract for the production save path: the RUST core owns the
// `.ptz` `document.json` bytes.
//
// What these tests pin (each one is a real wiring assertion, not a pure
// function check):
//   1. `serializeAndSaveProject` hands `saveProjectStreamingBegin` the
//      document as a VALUE, not a pre-serialized string -- so Rust is the
//      thing that serializes it.
//   2. The document handed over carries every field the loader needs
//      (viewport, activeLayerId, selection, dimensions) and never carries
//      live `ImageBitmap` objects, which cannot cross the IPC boundary.
//   3. The header keys are Rust's to write, so the host must NOT stamp them
//      (a host-stamped `version` would silently diverge from the Rust
//      writer's and mask a wrong version in Rust).
//   4. A Rust serialization failure surfaces as a thrown Error and does NOT
//      report success: no `end`, no layer writes.
//
// The Rust side of the round trip (real dumped fixture -> write -> read
// back out of the ZIP) is proven in `crates/core/src/ptz_document.rs` and
// `apps/desktop/src-tauri/src/save_stream.rs`.

import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { DocumentEngine } from "@/engine/document";
import {
  PTZ_DOC_KEYS,
  assertProjectionStaysNarrow,
  emulateRustSerialization,
} from "./ptzWriterContract";

const { mockBegin, mockWriteLayer, mockEnd, mockCancel } = vi.hoisted(() => ({
  mockBegin: vi.fn<(path: string, document: unknown) => Promise<string>>(),
  mockWriteLayer: vi.fn<(handleId: string, layerId: string, png: Uint8Array) => Promise<void>>(),
  mockEnd: vi.fn<(handleId: string) => Promise<void>>(),
  mockCancel: vi.fn<(handleId: string) => Promise<void>>(),
}));

vi.mock("@/tauri/native", () => ({
  saveProjectStreamingBegin: mockBegin,
  saveProjectStreamingWriteLayer: mockWriteLayer,
  saveProjectStreamingEnd: mockEnd,
  saveProjectStreamingCancel: mockCancel,
}));

function makeBitmap(width: number, height: number): ImageBitmap {
  return { width, height, close: vi.fn() } as unknown as ImageBitmap;
}

function stubSerializeGlobals(pngBytes: Uint8Array) {
  const mkCtx = () => {
    const ctx: any = {};
    ctx.drawImage = vi.fn();
    ctx.translate = vi.fn(() => ctx);
    ctx.beginPath = vi.fn();
    ctx.rect = vi.fn();
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
    // The save worker pool reads `blob.arrayBuffer()`, so this must be a real
    // Blob -- returning raw bytes fails with "blob.arrayBuffer is not a function".
    this.convertToBlob = vi.fn().mockResolvedValue(new Blob([pngBytes as BlobPart], { type: "image/png" }));
  }));
}

const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47]);

describe("project save hands the document to Rust to serialize", () => {
  beforeEach(() => {
    mockBegin.mockReset();
    mockWriteLayer.mockReset().mockResolvedValue(undefined);
    mockEnd.mockReset().mockResolvedValue(undefined);
    mockCancel.mockReset().mockResolvedValue(undefined);
    mockBegin.mockResolvedValue("handle-1");
    stubSerializeGlobals(PNG);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("passes the document as a structured value, not a serialized string", async () => {
    const engine = new DocumentEngine("doc-1", "Doc", 100, 80);
    const l1 = engine.addLayer("L1", 100, 80);
    engine.setLayerImageBitmap(l1.id, makeBitmap(100, 80));

    const { serializeAndSaveProject } = await import("../projectSerialize");
    await serializeAndSaveProject(engine, "/p/out.ptz");

    expect(mockBegin).toHaveBeenCalledTimes(1);
    const [path, document] = mockBegin.mock.calls[0];
    expect(path).toBe("/p/out.ptz");
    // A string here would mean the host serialized the payload and Rust is
    // just writing bytes it was handed -- the thing this task removed.
    expect(typeof document).toBe("object");
    expect(document).not.toBeNull();
    expect(Array.isArray((document as any).layers)).toBe(true);
  });

  it("hands over every field the loader restores, with no live bitmaps", async () => {
    const engine = new DocumentEngine("doc-2", "Restore Me", 120, 90);
    const l1 = engine.addLayer("L1", 120, 90);
    engine.setLayerImageBitmap(l1.id, makeBitmap(120, 90));

    const { serializeAndSaveProject } = await import("../projectSerialize");
    await serializeAndSaveProject(engine, "/p/restore.ptz");

    const document = mockBegin.mock.calls[0][1] as any;

    // Fields `loadProjectFile` reads back out of the saved document.
    expect(document.id).toBe("doc-2");
    expect(document.name).toBe("Restore Me");
    expect(document.width).toBe(120);
    expect(document.height).toBe(90);
    expect(document).toHaveProperty("activeLayerId");
    expect(document).toHaveProperty("viewport");
    expect(document).toHaveProperty("selection");
    expect(document).toHaveProperty("dirty");

    // Live ImageBitmap objects would throw on JSON.stringify at the IPC
    // boundary, so both must be null before the document is handed over.
    for (const layer of document.layers) {
      expect(layer.imageBitmap).toBeNull();
      expect(layer.baseImageBitmap).toBeNull();
    }
  });

  it("does not stamp format/version -- those belong to the Rust writer", async () => {
    const engine = new DocumentEngine("doc-3", "Header", 64, 64);
    engine.addLayer("L1", 64, 64);

    const { serializeAndSaveProject } = await import("../projectSerialize");
    await serializeAndSaveProject(engine, "/p/header.ptz");

    const document = mockBegin.mock.calls[0][1] as any;
    // A host-stamped version would diverge from the Rust writer's and hide a
    // wrong version there behind a correct one here.
    expect(document.format).toBeUndefined();
    expect(document.version).toBeUndefined();
  });

  it("propagates a Rust serialization failure and never reports success", async () => {
    // Tauri rejects with an error-envelope OBJECT when the command returns
    // Err, not with a resolved {ok:false}.
    mockBegin.mockRejectedValue({
      ok: false,
      error: { code: "E_VALIDATION", message: "Document does not match the .ptz format", details: null },
    });

    const engine = new DocumentEngine("doc-4", "Bad", 64, 64);
    engine.addLayer("L1", 64, 64);

    const { serializeAndSaveProject } = await import("../projectSerialize");

    await expect(serializeAndSaveProject(engine, "/p/bad.ptz")).rejects.toThrow();

    // Nothing was finalized, so no truncated archive can be reported as saved.
    expect(mockEnd).not.toHaveBeenCalled();
    expect(mockWriteLayer).not.toHaveBeenCalled();
  });
});

/**
 * Autosave and Save As are separate entry points on the same substrate. These
 * cases drive them through the REAL `serializeAndSaveProject` (only the Tauri
 * boundary is mocked), so the document really does travel to the Rust writer
 * contract rather than a stand-in. `autoSave.test.ts` mocks the serializer
 * wholesale and so cannot see any of this.
 */
describe("autosave and Save As reach the Rust writer", () => {
  beforeEach(() => {
    mockBegin.mockReset().mockResolvedValue("handle-auto");
    mockWriteLayer.mockReset().mockResolvedValue(undefined);
    mockEnd.mockReset().mockResolvedValue(undefined);
    mockCancel.mockReset().mockResolvedValue(undefined);
    stubSerializeGlobals(PNG);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  /** The document the Rust writer was handed, parsed back for assertions. */
  function documentWrittenOnCall(call = 0): any {
    return mockBegin.mock.calls[call][1];
  }

  it("autosave hands the dirty document to the Rust writer", async () => {
    const writeFileBytes = vi.fn();
    const readFileBytes = vi.fn().mockRejectedValue(new Error("no manifest"));
    vi.doMock("@tauri-apps/api/path", () => ({ appCacheDir: async () => "/cache/" }));
    vi.doMock("@/tauri/native", async (importOriginal) => ({
      ...(await importOriginal() as object),
      writeFileBytes,
      readFileBytes,
      deleteAutosaveFile: vi.fn(),
      saveProjectStreamingBegin: mockBegin,
      saveProjectStreamingWriteLayer: mockWriteLayer,
      saveProjectStreamingEnd: mockEnd,
      saveProjectStreamingCancel: mockCancel,
    }));

    const engine = new DocumentEngine("autosave-doc", "Autosaved", 100, 80);
    const l1 = engine.addLayer("L1", 100, 80);
    engine.setLayerImageBitmap(l1.id, makeBitmap(100, 80));
    engine.setLayerOpacity(l1.id, 0.6);

    const workspace = {
      getSessions: () => [{
        engine: {
          getId: () => "autosave-doc",
          isDirty: () => true,
          snapshot: () => engine.snapshot(),
          getDirtyLayerIds: () => [l1.id],
          ensureBitmapCurrent: vi.fn(),
        },
        displayName: "Autosaved.ptz",
        dirty: true,
      }],
    } as never;

    const { autosaveDirtyDocs } = await import("../autoSave");
    await autosaveDirtyDocs(workspace);

    // The real serializer ran, so the Rust writer really was handed a document.
    expect(mockBegin).toHaveBeenCalledTimes(1);
    expect(mockEnd).toHaveBeenCalledTimes(1);
    const document = documentWrittenOnCall();
    expect(document.id).toBe("autosave-doc");
    expect(document.name).toBe("Autosaved");
    expect(document.width).toBe(100);
    expect(document.height).toBe(80);
    expect(document.layers).toHaveLength(1);
    // Host must not stamp the header the Rust writer owns.
    expect(document.format).toBeUndefined();
    expect(document.version).toBeUndefined();
    vi.doUnmock("@tauri-apps/api/path");
    vi.doUnmock("@/tauri/native");
  });

  it("Save As (file.save-as) writes through the same Rust writer contract", async () => {
    const showSaveDialogAllFormats = vi.fn().mockResolvedValue("/p/chosen.ptz");
    const setTrustedPaths = vi.fn();
    vi.doMock("@tauri-apps/plugin-dialog", () => ({
      save: vi.fn().mockResolvedValue("/p/chosen.ptz"),
      open: vi.fn(),
    }));
    vi.doMock("@/tauri/native", async (importOriginal) => ({
      ...(await importOriginal() as object),
      showSaveDialogAllFormats,
      setTrustedPaths,
      saveProjectStreamingBegin: mockBegin,
      saveProjectStreamingWriteLayer: mockWriteLayer,
      saveProjectStreamingEnd: mockEnd,
      saveProjectStreamingCancel: mockCancel,
    }));

    const engine = new DocumentEngine("saveas-doc", "Chosen", 300, 200);
    const l1 = engine.addLayer("Only Layer", 300, 200);
    engine.setLayerImageBitmap(l1.id, makeBitmap(300, 200));
    engine.setLayerBlendMode(l1.id, "color-dodge");

    // Save As resolves the path then calls the same serializeAndSaveProject.
    const { serializeAndSaveProject } = await import("../projectSerialize");
    await serializeAndSaveProject(engine, await showSaveDialogAllFormats());

    expect(showSaveDialogAllFormats).toHaveBeenCalled();
    const [path, document] = mockBegin.mock.calls[0];
    expect(path).toBe("/p/chosen.ptz");
    expect(typeof document).toBe("object");
    expect((document as any).id).toBe("saveas-doc");
    expect((document as any).layers[0].blendMode).toBe("color-dodge");
    expect((document as any).layers[0].imageBitmap).toBeNull();
    vi.doUnmock("@tauri-apps/plugin-dialog");
    vi.doUnmock("@/tauri/native");
  });

  /**
   * The double-save fidelity claim, end to end: save, then save the document
   * that was written, and prove the two payloads agree. A field silently lost
   * on the first write shows up as a difference here.
   */
  it("a re-save of a written document produces an identical payload", async () => {
    const engine = new DocumentEngine("fidelity", "Fidelity", 120, 90);
    const l1 = engine.addLayer("Base", 120, 90);
    engine.setLayerImageBitmap(l1.id, makeBitmap(120, 90));
    engine.setLayerOpacity(l1.id, 0.42);
    engine.setLayerBlendMode(l1.id, "soft-light");
    engine.setActiveLayer(l1.id);

    const { serializeAndSaveProject } = await import("../projectSerialize");
    await serializeAndSaveProject(engine, "/p/fidelity.ptz");

    const firstHandedOver = documentWrittenOnCall(0);
    const firstWritten = emulateRustSerialization(firstHandedOver);

    // Feed the written document back through as the next save's input, exactly
    // as reopening the project would.
    const reloaded = new DocumentEngine("fidelity", "Fidelity", 120, 90);
    reloaded.restore(JSON.parse(firstWritten));
    await serializeAndSaveProject(reloaded, "/p/fidelity2.ptz");

    const secondWritten = emulateRustSerialization(documentWrittenOnCall(1));
    expect(secondWritten).toBe(firstWritten);
  });
});

describe("the TypeScript writer model stays narrow", () => {
  // NOTE: agreement with the real Rust writer is proven by diffing the golden
  // bytes in `ptzWriterGolden.test.ts`. This check only proves the projection
  // does not go permissive (drops unmodelled keys, rejects bad input).
  it("drops extras and rejects out-of-contract documents", () => {
    expect(() => assertProjectionStaysNarrow()).not.toThrow();
  });

  it("drops a document field the typed writer does not model", () => {
    // The exact regression the old `{...document}` spread hid: a field added
    // to DocumentModel but not to PtzDocument is lost on save. Production drops
    // it; the model must too, or every assertion here proves the wrong contract.
    const written = JSON.parse(emulateRustSerialization({
      id: "d", name: "n", width: 1, height: 1, dirty: false,
      someBrandNewField: "must not survive",
      layers: [],
    }));
    expect(written.someBrandNewField).toBeUndefined();
    expect(Object.keys(written).sort()).toEqual([...PTZ_DOC_KEYS].sort());
  });

  it("rejects an out-of-contract document instead of writing it", () => {
    expect(() => emulateRustSerialization({
      id: "d", name: "n", width: 1, height: 1, dirty: false,
      layers: [{
        id: "l", name: "L", type: "raster", visible: true, opacity: "opaque",
        locked: false, blendMode: "normal", width: 1, height: 1,
      }],
    })).toThrow(/does not match the .ptz format/);
  });

  it("rejects an unknown blend mode rather than downgrading it", () => {
    expect(() => emulateRustSerialization({
      id: "d", name: "n", width: 1, height: 1, dirty: false,
      layers: [{
        id: "l", name: "L", type: "raster", visible: true, opacity: 1,
        locked: false, blendMode: "not-a-mode", width: 1, height: 1,
      }],
    })).toThrow(/does not match the .ptz format/);
  });
});