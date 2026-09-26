// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Native-boundary rejection contract for `invokeApi` (exposed for tests only as
// `__invokeApiRaw`). Assertions are on the MESSAGE STRING, never on the
// rejection type: the throw type at native.ts's catch is locked, so what must be
// proven is that no shape reaches a caller as `Unknown IPC error`,
// `[object Object]`, or an empty message.
//
// Mock fidelity: Tauri v2 invoke() REJECTS with the bare string carried by a
// Rust `Result<_, String>`. A `{ ok: false }` body is a RESOLVED value (that is
// what `asError` converts), not a rejection.

import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

// Imported lazily so the mock above is installed before the module resolves it.
async function raw() {
  const mod = await import("@/tauri/native");
  expect(typeof mod.__invokeApiRaw, "test-only export must exist").toBe("function");
  return mod.__invokeApiRaw;
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe("__invokeApiRaw rejection messages", () => {
  it("surfaces a bare-string Rust rejection as its own message", async () => {
    invokeMock.mockRejectedValueOnce("E_RUST: rust ipc unavailable");
    const fn = await raw();
    await expect(fn("any_command")).rejects.toThrow("E_RUST: rust ipc unavailable");
  });

  it("surfaces an object-envelope rejection as 'CODE: message'", async () => {
    invokeMock.mockRejectedValueOnce({
      error: { code: "E_RUST", message: "boom", details: null },
    });
    const fn = await raw();
    await expect(fn("any_command")).rejects.toThrow("E_RUST: boom");
  });

  it("surfaces a bare { code, message } rejection as 'CODE: message'", async () => {
    invokeMock.mockRejectedValueOnce({ code: "E_SAVE", message: "disk full" });
    const fn = await raw();
    await expect(fn("any_command")).rejects.toThrow("E_SAVE: disk full");
  });

  it("never returns 'Unknown IPC error', '[object Object]', or an empty message", async () => {
    const cases: unknown[] = [
      "E_RUST: bare",
      { error: { code: "E_A", message: "a", details: null } },
      { code: "E_B", message: "b" },
      { message: "just a message" },
    ];
    const fn = await raw();
    for (const rejection of cases) {
      invokeMock.mockRejectedValueOnce(rejection);
      let message = "";
      try {
        await fn("any_command");
      } catch (err) {
        message = err instanceof Error ? err.message : String(err);
      }
      expect(message, `for ${JSON.stringify(rejection)}`).not.toBe("Unknown IPC error");
      expect(message, `for ${JSON.stringify(rejection)}`).not.toContain("[object Object]");
      expect(message.length, `for ${JSON.stringify(rejection)}`).toBeGreaterThan(0);
    }
  });
});

describe("__invokeApiRaw resolution contract (unchanged by the throw-type audit)", () => {
  it("rejects a resolved { ok: false } body with '<code>: <message>' via asError", async () => {
    invokeMock.mockResolvedValueOnce({
      ok: false,
      contract_version: "1",
      error: { code: "E_LOAD", message: "unreadable", details: null },
    });
    const fn = await raw();
    await expect(fn("any_command")).rejects.toThrow("E_LOAD: unreadable");
  });

  it("resolves a success envelope with its data", async () => {
    invokeMock.mockResolvedValueOnce({
      ok: true,
      contract_version: "1",
      data: { value: 7 },
    });
    const fn = await raw();
    await expect(fn("any_command")).resolves.toEqual({
      ok: true,
      contract_version: "1",
      data: { value: 7 },
    });
  });
});
