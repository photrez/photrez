// Shared emulator route for the suites that drive production entry points
// (facades, the legacy mirror, the commit shim) without a native engine.
//
// Production dispatch is native-only: bridge.applyCommand/getSnapshot/
// getHistoryQuery/historyCursorCommit reach the per-document ProtocolEngine
// over Tauri IPC. These suites measure FACADE/REGISTRY behavior against the
// TS emulator (bridge_emu), whose stream semantics the sequence-parity
// harness proves equal to the real engine. installEmuBridgeSpies reroutes the
// command surface onto the emulator's own instrumentation:
//
//   applyCommand       -> emulateApply (sync; callers await it, fine)
//   getSnapshot        -> emulateGetSnapshot
//   getVersion         -> emulateGetSnapshot().version (the emulator has no
//                         version-only probe; the snapshot carries it)
//   getLayerIds        -> ids of the emulator snapshot layers
//   getHistoryQuery    -> emulateGetHistoryQuery
//   historyCursorCommit-> emulateHistoryCursorCommit
//   everything else    -> untouched (seeds, barriers, registration and resets
//                         run the real production code)
//
// The history query has a second reader set: the production resync paths
// (nativeHistoryView.resyncDerivedView, nativeHistoryJump.
// navigateHistoryNative, facadeHistoryHandoff.rustStreamHoldsUserWork)
// import getHistoryQuery DIRECTLY from bridge_emu, so the bridge-surface
// spy cannot reach them. The flag collapse moved the emulator's query arm
// out of bridge_emu.getHistoryQuery (native-only now), so those readers
// lose the emulator stream unless this seam is routed too - the same
// reroute the retired in-module flag dispatch performed from inside the
// query itself.
//
// The contract-version gate is re-checked in the applyCommand wrapper: the
// live gate (bridge.applyCommand rejects E_CONTRACT_VERSION before any
// transport) would otherwise go unmeasured here, and the emulator never
// enforces it. The gate is ALSO pinned on the real path by
// protocolWiring.test.ts, so this wrapper cannot hide a production deletion.
//
// Spies, not vi.mock: a file-level mock cannot serve a file that ALSO needs
// the real native path (facadeMetadataOps' reorder harness), while spies
// install per describe-block. Every suite calls this in a beforeEach and
// relies on the file's existing afterEach vi.restoreAllMocks() to restore
// the real surface; a per-test vi.spyOn(bridge, "applyCommand") composes by
// overriding the route for that call only.
//
// USE (one line in a beforeEach; pass the already-imported bridge namespace):
//
//   import { installEmuBridgeSpies } from "./emulatorBridgeRoute";
//   beforeEach(() => { installEmuBridgeSpies(bridge); });
//
// Suites that ALSO reach the seed/registration path need a Tauri invoke mock
// (the real seed/registration code runs and must resolve). Pair with
// installSeedOnlyTransport in the same beforeEach:
//
//   vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
//   beforeEach(() => installSeedOnlyTransport(vi.mocked(invoke)));
//
// The seed-only transport resolves opens/seeds/registrations and THROWS on
// every command-path invoke: a command that bypasses the spies and reaches
// the native transport fails loud here instead of silently resolving
// undefined (the leak tripwire - it proves the suite measures the emulator
// route, not a hollow transport).
//
// Open-doc lifecycle (create-then-command rejection) is NOT modeled: the
// emulator is a single global engine with no registry. That contract is
// pinned with a faithful mock by nativeAuthorityReroute.test.ts; modeling it
// here would force every suite to open docs it never opens in production.

import { vi, type Mock } from "vitest";
import { CONTRACT_VERSION } from "../types";
import type { CommandEnvelope } from "../types";
import type * as bridgeNs from "../bridge";
import * as bridgeEmu from "../bridge_emu";

type InvokeMock = Mock<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>;

export function installEmuBridgeSpies(
  bridge: typeof bridgeNs,
  record?: (commandType: string) => void,
): void {
  vi.spyOn(bridge, "applyCommand").mockImplementation(async (envelope: CommandEnvelope) => {
    if (envelope.contractVersion !== CONTRACT_VERSION) {
      throw new Error(
        `E_CONTRACT_VERSION: expected ${CONTRACT_VERSION} got ${envelope.contractVersion}`,
      );
    }
    if (record) record(envelope.command.type);
    return bridgeEmu.emulateApply(envelope);
  });
  vi.spyOn(bridge, "getSnapshot").mockImplementation(async () => bridgeEmu.emulateGetSnapshot());
  vi.spyOn(bridge, "getVersion").mockImplementation(async () =>
    bridgeEmu.emulateGetSnapshot().version,
  );
  vi.spyOn(bridge, "getLayerIds").mockImplementation(async () =>
    (bridgeEmu.emulateGetSnapshot().layers ?? []).map((l) => l.id),
  );
  vi.spyOn(bridge, "getHistoryQuery").mockImplementation(async () =>
    bridgeEmu.emulateGetHistoryQuery(),
  );
  vi.spyOn(bridge, "historyCursorCommit").mockImplementation(async (seq, direction) =>
    bridgeEmu.emulateHistoryCursorCommit(seq, direction),
  );
  // bridge_emu.getHistoryQuery is spied on the bridge_emu namespace
  // (not the bridge surface above) because the direct-import readers
  // listed in the header bypass the bridge module entirely.
  vi.spyOn(bridgeEmu, "getHistoryQuery").mockImplementation(async () =>
    bridgeEmu.emulateGetHistoryQuery(),
  );
}

export function installSeedOnlyTransport(invokeMock: InvokeMock): void {
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    const docId = ((args.docId as string) ?? "default") || "default";
    switch (cmd) {
      case "rust_pixels_open_document":
        return undefined;
      case "protocol_seed_native":
        return JSON.stringify({ version: 0, layers: [] });
      case "protocol_seed_canonical_native":
        return null;
      case "protocol_register_adapter_native":
        return null;
      default:
        // Leak tripwire (see module header): the command surface must go
        // through the spies above, never the native transport.
        throw `E_LEAK: ${cmd} reached the seed-only transport (docId=${docId})`;
    }
  });
}

export function resetEmulatorRoute(bridge: typeof bridgeNs): void {
  bridge.__resetNativeAuthorityForTests();
  bridge.__resetEmulatedForTests();
}
