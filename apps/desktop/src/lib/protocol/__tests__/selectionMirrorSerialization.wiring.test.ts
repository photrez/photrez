// Serialization of the selection-mirror funnel (mirrorSelectionCommand).
//
// A facade command bumps its renderedVersion only in a post-await microtask, so
// two selection mirrors dispatched in the same tick read the SAME
// expectedVersion; the engine accepts the first and rejects the second with
// E_VERSION_MISMATCH. An absolute op (setSelection / clearSelection / selectAll)
// would heal at the next absolute op, but the native invert arm TOGGLES the
// shadow's inverted flag, so a lost invert leaves host and shadow permanently
// opposite. mirrorSelectionCommand chains each dispatch onto a per-document tail
// so dispatches serialize.
//
// MOCK FIDELITY: the native transport is routed to the real TS emulator
// (emulateApply), so the version check and the invert toggle are the production
// logic, not a hand-rolled stub. The rapid-invert test below FAILS on the
// pre-serialization fire-and-forget implementation and passes with the tail.
//
// The emulator has no canonical shadow, so selectAll/invert-fallback need the
// explicit dims hook; these tests set it where a fallback can occur.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DocumentEngine } from "@/engine/document";
import * as bridge from "@/lib/protocol/bridge";
import { nativeProtocol } from "@/lib/protocol/nativeClient";
import {
  __resetFacadeRegistryForTests,
  commitFacadeInvertSelection,
  commitFacadeSetSelection,
  mirrorSelectionCommand,
} from "@/lib/protocol/facadeRegistry";

const DOC = "mirror-doc";

// Mutable reject switch: a test sets it true to force the next apply to fail with
// the bare Tauri v2 rejection string a Rust Err(String) produces.
const nativeState = { rejectNextApply: false };

// Route every native transport call to the real emulator.
function routeNativeToEmu(): void {
  const state = nativeState;
  state.rejectNextApply = false;
  vi.spyOn(nativeProtocol, "protocol_version_native").mockImplementation(async () =>
    bridge.emulateGetSnapshot().version,
  );
  vi.spyOn(nativeProtocol, "protocol_snapshot_native").mockImplementation(async () =>
    JSON.stringify(bridge.emulateGetSnapshot()),
  );
  vi.spyOn(nativeProtocol, "protocol_apply_command_native").mockImplementation(
    async (json: string) => {
      if (state.rejectNextApply) {
        state.rejectNextApply = false;
        // Tauri v2 invoke() REJECTS with the bare "CODE: message" string on a
        // Rust Err(String); the bridge normalizes it into an Error.
        throw "E_VERSION_MISMATCH: forced for the rejection-path test";
      }
      const env = JSON.parse(json) as {
        contractVersion: number;
        expectedVersion?: number;
        command: Record<string, unknown>;
      };
      const res = bridge.emulateApply({
        contractVersion: env.contractVersion,
        expectedVersion: env.expectedVersion,
        command: env.command as never,
      });
      return JSON.stringify(res);
    },
  );
}

const flush = () => new Promise((r) => setTimeout(r, 0));

describe("selection mirror serialization (mirrorSelectionCommand)", () => {
  beforeEach(() => {
    localStorage.setItem("photrez.facade", "1");
    localStorage.setItem("photrez.facadeAuthority", "native");
    bridge.__resetEmulatedForTests();
    bridge.__resetNativeAuthorityForTests();
    __resetFacadeRegistryForTests();
    routeNativeToEmu();
  });

  afterEach(() => {
    localStorage.removeItem("photrez.facade");
    localStorage.removeItem("photrez.facadeAuthority");
    vi.restoreAllMocks();
  });

  it("two rapid mirrors in one tick stay ordered: setSelection then invert lands", async () => {
    const engine = new DocumentEngine(DOC, "D", 800, 600);
    engine.createSelection(10, 20, 30, 40, 0);
    // Copy: the host op replaces model.selection, so the mirrored payload must be
    // the pre-invert rect the native shadow would have received in production.
    const s1 = { ...engine.getSelection()! };

    // Production order: host mutation first (visual authority), then the mirror.
    mirrorSelectionCommand(engine, () => commitFacadeSetSelection(engine as never, s1));
    engine.invertSelection();
    mirrorSelectionCommand(engine, () => commitFacadeInvertSelection(engine as never));
    await flush();

    // Serialized: the invert ran AFTER the setSelection bumped the version, so it
    // toggled the shadow's rect instead of being rejected. Shadow == host.
    expect(bridge.getEmuSelection()).toEqual(engine.getSelection());

    // Invert once more: still in step (the toggle is not one-behind).
    engine.invertSelection();
    mirrorSelectionCommand(engine, () => commitFacadeInvertSelection(engine as never));
    await flush();
    expect(bridge.getEmuSelection()).toEqual(engine.getSelection());
  });

  it("a rejected mirror leaves the host selection intact and the next mirror still lands", async () => {
    const engine = new DocumentEngine(DOC, "D", 800, 600);
    engine.createSelection(10, 20, 30, 40, 0);
    const hostSel = { ...engine.getSelection()! };

    nativeState.rejectNextApply = true;
    mirrorSelectionCommand(engine, () => commitFacadeSetSelection(engine as never, hostSel));
    await flush();

    // (a) Visual authority unaffected: the funnel never projects a snapshot back,
    // so a rejected shadow sync cannot touch model.selection.
    expect(engine.getSelection()).toEqual(hostSel);

    // (b) The chain survives the rejection: the next (serialized) mirror still
    // runs and lands, so one failed sync does not wedge the tail.
    mirrorSelectionCommand(engine, () => commitFacadeSetSelection(engine as never, hostSel));
    await flush();
    expect(bridge.getEmuSelection()).toEqual(hostSel);
  });

  it("flag OFF: mirrorSelectionCommand is a no-op (default path byte-identical)", () => {
    localStorage.removeItem("photrez.facade");
    const engine = new DocumentEngine(DOC, "D", 800, 600);
    engine.createSelection(10, 20, 30, 40, 0);
    mirrorSelectionCommand(engine, () => commitFacadeSetSelection(engine as never, { ...engine.getSelection()! }));
    // No dispatch happened, so the shadow was never touched.
    expect(bridge.getEmuSelection()).toBeNull();
  });
});
