// getVersion (bridge.ts) unit tests. getVersion is the lightweight native-
// authority version read: under native authority it drives protocol_version_native
// (returns a u64, not a serialized snapshot); on the wasm path it parses the
// snapshot's version. Mirror the nativeClient + bridgeErrorEnvelope patterns.
//
// Mock fidelity (CRITICAL): Tauri v2 `invoke()` REJECTS with the bare
// `"CODE: message"` string on a Rust `Err(String)`. The mock must reject with
// that bare string (never resolve `{ok:false}`), and getVersion must surface it
// via normalizeProtocolError as an Error - this is the documented real contract.

import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { getVersion, setProtocolWasm, isNativeAuthority, __resetNativeAuthorityForTests } from "../bridge";
import { CONTRACT_VERSION } from "../types";

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
  __resetNativeAuthorityForTests();
  setProtocolWasm(null as unknown as Parameters<typeof setProtocolWasm>[0]);
});

describe("bridge.getVersion", () => {
  it("native authority: returns the u64 from protocol_version_native", async () => {
    localStorage.setItem("photrez.facadeAuthority", "native");
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "protocol_version_native") return 5;
      throw `E_UNKNOWN_COMMAND: ${cmd}`;
    });

    const v = await getVersion("docA");
    expect(v).toBe(5);
    expect(invokeMock).toHaveBeenCalledWith("protocol_version_native", { docId: "docA" });
  });

  it("native authority: error path rejects with the real bare Rust string normalized to Error", async () => {
    localStorage.setItem("photrez.facadeAuthority", "native");
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

  it("wasm path: parses the version from the wasm snapshot", async () => {
    // native authority OFF -> wasm branch. Arm a fake wasm module.
    expect(isNativeAuthority()).toBe(false);
    setProtocolWasm({
      protocol_contract_version: () => CONTRACT_VERSION,
      protocol_apply_command: (_json: string, _docId: string) => JSON.stringify({}),
      protocol_snapshot_json: (_docId: string) =>
        JSON.stringify({ version: 9, layers: [{ id: "l" }] }),
    });

    const v = await getVersion("docB");
    expect(v).toBe(9);
  });

  it("wasm path with no wasm armed returns 0 (total default)", async () => {
    expect(isNativeAuthority()).toBe(false);
    const v = await getVersion("docC");
    expect(v).toBe(0);
  });
});
