// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Caller contract for `rust_pixels_store_bytes`: every returned field is mapped
// explicitly, and an IPC failure is a REJECTED promise - never a zero-byte
// report, because `total_bytes: 0` from a failed call would read as "this
// document is free".
//
// Mock fidelity: Tauri v2 invoke() REJECTS with the bare string carried by a Rust
// `Err(String)`, so the failure case rejects with "E_RUST: boom", not an Error
// instance and not a resolved `{ ok: false }` body.

import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { getPixelStoreBytes } from "../pixelStoreBytes";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

const BYTES = {
  layer_count: 1,
  row_major_bytes: 64,
  tile_graph: {
    total_bytes: 128,
    shared_bytes: 0,
    private_bytes: 128,
    tile_count: 2,
    state_count: 2,
    tile_reference_count: 2,
  },
  total_bytes: 192,
};

beforeEach(() => {
  invokeMock.mockReset();
});

describe("getPixelStoreBytes", () => {
  it("maps every returned field from the probe response", async () => {
    invokeMock.mockResolvedValueOnce(BYTES);
    await expect(getPixelStoreBytes("doc-1")).resolves.toEqual(BYTES);
    expect(invokeMock).toHaveBeenCalledWith("rust_pixels_store_bytes", { docId: "doc-1" });
  });

  it("rejects on a bare-string Rust failure instead of resolving zero bytes", async () => {
    invokeMock.mockRejectedValueOnce("E_RUST: boom");
    await expect(getPixelStoreBytes("doc-1")).rejects.toBe("E_RUST: boom");
  });

  it("rejects a null response rather than reporting a free document", async () => {
    invokeMock.mockResolvedValueOnce(null);
    await expect(getPixelStoreBytes("doc-1")).rejects.toThrow(
      "rust_pixels_store_bytes: malformed byte report",
    );
  });

  it("never yields zero bytes from a malformed response", async () => {
    invokeMock.mockResolvedValueOnce({ layer_count: 1 });
    await expect(getPixelStoreBytes("doc-1")).rejects.toThrow(
      "rust_pixels_store_bytes: malformed byte report",
    );
  });

  it("rejects a non-numeric total_bytes rather than coercing it to zero", async () => {
    invokeMock.mockResolvedValueOnce({ ...BYTES, total_bytes: "192" });
    await expect(getPixelStoreBytes("doc-1")).rejects.toThrow(
      "rust_pixels_store_bytes: malformed byte report",
    );
  });

  it("rejects a missing tile_graph rather than defaulting it to an empty one", async () => {
    invokeMock.mockResolvedValueOnce({ ...BYTES, tile_graph: undefined });
    await expect(getPixelStoreBytes("doc-1")).rejects.toThrow(
      "rust_pixels_store_bytes: malformed byte report",
    );
  });

  // A Rust field rename must redden this wrapper instead of arriving as
  // `undefined` and reading as 0: the wrapper reads the snake_case Rust names
  // deliberately, so this is the assertion that keeps the two sides bound. The
  // renamed field REPLACES the real one - adding a stray key would leave the
  // response well-formed and prove nothing.
  it("rejects a renamed tile_graph field rather than reading it as zero", async () => {
    const { tile_count: _renamed, ...withoutCount } = BYTES.tile_graph;
    invokeMock.mockResolvedValueOnce({
      ...BYTES,
      tile_graph: { ...withoutCount, block_count: 2 },
    });
    await expect(getPixelStoreBytes("doc-1")).rejects.toThrow(
      "rust_pixels_store_bytes: malformed byte report",
    );
  });

  it("rejects a non-finite byte figure rather than passing NaN through", async () => {
    invokeMock.mockResolvedValueOnce({
      ...BYTES,
      tile_graph: { ...BYTES.tile_graph, shared_bytes: null },
    });
    await expect(getPixelStoreBytes("doc-1")).rejects.toThrow(
      "rust_pixels_store_bytes: malformed byte report",
    );
  });

  it("lets an unregistered command reject (backend too old for this probe)", async () => {
    invokeMock.mockRejectedValueOnce("command rust_pixels_store_bytes not found");
    await expect(getPixelStoreBytes("doc-1")).rejects.toThrow(
      "command rust_pixels_store_bytes not found",
    );
  });

  // A well-typed response can still carry crossed numbers. These two identities
  // hold by construction in Rust, so breaking one means the values were
  // transposed in transit - not that the document is in a strange state.
  it("rejects a total that is not row_major + tile_graph.total_bytes", async () => {
    invokeMock.mockResolvedValueOnce({
      ...BYTES,
      total_bytes: 999,
    });
    await expect(getPixelStoreBytes("doc-1")).rejects.toThrow(
      "total_bytes is not row_major + tile_graph.total_bytes",
    );
  });

  it("rejects a split that does not add up to tile_graph.total_bytes", async () => {
    invokeMock.mockResolvedValueOnce({
      ...BYTES,
      tile_graph: { ...BYTES.tile_graph, shared_bytes: 64, private_bytes: 1 },
    });
    await expect(getPixelStoreBytes("doc-1")).rejects.toThrow(
      "shared + private does not equal tile_graph.total_bytes",
    );
  });
});
