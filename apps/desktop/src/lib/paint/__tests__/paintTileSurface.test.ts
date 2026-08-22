import { describe, expect, it, vi, beforeEach } from "vitest";
import {
  PAINT_TILE_SIZE,
  PaintTileSurface,
  tileKey,
  tilesInRect,
  type TileKeyed,
  type TileRect,
} from "../paintTileSurface";

// ── Minimal functional 2D-context stub (unit-node has no OffscreenCanvas) ────
class FakeCtx {
  data: Uint8ClampedArray;
  globalCompositeOperation: GlobalCompositeOperation = "source-over";
  savedStates: Array<{ op: GlobalCompositeOperation }> = [];
  drawImageCalls: unknown[] = [];
  constructor(public w: number, public h: number) {
    this.data = new Uint8ClampedArray(w * h * 4);
  }
  private idx(x: number, y: number) {
    return (y * this.w + x) * 4;
  }
  fillPixel(x: number, y: number, r: number, g: number, b: number, a: number) {
    const i = this.idx(x, y);
    this.data[i] = r; this.data[i + 1] = g; this.data[i + 2] = b; this.data[i + 3] = a;
  }
  getImageData(sx: number, sy: number, sw: number, sh: number) {
    const out = new Uint8ClampedArray(sw * sh * 4);
    for (let y = 0; y < sh; y++)
      for (let x = 0; x < sw; x++) {
        const s = ((sy + y) * this.w + (sx + x)) * 4, d = (y * sw + x) * 4;
        out[d] = this.data[s]; out[d + 1] = this.data[s + 1];
        out[d + 2] = this.data[s + 2]; out[d + 3] = this.data[s + 3];
      }
    return { data: out, width: sw, height: sh, colorSpace: "srgb" };
  }
  putImageData(img: { data: Uint8ClampedArray; width: number; height: number }, dx: number, dy: number) {
    for (let y = 0; y < img.height; y++)
      for (let x = 0; x < img.width; x++) {
        const s = (y * img.width + x) * 4, d = ((dy + y) * this.w + (dx + x)) * 4;
        this.data[d] = img.data[s]; this.data[d + 1] = img.data[s + 1];
        this.data[d + 2] = img.data[s + 2]; this.data[d + 3] = img.data[s + 3];
      }
  }
  clearRect(x: number, y: number, w: number, h: number) {
    for (let yy = y; yy < y + h; yy++)
      for (let xx = x; xx < x + w; xx++) {
        const i = this.idx(xx, yy);
        this.data[i] = this.data[i + 1] = this.data[i + 2] = this.data[i + 3] = 0;
      }
  }
  drawImage(
    src: { __pixels?: Uint8ClampedArray; width?: number; height?: number },
    sx: number, sy: number, sw: number, sh: number,
    dx: number, dy: number, dw: number, dh: number,
  ) {
    this.drawImageCalls.push([sx, sy, sw, sh]);
    const px = (src as any).__pixels as Uint8ClampedArray | undefined;
    if (!px || !src.width) return; // non-pixel sources are recorded only
    for (let y = 0; y < sh && sy + y < src.height!; y++)
      for (let x = 0; x < sw && sx + x < src.width!; x++) {
        const s = ((sy + y) * src.width! + (sx + x)) * 4, d = ((dy + y) * this.w + (dx + x)) * 4;
        this.data[d] = px[s]; this.data[d + 1] = px[s + 1];
        this.data[d + 2] = px[s + 2]; this.data[d + 3] = px[s + 3];
      }
  }
  save() { this.savedStates.push({ op: this.globalCompositeOperation }); }
  restore() { const s = this.savedStates.pop(); if (s) this.globalCompositeOperation = s.op; }
}

function installFakeOffscreen() {
  const made: Array<{ w: number; h: number; ctx: FakeCtx }> = [];
  class FakeOffscreen {
    width: number; height: number; private _ctx: FakeCtx;
    constructor(w: number, h: number) {
      this.width = w; this.height = h;
      this._ctx = new FakeCtx(w, h);
      made.push({ w, h, ctx: this._ctx });
    }
    getContext(_type: "2d", _opts?: { willReadFrequently?: boolean }) { return this._ctx; }
  }
  vi.stubGlobal("OffscreenCanvas", FakeOffscreen);
  return made;
}

// PaintTileSurface stores its ctx privately; expose the fake through a probe.
function ctxOf(surface: PaintTileSurface, made: Array<{ ctx: FakeCtx }>): FakeCtx {
  void surface;
  return made[made.length - 1].ctx;
}

describe("tilesInRect", () => {
  it("enumerates clamped tiles including partial edge tiles", () => {
    // 700x700 surface, tile 256 -> cols 0..2, rows 0..2 (last col/row partial)
    const t = tilesInRect(0, 0, 700, 700, 700, 700);
    expect(t.length).toBe(9);
    const last = t[t.length - 1];
    expect(last.tx).toBe(2); expect(last.ty).toBe(2);
    expect(last.w).toBe(188); expect(last.h).toBe(188); // 700 - 2*256
  });

  it("clamps negative origins to first tile", () => {
    const t = tilesInRect(-50, -50, 10, 10, 1000, 1000);
    expect(t.length).toBe(1);
    expect(t[0].tx).toBe(0); expect(t[0].ty).toBe(0);
  });

  it("returns single exact-boundary tile correctly", () => {
    const T = PAINT_TILE_SIZE;
    const t = tilesInRect(T, T, T + 10, T + 10, 2000, 2000);
    expect(t.length).toBe(1);
    expect(t[0].key).toBe(tileKey(1, 1));
    expect(t[0].x).toBe(T); expect(t[0].y).toBe(T);
  });
});

describe("PaintTileSurface", () => {
  let made: Array<{ w: number; h: number; ctx: FakeCtx }>;
  beforeEach(() => {
    vi.unstubAllGlobals();
    made = installFakeOffscreen();
  });

  it("snapshot -> external mutation -> restoreTile roundtrips byte-exact", () => {
    const surf = new PaintTileSurface(300, 300);
    const ctx = ctxOf(surf, made);
    ctx.fillPixel(5, 5, 200, 80, 40, 255);
    ctx.fillPixel(260, 260, 1, 2, 3, 255);

    const r: TileRect = { tx: 0, ty: 0, key: tileKey(0, 0), x: 0, y: 0, w: PAINT_TILE_SIZE, h: PAINT_TILE_SIZE };
    const snap = surf.snapshotTile(r);

    // paint over both tiles
    ctx.fillPixel(5, 5, 0, 0, 0, 0);
    ctx.fillPixel(260, 260, 99, 99, 99, 255);

    surf.restoreTile({ key: r.key, tx: r.tx, ty: r.ty, value: snap.value });

    const back = surf.readTile(r); // tile-local stride = PAINT_TILE_SIZE
    expect(back.data[(5 * PAINT_TILE_SIZE + 5) * 4]).toBe(200);
    // pixel outside the restored tile must be untouched
    const t11 = { tx: 1, ty: 1, key: tileKey(1, 1), x: PAINT_TILE_SIZE, y: PAINT_TILE_SIZE, w: 44, h: 44 };
    const other = surf.readTile(t11);
    expect(other.data[(4 * 44 + 4) * 4 + 0]).toBe(99);
  });

  it("snapshotMissingTiles captures each tile once per session map", () => {
    const surf = new PaintTileSurface(PAINT_TILE_SIZE * 2 + 10, 500);
    const captured = new Map<string, TileKeyed<ImageData>>();

    const first = surf.snapshotMissingTiles({ x0: 0, y0: 0, x1: 600, y1: 400 }, captured);
    expect(first.length).toBe(6); // 3 cols x 2 rows

    const second = surf.snapshotMissingTiles({ x0: 0, y0: 0, x1: 600, y1: 400 }, captured);
    expect(second.length).toBe(0); // all already captured
  });

  it("applyOverlayTiles copies only requested tiles from overlay", () => {
    const surf = new PaintTileSurface(512, 256);
    const ctx = ctxOf(surf, made);
    // fake overlay canvas with recognizable pixels
    const overlayW = 512, overlayH = 256;
    const px = new Uint8ClampedArray(overlayW * overlayH * 4);
    for (let i = 0; i < overlayW * overlayH; i++) {
      px[i * 4] = i & 255; px[i * 4 + 1] = 80; px[i * 4 + 2] = 40; px[i * 4 + 3] = 255;
    }
    const overlay = {
      canvas: { width: overlayW, height: overlayH, __pixels: px },
      getImageData: () => ({}), putImageData: () => {}, clearRect: () => {},
      save: () => {}, restore: () => {}, globalCompositeOperation: "source-over",
      drawImage: (...args: unknown[]) => (ctx as any).drawImage(...args),
    } as unknown as CanvasRenderingContext2D;

    const tiles = [
      { tx: 0, ty: 0, key: tileKey(0, 0), x: 0, y: 0, w: PAINT_TILE_SIZE, h: PAINT_TILE_SIZE },
      { tx: 1, ty: 0, key: tileKey(1, 0), x: PAINT_TILE_SIZE, y: 0, w: PAINT_TILE_SIZE, h: PAINT_TILE_SIZE },
    ];
    surf.applyOverlayTiles(overlay as any, tiles as any);

    const got = surf.readTile(tiles[0]);
    expect(got.data[0]).toBe(0);          // i=0 red channel
    expect(got.data[4]).toBe(1);          // i=1
    const right = surf.readTile(tiles[1]);
    const idx = ((10 * PAINT_TILE_SIZE) + (PAINT_TILE_SIZE + 20)) * 4;
    void idx;
    expect(right.data[((10 * PAINT_TILE_SIZE) + 20) * 4]).toBe((PAINT_TILE_SIZE + 20) & 255);
    // untouched third column region (none here since W=512=2 tiles) — verified via size
  });

  it("constructor rejects invalid dimensions", () => {
    expect(() => new PaintTileSurface(0, 10)).toThrow();
    expect(() => new PaintTileSurface(-5, 10)).toThrow();
  });
});
