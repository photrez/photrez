// getVersion (bridge.ts) unit tests. getVersion is the lightweight native
// version read: it drives protocol_version_native, which returns a u64 rather
// than a serialized snapshot. Mirror the nativeClient + bridgeErrorEnvelope
// patterns.
//
// Mock fidelity (CRITICAL): Tauri v2 `invoke()` REJECTS with the bare
// `"CODE: message"` string on a Rust `Err(String)`. The mock must reject with
// that bare string (never resolve `{ok:false}`), and getVersion must surface it
// via normalizeProtocolError as an Error - this is the documented real contract.

import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "../bridge";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

// Self-contained localStorage so this test runs in the node project.
if (typeof (globalThis as { localStorage?: unknown }).localStorage === "undefined") {
  const __lsStore = new Map<string, string>();
  (globalThis as { localStorage: Storage }).localStorage = {
    getItem: (k: string) => (__lsStore.has(k) ? (__lsStore.get(k) as string) : null),
    setItem: (k: string, v: string) => {
      __lsStore.set(k, String(v));
    },
    removeItem: (k: string) => {
      __lsStore.delete(k);
    },
    clear: () => __lsStore.clear(),
    key: (i: number) => Array.from(__lsStore.keys())[i] ?? null,
    get length() {
      return __lsStore.size;
    },
  } as Storage;
}

beforeEach(() => {
  localStorage.clear();
  invokeMock.mockReset();
});

describe("bridge.getVersion", () => {
  it("returns the u64 from protocol_version_native", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "protocol_version_native") return 5;
      throw `E_UNKNOWN_COMMAND: ${cmd}`;
    });

    const v = await getVersion("docA");
    expect(v).toBe(5);
    expect(invokeMock).toHaveBeenCalledWith("protocol_version_native", { docId: "docA" });
  });

  it("error path rejects with the real bare Rust string normalized to Error", async () => {
    // Mock-fidelity: real protocol_version_native rejects with the BARE
    // "document not open: {key}" string (no E_ code prefix) on a Rust Err(String).
    invokeMock.mockRejectedValue("document not open: default");

    let caught: unknown;
    try {
      await getVersion("default");
    } catch (e) {
      caught = e;
    }
    expect(caught).toBeInstanceOf(Error);
    expect((caught as Error).message).toBe("document not open: default");
  });
});