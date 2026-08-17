// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, it, expect } from "vitest";
import { invertRgbaWithWasm, invertRgbaFallback } from "../../components/editor/wasmExport";

// Gated benchmark — runs only with PHASE2_BENCH=1 so the default suite stays
// fast. Mirrors the Phase 0 CPU-benchmark protocol.
const RUN = process.env.PHASE2_BENCH === "1";
describe.skipIf(!RUN)("Phase 2 kernel benchmark (gated: PHASE2_BENCH=1)", () => {
  it("invert: wasm vs ts fallback", async () => {
    const w = 1920;
    const h = 1080;
    const buf = new Uint8Array(w * h * 4);
    for (let i = 0; i < buf.length; i++) buf[i] = (i * 31) & 255;

    const wasmOut = await invertRgbaWithWasm(buf);
    expect(wasmOut).not.toBeNull();

    const N = 5;
    const tw0 = performance.now();
    for (let k = 0; k < N; k++) await invertRgbaWithWasm(buf);
    const tw = (performance.now() - tw0) / N;

    const tf0 = performance.now();
    for (let k = 0; k < N; k++) invertRgbaFallback(buf);
    const tf = (performance.now() - tf0) / N;

    // eslint-disable-next-line no-console
    console.log(`[phase2-bench] invert  ${w}x${h}: wasm ${tw.toFixed(2)}ms  ts ${tf.toFixed(2)}ms`);
    // Sanity: wasm output must match the TS fallback exactly.
    const ts = invertRgbaFallback(buf);
    expect(Array.from(wasmOut!)).toEqual(Array.from(ts));
  });
});
