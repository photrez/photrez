// PaintTileSurface (Fase 1) — persistent software-backed paint surface with
// tile-keyed snapshots for cheap paint commits and undo patches.
//
// Design notes (docs/plans/2026-08-21-brush-engine-research.md, FASE 1 DRAFT):
// - willReadFrequently:true => browser keeps pixels in RAM (software canvas),
//   making per-tile getImageData/putImageData cheap without GPU stalls
//   (verified: MDN HTMLCanvasElement.getContext).
// - Undo patches store pre-stroke ImageData PER TOUCHED TILE, so undo cost is
//   proportional to painted area, not canvas size.
// - API intentionally mirrors the future Rust contract
//   (apply_tile_patch / snapshot_tiles) so Fase 2 migration keeps call sites.

export const PAINT_TILE_SIZE = 256;

export interface TileKeyed<T> {
  key: string;
  tx: number;
  ty: number;
  value: T;
}

export interface TileRect {
  tx: number;
  ty: number;
  key: string;
  /** pixel-space origin */
  x: number;
  y: number;
  w: number;
  h: number;
}

export function tileKey(tx: number, ty: number): string {
  return `${tx},${ty}`;
}

/** Enumerate all tiles intersecting a pixel-space rect (clamped). */
export function tilesInRect(
  x0: number,
  y0: number,
  x1: number,
  y1: number,
  surfaceW: number,
  surfaceH: number,
  tileSize = PAINT_TILE_SIZE,
): TileRect[] {
  const out: TileRect[] = [];
  const cx0 = Math.max(0, Math.floor(x0 / tileSize));
  const cy0 = Math.max(0, Math.floor(y0 / tileSize));
  const cx1 = Math.min(Math.ceil(surfaceW / tileSize) - 1, Math.floor((x1 - 1) / tileSize));
  const cy1 = Math.min(Math.ceil(surfaceH / tileSize) - 1, Math.floor((y1 - 1) / tileSize));
  for (let ty = cy0; ty <= cy1; ty++) {
    for (let tx = cx0; tx <= cx1; tx++) {
      const x = tx * tileSize;
      const y = ty * tileSize;
      out.push({
        tx,
        ty,
        key: tileKey(tx, ty),
        x,
        y,
        w: Math.min(tileSize, surfaceW - x),
        h: Math.min(tileSize, surfaceH - y),
      });
    }
  }
  return out;
}

/**
 * Persistent paint surface for one layer. Software-backed on purpose
 * (willReadFrequently keeps pixels in RAM — cheap tile reads/writes).
 */
export class PaintTileSurface {
  readonly width: number;
  readonly height: number;
  private readonly canvas: OffscreenCanvas;
  private readonly ctx: OffscreenCanvasRenderingContext2D;

  constructor(width: number, height: number, initial?: ImageBitmap | null) {
    if (width <= 0 || height <= 0) throw new Error("PaintTileSurface: invalid dimensions");
    this.width = width;
    this.height = height;
    this.canvas = new OffscreenCanvas(width, height);
    const ctx = this.canvas.getContext("2d", { willReadFrequently: true });
    if (!ctx) throw new Error("PaintTileSurface: no 2d context");
    this.ctx = ctx;
    if (initial) this.ctx.drawImage(initial, 0, 0);
  }

  /** Pre-stroke capture of one tile region (call BEFORE painting touches it). */
  snapshotTile(r: TileRect): TileKeyed<ImageData> {
    return { key: r.key, tx: r.tx, ty: r.ty, value: this.ctx.getImageData(r.x, r.y, r.w, r.h) };
  }

  /** Undo/redo patch application (putImageData replaces tile pixels exactly). */
  restoreTile(patch: TileKeyed<ImageData>): void {
    this.ctx.putImageData(patch.value, patch.tx * PAINT_TILE_SIZE, patch.ty * PAINT_TILE_SIZE);
  }

  /**
   * Snapshot every tile in rect that isn't already captured. Returns only NEWLY
   * captured patches so history never stores duplicate copies of a tile within
   * one stroke session.
   */
  snapshotMissingTiles(
    rect: { x0: number; y0: number; x1: number; y1: number },
    captured: Map<string, TileKeyed<ImageData>>,
  ): TileKeyed<ImageData>[] {
    const fresh: TileKeyed<ImageData>[] = [];
    for (const t of tilesInRect(rect.x0, rect.y0, rect.x1, rect.y1, this.width, this.height)) {
      if (!captured.has(t.key)) {
        const snap = this.snapshotTile(t);
        captured.set(t.key, snap);
        fresh.push(snap);
      }
    }
    return fresh;
  }

  /**
   * Copy painted pixels from the overlay canvas into this surface, restricted
   * to the given tiles. Overlay is assumed to contain base+stroke composited
   * content in document space (same coordinates).
   */
  applyOverlayTiles(
    overlay: { canvas: { width: number; height: number } } & CanvasRenderingContext2D,
    tiles: TileRect[],
    blendMode: GlobalCompositeOperation = "source-over",
  ): void {
    this.ctx.save();
    this.ctx.globalCompositeOperation = blendMode;
    for (const t of tiles) {
      this.ctx.clearRect(t.x, t.y, t.w, t.h);
      // drawImage source-rect variant from the overlay canvas
      this.ctx.drawImage(overlay.canvas as unknown as CanvasImageSource, t.x, t.y, t.w, t.h, t.x, t.y, t.w, t.h);
    }
    this.ctx.restore();
  }

  /** Byte-exact tile readback (for tests and precise uploads). */
  readTile(r: TileRect): ImageData {
    return this.ctx.getImageData(r.x, r.y, r.w, r.h);
  }
}
