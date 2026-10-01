// WHAT THIS FILE PINS: the emulator's propagation of the host document-size pair
// (FIX 2), end to end through the real record path.
//
//   recordExternalTransitionFor -> applyCommand -> emulateApply -> undo
//     -> delta carrying the restored document size -> engine dims updated
//
// WHAT THIS FILE DOES NOT PIN: the wire mapping in toRustEnvelope. With the facade
// off and wasm unarmed, applyCommand takes the emulator branch (bridge.ts:360-368)
// and hands emulateApply the RAW camelCase envelope; the toRustEnvelope call on the
// way there computes `json` and then discards it. So deleting the wire mapping
// entirely would leave this file green.
//
// The wire is covered where it actually happens, in
// toRustEnvelopeCarriesDocSize.wiring.test.ts, which drives toRustEnvelope
// directly. armMirror.test.ts separately binds the bridge's snake_case keys to the
// Rust field names, so the two sides cannot drift apart unnoticed.
//
// Both halves are worth keeping. This file's coverage is real - the emulator must
// read the supplied pair rather than hardcode the current dims, which would make
// both halves equal and let the emit rule suppress the delta - and reverting that
// fix does redden it.

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  __resetEmulatedForTests,
  setEmuDocumentDims,
  getEmuDocumentDims,
} from "@/lib/protocol/bridge_emu";
import { applyCommand } from "@/lib/protocol/bridge";
import { CONTRACT_VERSION } from "@/lib/protocol/types";
import { __resetFacadeRegistryForTests, getFacade } from "@/lib/protocol/facadeRegistry";
import { resolveCropDocumentSize } from "@/engine/document";
import { MAX_CANVAS_DIM } from "@/engine/types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

if (typeof (globalThis as { localStorage?: unknown }).localStorage === "undefined") {
  const __ls = new Map<string, string>();
  (globalThis as { localStorage: Storage }).localStorage = {
    getItem: (k: string) => (__ls.has(k) ? (__ls.get(k) as string) : null),
    setItem: (k: string, v: string) => {
      __ls.set(k, String(v));
    },
    removeItem: (k: string) => {
      __ls.delete(k);
    },
    clear: () => __ls.clear(),
    key: (i: number) => Array.from(__ls.keys())[i] ?? null,
    get length() {
      return __ls.size;
    },
  } as Storage;
}

describe("crop document-size pair survives the real bridge to the engine", () => {
  beforeEach(() => {
    localStorage.clear();
    invokeMock && (invokeMock as unknown as { mockReset?: () => void });
    __resetEmulatedForTests();
    __resetFacadeRegistryForTests();
    // facade OFF + wasm authority: the emulator backs the stream. The facade flag
    // must be OFF here - with it ON, applyCommand refuses to emulate and demands
    // a real armed wasm engine (E_FACADE_NOT_READY), which is the correct
    // production behaviour and exactly why the emulator is the only seam
    // available to a jsdom test. What is under test is the SERIALIZER and the
    // stream handoff, both of which are the same either way.
    localStorage.setItem("photrez.facade", "0");
    localStorage.setItem("photrez.facadeAuthority", "wasm");
    setEmuDocumentDims(128, 128);
  });
  afterEach(() => {
    localStorage.clear();
    __resetEmulatedForTests();
    __resetFacadeRegistryForTests();
  });

  it("a crop's undo emits the restored document size (the pair is not dropped)", async () => {
    const { recordExternalTransitionFor } = await import("@/lib/protocol/facadeRegistry");

    // The crop: 128x128 -> 13x13, declared by the host at commit time exactly as
    // cropToolActions does it.
    await recordExternalTransitionFor("docE2E", {
      label: "Crop Canvas",
      affectedLayerIds: [],
      snapshot: { id: "docE2E", width: 128, height: 128, layers: [] },
      docSizeChange: {
        before: { width: 128, height: 128 },
        after: { width: 13, height: 13 },
      },
    });

    // The host applied the crop out-of-band; the engine's doc dims now read 13x13.
    setEmuDocumentDims(13, 13);

    // ONE undo, observed at both seams: the raw delta the engine produced, and the
    // engine state it left behind. Both come from the real serializer.
    const raw = await applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: undefined,
      docId: "docE2E",
      command: { type: "undo" },
    } as never);

    // The delta carried the pre-crop size on the wire. Before the undo the engine
    // read 13x13 (the host applied the crop out-of-band); nothing but the recorded
    // before-half could produce 128x128 here.
    expect(raw.delta.width).toBe(128);
    expect(raw.delta.height).toBe(128);
    expect(raw.status).toBe("external");
    expect(getEmuDocumentDims()).toEqual({ width: 128, height: 128 });
  });

  it("a size-neutral transition emits NO size and stays an empty step", async () => {
    const { recordExternalTransitionFor } = await import("@/lib/protocol/facadeRegistry");
    setEmuDocumentDims(200, 200);

    await recordExternalTransitionFor("docMetaE2E", {
      label: "Delete Layer",
      affectedLayerIds: [],
      snapshot: { id: "docMetaE2E", width: 200, height: 200, layers: [] },
      // no docSizeChange
    });

    const facade = getFacade("docMetaE2E");
    await facade.undo();

    // The document size must be untouched, and the step must read as empty so the
    // host still falls through to its own TS history.
    expect(facade.lastHistoryDeltaWasEmpty).toBe(true);
    expect(facade.lastProjectionDimsAuthoritative).toBe(false);
  });
});

describe("a crop the engine would REJECT records no size at all", () => {
  // The crop commits its history entry BEFORE calling applyCrop, and applyCrop
  // silently no-ops on a non-positive size or one past the device ceiling. If the
  // commit recorded a size anyway, a later redo would write a size the document
  // never took straight into the live model.
  //
  // `resolveCropDocumentSize` is the SAME predicate applyCrop uses, so these
  // expectations describe the engine's real acceptance rule rather than a copy.

  it("rejects a zero-size rect (a sub-pixel crop rounding to 0)", () => {
    expect(resolveCropDocumentSize(0, 61, { targetSize: null })).toBeNull();
    expect(resolveCropDocumentSize(100, 0, { targetSize: null })).toBeNull();
  });

  it("rejects a target size past the app ceiling, so 20000 cannot be recorded", () => {
    expect(resolveCropDocumentSize(100, 100, { targetSize: { w: 20000, h: 20000 } })).toBeNull();
    // Sanity: the ceiling really is below 20000, so the above is a rejection and
    // not an accident of an unset device limit.
    expect(20000).toBeGreaterThan(MAX_CANVAS_DIM);
  });

  it("accepts an ordinary crop and reports exactly the size applyCrop will use", () => {
    expect(resolveCropDocumentSize(100, 61, { targetSize: null })).toEqual({
      width: 100,
      height: 61,
    });
    expect(resolveCropDocumentSize(100, 61, { targetSize: { w: 300, h: 400 } })).toEqual({
      width: 300,
      height: 400,
    });
  });
});
