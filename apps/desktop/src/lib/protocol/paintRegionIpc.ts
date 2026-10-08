// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * How one canonical region write crosses the process boundary.
 *
 * WHY RAW EXISTS. The base64 command (`rust_pixels_write_region`) was measured
 * on the shipped WebView at 4096x4096 with a 3254x208 dirty rect: readRect 6.0,
 * encode 19.5, parse 1.3, decode 16.2, apply 8.3 ms - and a round trip of
 * 344-387 ms (docs/frame-pacing-baseline.md, same 3254x208 dirty rect), which
 * leaves the majority of the cost INSIDE the invoke, where Tauri builds one
 * JSON character per base64 character in each direction. That is the transport half of the frame-pacing
 * miss, and it is what this module removes: the request body is the RGBA bytes
 * themselves (`application/octet-stream`) and the reply is one
 * `tauri::ipc::Response` ArrayBuffer, both documented Tauri mechanisms.
 *
 * WHAT THE FALLBACK IS FOR. The raw path needs a webview that can deliver a
 * typed-array body and read a binary reply. Tests run in jsdom and never are
 * that; a packaged build on a runtime without a custom protocol is not either.
 * So the transport is CHOSEN ONCE, by evidence (`rust_pixels_raw_echo` round
 * trip), and every caller keeps working unchanged on the base64 command when
 * the answer is no. Nothing about the write's semantics differs between the two
 * arms - same command on the Rust side, same one `Pixel` entry, same reply
 * tiles - so a fallback is a transport choice, never a behaviour change.
 *
 * WIRE FORMAT is bound to `apps/desktop/src-tauri/src/paint_parity_raw.rs`.
 * Change BOTH together, the same rule the base64 fixtures follow.
 */

import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { decodeRustBytesSliced } from "./commitFrameYield";
import { pixelInvoke } from "./pixelInvokeCensus";
import { decodeRustBytes, encodePixelBytes } from "./pixelSeedCall";

/** Must equal `MAGIC` in paint_parity_raw.rs (8 bytes). */
export const RAW_REGION_MAGIC = "PTZRW01!";
/** u32 little-endian length prefix between the magic and a JSON header. */
const LEN_PREFIX_BYTES = 4;

/** Dev/test gate: force the raw transport on without probing a real runtime. */
const RAW_TRANSPORT_GATE = "photrez.rawPixelTransport";

export interface RegionWriteTarget {
  docId: string;
  layerId: string;
  x: number;
  y: number;
  w: number;
  h: number;
  rgba: ArrayLike<number>;
}

export interface RegionWriteTile {
  x: number;
  y: number;
  w: number;
  h: number;
  data: Uint8Array;
}

export interface RegionWriteReply {
  after: RegionWriteTile[];
  epoch: number;
  version: number;
}

export function rawTransportForcedOn(): boolean {
  try {
    return (
      typeof localStorage !== "undefined" &&
      localStorage.getItem(RAW_TRANSPORT_GATE) === "1"
    );
  } catch {
    return false;
  }
}

/**
 * The request: the RGBA bytes as the WHOLE body, metadata in the
 * `X-Photrez-Region` header.
 *
 * The body carries pixels and nothing else. `rust_pixels_write_region_raw`
 * re-assembles its own length-prefixed frame from the header plus this body
 * before parsing, so a prefix here would not be skipped - it would be read as
 * pixel data and the write would be rejected for a byte-count mismatch. The
 * metadata therefore lives in exactly one place, and the frame with the magic
 * prefix exists only inside Rust (see `paint_parity_raw.rs`).
 */
export function encodeRawRegionRequest(target: RegionWriteTarget): {
  body: Uint8Array;
  headers: Record<string, string>;
} {
  for (const key of ["docId", "layerId"] as const) {
    if (!target[key]) throw new RangeError(`region write requires a non-empty ${key}`);
  }
  for (const key of ["x", "y", "w", "h"] as const) {
    if (!Number.isInteger(target[key])) {
      throw new RangeError(`region write ${key} must be an integer, got ${target[key]}`);
    }
  }
  const expected = target.w * target.h * 4;
  if (target.rgba.length !== expected) {
    throw new RangeError(
      `region write rgba is ${target.rgba.length} bytes; expected ${expected} for ${target.w}x${target.h}`,
    );
  }
  const metadata = JSON.stringify({
    docId: target.docId,
    layerId: target.layerId,
    x: target.x,
    y: target.y,
    w: target.w,
    h: target.h,
    byteLen: target.rgba.length,
  });
  const body = new Uint8Array(target.rgba.length);
  body.set(target.rgba);
  return { body, headers: { "X-Photrez-Region": metadata } };
}

/**
 * The reply frame: magic | header-len | header JSON | tile data blocks.
 *
 * Every length is validated against the buffer it travels in, so a truncated or
 * foreign reply throws with the field named instead of yielding short tiles that
 * would land in history as silent pixel loss.
 */
export function decodeRawRegionReply(value: unknown): RegionWriteReply {
  const bytes =
    value instanceof ArrayBuffer
      ? new Uint8Array(value)
      : value instanceof Uint8Array
        ? value
        : null;
  if (!bytes) {
    throw new TypeError(
      `region write reply must be an ArrayBuffer, got ${Object.prototype.toString.call(value)}`,
    );
  }
  const magic = RAW_REGION_MAGIC.length;
  const magicBytes = new TextEncoder().encode(RAW_REGION_MAGIC);
  if (bytes.length < magic + LEN_PREFIX_BYTES) {
    throw new RangeError(`region write reply is ${bytes.length} bytes; too short for a frame header`);
  }
  for (let i = 0; i < magic; i++) {
    if (bytes[i] !== magicBytes[i]) {
      throw new RangeError("region write reply has a foreign magic; refusing to parse it");
    }
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const headerLen = view.getUint32(magic, true);
  const headerEnd = magic + LEN_PREFIX_BYTES + headerLen;
  if (headerEnd > bytes.length) {
    throw new RangeError(
      `region write reply header length ${headerLen} runs past the ${bytes.length}-byte buffer`,
    );
  }
  let header: { epoch?: unknown; version?: unknown; tiles?: unknown };
  try {
    header = JSON.parse(new TextDecoder().decode(bytes.subarray(magic + LEN_PREFIX_BYTES, headerEnd)));
  } catch (err) {
    throw new RangeError(`region write reply header is not valid JSON: ${String(err)}`);
  }
  if (typeof header.epoch !== "number" || typeof header.version !== "number") {
    throw new RangeError("region write reply header is missing numeric epoch/version");
  }
  if (!Array.isArray(header.tiles)) {
    throw new RangeError("region write reply header is missing its tiles list");
  }
  const after: RegionWriteTile[] = [];
  let offset = headerEnd;
  for (const entry of header.tiles as Record<string, unknown>[]) {
    const x = Number(entry?.x);
    const y = Number(entry?.y);
    const w = Number(entry?.w);
    const h = Number(entry?.h);
    if (![x, y, w, h].every(Number.isInteger) || w < 0 || h < 0) {
      throw new RangeError(`region write reply tile has invalid geometry: ${JSON.stringify(entry)}`);
    }
    const blockLen = w * h * 4;
    if (offset + blockLen > bytes.length) {
      throw new RangeError(
        `region write reply tile ${x},${y} needs ${blockLen} bytes but only ${bytes.length - offset} remain`,
      );
    }
    after.push({ x, y, w, h, data: bytes.slice(offset, offset + blockLen) });
    offset += blockLen;
  }
  if (offset !== bytes.length) {
    throw new RangeError(
      `region write reply has ${bytes.length - offset} trailing bytes after its ${after.length} tiles`,
    );
  }
  return { after, epoch: header.epoch, version: header.version };
}

let rawTransport: boolean | null = null;
let rawProbeInFlight: Promise<boolean> | null = null;

/**
 * Decide ONCE per session whether the raw transport is usable, by round-tripping
 * `rust_pixels_raw_echo` - a read-only command that proves BOTH directions
 * deliver bytes. Absence of the command (an older native build), a rejection, or
 * a non-binary reply all mean "use the base64 command".
 */
export async function probeRawTransport(): Promise<boolean> {
  if (rawTransportForcedOn()) return true;
  if (rawTransport !== null) return rawTransport;
  if (!isTauriRuntime()) {
    rawTransport = false;
    return false;
  }
  if (!rawProbeInFlight) {
    rawProbeInFlight = (async () => {
      try {
        const reply = await invoke("rust_pixels_raw_echo", new Uint8Array([0x5a]));
        const bytes =
          reply instanceof ArrayBuffer
            ? new Uint8Array(reply)
            : reply instanceof Uint8Array
              ? reply
              : null;
        const magicBytes = new TextEncoder().encode(RAW_REGION_MAGIC);
        const ok =
          bytes !== null &&
          bytes.length === magicBytes.length + LEN_PREFIX_BYTES + 1 &&
          magicBytes.every((b, i) => bytes[i] === b) &&
          bytes[magicBytes.length + LEN_PREFIX_BYTES] === 0x5a;
        return ok;
      } catch {
        return false;
      }
    })();
  }
  rawTransport = await rawProbeInFlight;
  rawProbeInFlight = null;
  return rawTransport;
}

/** Test/bench-only: forget the probe result so the next call re-decides. */
export function resetRawTransportDecision(): void {
  rawTransport = null;
  rawProbeInFlight = null;
}

/**
 * The exact string `rust_pixels_write_region_raw` rejects a JSON body
 * with (paint_parity_raw.rs). `scripts/ipc-protocol.js` latches
 * `customProtocolIpcFailed` permanently on the first failed `ipc:`
 * fetch, so the webview's IPC branch can flip to postMessage
 * mid-session - after a successful probe - and every later invoke
 * then delivers a JSON body.
 */
const RAW_REQUIRES_BINARY =
  "raw write_region requires a binary payload, not JSON";

/** The base64 arm: the existing fallback path, shared by the initial
 *  decision and the mid-session retry below. */
async function writeRegionPixelsBase64(
  target: RegionWriteTarget,
  options?: {
    sliced?: boolean;
    fallbackEncode?: (bytes: ArrayLike<number>) => Promise<string>;
  },
): Promise<RegionWriteReply> {
  const rgbaBase64 = options?.fallbackEncode
    ? await options.fallbackEncode(target.rgba)
    : encodePixelBytes(target.rgba);
  const wire = await pixelInvoke("rust_pixels_write_region", {
    docId: target.docId,
    layerId: target.layerId,
    x: target.x,
    y: target.y,
    w: target.w,
    h: target.h,
    rgbaBase64,
  });
  return options?.sliced
    ? decodeRustBytesSliced<RegionWriteReply>(wire)
    : decodeRustBytes<RegionWriteReply>(wire);
}

/**
 * One canonical region write, over whichever transport the probe selected.
 *
 * Both arms are census-routed: the write is a state change, and the ordering
 * oracle must see it whichever command carried it.
 *
 * `sliced` is the frame-pacing contract of the base64 arm. The brush commit must
 * never run a whole region's base64 encode AND decode in one slice (measured at
 * 4096^2: encode 19.5 ms, decode 16.2 ms), so it hands down the sliced pair and
 * the reply is decoded through the yielding reader. The raw arm needs neither:
 * it copies opaque bytes and parses a short header, which is the whole point of
 * the transport split.
 *
 * Mid-session branch flip: if the raw invoke rejects with exactly the
 * binary-payload-required error, the IPC branch may have flipped after the
 * probe ran. The write re-probes ONCE and, if the raw transport is gone,
 * retries ONCE on the base64 arm. Exactly one re-probe and one retry, no
 * loop; every other rejection propagates unchanged.
 */
export async function writeRegionPixels(
  target: RegionWriteTarget,
  options?: {
    sliced?: boolean;
    fallbackEncode?: (bytes: ArrayLike<number>) => Promise<string>;
  },
): Promise<RegionWriteReply> {
  const raw = await probeRawTransport();
  if (raw) {
    const { body, headers } = encodeRawRegionRequest(target);
    try {
      const reply = await pixelInvoke("rust_pixels_write_region_raw", body, { headers });
      return decodeRawRegionReply(reply);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      if (message.includes(RAW_REQUIRES_BINARY)) {
        resetRawTransportDecision();
        if (!(await probeRawTransport())) {
          return writeRegionPixelsBase64(target, options);
        }
      }
      throw err;
    }
  }
  return writeRegionPixelsBase64(target, options);
}
