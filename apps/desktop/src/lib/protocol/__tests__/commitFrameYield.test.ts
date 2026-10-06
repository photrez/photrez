// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Frame-sliced pixel encode/decode: the sliced forms must be indistinguishable
 * from the unsliced ones in output, and must actually break the run.
 *
 * "Indistinguishable" is the whole risk here. A sliced encoder that drops a
 * chunk, or reorders two, produces base64 that decodes to different pixels - a
 * silent corruption with no exception anywhere. So both cases assert against the
 * unsliced function's own output rather than a hand-written expectation.
 */
import { describe, expect, it } from "vitest";
import { decodeRustBytes, encodePixelBytes } from "@/lib/protocol/pixelSeedCall";
import {
  DECODE_PAYLOADS_PER_SLICE,
  ENCODE_STEPS_PER_SLICE,
  decodeRustBytesSliced,
  encodePixelBytesSliced,
  yieldToFrame,
} from "@/lib/protocol/commitFrameYield";

/** Sizes chosen around the 24,576-byte encode stride: exact multiples, and one
 *  partial stride on either side, because the tail path is a separate loop. */
const ENCODE_SIZES = [0, 1, 3, 4, 24575, 24576, 24577, 49152, 49153, 24576 * 110 + 17];

function pattern(n: number): Uint8Array {
  const b = new Uint8Array(n);
  for (let i = 0; i < n; i++) b[i] = (i * 31 + (i >> 5)) & 0xff;
  return b;
}

function b64(n: number): string {
  const bytes = pattern(n);
  let s = "";
  for (let i = 0; i < n; i++) s += String.fromCharCode(bytes[i]);
  return btoa(s);
}

/** One 256x256 tile's worth of base64, the shape Rust really answers with. */
function tileB64(): string {
  return b64(256 * 256 * 4);
}

describe("sliced encode", () => {
  it("produces byte-identical output to the unsliced encoder at every stride boundary", async () => {
    for (const n of ENCODE_SIZES) {
      const bytes = pattern(n);
      expect(await encodePixelBytesSliced(bytes)).toBe(encodePixelBytes(bytes));
    }
  });

  it("decodes back to the exact input bytes", async () => {
    const bytes = pattern(24576 * 45 + 91);
    const encoded = await encodePixelBytesSliced(bytes);
    const binary = atob(encoded);
    expect(binary.length).toBe(bytes.length);
    for (let i = 0; i < bytes.length; i++) {
      if (binary.charCodeAt(i) !== bytes[i]) throw new Error(`byte ${i} differs`);
    }
  });

  it("yields once per slice, not once per chunk, and never after the last chunk", async () => {
    // 110 strides -> 5 full slices of 20 steps, plus a tail. The yields must be
    // 5: a yield after the final chunk would cost a frame for no work.
    const bytes = pattern(24576 * 110);
    let yields = 0;
    await encodePixelBytesSliced(bytes, async () => {
      yields++;
    });
    expect(yields).toBe(Math.floor(110 / ENCODE_STEPS_PER_SLICE));
    expect(yields).toBeGreaterThan(0);
  });

  it("does not yield for a payload smaller than one slice", async () => {
    let yields = 0;
    await encodePixelBytesSliced(pattern(24576 * 3), async () => {
      yields++;
    });
    expect(yields).toBe(0);
  });
});

describe("sliced decode", () => {
  /** The reply shape `rust_pixels_write_region` really sends. */
  function reply(tileCount: number): unknown {
    return {
      after: Array.from({ length: tileCount }, (_, i) => ({
        x: (i % 5) * 256,
        y: Math.floor(i / 5) * 256,
        w: 256,
        h: 256,
        dataBase64: b64(256 * 256 * 4),
      })),
      epoch: 7,
      version: 3,
    };
  }

  it("rewrites every payload field exactly as the unsliced decoder does", async () => {
    for (const tileCount of [0, 1, 4, 5, 13]) {
      const raw = reply(tileCount);
      expect(await decodeRustBytesSliced(raw)).toEqual(decodeRustBytes(raw));
    }
  });

  it("leaves a reply with no payload fields untouched and yields nothing", async () => {
    const raw = { after: [{ x: 0, y: 0, w: 4, h: 4, data: [1, 2, 3, 4] }], epoch: 2, version: 1 };
    let yields = 0;
    const out = await decodeRustBytesSliced<typeof raw>(raw, async () => {
      yields++;
    });
    expect(out).toEqual(raw);
    // The tile's `data` is already a byte array in-process, so nothing is
    // decoded and there is nothing expensive to slice.
    expect(yields).toBe(0);
  });

  it("yields once per slice of decoded payloads", async () => {
    let yields = 0;
    await decodeRustBytesSliced(reply(13), async () => {
      yields++;
    });
    expect(yields).toBe(Math.floor(13 / DECODE_PAYLOADS_PER_SLICE));
    expect(yields).toBeGreaterThan(0);
  });

  it("decodes base64 to the same bytes the unsliced decoder produces", async () => {
    const raw = reply(2);
    const sliced = await decodeRustBytesSliced<{ after: { data: Uint8Array }[] }>(raw);
    const direct = decodeRustBytes<{ after: { data: Uint8Array }[] }>(raw);
    expect(Array.from(sliced.after[0].data)).toEqual(Array.from(direct.after[0].data));
    expect(sliced.after[0].data.length).toBe(256 * 256 * 4);
  });

  it("decodes every documented payload field name, nested at depth", async () => {
    const raw = {
      outer: { inner: [{ rgbaBase64: tileB64() }, { bytesBase64: tileB64() }] },
      dataBase64: tileB64(),
    };
    expect(await decodeRustBytesSliced(raw)).toEqual(decodeRustBytes(raw));
  });
});

describe("yieldToFrame", () => {
  it("resolves through requestAnimationFrame when the environment has one", async () => {
    const real = globalThis.requestAnimationFrame;
    let ran = 0;
    globalThis.requestAnimationFrame = ((cb: FrameRequestCallback) => {
      ran++;
      return real(cb);
    }) as typeof globalThis.requestAnimationFrame;
    try {
      await yieldToFrame();
      expect(ran).toBe(1);
    } finally {
      globalThis.requestAnimationFrame = real;
    }
  });

  it("falls back to a macrotask when there is no requestAnimationFrame", async () => {
    // A node-environment unit test has no rAF. The callers only need the
    // synchronous run to end, so a macrotask is the correct degradation and a
    // missing rAF must not deadlock the commit queue.
    const real = globalThis.requestAnimationFrame;
    // @ts-expect-error deliberately removing the global for the fallback path
    delete globalThis.requestAnimationFrame;
    try {
      let resolved = false;
      const p = yieldToFrame().then(() => {
        resolved = true;
      });
      expect(resolved).toBe(false);
      await p;
      expect(resolved).toBe(true);
    } finally {
      globalThis.requestAnimationFrame = real;
    }
  });
});