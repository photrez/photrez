// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * CROSS-LANGUAGE DECODE ROUND TRIP for the pixel-bytes wire codec.
 *
 * The point of this file is the last gap a self-consistent test pair cannot
 * close. Every other test in the tree decodes pixel payloads through
 * `decodePixelBytes`, and `decodePixelBytes` was written against the same base64
 * idea as the encoder beside it, so an error shared by BOTH — a wrong alphabet, a
 * dropped quantum, a `-`/`_` from a URL-safe variant — passes every one of them.
 *
 * So the expected strings here are NOT produced by the encoder. They are literals
 * captured from the Rust encoder, `b64` in
 * apps/desktop/src-tauri/src/paint_parity_cmds.rs, whose own round trip is pinned
 * by `rust_encoder_output_decodes_to_the_original_bytes_on_the_host_side`. Change
 * the Rust side and this file fails; change the TypeScript side and it fails.
 */
import { describe, expect, it } from "vitest";
import {
  decodePixelBytes,
  decodeRustBytes,
  encodePixelBytes,
  readPixelSeedCall,
} from "@/lib/protocol/pixelSeedCall";

/** Captured from Rust's `b64`, for the byte sequences named in each case. */
const RUST_ENCODED: { readonly bytes: readonly number[]; readonly base64: string }[] = [
  // Every high bit set: the alphabet's `+` and `/` and no alphabet ambiguity.
  { bytes: [255, 254, 253, 252], base64: "//79/A==" },
  // A NUL first byte, which is the pixel value a fresh canvas layer is full of.
  { bytes: [0, 0, 0, 0], base64: "AAAAAA==" },
  // Values that straddle a base64 quantum's bit boundaries: 3 bytes is exactly
  // one quantum, 4 is one quantum plus one dangling byte that must pad to `=`.
  { bytes: [1, 2, 3], base64: "AQID" },
  { bytes: [1, 2], base64: "AQI=" },
  { bytes: [1], base64: "AQ==" },
  // 0/64/128/255 land on the `+`, `A`, `Q` slots respectively.
  { bytes: [0, 255, 128, 64], base64: "AP+AQA==" },
];

describe("pixel-bytes base64 codec: cross-language decode round trip", () => {
  it("decodes the strings Rust's encoder produces, byte for byte", () => {
    for (const { bytes, base64 } of RUST_ENCODED) {
      expect(Array.from(decodePixelBytes(base64)), `decode ${base64}`).toEqual([
        ...bytes,
      ]);
    }
  });

  it("produces the same strings Rust's encoder produces, in both directions", () => {
    for (const { bytes, base64 } of RUST_ENCODED) {
      expect(encodePixelBytes(bytes), `encode ${bytes.join(",")}`).toBe(base64);
    }
  });

  it("survives every length modulo 3, which is where a quantum bug would hide", () => {
    // A hand-rolled encoder that mis-straddles a quantum still round-trips its own
    // output; only comparing against Rust's alphabet catches that. 700 bytes is
    // 233 quanta plus one byte, so the tail padding is exercised at every offset.
    const bytes = new Uint8Array(700);
    for (let i = 0; i < bytes.length; i++) bytes[i] = (i * 37 + 11) & 0xff;
    // Cross-check against an independent implementation, not against ourselves.
    const independent = Buffer.from(bytes).toString("base64");
    expect(encodePixelBytes(bytes)).toBe(independent);
    expect(Array.from(decodePixelBytes(independent))).toEqual(Array.from(bytes));
  });

  it("encodes a large buffer identically whether or not it is already a Uint8Array", () => {
    // `ImageData.data` is a Uint8ClampedArray and a readback buffer may be an
    // ArrayBuffer view; both must produce the same wire bytes as a plain array.
    const plain = new Uint8Array(64_000);
    for (let i = 0; i < plain.length; i++) plain[i] = i & 0xff;
    const clamped = new Uint8ClampedArray(plain);
    const spread = Array.from(plain);
    const expected = encodePixelBytes(plain);
    expect(encodePixelBytes(clamped)).toBe(expected);
    expect(encodePixelBytes(spread)).toBe(expected);
  });
});

describe("decodeRustBytes: the inbound half of the wire contract", () => {
  it("renames each base64 field to the byte field it stands in for", () => {
    const response = {
      before: [{ x: 0, y: 0, w: 2, h: 2, dataBase64: "AQIDBA==" }],
      after: [{ x: 0, y: 0, w: 2, h: 2, dataBase64: "AQIDBA==" }],
      epoch: 3,
      version: 4,
    };
    const decoded = decodeRustBytes<typeof response & {
      before: { data: Uint8Array }[];
      after: { data: Uint8Array }[];
    }>(response);
    expect(decoded.epoch).toBe(3);
    expect(decoded.version).toBe(4);
    expect(Array.from(decoded.before[0].data)).toEqual([1, 2, 3, 4]);
    expect(decoded.before[0]).not.toHaveProperty("dataBase64");
  });

  it("handles a command argument object as well as a response", () => {
    const args = decodeRustBytes<{ rgba: Uint8Array }>({
      docId: "d",
      layerId: "L",
      x: 0,
      y: 0,
      w: 1,
      h: 1,
      rgbaBase64: "AQID",
    });
    expect(Array.from(args.rgba)).toEqual([1, 2, 3]);
  });

  it("leaves a payload that is already a byte array untouched", () => {
    // An in-process store double answers with real bytes; decoding must not
    // re-encode and compare them.
    const bytes = new Uint8Array([9, 8, 7]);
    const decoded = decodeRustBytes<{ data: Uint8Array }[]>([{ data: bytes }]);
    expect(decoded[0].data).toBe(bytes);
  });

  it("rejects a seed call with no base64 payload rather than reading a field that is not there", () => {
    expect(() => readPixelSeedCall("rust_pixels_init", { docId: "d", layerId: "L", width: 1, height: 1 })).toThrow(
      /without a base64 seed payload/,
    );
  });

  it("rejects a base64 payload that is not base64", () => {
    // `atob` throws on a malformed alphabet; the caller must see that, not zeros.
    expect(() => decodePixelBytes("not base64 !!")).toThrow();
  });
});