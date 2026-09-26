// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Caller depth contract for `rust_pixels_history_depth`:
// every returned field is mapped explicitly, and an IPC failure is a REJECTED
// promise - never a zero-filled depth struct, because `total_depth: 0` from a
// failed call would read as "the document has no history".
//
// Mock fidelity: Tauri v2 invoke() REJECTS with the bare string carried by a
// Rust `Err(String)`, so the failure case rejects with "E_RUST: boom", not an
// Error instance and not a resolved `{ ok: false }` body.

import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { getPixelHistoryDepth } from "../pixelHistoryDepth";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

const DEPTH = {
  total_depth: 3,
  undo_depth: 2,
  redo_depth: 1,
  affected_layer_ids: ["L2", "L1"],
};

beforeEach(() => {
  invokeMock.mockReset();
});

describe("getPixelHistoryDepth", () => {
  it("maps every returned field from the probe response", async () => {
    invokeMock.mockResolvedValueOnce(DEPTH);
    await expect(getPixelHistoryDepth("doc-1")).resolves.toEqual(DEPTH);
    expect(invokeMock).toHaveBeenCalledWith("rust_pixels_history_depth", { docId: "doc-1" });
  });

  it("rejects on a bare-string Rust failure instead of resolving zero depths", async () => {
    invokeMock.mockRejectedValueOnce("E_RUST: boom");
    await expect(getPixelHistoryDepth("doc-1")).rejects.toBe("E_RUST: boom");
  });

  it("never yields depth zero from a malformed response", async () => {
    invokeMock.mockResolvedValueOnce({ total_depth: 0 });
    await expect(getPixelHistoryDepth("doc-1")).rejects.toThrow(
      "rust_pixels_history_depth: malformed depth response",
    );
  });

  it("rejects a null response rather than reporting an empty history", async () => {
    invokeMock.mockResolvedValueOnce(null);
    await expect(getPixelHistoryDepth("doc-1")).rejects.toThrow(
      "rust_pixels_history_depth: malformed depth response",
    );
  });

  it("rejects a non-numeric total_depth rather than coercing it to zero", async () => {
    invokeMock.mockResolvedValueOnce({ ...DEPTH, total_depth: "3" });
    await expect(getPixelHistoryDepth("doc-1")).rejects.toThrow(
      "rust_pixels_history_depth: malformed depth response",
    );
  });

  it("lets an unregistered command reject (backend too old for this probe)", async () => {
    invokeMock.mockRejectedValueOnce(
      "command rust_pixels_history_depth not found",
    );
    await expect(getPixelHistoryDepth("doc-1")).rejects.toThrow(
      "command rust_pixels_history_depth not found",
    );
  });
});
