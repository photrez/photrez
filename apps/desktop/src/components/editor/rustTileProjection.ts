// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * One Rust-authoritative tile restore, for the undo/redo tile branch.
 *
 * Extracted from `useEditorCommands` so the projection (surface bytes, the epoch /
 * version stamps, the facade version sync) has one home and the dispatcher reads as
 * the sequence it is. Behaviour is unchanged: the same gate, the same fail-soft
 * `try`, the same refusal for a Rust-owned entry that yields no tiles.
 *
 * TWO SEPARATE CONCERNS, deliberately not fused:
 *  - the FETCH: may Rust's pixels be this entry's source of truth? Gated on
 *    `rustOwned || photrez.rustPixels` (the latter TRANSITIONAL).
 *  - the cursor STEP: did Rust record an entry for this pop at all? That belongs to
 *    `CommandHistory.stepRustCursor`, which fires exactly once per popped entry.
 *    This module AWAITS that step; it never issues an invoke of its own, because a
 *    second one is both a double cursor step and a second chance to read the wrong
 *    step's tiles.
 */
import { applyRustTilesToSurface } from "@/lib/rustShadow";
import { syncFacadeVersionFromPixel } from "@/lib/protocol/facadeRegistry";
import { decodeRustBytes } from "@/lib/protocol/pixelSeedCall";

/** Wire shape `rust_pixels_undo` / `rust_pixels_redo` answer with. */
export interface RustCursorStepTiles {
  tiles: { x: number; y: number; w: number; h: number; dataBase64: string }[];
  epoch: number;
  version: number;
}


/** `RustCursorStepTiles` with each tile's base64 payload decoded to bytes. */
export type DecodedCursorStepTiles = Omit<RustCursorStepTiles, "tiles"> & {
  tiles: { x: number; y: number; w: number; h: number; data: ArrayLike<number> }[];
};

/** The paint-surface handle this module stamps, in the shape it needs. */
interface PaintSurfaceLike {
  context: {
    putImageData(
      img: { width: number; height: number; data: Uint8ClampedArray },
      x: number,
      y: number,
    ): void;
  };
  pixelEpoch: number;
  pixelVersion?: number;
}

export interface RustTileProjection {
  /** Tiles for `uploadSurfaceTiles`, in the renderer's width/height shape. */
  tiles: { x: number; y: number; width: number; height: number; data: Uint8ClampedArray }[];
  /** The step's answer, tile bytes decoded, or null when the fetch did not run. */
  step: DecodedCursorStepTiles | null;
}

export interface RustTileProjectionDeps {
  /** The step the pop already fired, or null when it fired none. Consume-once. */
  takeCursorStep: () => Promise<unknown> | null;
  getEngine: () => { getId(): string; getPaintSurface(id: string): unknown } | null | undefined;
  layerId: string;
  rustOwned: boolean;
  /** TRANSITIONAL (`photrez.rustPixels`): the entry's own memento fallback. */
  rustPixelsFlag: boolean;
  /** The entry's memento, in the renderer's upload shape. */
  fallbackTiles: RustTileProjection["tiles"];
}

/**
 * Project one step's authoritative tiles, or fall back to the memento.
 *
 * Never throws: a failure degrades to a `console.warn` and the caller's fallback,
 * because the undo path is fail-soft and a toast here would report "Undo failed"
 * for a step whose model restore already succeeded.
 */
export async function projectRustTiles(deps: RustTileProjectionDeps): Promise<RustTileProjection> {
  if (!deps.rustOwned && !deps.rustPixelsFlag) {
    return { tiles: deps.fallbackTiles, step: null };
  }
  let step: DecodedCursorStepTiles | null = null;
  try {
    const cursorStep = deps.takeCursorStep();
    if (cursorStep) step = decodeRustBytes<DecodedCursorStepTiles>(await cursorStep);
    if (!step || !step.tiles.length) {
      // Rust took the cursor step but produced no tiles for it. The memento this
      // entry carries describes a step Rust already owns, so replaying it would
      // repaint the surface from bytes no store holds - the second pixel-history
      // owner this branch exists to remove. Uploads NOTHING: the returned tile list
      // is empty. The caller's `[paint] Rust-owned step returned no tiles` warn
      // fires, because the refusal is the caller's to announce - it owns the layer
      // id and knows whether a cursor step happened at all.
      return {
        tiles: deps.rustOwned ? [] : deps.fallbackTiles,
        step,
      };
    }
    const engine = deps.getEngine();
    const surf = engine?.getPaintSurface(deps.layerId) as PaintSurfaceLike | null | undefined;
    if (surf) {
      applyRustTilesToSurface(
        surf.context,
        step.tiles.map((t) => ({
          x: t.x,
          y: t.y,
          w: t.w,
          h: t.h,
          data: new Uint8ClampedArray(t.data),
        })),
      );
      surf.pixelEpoch = step.epoch;
      // Record which authoritative history cursor these pixels reflect.
      surf.pixelVersion = step.version;
      syncFacadeVersionFromPixel(engine?.getId() ?? "default", step.version);
    }
    // Re-map to the renderer's upload shape (width/height) for GPU upload.
    return {
      tiles: step.tiles.map((t) => ({
        x: t.x,
        y: t.y,
        width: t.w,
        height: t.h,
        data: new Uint8ClampedArray(t.data),
      })),
      step,
    };
  } catch (err) {
    console.warn("[paint] undo/redo Rust sync failed:", err);
    return { tiles: deps.rustOwned ? [] : deps.fallbackTiles, step };
  }
}
