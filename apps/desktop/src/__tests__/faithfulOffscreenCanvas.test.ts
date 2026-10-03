// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * The shim's own rules, tested against the shim alone.
 *
 * WHY THIS FILE IS SEPARATE AND WHY IT IS NOT OPTIONAL. The shim is the
 * falsifiability foundation for every pixel claim in the editor test suite: when
 * its `drawImage` drew nothing, `destination-out` became mathematically inert, four
 * rounds of investigation concluded "the eraser does not change the raster", and a
 * production fix was nearly written on that conclusion. A real-app run then found
 * brush and eraser sound. Every suite was green throughout, because the thing that
 * was broken was the instrument.
 *
 * So a defect in the shim does not fail a test - it DELETES the meaning of every
 * test above it. Each case below states a platform rule, drives the shim directly,
 * and counts real pixels: a passing assertion here is what makes a pixel assertion
 * anywhere else mean something.
 *
 * Every count is a real pixel count, never a hash or a digest over a field that may
 * be absent: a defect that hands back `undefined` pixels must surface as a count of
 * zero inside the assertion message, not as a TypeError that proves nothing.
 *
 * The rules and the ones deliberately NOT modelled are listed in the shim's header.
 */
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { installFaithfulCanvas, snapshotBitmap } from "./faithfulOffscreenCanvas";
import { PaintTileSurface } from "@/lib/paint/paintTileSurface";
import type { TileRect } from "@/lib/paint/paintTileSurface";

type Ctx2D = {
  canvas?: unknown;
  fillStyle: string;
  globalAlpha: number;
  globalCompositeOperation: string;
  clearRect(x?: number, y?: number, w?: number, h?: number): void;
  fillRect(x: number, y: number, w: number, h: number): void;
  drawImage(src: unknown, ...rest: number[]): void;
  getImageData(x?: number, y?: number, w?: number, h?: number): { data: Uint8ClampedArray };
  save(): void;
  restore(): void;
  translate(x?: number, y?: number): void;
  scale(x?: number, y?: number): void;
  rotate(radians?: number): void;
};

type BitmapLike = {
  width: number;
  height: number;
  // `data` is always present: every bitmap this shim mints returns it, so a case that
  // asserts on its length is asserting a real buffer rather than an optional field.
  getImageData(x?: number, y?: number, w?: number, h?: number): { data: Uint8ClampedArray };
};

const ctx2d = (canvas: unknown): Ctx2D =>
  (canvas as { getContext(type: string): Ctx2D }).getContext("2d");

/** A solid canvas of one colour, through the shim's own fillRect. */
function solid(width: number, height: number, hex: string) {
  const canvas = new OffscreenCanvas(width, height);
  const ctx = ctx2d(canvas);
  ctx.fillStyle = hex;
  ctx.fillRect(0, 0, width, height);
  return canvas;
}

/**
 * Count pixels matching a predicate. An absent pixel array counts as 0 matches, so
 * the assertion message carries the pixel evidence instead of a thrown TypeError.
 */
function countMatching(
  data: ArrayLike<number> | undefined,
  match: (r: number, g: number, b: number, a: number) => boolean,
): number {
  const d = data;
  if (!d) return 0;
  let n = 0;
  for (let i = 0; i + 3 < d.length; i += 4) {
    if (match(d[i], d[i + 1], d[i + 2], d[i + 3])) n += 1;
  }
  return n;
}

/** Any channel non-zero: "this pixel is not transparent black". */
const isNonZero = (r: number, g: number, b: number, a: number): boolean =>
  r !== 0 || g !== 0 || b !== 0 || a !== 0;

const isOpaqueWhite = (r: number, g: number, b: number, a: number): boolean =>
  r === 255 && g === 255 && b === 255 && a === 255;

const isOpaqueBlack = (r: number, g: number, b: number, a: number): boolean =>
  r === 0 && g === 0 && b === 0 && a === 255;

const isPaintRed = (r: number, g: number, b: number, a: number): boolean =>
  r === 204 && g === 51 && b === 0 && a === 255;

const isLayerGreen = (r: number, g: number, b: number, a: number): boolean =>
  r === 51 && g === 204 && b === 102 && a === 255;

const tile = (x: number, y: number, w: number, h: number): TileRect => ({
  tx: 0,
  ty: 0,
  key: "0,0",
  x,
  y,
  w,
  h,
});

let restoreCanvas: (() => void) | undefined;

beforeEach(() => {
  restoreCanvas = installFaithfulCanvas();
});

afterEach(() => {
  restoreCanvas?.();
  restoreCanvas = undefined;
});

describe("faithful canvas shim: createImageBitmap carries real pixels", () => {
  it("snapshots the source's real pixels", async () => {
    const source = solid(4, 4, "#ff0000");
    const bitmap = (await createImageBitmap(source as unknown as ImageBitmapSource)) as unknown as BitmapLike;

    const red = countMatching(
      bitmap.getImageData().data,
      (r, g, b, a) => r === 255 && g === 0 && b === 0 && a === 255,
    );

    expect(
      red,
      "createImageBitmap must snapshot real pixels. A canvas has no `.data`, so a bitmap " +
        "built from `src.data` carries undefined pixels and every re-seed from it paints nothing.",
    ).toBe(16);
  });

  it("re-seeds a destination canvas through drawImage (the model-bitmap re-seed path)", async () => {
    // The live chain: paintTileSurface.toImageBitmap -> engine.setLayerImageBitmap ->
    // overlayCtx.drawImage(layer.imageBitmap, 0, 0) (useBrushOverlay.ts:865, :1034, :1716).
    const source = solid(4, 4, "#ff0000");
    const bitmap = await createImageBitmap(source as unknown as ImageBitmapSource);

    const overlay = solid(4, 4, "#0000ff");
    const octx = ctx2d(overlay);
    octx.globalCompositeOperation = "source-over";
    octx.drawImage(bitmap, 0, 0);

    const data = octx.getImageData().data;
    expect(
      countMatching(data, (r, g, b, a) => r === 255 && g === 0 && b === 0 && a === 255),
      "drawing a bitmap must paint the bitmap's pixels over the destination, not leave the " +
        "destination untouched",
    ).toBe(16);
    expect(
      countMatching(data, (r, g, b) => r === 0 && g === 0 && b === 255),
      "and the destination's own pixels must be fully covered",
    ).toBe(0);
  });

  it("carries paint pixels through the real surface re-seed (PaintTileSurface.toImageBitmap)", async () => {
    const surface = new PaintTileSurface(8, 8, null as never);
    const source = solid(8, 8, "#cc3300");
    const sourceCtx = ctx2d(source);
    // `applyOverlayTiles` reads the overlay through its `.canvas`, exactly as
    // useBrushOverlay.ts:1474 passes the overlay canvas.
    (sourceCtx as { canvas?: unknown }).canvas = source;
    surface.applyOverlayTiles(sourceCtx as never, [tile(0, 0, 8, 8)], "source-over");

    const bitmap = (await surface.toImageBitmap()) as unknown as BitmapLike;
    const overlay = solid(8, 8, "#000000");
    const octx = ctx2d(overlay);
    octx.drawImage(bitmap, 0, 0);

    expect(
      countMatching(octx.getImageData().data, isPaintRed),
      "a committed surface must re-seed the next stroke with its own painted pixels",
    ).toBe(64);
  });
});

describe("faithful canvas shim: drawImage argument forms", () => {
  it("scales and offsets in the 5-argument form", () => {
    const source = solid(4, 4, "#ffffff");
    const dest = solid(12, 12, "#000000");
    const dctx = ctx2d(dest);
    // (image, dx, dy, dw, dh): the whole source, scaled into that destination rect.
    dctx.drawImage(source, 2, 2, 8, 8);

    const data = dctx.getImageData().data;
    expect(
      countMatching(data, isOpaqueWhite),
      "a 4x4 source drawn into an 8x8 destination rect must cover 64 pixels, not 16",
    ).toBe(64);
    expect(
      countMatching(data, isOpaqueBlack),
      "and every pixel outside that rect must keep the destination's own colour: " +
        "12*12 - 8*8 = 80",
    ).toBe(80);
  });

  it("keeps the 2-number form at 1:1 scale", () => {
    // Scale only. The 2-number form's dx/dy are NOT honoured by the shim (see the
    // shim header's not-modelled list), and a pair of counts cannot see a position,
    // so this case pins the scale and nothing else. Every live 2-number caller
    // passes (0, 0), so no paint path depends on the offset.
    const source = solid(4, 4, "#ffffff");
    const dest = solid(12, 12, "#000000");
    const dctx = ctx2d(dest);
    dctx.drawImage(source, 2, 2);

    const data = dctx.getImageData().data;
    expect(countMatching(data, isOpaqueWhite), "2-number draws the source unscaled").toBe(16);
    expect(countMatching(data, isOpaqueBlack), "and covers no other pixel").toBe(128);
  });

  it("still honours the 9-argument source sub-rect form", () => {
    const source = solid(8, 8, "#ffffff");
    const dest = solid(8, 8, "#000000");
    const dctx = ctx2d(dest);
    dctx.drawImage(source, 0, 0, 4, 4, 0, 0, 4, 4);

    expect(
      countMatching(dctx.getImageData().data, isOpaqueWhite),
      "9-arg draws only the source sub-rect",
    ).toBe(16);
  });
});

describe("faithful canvas shim: clearRect respects its rect", () => {
  it("clears its rect and leaves every other pixel", () => {
    const canvas = solid(8, 8, "#ffffff");
    const ctx = ctx2d(canvas);
    ctx.clearRect(0, 0, 2, 2);

    const data = ctx.getImageData().data;
    expect(
      countMatching(data, isNonZero),
      "clearing a 2x2 rect of an 8x8 white canvas must leave 60 non-zero pixels; over-" +
        "clearing the whole buffer hides a stale-pixel regression and invents a false " +
        "'pixels were destroyed' reading",
    ).toBe(60);
    expect(
      countMatching(data, (r, g, b, a) => r === 0 && g === 0 && b === 0 && a === 0),
      "the cleared pixels must be transparent black, as on a real canvas",
    ).toBe(4);
  });

  it("clears an interior rect without touching either neighbour", () => {
    const canvas = solid(8, 8, "#ffffff");
    const ctx = ctx2d(canvas);
    ctx.clearRect(3, 3, 2, 2);

    const data = ctx.getImageData().data;
    expect(countMatching(data, isNonZero), "only the interior rect may go").toBe(60);
  });

  it("applyOverlayTiles clears only the tiles it replaces (production clearRect path)", () => {
    const surface = new PaintTileSurface(16, 16, null as never);

    const whiteOverlay = solid(16, 16, "#ffffff");
    const whiteCtx = ctx2d(whiteOverlay);
    (whiteCtx as { canvas?: unknown }).canvas = whiteOverlay;
    surface.applyOverlayTiles(whiteCtx as never, [tile(0, 0, 16, 16)], "source-over");
    expect(
      countMatching(surface.readRect(0, 0, 16, 16).data, isOpaqueWhite),
      "premise: the surface starts fully white",
    ).toBe(256);

    const redOverlay = solid(8, 16, "#cc3300");
    const redCtx = ctx2d(redOverlay);
    (redCtx as { canvas?: unknown }).canvas = redOverlay;
    surface.applyOverlayTiles(redCtx as never, [tile(0, 0, 8, 16)], "source-over");

    const data = surface.readRect(0, 0, 16, 16).data;
    expect(
      countMatching(data, isPaintRed),
      "the replaced half must hold the overlay's pixels",
    ).toBe(128);
    expect(
      countMatching(data, isOpaqueWhite),
      "and the half no tile covered must SURVIVE: a clearRect that wipes the whole buffer " +
        "destroys untouched tiles and reports them as destroyed pixels",
    ).toBe(128);
  });
});

describe("faithful canvas shim: save/restore scopes the blend state", () => {
  it("restores what save captured", () => {
    const canvas = solid(8, 8, "#ffffff");
    const ctx = ctx2d(canvas);
    ctx.globalCompositeOperation = "destination-out";
    ctx.globalAlpha = 0.5;
    ctx.fillStyle = "#00ff00";

    ctx.save();
    ctx.globalCompositeOperation = "source-over";
    ctx.globalAlpha = 1;
    ctx.fillStyle = "#0000ff";
    ctx.restore();

    expect(
      ctx.globalCompositeOperation,
      "a leaked blend mode makes a later destination-out or destination-in assertion " +
        "measure the leak instead of the intent",
    ).toBe("destination-out");
    expect(ctx.globalAlpha, "and a leaked globalAlpha silently rescales every later dab").toBe(0.5);
    expect(ctx.fillStyle).toBe("#00ff00");
  });

  it("restores nested levels and ignores restore on an empty stack", () => {
    const ctx = ctx2d(solid(4, 4, "#ffffff"));
    ctx.globalAlpha = 0.25;
    ctx.save();
    ctx.globalAlpha = 0.5;
    ctx.save();
    ctx.globalAlpha = 0.75;

    ctx.restore();
    expect(ctx.globalAlpha, "the inner restore returns to the inner save point").toBe(0.5);
    ctx.restore();
    expect(ctx.globalAlpha, "the outer restore returns to the outer save point").toBe(0.25);
    ctx.restore();
    expect(ctx.globalAlpha, "restore with an empty stack is ignored, not a reset").toBe(0.25);
  });

  it("leaves the surface blend state as it found it (applyOverlayTiles save/restore pair)", () => {
    const surface = new PaintTileSurface(8, 8, null as never);
    const surfaceCtx = (surface as unknown as { ctx: Ctx2D }).ctx;
    surfaceCtx.globalCompositeOperation = "destination-out";
    surfaceCtx.globalAlpha = 0.5;

    const overlay = solid(8, 8, "#ff0000");
    const overlayCtx = ctx2d(overlay);
    (overlayCtx as { canvas?: unknown }).canvas = overlay;
    surface.applyOverlayTiles(overlayCtx as never, [tile(0, 0, 8, 8)], "source-over");

    expect(
      surfaceCtx.globalCompositeOperation,
      "applyOverlayTiles saves the blend mode, sets its own, and restores: the next rect " +
        "must start from the mode the surface was in",
    ).toBe("destination-out");
    expect(surfaceCtx.globalAlpha).toBe(0.5);
  });
});

describe("faithful canvas shim: reading a canvas source with a strict reader", () => {
  it("passes the rect explicitly, so a getImageData with no defaults still reads pixels", () => {
    // The exact shape of the per-suite canvas doubles in this repo: `getImageData`
    // takes (x, y, w, h) and has NO defaults, so a bare call computes NaN * NaN * 4
    // and hands back a ZERO-LENGTH array. Treating that as a valid read produces a
    // bitmap with real dimensions and no bytes, and every draw of it then spreads
    // zeros - which is how a canvas that was filled a moment earlier came back
    // reading 0 where 255 was expected.
    const canvas = new OffscreenCanvas(4, 4);
    const ctx = ctx2d(canvas);
    ctx.fillStyle = "#ff0000";
    ctx.fillRect(0, 0, 4, 4);
    const real = ctx.getImageData();

    const strictDouble = {
      width: 4,
      height: 4,
      getContext: () => ({
        getImageData: (x: number, y: number, w: number, h: number) =>
          w * h * 4 === real.data.length
            ? { data: real.data, width: w, height: h }
            : { data: new Uint8ClampedArray(NaN), width: w, height: h },
      }),
    };

    const bitmap = snapshotBitmap(strictDouble) as unknown as BitmapLike;
    expect(
      countMatching(
        bitmap.getImageData().data,
        (r, g, b, a) => r === 255 && g === 0 && b === 0 && a === 255,
      ),
      "a full-canvas rect must be passed explicitly, or a no-default reader returns nothing",
    ).toBe(16);
  });

  it("rejects a read whose byte count disagrees with its own dimensions", () => {
    // The other half of the guard: a reader that reports 2x2 over zero bytes is not a
    // 2x2 transparent raster, it is a broken read. It must degrade to a correctly
    // sized transparent bitmap rather than propagate the mismatch.
    const mismatched = {
      width: 2,
      height: 2,
      getContext: () => ({
        getImageData: () => ({ data: new Uint8ClampedArray(0), width: 2, height: 2 }),
      }),
    };

    const bitmap = snapshotBitmap(mismatched) as unknown as BitmapLike;
    expect(bitmap.width, "the bitmap keeps the source's real dimensions").toBe(2);
    expect(bitmap.getImageData().data.length, "and a full-size buffer").toBe(2 * 2 * 4);
    expect(
      countMatching(bitmap.getImageData().data, isNonZero),
      "with nothing painted, rather than a short buffer that would smear",
    ).toBe(0);
  });
});

describe("faithful canvas shim: the transform is real", () => {
  it("maps the destination rect through translate (the composite's centred draw)", () => {
    // The exact sequence `layerComposite.drawLayerToContext` composes: translate to
    // the layer centre, then draw the bitmap centred on that point. With the
    // translate ignored, the destination rect is drawn where it was written and a
    // 64x64 layer fills only the top-left QUADRANT of a 64x64 composite - which is
    // how a green structural-undo suite came to assert a raster the product does not
    // produce. The pixel count is the whole claim: 4096 or it is broken.
    const canvas = solid(64, 64, "#000000");
    const ctx = ctx2d(canvas);
    const layer = solid(64, 64, "#33cc66");

    ctx.save();
    ctx.translate(32, 32);
    ctx.drawImage(layer, -32, -32, 64, 64);
    ctx.restore();

    expect(
      countMatching(ctx.getImageData().data, isLayerGreen),
      "a 64x64 layer translated to the centre of a 64x64 composite must cover all 4096 " +
        "pixels, not the 1024 of an untransformed -32,-32 draw",
    ).toBe(64 * 64);
  });

  it("save/restore scopes the transform as well as the blend state", () => {
    const canvas = solid(16, 16, "#000000");
    const ctx = ctx2d(canvas);

    ctx.save();
    ctx.translate(8, 8);
    ctx.drawImage(solid(8, 8, "#ffffff"), -4, -4, 8, 8); // lands at 4,4 - 12,12
    ctx.restore();
    // Identity again: this one must land at the ORIGIN, not at 4,4.
    ctx.drawImage(solid(4, 4, "#ffffff"), 0, 0);

    expect(
      countMatching(ctx.getImageData().data, isOpaqueWhite),
      "64 pixels from the translated draw plus 16 from the restored identity draw",
    ).toBe(80);
  });

  it("scales the destination rect rather than drawing it at its authored size", () => {
    const canvas = solid(16, 16, "#000000");
    const ctx = ctx2d(canvas);
    ctx.scale(2, 2);
    // A 4x4 destination rect under scale(2,2) covers device pixels 0..7, i.e. 64
    // painted pixels. Without the transform the same call paints its own 4x4, 16.
    ctx.drawImage(solid(4, 4, "#ffffff"), 0, 0, 4, 4);

    const data = ctx.getImageData().data;
    expect(countMatching(data, isOpaqueWhite), "scale(2,2) doubles the drawn footprint").toBe(64);
    expect(countMatching(data, isOpaqueBlack), "and the rest of the canvas stays untouched").toBe(192);
  });
});

describe("faithful canvas shim: destination-out still cuts", () => {
  it("a solid tip punches alpha-zero holes in an opaque field", () => {
    const canvas = solid(16, 16, "#ffffff");
    const ctx = ctx2d(canvas);
    const tip = solid(8, 8, "#000000");

    ctx.globalCompositeOperation = "destination-out";
    ctx.globalAlpha = 1;
    ctx.drawImage(tip, 0, 0, 8, 8, 4, 4, 8, 8);

    const data = ctx.getImageData().data;
    expect(
      countMatching(data, (r, g, b, a) => a === 0),
      "destination-out must reduce the covered pixels' alpha to zero",
    ).toBe(64);
    expect(countMatching(data, isOpaqueWhite), "and leave the rest opaque").toBe(192);
  });
});