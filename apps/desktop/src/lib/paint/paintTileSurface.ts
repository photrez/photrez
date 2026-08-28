// PaintTileSurface (Fase 1) — persistent software-backed paint surface with
// tile-keyed snapshots for cheap paint commits and undo patches.
//
// Design notes (2026-08-21 research):
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

export interface MergedRect {
  x: number;
  y: number;
  w: number;
  h: number;
  /** Member tiles in row-major order (history patches stay per-tile). */
  tiles: TileRect[];
}

/**
 * Merge axis-aligned tile rects into larger rectangles — the
 * `mergeSparseRects` pattern (horizontal runs first, then merge
 * vertically adjacent runs with identical x-extent), verified 2026-08-23.
 * Used to keep GPU readback/upload calls
 * O(rects) instead of O(tiles) on large-brush strokes, while history patches
 * remain per-tile (display granularity != history granularity).
 * Output is y-sorted so a budgeted commit processes top-down.
 */
export function mergeTilesToRects(tiles: TileRect[], maxRectPixels = 4096 * 4096): MergedRect[] {
  if (tiles.length === 0) return [];
  const byRow = new Map<number, TileRect[]>();
  for (const t of tiles) {
    const row = byRow.get(t.ty);
    if (row) row.push(t);
    else byRow.set(t.ty, [t]);
  }
  type Run = MergedRect;
  const runs: Run[] = [];
  const rows = [...byRow.keys()].sort((a, b) => a - b);
  for (const ty of rows) {
    let cur: Run | null = null;
    for (const t of byRow.get(ty)!.sort((a, b) => a.tx - b.tx)) {
      if (cur && t.x === cur.x + cur.w) {
        cur.w += t.w;
        cur.tiles.push(t);
      } else {
        cur = { x: t.x, y: t.y, w: t.w, h: t.h, tiles: [t] };
        runs.push(cur);
      }
    }
  }
  runs.sort((a, b) => a.y - b.y || a.x - b.x);
  const merged: Run[] = [];
  // Track the currently-open run per x-origin so interleaved column groups
  // (A rows0-1 @x0, B rows0-1 @x2, ...) each merge independently.
  const openByX = new Map<number, Run>();
  for (const run of runs) {
    const cand = openByX.get(run.x);
    if (cand && cand.w === run.w && cand.y + cand.h === run.y) {
      cand.h += run.h;
      for (const t of run.tiles) cand.tiles.push(t);
    } else {
      const fresh: Run = { ...run, tiles: run.tiles.slice() };
      merged.push(fresh);
      openByX.set(run.x, fresh);
    }
  }
  merged.sort((a, b) => a.y - b.y || a.x - b.x);

  // Split rects exceeding maxRectPixels back down. Dense mega-strokes merge
  // into near-full-canvas rects otherwise - measured 2026-08-23: one
  // 5800x3700 rect => 86MB compose buffer + zero yields + multi-second
  // commits. Merged rects have a UNIFORM x-extent across their rows (vertical
  // merge requires identical x/w), so cutting by whole tile rows bounds each
  // output rect's area to <= maxRectPixels exactly (bbox height is a whole
  // number of rows).
  if (maxRectPixels > 0) {
    const out: MergedRect[] = [];
    for (const r of merged) {
      if (r.w * r.h <= maxRectPixels) {
        out.push(r);
        continue;
      }
      const rowsOf = new Map<number, TileRect[]>();
      for (const t of r.tiles) {
        const row = rowsOf.get(t.ty);
        if (row) row.push(t);
        else rowsOf.set(t.ty, [t]);
      }
      const tys = [...rowsOf.keys()].sort((a, b) => a - b);
      const maxRows = Math.max(1, Math.floor(maxRectPixels / (r.w * PAINT_TILE_SIZE)));
      let i = 0;
      while (i < tys.length) {
        const groupTys = tys.slice(i, i + maxRows);
        const group = groupTys.flatMap((ty) => rowsOf.get(ty)!);
        const gy0 = groupTys[0] * PAINT_TILE_SIZE;
        const gy1 = Math.min(r.y + r.h, (groupTys[groupTys.length - 1] + 1) * PAINT_TILE_SIZE);
        out.push({ x: r.x, y: gy0, w: r.w, h: gy1 - gy0, tiles: group });
        i += groupTys.length;
      }
    }
    return out;
  }
  return merged;
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
  /** C5.2: epoch of the Rust canonical state this derived cache currently
   *  reflects. Bumped to the Rust epoch on every commit/undo/redo/rehydrate. */
  pixelEpoch: number = 0;
  /** C5.3-A: `DocumentVersion` returned by the Rust unified history command.
   *  Lets the TS cache record exactly which authoritative history cursor the
   *  pixels reflect (cursor == sole truth; this is a cache, not the source). */
  pixelVersion: number = 0;

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

  /**
   * One getImageData for a whole merged rect (Fase 1.5): the single GPU/CPU
   * readback feeding both the rect upload and (via subarray views) the
   * per-tile after-patches. Rect must be surface-clamped.
   */
  readRect(x: number, y: number, w: number, h: number): ImageData {
    return this.ctx.getImageData(x, y, w, h);
  }

  /** Direct 2d context for the tile-commit path (raw dab drawImage ops). */
  get context(): OffscreenCanvasRenderingContext2D {
    return this.ctx;
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

  /** C5.2: mark the derived cache stale so the next commit forces rehydration. */
  markStale(): void {
    this.pixelEpoch = -1;
  }
}
