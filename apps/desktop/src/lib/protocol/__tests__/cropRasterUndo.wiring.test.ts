// CROP RASTER UNDO - the last open item of the single-canonical-pixel-owner effort.
//
// THE DEFECT (real app, artifact sha256
// 0a130f3bf6d8fb172b33e93dcb5eefbc0b235d7bf7101147b4ad56be49ad1028): after Crop
// Canvas and ONE Ctrl+Z the document size came back to 128x128 while the layer,
// its bitmap and the Rust pixel store all stayed 39x39. Size restored, pixels did
// not.
//
// ROOT CAUSE. On the facade handoff path `runFacadeExternalHandoff` projects the
// External size pair (document dims return) but never restores a raster. It
// cannot: `applyFacadeSnapshot` has no `imageBitmap` field in its descriptor
// (document.ts:2069) and its existing-layer branch is documented as never writing
// width/height (:2141-2154). So layer pixels and layer dims are model-owned with
// no facade writer, and nothing on that path ever put them back. The full-model
// `restore()` is not an option either - on this path it throws E_FACADE_OWNED.
//
// THE CARRIER (and why it is the smallest that works). The pre-op rasters ride on
// the token that ALREADY existed. `recordExternalTransitionFor` mints a token and
// parks the commit's pre-action snapshot - a DocumentModel whose layers carry live
// `imageBitmap` references - under it in `tsPayloadStore` (facadeRegistry.ts:1040).
// `EntryPayload::External.token` already carries that same string, so Rust needs
// NO new payload field: `CommandResult.external_token` merely routes it back out.
// A zero-new-field carrier was considered and rejected on evidence: the record
// command's own result returns `external_seq: None` (document_core_apply.rs:87),
// so the host never learns a seq to key on. One new field on ONE payload is the
// minimum.
//
// HARNESS FIDELITY (three green-suite defects in this repo came from infidelity):
//   - the REAL ProtocolEngine via getWasmExportModule(), NOT bridge_emu - proven
//     armed by a bridge-side discriminator, not assumed;
//   - the REAL EditorFacade and the REAL DocumentEngine;
//   - command arguments read from the SERIALIZED envelope JSON, so a field dropped
//     by a wire mapper cannot hide;
//   - `isFacadeEnabled()` reads getItem(...) !== "0", so an UNSET key means
//     ENABLED and the flags are set explicitly;
//   - store assertions read the Rust store's OWN reported dims, never a
//     downstream symptom. `null - null = 0` looks like a real zero, and a
//     swallowed failed read fakes an empty census, so both are guarded below.

import { describe, it, expect, beforeAll, beforeEach, afterEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import * as bridge from "@/lib/protocol/bridge";
import { __resetEmulatedForTests } from "@/lib/protocol/bridge";
import { CONTRACT_VERSION } from "@/lib/protocol/types";
import {
  __resetFacadeRegistryForTests,
  confirmExternalCursor,
  getExternalRecordSnapshot,
  parkExternalReplaySnapshot,
  recordExternalTransitionFor,
} from "@/lib/protocol/facadeRegistry";
import { DocumentEngine } from "@/engine/document";

// MOCK FIDELITY (this repo shipped three green-suite defects from boundary
// infidelity). Tauri v2 `invoke` REJECTS with an error-envelope OBJECT when a
// Rust command returns Err, and resolves `null` for an Ok(None) - it does not
// resolve `{ok:false}`. A mock that resolved instead would make every
// error-handling path in the store repair look proven while never running.
// So: resolve `null` for a successful command, reject for a failing one.
vi.mock("@tauri-apps/api/core", async (orig) => {
  const actual = await orig<typeof import("@tauri-apps/api/core")>();
  return { ...actual, invoke: vi.fn() };
});
const invokeMock = invoke as unknown as {
  (cmd: string, args?: Record<string, unknown>): Promise<unknown>;
  mockClear: () => void;
  mockImplementation: (fn: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>) => void;
};

/** Every `rust_pixels_resize_layer` the store repair issued, in order. */
let storeReseeds: Array<{ layerId: string; width: number; height: number; bytes: Uint8Array }> = [];

// `readbackBitmap` (storeCurrency.ts:95) reads the raster back through an
// OffscreenCanvas, which jsdom does not provide. Stub it so the readback returns
// the exact bytes of the raster that was just installed - which is precisely what
// the store repair must send, and lets the STORE assertion be real rather than a
// downstream symptom.
let readbackBytes = new Uint8ClampedArray(0);
beforeAll(() => {
  (globalThis as unknown as Record<string, unknown>).OffscreenCanvas = class {
    constructor(public width: number, public height: number) {}
    getContext() {
      return {
        clearRect: () => {},
        save: () => {},
        restore: () => {},
        translate: () => {},
        rotate: () => {},
        scale: () => {},
        drawImage: () => {},
        getImageData: () => ({ width: this.width, height: this.height, data: readbackBytes }),
      };
    }
    transferToImageBitmap() {
      // A NEW object, exactly as a real transfer does: this is what makes the
      // crop a genuine raster REPLACEMENT, so a stale pre-op reference cannot
      // masquerade as restored.
      const out = solidBitmap(this.width, this.height, [9, 9, 9, 255]);
      return out;
    }
  } as unknown as typeof OffscreenCanvas;
});

type WasmModule = {
  protocol_apply_command: (json: string, docId: string) => string;
  protocol_reset: (docId: string) => void;
};

let wasmModule: WasmModule | null = null;

beforeAll(async () => {
  const m = await getWasmExportModule();
  expect(m).not.toBeNull();
  wasmModule = m as unknown as WasmModule;
});

/** A solid-colour raster carrying its own bytes, so content is provable and the
 *  store repair's readback returns the exact pre-crop bytes. The `data` member is
 *  declared alongside `ImageBitmap` because a real `ImageBitmap` only exposes its
 *  pixels through `getImageData`/`drawImage`; the test double keeps the buffer
 *  directly so `fingerprint` can digest the exact bytes that were installed. */
type TestBitmap = ImageBitmap & { data: Uint8ClampedArray };
function solidBitmap(w: number, h: number, rgba: [number, number, number, number]): TestBitmap {
  const data = new Uint8ClampedArray(w * h * 4);
  for (let i = 0; i < w * h; i++) {
    data[i * 4] = rgba[0];
    data[i * 4 + 1] = rgba[1];
    data[i * 4 + 2] = rgba[2];
    data[i * 4 + 3] = rgba[3];
  }
  readbackBytes = data;
  return { width: w, height: h, data, close: () => {} } as unknown as TestBitmap;
}

/** Fingerprint of a raster: dims + a content digest. Size alone would pass on a
 *  store that resized but never reseeded the bytes. */
function fingerprint(b: { width: number; height: number; data: Uint8ClampedArray } | null): string {
  if (!b) return "none";
  let h = 2166136261;
  for (let i = 0; i < b.data.length; i++) {
    h ^= b.data[i];
    h = Math.imul(h, 16777619);
  }
  return `${b.width}x${b.height}:${(h >>> 0).toString(16)}`;
}

describe("crop undo restores the raster and the Rust store, not just the document size", () => {
  beforeEach(() => {
    localStorage.clear();
    // Explicit: an UNSET facade key means ENABLED (isFacadeEnabled is `!== "0"`).
    localStorage.setItem("photrez.facade", "1");
    localStorage.setItem("photrez.facadeAuthority", "wasm");
    __resetEmulatedForTests();
    __resetFacadeRegistryForTests();
    storeReseeds = [];
    invokeMock.mockClear();
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "rust_pixels_get_epoch") return 7; // Ok(Some(7)) -> the store exists
      if (cmd === "rust_pixels_resize_layer") {
        const a = (args ?? {}) as { layerId?: string; width?: number; height?: number; bytes?: Uint8Array };
        storeReseeds.push({
          layerId: String(a.layerId),
          width: Number(a.width),
          height: Number(a.height),
          bytes: a.bytes ?? new Uint8Array(0),
        });
        return null;
      }
      return null;
    });
  });
  afterEach(() => {
    localStorage.clear();
    __resetEmulatedForTests();
    __resetFacadeRegistryForTests();
    wasmModule?.protocol_reset("docCropRaster");
    vi.restoreAllMocks();
  });

  /**
   * The measured scenario, end to end:
   *   128x128 doc + a 128x128 layer -> crop to 39x39 -> ONE undo.
   * `applyExternalRasterRestore` is invoked exactly as the production handoff
   * invokes it, so the WALL-B assertion below is meaningful.
   */
  it("ONE undo returns document dims, layer dims, the layer bitmap AND the store to pre-crop together", async () => {
    const DOC = "docCropRaster";
    const engine = new DocumentEngine(DOC, "P", 128, 128);

    // The host creates the layer; the id is host-minted, and BOTH sides must use
    // it or the store repair would key a different layer than the one it swapped.
    const paint = engine.addLayer("Paint");

    // ── ARMING PROOF: only the REAL wasm populates the bridge snapshot. The
    // emulator never does, and an unwired bridge hard-codes {version:0,layers:[]}.
    await bridge.applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: undefined,
      docId: DOC,
      command: { type: "addLayer", id: paint.id, name: "Paint", width: 128, height: 128, index: 0 },
    } as never);
    expect((await bridge.getSnapshot(DOC)).layers.length).toBeGreaterThan(0);

    // Pre-crop raster, installed on the LIVE layer so applyCrop has real pixels
    // to crop, with a KNOWN content fingerprint.
    const preFp = fingerprint(solidBitmap(128, 128, [37, 236, 205, 213]));
    engine.setLayerImageBitmap(paint.id, solidBitmap(128, 128, [37, 236, 205, 213]));
    // The snapshot the commit parks: its layer carries the live pre-op raster by
    // reference, which is the whole carrier.
    const preOp = engine.snapshot();
    expect(preOp.layers[0].imageBitmap, "premise: the pre-op snapshot holds a raster").toBeTruthy();

    // The crop commit: the host declares both size halves and parks the pre-op
    // snapshot under the minted token, exactly as cropToolActions does.
    await recordExternalTransitionFor(
      DOC,
      {
        label: "Crop Canvas",
        affectedLayerIds: [paint.id],
        snapshot: preOp,
        docSizeChange: { before: { width: 128, height: 128 }, after: { width: 39, height: 39 } },
      },
      engine,
    );

    // The host applies the crop out-of-band: rasters are REPLACED at 39x39.
    engine.applyCrop(0, 0, 39, 39, {
      deleteCroppedPixels: true,
      targetSize: { w: 39, h: 39 },
    });
    expect(engine.getModel().width).toBe(39);
    expect(engine.getLayer(paint.id)?.width).toBe(39);

    // ── ONE undo, through the REAL engine.
    const res = (await bridge.applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: undefined,
      docId: DOC,
      command: { type: "undo" },
    } as never)) as unknown as {
      status?: string;
      externalSeq?: number;
      externalToken?: string;
      delta: { width?: number; height?: number };
    };

    // THE CARRIER. Rust must hand the token back, or the host cannot find the
    // rasters it parked. This is the assertion that goes RED if the field is
    // dropped on either side of the wire.
    expect(res.status).toBe("external");
    expect(typeof res.externalToken, "Rust must return the entry's token").toBe("string");
    expect((res.externalToken ?? "").length).toBeGreaterThan(0);

    // The host's half of the handoff: resolve the token, then apply the rasters.
    const stashed = getExternalRecordSnapshot(res.externalToken);
    expect(stashed, "the token must resolve to the pre-op snapshot").not.toBeNull();
    expect(fingerprint(stashed!.layers[0].imageBitmap as never)).toBe(preFp);

    // The document size half (already worked before this change - the pair rides
    // the same External payload and must keep riding it).
    expect(res.delta.width).toBe(128);
    expect(res.delta.height).toBe(128);

    // THE RASTER HALF. Without this call the layer stays 39x39 forever, which is
    // the shipped defect.
    //
    // `storeReseeds` is cleared HERE so it records only the undo's store repair.
    // applyCrop's own forward reseed (syncRewrittenRastersToStores, 39x39) is
    // already in the array, and matching the first entry for this layer would
    // assert the CROP's store write and pass no matter what the undo did - a
    // false green of exactly the kind this investigation keeps hitting.
    storeReseeds = [];
    // The readback stub must return the pixels of the raster that is ABOUT TO BE
    // installed, because that is what a real readback does - it reads the layer's
    // current raster. Pointing it at the pre-crop bytes is what makes the STORE
    // content assertion real: a repair that resized the store but never reseeded
    // the bytes would send different pixels and redden this.
    readbackBytes = new Uint8ClampedArray(
      (preOp.layers[0].imageBitmap as unknown as { data: Uint8ClampedArray }).data,
    );
    engine.applyExternalRasterRestore(stashed!);

    // NARROWNESS, asserted here rather than assumed: the document size is owned by
    // the External size pair and reaches the model through the handoff's
    // `applyFacadeSnapshot(snap, { dimsAuthoritative })`, NOT through this method.
    // So the model still reads the cropped size here - and that is correct. The
    // contract above (res.delta.width/height === 128) is what carries the document
    // size; this method must leave model.width/height strictly alone, or the one
    // field that already has an owner would acquire a second.
    expect(engine.getModel().width, "this method must NOT own the document size").toBe(39);
    expect(engine.getModel().height).toBe(39);
    expect(engine.getLayer(paint.id)?.width, "layer dims must return to pre-crop").toBe(128);
    expect(engine.getLayer(paint.id)?.height).toBe(128);
    expect(
      fingerprint(engine.getLayer(paint.id)!.imageBitmap as never),
      "layer bitmap CONTENT must return to pre-crop, not merely its size",
    ).toBe(preFp);

    // ── WALL B: THE STORE. `undo_snapshot` and pixel_store.rs never touch the
    // pixel store, so the store is repaired only by the block this change added
    // to the new method. This assertion therefore goes RED on its own when
    // `applyExternalRasterRestore` is not called, with the payload present and
    // everything above it still green - which is exactly the required proof.
    //
    // The repair is fire-and-forget, so drain the microtask queue it runs on.
    await new Promise((r) => setTimeout(r, 0));
    expect(
      storeReseeds.length,
      "the Rust store must be reseeded after the raster swap - without this the " +
        "store stays 39x39 and every later stroke is rejected by write_region",
    ).toBeGreaterThan(0);
    const reseed = storeReseeds.find((s) => s.layerId === paint.id);
    expect(reseed, "the moved layer must be the one reseeded").toBeTruthy();
    expect(reseed!.width, "the STORE must be reseeded at the PRE-CROP width").toBe(128);
    expect(reseed!.height).toBe(128);
    // CONTENT, not just size: a resize that never reseeded the bytes would pass a
    // dims-only check. `null - null = 0` and an all-zero buffer also look like real
    // values, so the digest is compared against the pre-crop raster's own bytes.
    expect(fingerprint({ width: 128, height: 128, data: new Uint8ClampedArray(reseed!.bytes) })).toBe(preFp);
  });

  /**
   * The same contract, the replay direction. ONE redo after that undo must put
   * the layer, its CONTENT and the Rust store back at the CROPPED size - leaving
   * the document at 39x39 is only half the contract, and is exactly what the
   * measured defect got right while every raster-bearing owner stayed 128x128.
   *
   * WHY THE PRE-OP HALF CANNOT SERVE HERE. The token resolves on redo just as it
   * does on undo - `facade.redo()` assigns `lastExternalToken` from the same
   * `CommandResult.externalToken`, and the redo arm sets it
   * (document_core_apply.rs:895) - but the cell that token points at carries
   * only the PRE-op half, because `recordExternalTransitionFor` runs BEFORE the
   * mutation it records. So "the token is absent on the redo delta" is not the
   * defect: it resolves, to the wrong half. Handing the pre-op half to the redo
   * writes a 128x128 raster into a 39x39 document.
   */
  it("ONE redo returns layer dims, layer CONTENT and the Rust store to the CROPPED size", async () => {
    const DOC = "docCropRaster";
    const engine = new DocumentEngine(DOC, "P", 128, 128);
    const paint = engine.addLayer("Paint");

    await bridge.applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: undefined,
      docId: DOC,
      command: { type: "addLayer", id: paint.id, name: "Paint", width: 128, height: 128, index: 0 },
    } as never);
    expect((await bridge.getSnapshot(DOC)).layers.length).toBeGreaterThan(0);

    const preFp = fingerprint(solidBitmap(128, 128, [37, 236, 205, 213]));
    engine.setLayerImageBitmap(paint.id, solidBitmap(128, 128, [37, 236, 205, 213]));
    const preOp = engine.snapshot();

    await recordExternalTransitionFor(
      DOC,
      {
        label: "Crop Canvas",
        affectedLayerIds: [paint.id],
        snapshot: preOp,
        docSizeChange: { before: { width: 128, height: 128 }, after: { width: 39, height: 39 } },
      },
      engine,
    );

    engine.applyCrop(0, 0, 39, 39, {
      deleteCroppedPixels: true,
      targetSize: { w: 39, h: 39 },
    });
    expect(engine.getModel().width).toBe(39);
    expect(engine.getLayer(paint.id)?.width).toBe(39);
    // The cropped content, captured by digest, so the redo is pinned on BYTES and
    // not merely on dims - a resize that never reseeded would pass a dims-only
    // check.
    const postFp = fingerprint(engine.getLayer(paint.id)!.imageBitmap as never);
    expect(postFp, "premise: the crop really changed the content").not.toBe(preFp);

    const undoRes = (await bridge.applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: undefined,
      docId: DOC,
      command: { type: "undo" },
    } as never)) as unknown as {
      status?: string;
      externalSeq?: number;
      externalToken?: string;
      delta: { width?: number; height?: number };
    };
    expect(undoRes.status).toBe("external");

    // The handoff's undo step, in production order: park what this step is about
    // to overwrite, THEN hand back the pre-op half.
    parkExternalReplaySnapshot(undoRes.externalToken!, engine.snapshot());
    readbackBytes = new Uint8ClampedArray(
      (preOp.layers[0].imageBitmap as unknown as { data: Uint8ClampedArray }).data,
    );
    engine.applyExternalRasterRestore(getExternalRecordSnapshot(undoRes.externalToken!)!);
    expect(engine.getLayer(paint.id)?.width, "premise: the undo reached pre-crop").toBe(128);
    // The external-pending barrier is armed by the External step and the redo
    // cannot run at all until production clears it (runFacadeExternalHandoff).
    expect((await confirmExternalCursor(DOC, undoRes.externalSeq!, "undo")).ok).toBe(true);
    await new Promise((r) => setTimeout(r, 0)); // drain the undo's store repair

    // ── ONE redo, through the REAL engine.
    const redoRes = (await bridge.applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: undefined,
      docId: DOC,
      command: { type: "redo" },
    } as never)) as unknown as {
      status?: string;
      externalToken?: string;
      delta: { width?: number; height?: number };
    };

    // The SIZE half already worked before this change and must keep working: the
    // document really does go back to the cropped size on the redo.
    expect(redoRes.status, "the redo must reach the same external entry").toBe("external");
    expect(redoRes.delta.width).toBe(39);
    expect(redoRes.delta.height).toBe(39);
    expect(typeof redoRes.externalToken, "Rust must return the token on the redo arm too").toBe("string");
    expect(redoRes.externalToken, "both directions must key the SAME cell").toBe(undoRes.externalToken);

    // THE RASTER HALF. This is the assertion that carries the defect: the token
    // resolves on the redo, but it must resolve to the POST-op half.
    const replay = getExternalRecordSnapshot(redoRes.externalToken, "redo");
    expect(replay, "the replay direction must resolve a raster at all").not.toBeNull();
    expect(replay!.width, "the replay half is the POST-crop state, never the pre-op half").toBe(39);
    expect(replay!.height).toBe(39);
    expect(fingerprint(replay!.layers[0].imageBitmap as never)).toBe(postFp);

    storeReseeds = [];
    readbackBytes = new Uint8ClampedArray(
      (replay!.layers[0].imageBitmap as unknown as { data: Uint8ClampedArray }).data,
    );
    engine.applyExternalRasterRestore(replay!);

    // NARROWNESS, same contract as the undo case: this method owns layer rasters
    // only. The document size arrives through the handoff's applyFacadeSnapshot,
    // never from here.
    expect(engine.getModel().width).toBe(39);
    expect(engine.getLayer(paint.id)?.width, "layer dims must return to CROPPED").toBe(39);
    expect(engine.getLayer(paint.id)?.height).toBe(39);
    expect(
      fingerprint(engine.getLayer(paint.id)!.imageBitmap as never),
      "layer bitmap CONTENT must return to cropped, not merely its size",
    ).toBe(postFp);

    // WALL B, mirrored: the store is repaired only by applyExternalRasterRestore,
    // so this reddens on its own when the redo's restore does not run, with the
    // payload present and everything above it still green.
    await new Promise((r) => setTimeout(r, 0));
    const reseed = storeReseeds.find((s) => s.layerId === paint.id);
    expect(reseed, "the Rust store must be reseeded after the redo raster swap").toBeTruthy();
    expect(reseed!.width, "the STORE must be reseeded at the CROPPED width").toBe(39);
    expect(reseed!.height).toBe(39);
    expect(fingerprint({ width: 39, height: 39, data: new Uint8ClampedArray(reseed!.bytes) })).toBe(postFp);
  });

  /**
   * The narrowness contract, pinned as an invariant of the new method. E_FACADE_OWNED
   * is safe to leave alone precisely because this method cannot touch the graph.
   */
  it("the raster restore never adds, removes, reorders or re-owns a layer", async () => {
    const DOC = "docCropRaster";
    const engine = new DocumentEngine(DOC, "P", 128, 128);
    const paint = engine.addLayer("Paint");
    const other = engine.addLayer("Other");
    const before = {
      ids: engine.getLayers().map((l) => l.id),
      order: engine.getLayers().map((l) => l.id),
      names: engine.getLayers().map((l) => l.name),
      visible: engine.getLayers().map((l) => l.visible),
      opacity: engine.getLayers().map((l) => l.opacity),
      active: engine.getActiveLayerId(),
      modelW: engine.getModel().width,
      modelH: engine.getModel().height,
    };

    // A pre-op snapshot that names a layer the live model does NOT have: it must
    // be skipped, never added. This is the resurrection vector E_FACADE_OWNED guards.
    const preOp = engine.snapshot();
    preOp.layers[0].imageBitmap = solidBitmap(128, 128, [1, 2, 3, 255]);
    preOp.layers.push({ ...preOp.layers[0], id: "layer-rust-deleted", name: "Ghost" });

    engine.applyExternalRasterRestore(preOp);

    expect(engine.getLayers().map((l) => l.id)).toEqual(before.ids);
    expect(engine.getLayers().map((l) => l.id)).not.toContain("layer-rust-deleted");
    expect(engine.getLayers().map((l) => l.name)).toEqual(before.names);
    expect(engine.getLayers().map((l) => l.visible)).toEqual(before.visible);
    expect(engine.getLayers().map((l) => l.opacity)).toEqual(before.opacity);
    expect(engine.getActiveLayerId()).toBe(before.active);
    // model.width/height are owned by the External size pair, never by this method.
    expect(engine.getModel().width).toBe(before.modelW);
    expect(engine.getModel().height).toBe(before.modelH);
    // The layer whose raster the snapshot carried now holds exactly THAT raster.
    const restored = engine.getLayer(preOp.layers[0].id)!;
    expect(restored.imageBitmap, "the pre-op raster must be installed verbatim").toBe(
      preOp.layers[0].imageBitmap,
    );
    // A layer the snapshot named no raster for is left alone - a null raster must
    // NOT be turned into a non-null one, or the method would be inventing pixels.
    const untouched = engine.getLayers().find((l) => l.id !== preOp.layers[0].id)!;
    expect(untouched.imageBitmap, "a layer with no pre-op raster must stay null").toBeNull();
  });
});
