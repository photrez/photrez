// Default-flip wiring for the photrez.facade / photrez.facadeAuthority flags.
// TRANSITIONAL: pins the default-ON behavior and the opt-out values. Delete
// this file when the opt-out flags are retired (nothing else depends on it).
//
// With no flags stored the facade is enabled and the native engine backs the
// command path. photrez.facade="0" with photrez.facadeAuthority="wasm" takes
// the legacy emulator path; photrez.facadeAuthority="wasm" under facade=1
// keeps the wasm path and never issues a native call.

import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  applyCommand,
  emulateApply,
  isFacadeEnabled,
  isNativeAuthority,
  __resetEmulatedForTests,
  __resetNativeAuthorityForTests,
} from "../bridge";
import { CONTRACT_VERSION } from "../types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as Mock<
  (cmd: string, args: Record<string, unknown>) => Promise<unknown>
>;

const NOOP = { contractVersion: CONTRACT_VERSION, command: { type: "noop" } } as const;

beforeEach(() => {
  localStorage.clear();
  __resetEmulatedForTests();
  __resetNativeAuthorityForTests();
  invokeMock.mockReset();
});
afterEach(() => {
  localStorage.clear();
  __resetEmulatedForTests();
  __resetNativeAuthorityForTests();
  invokeMock.mockReset();
});

describe("photrez.facade default-ON flip (transitional)", () => {
  it("unset flags -> facade enabled + native authority", () => {
    expect(isFacadeEnabled()).toBe(true);
    expect(isNativeAuthority()).toBe(true);
  });

  it("unset flags -> applyCommand routes to the native engine", async () => {
    const canned = { documentVersion: 7, delta: {} };
    invokeMock.mockResolvedValue(JSON.stringify(canned));
    const res = await applyCommand({ ...NOOP });
    expect(invokeMock).toHaveBeenCalledWith(
      "protocol_apply_command_native",
      expect.objectContaining({ docId: "default" }),
    );
    expect(res).toEqual(canned);
  });
});

describe("photrez.facade=0 + photrez.facadeAuthority=wasm legacy opt-out (transitional)", () => {
  it("routes to the emulator with no native dispatch, result identical to emulateApply", async () => {
    localStorage.setItem("photrez.facade", "0");
    localStorage.setItem("photrez.facadeAuthority", "wasm");
    expect(isFacadeEnabled()).toBe(false);
    expect(isNativeAuthority()).toBe(false);
    __resetEmulatedForTests();
    const viaBridge = await applyCommand({ ...NOOP });
    __resetEmulatedForTests();
    const direct = emulateApply({ ...NOOP }, "default");
    expect(viaBridge).toEqual(direct);
    expect(invokeMock).not.toHaveBeenCalled();
  });
});

describe("photrez.facadeAuthority=wasm under facade=1 keeps the wasm path (transitional)", () => {
  it("unarmed wasm refuses loudly and no native call is issued", async () => {
    localStorage.setItem("photrez.facade", "1");
    localStorage.setItem("photrez.facadeAuthority", "wasm");
    expect(isFacadeEnabled()).toBe(true);
    expect(isNativeAuthority()).toBe(false);
    await expect(applyCommand({ ...NOOP })).rejects.toThrow("E_FACADE_NOT_READY");
    expect(invokeMock).not.toHaveBeenCalled();
  });
});
