// Production undo path for a routed canvas entry.
//
// The routed canvas ops (resize canvas / apply crop) own their history in the
// native engine, so the TS history store stays empty for them. Undo is driven by
// the real production orchestration unit, runFacadeExternalHandoff (this is what
// useEditorCommands.restoreHistorySnapshot calls behind the command handler),
// not by calling facade.undo() directly. The test proves a handled step restores
// the document size and does NOT fall through to (pop) the TS history store.
//
// Coverage boundary: the handoff function is driven directly with the same
// editor context it reads in production (engine + renderer + scheduler +
// workspace notifier). The emulator backs the native delta so the size plumbing
// is exercised end to end; the real-wasm parity matrix owns layer geometry.
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { DocumentEngine } from "@/engine/document";
import {
  getFacade,
  seedFacadeFromEngine,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";
import { __resetEmulatedForTests, setEmuDocumentDims } from "@/lib/protocol/bridge";
import { runFacadeExternalHandoff } from "../facadeHistoryHandoff";
import type { EditorContextValue } from "../shell/EditorContext";
import * as bridge from "@/lib/protocol/bridge";
import { installEmuBridgeSpies } from "@/lib/protocol/__tests__/emulatorBridgeRoute";

// Production dispatch is native-only; the emulator route below backs the
// command surface, and the seed path (rust_pixels_open_document /
// protocol_seed_native) resolves on the mocked transport.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("routed canvas undo through the production handoff", () => {
  beforeEach(() => {
    installEmuBridgeSpies(bridge);
    vi.mocked(invoke).mockImplementation(async () => undefined);
    __resetEmulatedForTests();
    __resetFacadeRegistryForTests();
  });

  afterEach(() => {
    __resetEmulatedForTests();
    __resetFacadeRegistryForTests();
    vi.restoreAllMocks();
  });

  it("a routed crop undo restores dims through the production handoff", async () => {
    // The native engine (emulator) starts at the document size.
    setEmuDocumentDims(800, 600);
    const engine = new DocumentEngine("routed-und", "Routed", 800, 600);
    engine.addLayer("A", 100, 100);
    const facade = getFacade(engine.getId());
    await seedFacadeFromEngine(engine as never, facade);

    // Route the crop: the native arm records the entry and moves the size.
    await facade.applyCrop(0, 0, 100, 100, 0);
    engine.applyFacadeSnapshot(facade.snapshot as never, { dimsAuthoritative: true });
    expect([engine.getWidth(), engine.getHeight()]).toEqual([100, 100]);

    const editor = {
      workspace: {
        getActiveEngine: () => engine,
        notifyVisualChange: vi.fn(),
      },
      renderer: { uploadImage: vi.fn() },
      scheduler: { requestRender: vi.fn() },
    } as unknown as EditorContextValue;

    const handled = await runFacadeExternalHandoff(editor, "undo");

    expect(handled).toBe(true);
    expect(facade.lastHistoryDeltaWasEmpty).toBe(false);
    expect([engine.getWidth(), engine.getHeight()]).toEqual([800, 600]);
  });
});
