// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * How pixel bytes cross a Tauri command, in both directions.
 *
 * Every pixel-bytes field on the pixel commands is BASE64, never a byte
 * sequence. Tauri v2 carries both directions as JSON: outbound serde expands a
 * `Vec<u8>` into one JSON number per byte, and inbound the `JSON.stringify`
 * replacer expands a `Uint8Array` argument with `Array.from(val)`
 * (tauri-2.11.5/scripts/process-ipc-message-fn.js). A whole 4096 x 4096 layer is
 * 67,108,864 bytes, which the number form costs 268,435,768 characters to
 * serialise and ~15 s of main thread; a 3254 x 208 dirty rect costs
 * 5,414,721 characters out and 27,264,034 back.
 *
 * `readPixelSeedCall` exists because a test double that reads `args.bytes` is
 * reading a shape the runtime never sends, so it proves nothing about the
 * production path.
 */

/** Metadata + bytes as they reach a test double standing in for Tauri. */
export interface PixelSeedCall {
  docId: string;
  layerId: string;
  width: number;
  height: number;
  bytes: Uint8Array;
}

/**
 * Encode pixel bytes as base64, the way the pixel commands expect them.
 *
 * Two things make this cost 530 ms instead of 2,412 ms for a 4096 x 4096 layer,
 * both measured in the shipped WebView, both byte-identical to the naive form:
 *
 * 1. CHUNKED, every chunk a multiple of 3 so each encodes independently. The
 *    one-shot form - build one giant binary string, then `btoa` it - costs
 *    4,736 ms, because the 67 MB intermediate string is assembled by repeated
 *    concatenation.
 * 2. `String.fromCharCode.apply` is given the TYPED ARRAY. Wrapping it in
 *    `Array.from` first materialises a 24,576-element plain JS array per chunk,
 *    2,731 times for a whole layer; that copy alone is the other 1,880 ms.
 *
 * This stays hand-rolled. The stdlib has no base64 encoder on this runtime, and
 * the alternatives measured worse - a byte table is faster (308 ms) but did not
 * reproduce the same string, so it is not worth trading correctness for here.
 */
export function encodePixelBytes(bytes: ArrayLike<number>): string {
  const STEP = 24576; // 3 x 8192, so no chunk straddles a base64 quantum
  const view = bytes instanceof Uint8Array ? bytes : Uint8Array.from(bytes);
  const scratch = new Uint8Array(STEP);
  const parts: string[] = [];
  // The casts are for the TYPE, not the runtime: `apply` is declared to take a
  // `number[]`, and a Uint8Array is array-like and is what the engine actually
  // reads. Measured byte-identical, and 1,880 ms cheaper at 4096 x 4096.
  for (let i = 0; i + STEP <= view.length; i += STEP) {
    scratch.set(view.subarray(i, i + STEP));
    parts.push(btoa(String.fromCharCode.apply(null, scratch as unknown as number[])));
  }
  const tail = view.subarray(view.length - (view.length % STEP));
  if (tail.length) parts.push(btoa(String.fromCharCode.apply(null, tail as unknown as number[])));
  return parts.join("");
}

/** Decode a base64 pixel payload the way the Rust command does. */
export function decodePixelBytes(bytesBase64: string): Uint8Array {
  const binary = atob(bytesBase64);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) out[i] = binary.charCodeAt(i);
  return out;
}

/** The base64 field name -> the byte-array field name it stands in for. */
const PIXEL_BYTE_FIELDS: Record<string, string> = {
  dataBase64: "data",
  rgbaBase64: "rgba",
  bytesBase64: "bytes",
};

/**
 * Turn a pixel command's base64 payload fields back into byte arrays, wherever
 * they sit in the value - a command argument object, or a response's
 * `before`/`after`/`tiles` list, at any depth.
 *
 * This is the whole inbound half of the wire contract: Rust sends
 * `{x, y, w, h, dataBase64}` (and `rgbaBase64` / `bytesBase64` outbound), and
 * every consumer downstream wants `data`.
 *
 * A field that is already a byte array passes through untouched, so an
 * in-process store double answering with real bytes keeps working. The wire
 * SHAPE is proved by the transport tests, not here.
 */
export function decodeRustBytes<T>(value: unknown): T {
  if (Array.isArray(value)) return value.map((v) => decodeRustBytes(v)) as unknown as T;
  if (value === null || typeof value !== "object") return value as T;
  const record = value as Record<string, unknown>;
  let out: Record<string, unknown> | null = null;
  for (const [k, v] of Object.entries(record)) {
    const rename = typeof v === "string" ? PIXEL_BYTE_FIELDS[k] : undefined;
    const decoded = rename ? decodePixelBytes(v as string) : decodeRustBytes(v);
    const key = rename ?? k;
    if ((decoded !== v || key !== k) && out === null) out = { ...record };
    if (out) {
      delete out[k];
      out[key] = decoded;
    }
  }
  return (out ?? record) as unknown as T;
}

/**
 * Convert a caller-side tile list into the `dataBase64` shape `apply_tile_patch`
 * deserializes. The pixel-bytes field is replaced rather than added, so a byte
 * array can never ride along beside it.
 */
export function encodeRustTiles(
  tiles: readonly { x: number; y: number; width: number; height: number; data: ArrayLike<number> }[],
): { x: number; y: number; w: number; h: number; dataBase64: string }[] {
  return tiles.map((t) => ({
    x: t.x,
    y: t.y,
    w: t.width,
    h: t.height,
    dataBase64: encodePixelBytes(t.data),
  }));
}

/**
 * The argument object one `rust_pixels_write_region` call must be dispatched
 * with. Use this wherever a test writes a region itself, so the write goes over
 * the same transport production uses.
 */
export function pixelRegionDispatch(
  docId: string,
  layerId: string,
  x: number,
  y: number,
  w: number,
  h: number,
  rgba: ArrayLike<number>,
): { docId: string; layerId: string; x: number; y: number; w: number; h: number; rgbaBase64: string } {
  return { docId, layerId, x, y, w, h, rgbaBase64: encodePixelBytes(rgba) };
}

/**
 * Read one seed call out of a mocked `invoke`.
 *
 * `args` is the invoke payload, exactly as `invoke(cmd, args)` receives it.
 * Returns null when the call is not a seed, so a double can fall through to its
 * other commands.
 */
export function readPixelSeedCall(cmd: string, args: unknown): PixelSeedCall | null {
  if (cmd !== "rust_pixels_init") return null;
  const a = args as { docId?: string; layerId?: string; width?: number; height?: number; bytesBase64?: string };
  if (typeof a?.bytesBase64 !== "string") {
    throw new Error("rust_pixels_init was called without a base64 seed payload");
  }
  for (const key of ["docId", "layerId"] as const) {
    if (!a[key]) throw new Error(`rust_pixels_init is missing ${key}`);
  }
  for (const key of ["width", "height"] as const) {
    if (!Number.isInteger(a[key])) {
      throw new Error(`rust_pixels_init ${key} is missing or not an integer: ${a[key]}`);
    }
  }
  return {
    docId: a.docId as string,
    layerId: a.layerId as string,
    width: a.width as number,
    height: a.height as number,
    bytes: decodePixelBytes(a.bytesBase64),
  };
}

/**
 * The argument object a seed call must be dispatched with.
 *
 * Use this wherever a test drives `rust_pixels_init` itself (to arrange a store),
 * so the setup goes over the same transport production uses.
 */
export function pixelSeedDispatch(
  docId: string,
  layerId: string,
  width: number,
  height: number,
  bytes: ArrayLike<number>,
): { docId: string; layerId: string; width: number; height: number; bytesBase64: string } {
  return {
    docId,
    layerId,
    width,
    height,
    bytesBase64: encodePixelBytes(bytes),
  };
}