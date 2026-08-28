// SPDX-License-Identifier: AGPL-3.0-or-later
// Brush dab producer abstraction (Fase 3 package 1).
//
// The stroke spacing/carry state machine (getBrushDabSpacing +
// interpolateDabs in brushTipMask.ts) is the "brush logic" seam. This module
// wraps it behind a tiny producer interface so the producer can be swapped
// between the TS implementation and the Rust port (BrushStrokeEngine in
// photrez-core) without touching the stroke consumer.
//
// Flag: localStorage.photrez.rustDabs === "1" prefers the Rust producer.
// Falls back to TS silently when the wasm module is not yet loaded or the
// export is missing - the paint path must never await.

import { getBrushDabSpacing, interpolateDabs } from "./brushTipMask";
import { getLoadedWasmModule } from "./wasmExport";

export interface DabProducer {
  /** Feed one pointer move; returns the number of dabs produced. */
  update(x: number, y: number): number;
  /** x,y pairs from the last update. Valid only until the next update call
   *  (Rust impl: view into wasm memory - consume immediately). */
  view(): Float64Array | null;
  /** Current spacing carry (diagnostics/parity tests). */
  carry(): number;
  /** React to brush size changes mid-stroke (carry persists). */
  setSize(size: number): void;
}

/** TS reference producer - wraps the legacy interpolateDabs state machine. */
export function createTsDabProducer(size: number): DabProducer {
  // hardness/flow never affect spacing (asserted by brushTipMask.test.ts).
  let spacing = getBrushDabSpacing(size, 0, 1);
  let carry = 0;
  let lastX = 0;
  let lastY = 0;
  let started = false;
  let dabs: Array<{ x: number; y: number }> = [];
  return {
    update(x, y) {
      if (!started) {
        started = true;
        lastX = x;
        lastY = y;
        dabs = [];
        return 0;
      }
      const result = interpolateDabs({ x: lastX, y: lastY }, { x, y }, spacing, carry);
      carry = result.carry;
      dabs = result.dabs;
      lastX = x;
      lastY = y;
      return dabs.length;
    },
    view() {
      if (dabs.length === 0) return null;
      const out = new Float64Array(dabs.length * 2);
      for (let i = 0; i < dabs.length; i++) {
        out[i * 2] = dabs[i].x;
        out[i * 2 + 1] = dabs[i].y;
      }
      return out;
    },
    carry() {
      return carry;
    },
    setSize(size) {
      spacing = getBrushDabSpacing(size, 0, 1);
    },
  };
}

/** Rust producer over photrez-core BrushStrokeEngine. Null when wasm absent. */
export function createRustDabProducer(size: number): DabProducer | null {
  const mod = getLoadedWasmModule();
  if (!mod || typeof mod.BrushStrokeEngine !== "function") return null;
  try {
    const engine = new mod.BrushStrokeEngine(size);
    let count = 0;
    return {
      update(x, y) {
        count = engine.update(x, y);
        return count;
      },
      view() {
        // Short-lived window into wasm memory (heap may grow on realloc):
        // valid until the next update()/begin() on this engine.
        return count > 0 ? engine.dab_view() : null;
      },
      carry() {
        return engine.carry();
      },
      setSize(size) {
        engine.set_size(size);
      },
    };
  } catch (err) {
    console.warn("[rustDabs] BrushStrokeEngine unavailable, using TS producer:", err);
    return null;
  }
}

export function isRustDabsEnabled(): boolean {
  try {
    return typeof localStorage !== "undefined" && localStorage.getItem("photrez.rustDabs") === "1";
  } catch {
    return false;
  }
}

/** Factory used by the paint path. Prefers Rust when flag set and available;
 *  otherwise the TS reference producer (identical outputs). */
export function createDabProducer(size: number): DabProducer {
  if (isRustDabsEnabled()) {
    const rust = createRustDabProducer(size);
    if (rust) return rust;
  }
  return createTsDabProducer(size);
}
