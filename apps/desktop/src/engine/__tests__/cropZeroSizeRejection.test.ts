// A crop with an explicit size of 0 must be REJECTED, not accepted.
//
// MEASURED IN THE REAL APP (artifact sha256
// cc39a2700911063dc7e0061fde3e57776944014c8409ece3e2f7d86fcd143b90, HEAD 4aa73c2):
// driving the real crop tool - Rasio Aspek: Free, Size, width field, Terapkan, with
// `0` - left the document at 0x600, i.e. model.width = 0. Redo returned 0x600. Undo
// DID restore 128x128, so the doc-size pair machinery is self-consistent; the crop
// itself was wrongly accepted. The same drive with 20000 is correctly rejected.
//
// ROOT CAUSE: `resolveCropDocumentSize` guards `width <= 0` on the INPUT rect and
// then ceiling-checks only the OUTPUT. An explicit target size bypasses the input
// guard entirely, and `0 > 16384` is false, so `{0, 600}` passed. This is
// PRE-EXISTING: the verifier read 2874f88^ and confirmed the pre-extraction guards
// in applyCrop were the identical logic, so extracting the predicate preserved the
// hole rather than creating it.
//
// WHY THE INPUT GUARD IS NOT ENOUGH: `CropSizeInputs`
// (CropOptionBarSections.tsx:405-430) renders both fields through
// `EditableNumField` with NO min and NO max, so a typed `0` or a negative value
// reaches the crop as a real target size. The input rect is derived from a
// selection drag and cannot be negative, but the size fields can be.
//
// This file pins the boundary in BOTH directions - 0 rejected, and the smallest
// still-legal size accepted - so a future guard cannot silently shrink the legal
// range, and it pins the store-currency consequence: a rejected crop must leave
// the Rust pixel store exactly as it found it.

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { resolveCropDocumentSize, DocumentEngine } from "@/engine/document";
import { MAX_CANVAS_DIM } from "@/engine/types";
import { __resetEmulatedForTests, setEmuDocumentDims, getEmuDocumentDims } from "@/lib/protocol/bridge_emu";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

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

describe("crop size acceptance: a zero or negative OUTPUT is rejected", () => {
  beforeEach(() => {
    localStorage.clear();
    __resetEmulatedForTests();
    setEmuDocumentDims(128, 128);
  });
  afterEach(() => {
    localStorage.clear();
    __resetEmulatedForTests();
  });

  // The measured defect.
  it("rejects an explicit target size of 0", () => {
    expect(resolveCropDocumentSize(100, 100, { targetSize: { w: 0, h: 600 } })).toBeNull();
  });

  it("rejects a target size of 0 in either dimension", () => {
    expect(resolveCropDocumentSize(100, 100, { targetSize: { w: 600, h: 0 } })).toBeNull();
  });

  it("rejects a NEGATIVE explicit target size", () => {
    // Reachable: EditableNumField is rendered with no min, so a typed minus sign
    // reaches the crop as a real target size.
    expect(resolveCropDocumentSize(100, 100, { targetSize: { w: -5, h: 600 } })).toBeNull();
    expect(resolveCropDocumentSize(100, 100, { targetSize: { w: 600, h: -5 } })).toBeNull();
  });

  it("keeps rejecting a zero or negative INPUT rect (the pre-existing guard)", () => {
    expect(resolveCropDocumentSize(0, 100, { targetSize: null })).toBeNull();
    expect(resolveCropDocumentSize(100, 0, { targetSize: null })).toBeNull();
    expect(resolveCropDocumentSize(-10, 100, { targetSize: null })).toBeNull();
  });

  // ── Boundary, the other direction ─────────────────────────────────────────
  // A guard that rejects `<= 0` must NOT also reject 1. If a future change clamps
  // or adds a lower bound above 1, these pin that the legal range did not shrink.
  it("still accepts the smallest legal size, 1x1", () => {
    expect(resolveCropDocumentSize(100, 100, { targetSize: { w: 1, h: 1 } })).toEqual({
      width: 1,
      height: 1,
    });
    // And from a 1x1 crop rect with no target size.
    expect(resolveCropDocumentSize(1, 1, { targetSize: null })).toEqual({ width: 1, height: 1 });
  });

  it("still accepts a 1-wide crop against a normal height, and vice versa", () => {
    expect(resolveCropDocumentSize(1, 600, { targetSize: null })).toEqual({
      width: 1,
      height: 600,
    });
    expect(resolveCropDocumentSize(600, 1, { targetSize: null })).toEqual({
      width: 600,
      height: 1,
    });
  });

  it("still accepts an ordinary size", () => {
    expect(resolveCropDocumentSize(100, 61, { targetSize: null })).toEqual({
      width: 100,
      height: 61,
    });
  });

  // ── The ceiling boundary, unchanged ───────────────────────────────────────
  it("still rejects above the canvas ceiling and accepts exactly at it", () => {
    expect(resolveCropDocumentSize(100, 100, { targetSize: { w: 20000, h: 20000 } })).toBeNull();
    expect(20000).toBeGreaterThan(MAX_CANVAS_DIM);
    expect(resolveCropDocumentSize(100, 100, { targetSize: { w: 16384, h: 16384 } })).toEqual({
      width: 16384,
      height: 16384,
    });
    expect(resolveCropDocumentSize(100, 100, { targetSize: { w: 16385, h: 100 } })).toBeNull();
  });
});

describe("a rejected zero crop leaves the document completely untouched", () => {
  beforeEach(() => {
    localStorage.clear();
    __resetEmulatedForTests();
    setEmuDocumentDims(128, 128);
  });
  afterEach(() => {
    localStorage.clear();
    __resetEmulatedForTests();
  });

  it("applyCrop with an explicit 0 size changes neither dimension nor the store", () => {
    const engine = new DocumentEngine("docZero", "doc", 128, 128);
    // Store dims start equal to the model - the currency invariant.
    expect(getEmuDocumentDims()).toEqual({ width: 128, height: 128 });

    engine.applyCrop(10, 10, 100, 100, { deleteCroppedPixels: true, targetSize: { w: 0, h: 600 } });

    // The document is untouched: this is the measured defect - it used to become
    // 0x600.
    expect(engine.getWidth()).toBe(128);
    expect(engine.getHeight()).toBe(128);
    // And the Rust pixel store still matches the model. A rejected crop must not
    // leave the store at a different geometry than the model, which is what the
    // store-currency invariant forbids.
    expect(getEmuDocumentDims()).toEqual({ width: 128, height: 128 });
  });

  it("a negative explicit size is rejected the same way", () => {
    const engine = new DocumentEngine("docNeg", "doc", 128, 128);
    engine.applyCrop(10, 10, 100, 100, { deleteCroppedPixels: true, targetSize: { w: -5, h: 600 } });
    expect(engine.getWidth()).toBe(128);
    expect(engine.getHeight()).toBe(128);
    expect(getEmuDocumentDims()).toEqual({ width: 128, height: 128 });
  });

  // Store currency forward must still work for a VALID crop - the closed path.
  it("a VALID crop still resizes the document (the 1x1 boundary end to end)", () => {
    const engine = new DocumentEngine("docTiny", "doc", 128, 128);
    engine.applyCrop(0, 0, 100, 100, { deleteCroppedPixels: true, targetSize: { w: 1, h: 1 } });
    expect(engine.getWidth()).toBe(1);
    expect(engine.getHeight()).toBe(1);
  });
});