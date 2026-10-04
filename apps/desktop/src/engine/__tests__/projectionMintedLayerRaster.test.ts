// SPDX-License-Identifier: AGPL-3.0-or-later

// THE PROJECTION OWNS THE RASTER FOR A GRAPH-MINTED LAYER - and case (a) is untouched.
//
// THE BRANCH. `applyFacadeSnapshot`'s new-layer `else` (document.ts, the branch that
// `b3649bbb` patched downstream at one call site) serves TWO situations that must behave
// differently, and until now treated them identically:
//
//   (a) a TRUE RE-APPEARANCE - the id was dropped by a delete and its retained node was
//       evicted or cleared. Its pixels genuinely cannot be reconstructed. `imageBitmap:
//       null` plus the loud warn is CORRECT and is what this file pins unchanged.
//   (b) a GENUINELY NEW id - the graph minted it and the model has never seen it.
//       `everDroppedIds.has(rl.id)` is FALSE, so there was no warning at all: a silent,
//       metadata-only layer with no raster. `getPaintSurface` then returns null
//       (document.ts:1447), the brush's tile path is skipped (useBrushOverlay.ts:1205-1221)
//       and the stroke commits through the LEGACY TYPESCRIPT path with a `ts:` payloadRef
//       and no row in the Rust pixel store at all.
//
// The split is on the fact the branch already computed. `isTrueReappearance` decides.
//
// WHY THE PROJECTION IS UNDER TEST HERE. `addLayerPaintableStore.wiring.test.tsx` drives
// `handleAddLayer`, so it can only observe the end state. This file calls
// `applyFacadeSnapshot` DIRECTLY with a hand-built descriptor, so the raster it asserts is
// provably the projection's and not a call-site patch. The Rust store is deliberately not
// seeded: `c4CoreCommit` already seeds it from the surface on the first paint
// (`rust_pixels_init`, useBrushOverlay.ts:217-220), and two owners of one step is the defect
// this effort keeps finding.
//
// NO HARNESS FACTS NEEDED HERE, DELIBERATELY: no facade registry, no wasm, no commit shim.
// The projection is synchronous and total, so a hand-built descriptor is a stronger test
// than a routed one - there is no arm that could be silently taken instead.

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { DocumentEngine } from "@/engine/document";
import { installFaithfulCanvas } from "@/__tests__/faithfulOffscreenCanvas";

const W = 40;
const H = 24;

type Engine = DocumentEngine;

let restoreCanvas: (() => void) | undefined;
let engine: Engine;
let warnSpy: ReturnType<typeof vi.spyOn>;

/** A facade layer descriptor, built by hand - this is what the projection consumes. */
function descriptor(id: string, over: Record<string, unknown> = {}) {
  return {
    id,
    name: id,
    visible: true,
    locked: false,
    opacity: 1,
    x: 0,
    y: 0,
    scaleX: 1,
    scaleY: 1,
    rotation: 0,
    width: W,
    height: H,
    ...over,
  };
}

/**
 * REAL pixel count, never a hash over a possibly-undefined field. `-1` is the sentinel for
 * "this layer carries no raster at all", which is a DIFFERENT fact from "a raster whose
 * pixels are all zero" - a transparent new layer is the latter, and the whole fix is the
 * difference between them.
 */
function paintedPixelCount(e: Engine, layerId: string): number {
  const bitmap = e.getLayer(layerId)?.imageBitmap as unknown as
    | { getImageData?: () => { data: Uint8ClampedArray } }
    | null
    | undefined;
  if (!bitmap) return -1;
  if (typeof bitmap.getImageData !== "function") return -1;
  const data = bitmap.getImageData().data;
  let painted = 0;
  for (let i = 0; i < data.length; i += 4) {
    if (data[i] !== 0 || data[i + 1] !== 0 || data[i + 2] !== 0 || data[i + 3] !== 0) painted += 1;
  }
  return painted;
}

beforeEach(() => {
  restoreCanvas = installFaithfulCanvas();
  engine = new DocumentEngine(`projectionRaster-${Math.random().toString(36).slice(2, 10)}`, "Proj", W, H);
  warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
});

afterEach(() => {
  warnSpy.mockRestore();
  restoreCanvas?.();
  restoreCanvas = undefined;
});

describe("a graph-minted layer arrives from the projection with a transparent raster", () => {
  it("case (b): the projection builds the raster, and it is transparent", () => {
    // An id the model has NEVER seen. This is the add-layer shape.
    engine.applyFacadeSnapshot({ layers: [descriptor("layer-newb")] } as never);

    const layer = engine.getLayer("layer-newb");
    expect(
      {
        present: Boolean(layer),
        painted: paintedPixelCount(engine, "layer-newb"),
        width: layer?.width,
        height: layer?.height,
        surfacePresent: (engine.getPaintSurface("layer-newb") ?? null) !== null,
      },
      "a graph-minted id must arrive with a transparent raster from the projection itself; painted 0 (transparent) and a non-null getPaintSurface, or the brush's tile path is skipped",
    ).toEqual({ present: true, painted: 0, width: W, height: H, surfacePresent: true });

    // Not a re-appearance, so nothing may warn about lost pixels.
    expect(
      warnSpy.mock.calls.map((c: unknown[]) => String(c[0])),
      "case (b) is not a lost layer, so the retention warn must not fire for it",
    ).toEqual([]);

    // And the raster must be the DESCRIPTOR's size, not the document's: the projection
    // already prefers `rl.width` (document.ts, the width/height lines of this branch), and
    // a mismatch here would silently resize every added layer.
    expect(
      { width: layer?.width, height: layer?.height, descriptorWidth: W, descriptorHeight: H },
      "the raster must be the size the descriptor carries",
    ).toMatchObject({ width: W, height: H });
  });

  it("case (b): the raster is transparent, not painted - the fix cannot fabricate content", () => {
    engine.applyFacadeSnapshot({ layers: [descriptor("layer-newb2")] } as never);
    // A transparent raster reads as zero painted pixels AND full-size data. A blanked or
    // zero-WIDTH canvas would read 0 painted too, so the size is asserted as well.
    expect(
      { painted: paintedPixelCount(engine, "layer-newb2"), bytes: (engine.getLayer("layer-newb2")?.imageBitmap as unknown as { getImageData?: () => { data: Uint8ClampedArray } })?.getImageData?.().data.length ?? -1 },
      "the new layer's raster must be transparent AND full size; 0 painted with a short buffer would be a blanked canvas rather than a transparent one",
    ).toEqual({ painted: 0, bytes: W * H * 4 });
  });
});

describe("case (a) is unchanged: a lost layer still says so loudly", () => {
  it("a dropped id whose retained node was evicted rebuilds with a null raster and warns", () => {
    // Register a layer, then drop it the way a delete does, so the id is recorded as ever
    // dropped and a retained node exists.
    engine.applyFacadeSnapshot({ layers: [descriptor("layer-lost")] } as never);
    // The 2d context must be obtained BEFORE the transfer, exactly as production does it:
    // a real OffscreenCanvas throws InvalidStateError without one, so a setup that
    // transfers first is asserting on a state production could never reach.
    const lostCanvas = new OffscreenCanvas(W, H);
    lostCanvas.getContext("2d");
    engine.setLayerImageBitmap("layer-lost", lostCanvas.transferToImageBitmap());
    // `applyFacadeSnapshot` marks every projected id facade-owned (document.ts:2314), and
    // `deleteLayer` refuses a facade-owned id (document.ts:754). Ownership is released when
    // an id LEAVES the projection (document.ts:2310-2312), so projecting the layer away
    // first is what makes a REAL delete reachable here - no private seeding, and the drop
    // is recorded by the real path (document.ts:759 / :771).
    engine.applyFacadeSnapshot({ layers: [] } as never);
    engine.deleteLayer("layer-lost");

    // Simulate the retention entry being evicted or cleared - the situation the warn exists
    // for. The DROP above is real (a real delete recorded it); this only simulates the
    // retention cache losing the node afterwards, which has no public API because it is a
    // cache-capacity event, not an operation. Overflowing the real cap instead would test
    // the eviction policy rather than the branch split.
    const dropped = (engine as unknown as { droppedNodes: Map<string, unknown> }).droppedNodes;
    const hadRetained = dropped.delete("layer-lost");
    expect(hadRetained, "premise: the real delete recorded a retained node for the dropped id").toBe(true);

    warnSpy.mockClear();
    engine.applyFacadeSnapshot({ layers: [descriptor("layer-lost")] } as never);

    // THE CONTRACT THAT MUST NOT REGRESS. Seeding case (b) must not swallow case (a): a
    // layer whose pixels are genuinely gone must still be null AND still say so, or a loud
    // failure becomes silent data loss.
    expect(
      {
        painted: paintedPixelCount(engine, "layer-lost"),
        warns: warnSpy.mock.calls.map((c: unknown[]) => String(c[0])),
      },
      "case (a) must be untouched: a re-appearance with no retained node keeps a NULL raster and the loud warn. A transparent raster here would be silent data loss",
    ).toEqual({
      painted: -1,
      warns: [
        `[facade-projection] layer layer-lost re-appeared with no retained node - pixels cannot be restored for it`,
      ],
    });
  });

  it("case (a): a re-appearance WITH a retained node still restores its pixels", () => {
    // The neighbouring branch, pinned so the split cannot have moved the boundary: a
    // retained node keeps its raster and must NOT warn.
    engine.applyFacadeSnapshot({ layers: [descriptor("layer-kept")] } as never);
    const canvas = new OffscreenCanvas(W, H);
    const ctx = canvas.getContext("2d")!;
    ctx.fillStyle = "#cc3366";
    ctx.fillRect(0, 0, W, H);
    engine.setLayerImageBitmap("layer-kept", canvas.transferToImageBitmap());
    expect(paintedPixelCount(engine, "layer-kept"), "premise: the layer really holds paint").toBe(W * H);

    // Release facade ownership by projecting the layer away, so the REAL delete runs and
    // records the drop. See the case-(a) sibling for why this is needed.
    engine.applyFacadeSnapshot({ layers: [] } as never);
    engine.deleteLayer("layer-kept");
    warnSpy.mockClear();
    engine.applyFacadeSnapshot({ layers: [descriptor("layer-kept")] } as never);

    expect(
      {
        painted: paintedPixelCount(engine, "layer-kept"),
        warns: warnSpy.mock.calls.map((c: unknown[]) => String(c[0])),
      },
      "a re-appearance WITH a retained node must come back with its own pixels and no warn",
    ).toEqual({ painted: W * H, warns: [] });
  });
});
