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
  owed_anchor_bytes: 0,
  owed_anchor_layer_count: 0,
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

  // A seeded, never-committed document reports a tile graph of zero, so
  // `total_bytes` alone reads as a complete footprint when the store would hold
  // twice that once the first commit lands. These three pin that the caller
  // cannot be handed the half answer: the figures must be present, must be
  // finite, and must not contradict each other.
  it("carries the tile graph a seeded layer still owes, so an empty graph is not read as complete", async () => {
    const owed = {
      ...BYTES,
      row_major_bytes: 1_048_576,
      tile_graph: {
        ...BYTES.tile_graph,
        total_bytes: 0,
        shared_bytes: 0,
        private_bytes: 0,
        tile_count: 0,
        state_count: 0,
        tile_reference_count: 0,
      },
      total_bytes: 1_048_576,
      owed_anchor_bytes: 1_048_576,
      owed_anchor_layer_count: 1,
    };
    invokeMock.mockResolvedValueOnce(owed);
    const read = await getPixelStoreBytes("doc-1");
    expect(read.tile_graph.total_bytes).toBe(0);
    expect(read.owed_anchor_layer_count).toBe(1);
    expect(read.owed_anchor_bytes).toBe(1_048_576);
  });

  it("rejects a response that omits the owed-anchor figures rather than defaulting them to zero", async () => {
    const { owed_anchor_bytes: _dropped, ...withoutOwed } = BYTES;
    invokeMock.mockResolvedValueOnce(withoutOwed);
    await expect(getPixelStoreBytes("doc-1")).rejects.toThrow(
      "rust_pixels_store_bytes: malformed byte report",
    );
  });

  it("rejects a non-finite owed-anchor figure", async () => {
    invokeMock.mockResolvedValueOnce({ ...BYTES, owed_anchor_bytes: Number.NaN });
    await expect(getPixelStoreBytes("doc-1")).rejects.toThrow(
      "rust_pixels_store_bytes: malformed byte report",
    );
  });

  // A 0x0 layer owes width*height*4 = 0 bytes and STILL counts as an unbuilt
  // layer, so "layers owed but zero bytes" is a REAL Rust response - pinned in
  // `a_zero_dimension_unbuilt_layer_owes_no_bytes_but_still_counts`. The guard
  // must not reject it, and the direction it does take is the one Rust cannot
  // produce: a non-zero byte figure implies at least one layer was counted.
  it("accepts a real 0x0 document: one owed layer, zero owed bytes", async () => {
    invokeMock.mockResolvedValueOnce({
      ...BYTES,
      row_major_bytes: 0,
      tile_graph: {
        ...BYTES.tile_graph,
        total_bytes: 0,
        shared_bytes: 0,
        private_bytes: 0,
        tile_count: 0,
        state_count: 0,
        tile_reference_count: 0,
      },
      total_bytes: 0,
      owed_anchor_bytes: 0,
      owed_anchor_layer_count: 1,
    });
    const read = await getPixelStoreBytes("doc-1");
    expect(read.owed_anchor_layer_count).toBe(1);
    expect(read.owed_anchor_bytes).toBe(0);
  });

  it("rejects owed bytes with nothing owing them (the two figures arrived swapped)", async () => {
    invokeMock.mockResolvedValueOnce({ ...BYTES, owed_anchor_bytes: 4096, owed_anchor_layer_count: 0 });
    await expect(getPixelStoreBytes("doc-1")).rejects.toThrow(
      "owed_anchor_bytes is 4096 but owed_anchor_layer_count is 0",
    );
  });

  it("rejects more owed layers than the document has layers", async () => {
    invokeMock.mockResolvedValueOnce({ ...BYTES, layer_count: 1, owed_anchor_layer_count: 3 });
    await expect(getPixelStoreBytes("doc-1")).rejects.toThrow(
      "owed_anchor_layer_count 3 exceeds layer_count 1",
    );
  });
});
