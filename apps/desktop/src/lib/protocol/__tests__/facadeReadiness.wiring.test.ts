// Facade load-order readiness (flag-ON): a facade command must NEVER silently
// run on the TS emulator while photrez.facade=1 but the real wasm is not armed.
//
// PROBLEM (run BEFORE this gate existed): getWasmExportModule() arms the bridge
// via setProtocolWasm(mod) only after the wasm loads. A facade command issued in
// that window fell through applyCommand -> emulateApply (the TS emulator, version
// 0, module-global state). When the real engine later armed it started fresh
// (version 0, empty), so the emu->Rust straddle silently diverged state/version
// (no atomic reconciliation).
//
// DISCRIMINATOR: this file relies ONLY on the facade-readiness gate + the
// production wiring.
//   * Test 1 (flag ON, wasm NOT armed): applyCommand must THROW E_FACADE_NOT_READY
//     while the emulator history stays EMPTY (proving the emulator was NOT run).
//     Delete the gate in bridge.ts (applyCommand -> emulateApply on !wasm) and
//     this test FAILS: applyCommand would emulate, push a history entry, and
//     return a normal result.
//   * Test 2 (flag OFF, wasm NOT armed): applyCommand STILL emulates (the legacy
//     path is unchanged) — proves the ALTERNATIVE is emulation only under flag OFF.
//   * Test 3 (flag ON, after ensureFacadeReady()): the SAME command runs on the
//     REAL wasm engine (getSnapshot() is non-empty), proving readiness was
//     reached through the production arming path.
//
// Tests 1 and 2 MUST run before Test 3 (declaration order). The bridge starts
// unarmed in a fresh isolated file, so Tests 1 and 2 observe the unarmed state.
// ensureFacadeReady is only called in Test 3 (which arms the bridge via
// getWasmExportModule -> setProtocolWasm, the production wiring).

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import {
  applyCommand,
  ensureFacadeReady,
  getHistoryQuery,
  getSnapshot,
  isFacadeArmed,
  __resetEmulatedForTests,
} from "@/lib/protocol/bridge";
import { CONTRACT_VERSION } from "../types";

beforeEach(() => {
  localStorage.setItem("photrez.facade", "1");
  // Start from a clean emulator (the real wasm is still UNARMED here).
  __resetEmulatedForTests();
});

afterEach(() => {
  localStorage.removeItem("photrez.facade");
  __resetEmulatedForTests();
});

describe("Facade readiness gate (flag ON, engine arming)", () => {
  it("flag ON + wasm NOT armed -> facade command throws E_FACADE_NOT_READY (never emulates)", () => {
    // Hardening: the discriminator below assumes the bridge starts unarmed, so
    // assert it explicitly (a test that accidentally ran after arming would make
    // the gate's effect invisible and the assertion vacuous).
    expect(isFacadeArmed()).toBe(false);

    // Bridge is unarmed (fresh file). A facade command MUST NOT emulate.
    expect(() =>
      applyCommand({
        contractVersion: CONTRACT_VERSION,
        command: { type: "addLayer", name: "ShouldNotEmulate" },
      }),
    ).toThrow(/E_FACADE_NOT_READY/);

    // If the gate fell through to the emulator, beginEmu would have pushed a
    // history entry. Since the gate blocks it, the emulator history stays EMPTY
    // — proof the emulator never ran and no emu state/version was produced to
    // diverge from Rust. (getSnapshot() is NOT a usable probe: on an unarmed
    // bridge it hard-codes {version:0,layers:[]} and never reflects emulation.)
    expect(getHistoryQuery().entries.length).toBe(0);
  });

  it("flag OFF + wasm NOT armed -> applyCommand STILL emulates (legacy path unchanged)", () => {
    // The default (flag OFF) production path must remain byte-identical: the
    // emulator is the legacy authority and must keep running (this is what a
    // non-facade app relies on today). Under flag OFF the gate must NOT fire.
    localStorage.removeItem("photrez.facade");

    const res = applyCommand({
      contractVersion: CONTRACT_VERSION,
      command: { type: "addLayer", name: "Legacy" },
    }) as unknown as { documentVersion: number };

    // The emulator applied the command (version bumped to 1, entry recorded).
    expect(res.documentVersion).toBe(1);
    expect(getHistoryQuery().entries.length).toBe(1);
  });

  it("flag ON + after ensureFacadeReady() -> the SAME command runs on the REAL wasm", async () => {
    // Production arming path: awaits getWasmExportModule which calls
    // setProtocolWasm(mod) (the ONLY production arming point).
    const armed = await ensureFacadeReady();
    expect(typeof armed.protocol_apply_command).toBe("function");

    const res = applyCommand({
      contractVersion: CONTRACT_VERSION,
      command: { type: "addLayer", name: "RealRust" },
    }) as unknown as { documentVersion: number };

    // DISCRIMINATOR — only real Rust populates the bridge snapshot:
    //   * armed:    getSnapshot() -> wasm.protocol_snapshot_json() (engine state)
    //   * unarmed:  getSnapshot() -> { version:0, layers:[] } (bridge.ts) and the
    //              TS emulator NEVER writes to it.
    const snap = getSnapshot();
    expect(snap.version).toBeGreaterThan(0);
    expect(snap.layers.length).toBeGreaterThan(0);
    expect(snap.layers[0].name).toBe("RealRust");
    expect(res.documentVersion).toBeGreaterThan(0);

    // The command was applied by the real engine, not emulated: the layer is
    // in the real engine snapshot and the version advanced 0 -> 1.
    expect(res.documentVersion).toBe(1);
  });
});
