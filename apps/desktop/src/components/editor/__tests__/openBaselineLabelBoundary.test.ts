// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * The document-open baseline label, read from an UNMOCKED history query.
 *
 * `deriveOpenBaselineFloor` recognizes the open-baseline run by its label
 * ("Set Background Flag"). A test that hand-builds `{label: "Set Background
 * Flag"}` in a query fixture only proves the predicate against its own
 * fixture: rename the label on either side of the boundary and both the
 * fixture and the assertion keep agreeing with each other while the shipped
 * predicate stops matching the real stream.
 *
 * So here the query is never built by hand. The emulated native engine (the TS
 * mirror of the Rust ProtocolEngine; Rust writes the same label via
 * `begin_forward("Set Background Flag", ...)` in document_core_apply.rs) emits
 * the entry from its own apply arm, and both consumers - the floor derivation
 * and the undo/redo refusal predicate - are driven off that real read.
 */
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import {
  __resetEmulatedForTests,
  emulateApply,
} from "@/lib/protocol/bridge";
import * as bridge from "@/lib/protocol/bridge";
import { getHistoryQuery } from "@/lib/protocol/bridge_emu";
import { deriveOpenBaselineFloor } from "@/engine/nativeHistoryView";
import { rustStreamHoldsUserWork } from "../facadeHistoryHandoff";
import type { EditorContextValue } from "../shell/EditorContext";
import { installEmuBridgeSpies } from "@/lib/protocol/__tests__/emulatorBridgeRoute";

const editor = {
  workspace: { getActiveDocumentId: () => "default" },
} as unknown as EditorContextValue;

function addLayer(id: string, name: string): void {
  emulateApply({
    contractVersion: 1,
    command: { type: "addLayer", id, name, width: 100, height: 100, index: 0 },
  });
}

describe("the open-baseline boundary read from the emulated native engine", () => {
  beforeEach(() => {
    // The retired photrez.facadeAuthority=wasm pin used to select
    // the emulator arm of getHistoryQuery through the flag-driven
    // dispatch; production dispatch is native-only now, so the
    // shared emulator route reroutes the query seam explicitly
    // (this file's own reads and rustStreamHoldsUserWork's direct
    // bridge_emu import both resolve to the emulator).
    __resetEmulatedForTests();
    installEmuBridgeSpies(bridge);
  });

  afterEach(() => {
    __resetEmulatedForTests();
  });

  it("pins the floor and the refusal on a query the test never constructed", async () => {
    // Document-open shape: the factory's layer reaches the native engine
    // without a history entry (the Rust Ping arm creates a layer the same way,
    // with no begin_forward), then the factory's SetBackgroundFlag apply lands
    // as stream position 0 - same command, same label as the Rust forward entry.
    emulateApply({ contractVersion: 1, command: { type: "ping", echo: "L1" } });
    emulateApply({ contractVersion: 1, command: { type: "setBackgroundFlag", id: "ping:L1" } });

    const query = await getHistoryQuery();
    expect(query.entries[0]?.label, "the engine's own label, not a fixture").toBe(
      "Set Background Flag",
    );
    expect(query.cursor).toBe(1);
    expect(deriveOpenBaselineFloor(query.entries)).toBe(1);

    // Only the baseline sits below the cursor: the refusal must REFUSE.
    expect(await rustStreamHoldsUserWork(editor, "undo")).toBe(false);

    // A real user edit moves past the floor: the same predicate now allows.
    addLayer("L2", "Layer 2");
    expect((await getHistoryQuery()).cursor).toBe(2);
    expect(deriveOpenBaselineFloor((await getHistoryQuery()).entries)).toBe(1);
    expect(await rustStreamHoldsUserWork(editor, "undo")).toBe(true);
  });
});
