// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Frame-sliced encode/decode for the deferred brush commit.
 *
 * WHY THIS EXISTS. Measured in the shipped WebView at 4096x4096 with a
 * 3254x208 dirty rect, one `c4CoreCommit` blocks the main thread for ~56 ms:
 * readRect 6.0, base64 encode 19.5, IPC argument serialisation 1.8, reply parse
 * 1.3, base64 decode 16.2, tile copy 2.8, surface apply 8.3. The IPC round trip
 * itself leaves the main thread idle, so the whole cost is six separable pure
 * chunks around it. Yielding between them keeps every slice inside one frame
 * without moving the commit off the main thread and without changing a pixel.
 *
 * The yield is one `requestAnimationFrame`. `scheduler.postTask` was
 * considered and not used: the jsdom this suite runs in does not implement it,
 * and rAF is what the rest of the paint path already yields with.
 *
 * SURFACE APPLY IS NOT SLICED, deliberately. `applyRustTilesToSurface` writes a
 * reply's tiles one at a time, so a yield between two of them would let a newer
 * stroke's `surface.snapshotTile` capture a half-applied tile set - the undo
 * corruption class documented in useBrushOverlay.ts. It stays one slice, and the
 * commit yields before and after it.
 */

import { PIXEL_BYTE_FIELDS, decodePixelBytes, pixelEncodeChunks } from "./pixelSeedCall";

/** Hand the main thread back to the compositor for one frame. */
export function yieldToFrame(): Promise<void> {
  return new Promise((resolve) => {
    if (typeof requestAnimationFrame === "function") {
      requestAnimationFrame(() => resolve());
    } else {
      // No rAF (a node-environment unit test): a macrotask still breaks the
      // synchronous run, which is the only property every caller here needs.
      setTimeout(resolve, 0);
    }
  });
}

/**
 * Encode steps between yields. The encode slice measured in the suite is
 * ~1 ms per 24,576-byte step, so 8 steps is an ~8 ms slice and leaves headroom
 * for how a loaded compositor honestly spaces those frames.
 */
export const ENCODE_STEPS_PER_SLICE = 8;

/**
 * Decoded byte payloads between yields. At ~3.7 ms per 256 KiB payload, 2
 * payloads is a ~7 ms slice, again with headroom.
 */
export const DECODE_PAYLOADS_PER_SLICE = 2;

/**
 * The encoder, sliced: byte-identical to `encodePixelBytes`, but the main thread
 * gets a frame between chunks instead of running the whole encode in one block.
 */
export async function encodePixelBytesSliced(
  bytes: ArrayLike<number>,
  yieldTo: () => Promise<void> = yieldToFrame,
): Promise<string> {
  const parts: string[] = [];
  let steps = 0;
  for (const part of pixelEncodeChunks(bytes)) {
    parts.push(part);
    steps++;
    if (steps % ENCODE_STEPS_PER_SLICE === 0) await yieldTo();
  }
  return parts.join("");
}

/**
 * The decoder, sliced: the same base64-to-byte-array rewrite `decodeRustBytes`
 * performs, with a frame yielded between list entries instead of one run to the
 * end.
 *
 * It uses `decodeRustBytes`'s own exported field map and byte decoder rather
 * than restating either, so the wire contract lives in one place: the only
 * thing this adds is where the awaits sit.
 */
export async function decodeRustBytesSliced<T>(
  value: unknown,
  yieldTo: () => Promise<void> = yieldToFrame,
): Promise<T> {
  let payloads = 0;
  const walk = async (node: unknown): Promise<unknown> => {
    if (Array.isArray(node)) {
      const out: unknown[] = [];
      for (const item of node) {
        out.push(await walk(item));
      }
      return out;
    }
    if (node === null || typeof node !== "object") return node;
    const record = node as Record<string, unknown>;
    let out: Record<string, unknown> | null = null;
    for (const [k, v] of Object.entries(record)) {
      const rename = typeof v === "string" ? PIXEL_BYTE_FIELDS[k] : undefined;
      let decoded: unknown;
      if (rename) {
        decoded = decodePixelBytes(v as string);
        payloads++;
        if (payloads % DECODE_PAYLOADS_PER_SLICE === 0) await yieldTo();
      } else {
        decoded = await walk(v);
      }
      const key = rename ?? k;
      if ((decoded !== v || key !== k) && out === null) out = { ...record };
      if (out) {
        delete out[k];
        out[key] = decoded;
      }
    }
    return out ?? record;
  };
  return (await walk(value)) as T;
}