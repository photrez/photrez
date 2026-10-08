// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * Transport tests for the canonical region write.
 *
 * RED-FIRST PROOF: the wiring case below parses `useBrushOverlay.ts` and requires
 * the brush commit to route through the shared dispatcher. Before the commit that
 * introduced `paintRegionIpc.ts`, useBrushOverlay called `pixelInvoke(
 * "rust_pixels_write_region", ...)` directly - that case FAILED, which is why the
 * call site moved. The pure unit cases pin the wire format itself: a reply that
 * crosses the boundary is validated field by field, and every malformed shape
 * (foreign magic, truncated header, trailing bytes, bad tile geometry) must throw
 * rather than yield short tiles that would enter history as silent pixel loss.
 */

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { beforeEach, describe, expect, it, vi } from "vitest";

const hoist = vi.hoisted(() => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: hoist.invoke }));

import {
  RAW_REGION_MAGIC,
  decodeRawRegionReply,
  encodeRawRegionRequest,
  probeRawTransport,
  resetRawTransportDecision,
  writeRegionPixels,
} from "../paintRegionIpc";

const MAGIC_LEN = RAW_REGION_MAGIC.length;
const LEN_PREFIX = 4;

/** The exact reply `rust_pixels_raw_echo` builds: magic | len=1 | the probe byte. */
function echoFrame(probeByte = 0x5a): Uint8Array {
  const out = new Uint8Array(MAGIC_LEN + LEN_PREFIX + 1);
  out.set(new TextEncoder().encode(RAW_REGION_MAGIC), 0);
  new DataView(out.buffer).setUint32(MAGIC_LEN, 1, true);
  out[MAGIC_LEN + LEN_PREFIX] = probeByte;
  return out;
}

function frame(header: unknown, tileBytes: Uint8Array[]): Uint8Array {
  const headerBytes = new TextEncoder().encode(JSON.stringify(header));
  const total =
    MAGIC_LEN + LEN_PREFIX + headerBytes.length + tileBytes.reduce((n, t) => n + t.length, 0);
  const out = new Uint8Array(total);
  out.set(new TextEncoder().encode(RAW_REGION_MAGIC), 0);
  new DataView(out.buffer).setUint32(MAGIC_LEN, headerBytes.length, true);
  out.set(headerBytes, MAGIC_LEN + LEN_PREFIX);
  let off = MAGIC_LEN + LEN_PREFIX + headerBytes.length;
  for (const t of tileBytes) {
    out.set(t, off);
    off += t.length;
  }
  return out;
}

describe("region write transport: request frame", () => {
  it("sends the rgba bytes as the ENTIRE body, with the metadata only in the header", () => {
    const rgba = new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8]);
    const { body, headers } = encodeRawRegionRequest({
      docId: "doc-1",
      layerId: "layer-9",
      x: 3,
      y: 4,
      w: 2,
      h: 1,
      rgba,
    });
    // `rust_pixels_write_region_raw` re-assembles its own frame from the
    // `X-Photrez-Region` header plus THIS body, so the body must be the pixels
    // and nothing else. A magic/metadata prefix here is not ignored - it is
    // counted as pixel data and the write is rejected. That is not hypothetical:
    // it was measured in the real release app on the first run of this
    // transport, as `rgba byte count mismatch: body carries 2707444, header
    // declares 2707328` (the 116 extra bytes were the duplicated metadata).
    expect(body.length).toBe(rgba.length);
    expect(Array.from(body)).toEqual([1, 2, 3, 4, 5, 6, 7, 8]);
    expect(JSON.parse(headers["X-Photrez-Region"])).toEqual({
      docId: "doc-1",
      layerId: "layer-9",
      x: 3,
      y: 4,
      w: 2,
      h: 1,
      byteLen: 8,
    });
  });

  it("rejects an empty identity, a non-integer region and a byte count that does not match w*h*4", () => {
    const base = { docId: "d", layerId: "l", x: 0, y: 0, w: 2, h: 2 };
    expect(() => encodeRawRegionRequest({ ...base, docId: "", rgba: new Uint8Array(16) })).toThrow(
      /non-empty docId/,
    );
    expect(() =>
      encodeRawRegionRequest({ ...base, x: 1.5, rgba: new Uint8Array(16) }),
    ).toThrow(/must be an integer/);
    expect(() => encodeRawRegionRequest({ ...base, rgba: new Uint8Array(12) })).toThrow(
      /expected 16 for 2x2/,
    );
  });
});

describe("region write transport: reply frame", () => {
  it("slices each tile's block at w*h*4 and reports epoch/version", () => {
    const a = new Uint8Array([1, 2, 3, 4]);
    const b = new Uint8Array([9, 8, 7, 6]);
    const reply = decodeRawRegionReply(
      frame({ epoch: 7, version: 11, tiles: [{ x: 0, y: 0, w: 1, h: 1 }, { x: 5, y: 6, w: 1, h: 1 }] }, [a, b]),
    );
    expect(reply.epoch).toBe(7);
    expect(reply.version).toBe(11);
    expect(reply.after).toHaveLength(2);
    expect(Array.from(reply.after[0].data)).toEqual([1, 2, 3, 4]);
    expect(Array.from(reply.after[1].data)).toEqual([9, 8, 7, 6]);
    expect(reply.after[1]).toMatchObject({ x: 5, y: 6, w: 1, h: 1 });
  });

  it("accepts a real ArrayBuffer, which is what the runtime delivers", () => {
    const bytes = frame({ epoch: 1, version: 1, tiles: [] }, []);
    const buffer = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
    expect(decodeRawRegionReply(buffer).after).toEqual([]);
  });

  it("throws on a foreign magic, a truncated header and trailing bytes", () => {
    const foreign = frame({ epoch: 1, version: 1, tiles: [] }, []);
    foreign[0] = 0x00;
    expect(() => decodeRawRegionReply(foreign)).toThrow(/foreign magic/);

    const short = frame({ epoch: 1, version: 1, tiles: [] }, []).subarray(0, MAGIC_LEN + 2);
    expect(() => decodeRawRegionReply(short)).toThrow(/too short/);

    const truncatedHeader = frame({ epoch: 1, version: 1, tiles: [] }, []);
    new DataView(truncatedHeader.buffer).setUint32(MAGIC_LEN, 4096, true);
    expect(() => decodeRawRegionReply(truncatedHeader)).toThrow(/runs past/);

    const withTail = new Uint8Array([...frame({ epoch: 1, version: 1, tiles: [] }, []), 0xde, 0xad]);
    expect(() => decodeRawRegionReply(withTail)).toThrow(/trailing bytes/);
  });

  it("throws on tile geometry that does not fit the buffer, instead of returning short tiles", () => {
    const short = frame({ epoch: 1, version: 1, tiles: [{ x: 0, y: 0, w: 4, h: 4 }] }, [new Uint8Array(2)]);
    expect(() => decodeRawRegionReply(short)).toThrow(/needs 64 bytes/);
  });

  it("throws on non-numeric epoch/version and a missing tiles list", () => {
    expect(() => decodeRawRegionReply(frame({ tiles: [] }, []))).toThrow(/epoch/);
    expect(() => decodeRawRegionReply(frame({ epoch: 1, version: 1 }, []))).toThrow(/tiles/);
  });
});

describe("region write transport: selection and fallback", () => {
  beforeEach(() => {
    hoist.invoke.mockReset();
    resetRawTransportDecision();
    localStorage.removeItem("photrez.rawPixelTransport");
  });

  it("selects raw when the echo probe round-trips a binary frame", async () => {
    // The probe only spends an IPC when the app is actually in the Tauri
    // runtime, so this case marks the runtime the way Tauri does.
    (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    try {
      hoist.invoke.mockImplementation(async (cmd: string) => {
        if (cmd === "rust_pixels_raw_echo") return echoFrame();
        return undefined;
      });
      await expect(probeRawTransport()).resolves.toBe(true);
    } finally {
      delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    }
  });

  it("decides NO when the runtime answers the probe with something that is not a binary frame", async () => {
    (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    try {
      hoist.invoke.mockImplementation(async (cmd: string) => {
        if (cmd === "rust_pixels_raw_echo") return { ok: true };
        return undefined;
      });
      await expect(probeRawTransport()).resolves.toBe(false);
    } finally {
      delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    }
  });

  it("decides NO when the echo reply carries a different probe byte", async () => {
    (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    try {
      hoist.invoke.mockImplementation(async (cmd: string) => {
        if (cmd === "rust_pixels_raw_echo") return echoFrame(0x11);
        return undefined;
      });
      await expect(probeRawTransport()).resolves.toBe(false);
    } finally {
      delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    }
  });

  it("falls back to the base64 command when the probe gives nothing usable", async () => {
    // jsdom has no Tauri runtime, so the probe short-circuits to false without
    // ever invoking: production cannot pay a failed IPC on every session start.
    const sentBase64: { cmd: string; args: unknown }[] = [];
    hoist.invoke.mockImplementation(async (cmd: string, args: unknown) => {
      sentBase64.push({ cmd, args });
      if (cmd === "rust_pixels_write_region") {
        return { after: [{ x: 0, y: 0, w: 1, h: 1, dataBase64: btoa("\u0001\u0002\u0003\u0004") }], epoch: 3, version: 4 };
      }
      throw new Error(`unexpected command ${cmd}`);
    });
    const reply = await writeRegionPixels({
      docId: "d",
      layerId: "l",
      x: 0,
      y: 0,
      w: 1,
      h: 1,
      rgba: new Uint8Array([1, 2, 3, 4]),
    });
    expect(sentBase64.map((c) => c.cmd)).toEqual(["rust_pixels_write_region"]);
    expect(reply.epoch).toBe(3);
    expect(Array.from(reply.after[0].data)).toEqual([1, 2, 3, 4]);
  });

  it("sends the raw command with a byte body and the metadata header when forced on", async () => {
    localStorage.setItem("photrez.rawPixelTransport", "1");
    const calls: { cmd: string; args: unknown; options?: unknown }[] = [];
    hoist.invoke.mockImplementation(async (cmd: string, args: unknown, options?: unknown) => {
      calls.push({ cmd, args, options });
      if (cmd === "rust_pixels_write_region_raw") {
        return frame({ epoch: 9, version: 12, tiles: [{ x: 0, y: 0, w: 1, h: 1 }] }, [
          new Uint8Array([5, 6, 7, 8]),
        ]);
      }
      throw new Error(`unexpected command ${cmd}`);
    });
    const reply = await writeRegionPixels({
      docId: "d",
      layerId: "l",
      x: 0,
      y: 0,
      w: 1,
      h: 1,
      rgba: new Uint8Array([5, 6, 7, 8]),
    });
    expect(calls).toHaveLength(1);
    expect(calls[0].cmd).toBe("rust_pixels_write_region_raw");
    expect(calls[0].args).toBeInstanceOf(Uint8Array);
    expect(calls[0].options).toMatchObject({
      headers: { "X-Photrez-Region": expect.stringContaining("\"docId\":\"d\"") },
    });
    expect(reply.epoch).toBe(9);
    expect(reply.version).toBe(12);
    expect(Array.from(reply.after[0].data)).toEqual([5, 6, 7, 8]);
  });

  it("re-probes once and retries on the base64 arm when the raw command rejects a JSON body", async () => {
    // Mid-session branch flip: the first probe proves the raw
    // transport, the IPC branch then flips (scripts/ipc-protocol.js
    // latches customProtocolIpcFailed permanently), the raw invoke
    // rejects with the native binary-payload error, the re-probe
    // fails, and the write lands on the base64 command - exactly
    // one re-probe (echo call 2) and one base64 retry.
    (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    try {
      let echoCalls = 0;
      let rawCalls = 0;
      const sentBase64: unknown[] = [];
      hoist.invoke.mockImplementation(async (cmd: string, args: unknown) => {
        if (cmd === "rust_pixels_raw_echo") {
          echoCalls += 1;
          if (echoCalls === 1) return echoFrame();
          throw new Error("echo unavailable after branch flip");
        }
        if (cmd === "rust_pixels_write_region_raw") {
          rawCalls += 1;
          throw new Error("raw write_region requires a binary payload, not JSON");
        }
        if (cmd === "rust_pixels_write_region") {
          sentBase64.push(args);
          return {
            after: [{ x: 0, y: 0, w: 1, h: 1, dataBase64: btoa(String.fromCharCode(1, 2, 3, 4)) }],
            epoch: 5,
            version: 6,
          };
        }
        throw new Error(`unexpected command ${cmd}`);
      });
      const reply = await writeRegionPixels({
        docId: "d",
        layerId: "l",
        x: 0,
        y: 0,
        w: 1,
        h: 1,
        rgba: new Uint8Array([1, 2, 3, 4]),
      });
      expect(rawCalls).toBe(1);
      expect(echoCalls).toBe(2);
      expect(sentBase64).toHaveLength(1);
      expect(reply.epoch).toBe(5);
      expect(reply.version).toBe(6);
      expect(Array.from(reply.after[0].data)).toEqual([1, 2, 3, 4]);
    } finally {
      delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    }
  });

  it("propagates a raw rejection that is not the binary-payload error", async () => {
    // Falsifiability guard for the retry above: only the exact
    // native binary-payload rejection may trigger a re-probe and a
    // base64 retry. Any other rejection must reach the caller with
    // no second invoke.
    (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    try {
      let base64Calls = 0;
      hoist.invoke.mockImplementation(async (cmd: string) => {
        if (cmd === "rust_pixels_raw_echo") return echoFrame();
        if (cmd === "rust_pixels_write_region_raw") {
          throw new Error("layer not initialized: l");
        }
        if (cmd === "rust_pixels_write_region") {
          base64Calls += 1;
          return { after: [], epoch: 1, version: 1 };
        }
        throw new Error(`unexpected command ${cmd}`);
      });
      await expect(
        writeRegionPixels({
          docId: "d",
          layerId: "l",
          x: 0,
          y: 0,
          w: 1,
          h: 1,
          rgba: new Uint8Array([1, 2, 3, 4]),
        }),
      ).rejects.toThrow(/layer not initialized/);
      expect(base64Calls).toBe(0);
    } finally {
      delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    }
  });
});

describe("region write transport: production wiring", () => {
  const brushOverlay = resolve(__dirname, "../../../components/editor/useBrushOverlay.ts");

  it("the brush commit routes through the shared region-write dispatcher", () => {
    const source = readFileSync(brushOverlay, "utf8");
    // RED before the migration: this file called pixelInvoke(
    // "rust_pixels_write_region", ...) itself. It must now go through the one
    // dispatcher that owns the transport choice, so a future transport change
    // cannot miss the brush.
    expect(source).toContain("writeRegionPixels(");
  });
});
