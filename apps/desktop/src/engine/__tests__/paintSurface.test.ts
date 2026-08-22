import { describe, expect, it, vi, beforeEach } from "vitest";
import { DocumentEngine } from "../document";
import { PAINT_TILE_SIZE, tileKey } from "../../lib/paint/paintTileSurface";

// Minimal OffscreenCanvas stub so PaintTileSurface can exist under test
// (mirrors the FakeCtx approach in lib/paint/__tests__/paintTileSurface.test.ts)
vi.stubGlobal("OffscreenCanvas", class {
  width: number; height: number;
  private data: Uint8ClampedArray;
  constructor(w: number, h: number) {
    this.width = w; this.height = h;
    this.data = new Uint8ClampedArray(w * h * 4);
  }
  getContext(_t: "2d", _o?: { willReadFrequently?: boolean }) {
    const self = this;
    return {
      drawImage(src: { __pixels?: Uint8ClampedArray; width?: number; height?: number }) {
        const px = (src as any).__pixels as Uint8ClampedArray | undefined;
        if (!px || !src.width) return;
        const n = Math.min(self.width, src.width);
        const rows = Math.min(self.height, Math.floor(px.length / 4 / src.width));
        for (let y = 0; y < rows; y++)
          for (let x = 0; x < n; x++) {
            const s = (y * src.width + x) * 4, d = (y * self.width + x) * 4;
            self.data[d] = px[s]; self.data[d + 1] = px[s + 1];
            self.data[d + 2] = px[s + 2]; self.data[d + 3] = px[s + 3];
          }
      },
      getImageData(sx: number, sy: number, sw: number, sh: number) {
        const out = new Uint8ClampedArray(sw * sh * 4);
        for (let y = 0; y < sh; y++)
          for (let x = 0; x < sw; x++) {
            const s = ((sy + y) * self.width + (sx + x)) * 4, d = (y * sw + x) * 4;
            out[d] = self.data[s]; out[d + 1] = self.data[s + 1];
            out[d + 2] = self.data[s + 2]; out[d + 3] = self.data[s + 3];
          }
        return { data: out, width: sw, height: sh, colorSpace: "srgb" };
      },
      clearRect() {}, save() {}, restore() {},
      putImageData() {},
      globalCompositeOperation: "source-over",
    };
  }
});

function fakeBitmap(w: number, h: number, fill: number): ImageBitmap {
  const px = new Uint8ClampedArray(w * h * 4);
  for (let i = 0; i < w * h; i++) {
    px[i * 4] = fill; px[i * 4 + 1] = fill; px[i * 4 + 2] = fill; px[i * 4 + 3] = 255;
  }
  return { width: w, height: h, __pixels: px } as unknown as ImageBitmap;
}

describe("DocumentEngine paint surface lifecycle (Fase 1 foundation)", () => {
  beforeEach(() => { vi.restoreAllMocks(); });

  function makeEngineWithLayer(): { eng: DocumentEngine; layerId: string } {
    const eng = new DocumentEngine("doc-t", "Test", 512, 512);
    const layer = eng.addLayer("L1", 512, 512);
    eng.setLayerImageBitmap(layer.id, fakeBitmap(512, 512, 10));
    return { eng, layerId: layer.id };
  }

  it("getPaintSurface lazily creates from bitmap and caches the SAME instance", () => {
    const { eng, layerId } = makeEngineWithLayer();
    const s1 = eng.getPaintSurface(layerId)!;
    expect(s1).not.toBeNull();
    expect(s1.width).toBe(512);
    const s2 = eng.getPaintSurface(layerId)!;
    expect(s2).toBe(s1); // cached
    // content synced from bitmap (fill=10 everywhere incl. tile corner)
    const tile = s1.readTile({ tx: 1, ty: 1, key: tileKey(1, 1), x: PAINT_TILE_SIZE, y: PAINT_TILE_SIZE, w: PAINT_TILE_SIZE, h: PAINT_TILE_SIZE });
    expect(tile.data[0]).toBe(10);
  });

  it("setLayerImageBitmap invalidates the cached surface (fresh instance, new pixels)", () => {
    const { eng, layerId } = makeEngineWithLayer();
    const s1 = eng.getPaintSurface(layerId)!;
    eng.setLayerImageBitmap(layerId, fakeBitmap(512, 512, 77));
    const s2 = eng.getPaintSurface(layerId)!;
    expect(s2).not.toBe(s1);
    const tile = s2.readTile({ tx: 0, ty: 0, key: tileKey(0, 0), x: 0, y: 0, w: PAINT_TILE_SIZE, h: PAINT_TILE_SIZE });
    expect(tile.data[0]).toBe(77);
  });

  it("restore(snapshot) clears cached surfaces (undo cannot serve stale pixels)", () => {
    const { eng, layerId } = makeEngineWithLayer();
    const snap = eng.snapshot();
    void eng.getPaintSurface(layerId);
    // simulate painting mutating the surface AFTER the snapshot
    const s = eng.getPaintSurface(layerId)!;
    s.readTile({ tx: 0, ty: 0, key: tileKey(0, 0), x: 0, y: 0, w: 1, h: 1 }); // touch read path
    eng.setLayerImageBitmap(layerId, fakeBitmap(512, 512, 33)); // post-snapshot commit state

    eng.restore(snap);

    const fresh = eng.getPaintSurface(layerId)!;
    const tile = fresh.readTile({ tx: 0, ty: 0, key: tileKey(0, 0), x: 0, y: 0, w: PAINT_TILE_SIZE, h: PAINT_TILE_SIZE });
    expect(tile.data[0]).toBe(10); // restored pre-stroke pixels
  });

  it("returns null when the layer has no bitmap yet", () => {
    const eng = new DocumentEngine("doc-e", "Empty", 256, 256);
    const layer = eng.addLayer("EmptyL", 256, 256);
    expect(eng.getPaintSurface(layer.id)).toBeNull();
  });
});
