// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * A History-panel jump must drive the NATIVE history stepper.
 *
 * The native walker restores this stream - one cursor step per model step,
 * each step's {delta, pixelPatches?, externalToken?} projected onto the
 * engine - and the host stack is re-synced from
 * protocol_history_query_native. The previous behaviour restored via
 * engine.restore from the TS stack and never moved the Rust cursor: the model
 * landed on an entry whose pixels (and cursor) Rust never reached. This test
 * pins the current behaviour; the earlier behaviour is the divergence that
 * failed against this same stream.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render } from "solid-js/web";
import { invoke } from "@tauri-apps/api/core";
import { WorkspaceManager } from "@/engine/workspace";
import { EditorProvider, useEditor } from "../shell/EditorContext";
import * as bridge from "@/lib/protocol/bridge";
import * as facadeRegistry from "@/lib/protocol/facadeRegistry";
import { installEmuBridgeSpies } from "@/lib/protocol/__tests__/emulatorBridgeRoute";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(() => Promise.resolve("0.0.0")) }));

const DOC = "doc-nav";

const settle = async () => {
  await new Promise<void>((r) => setTimeout(r, 0));
};

describe("navigateHistory: a panel jump moves the native cursor and projects each step", () => {
  let ws: WorkspaceManager;
  let renderer: Record<string, unknown>;
  let scheduler: Record<string, unknown>;
  let container: HTMLDivElement;
  let dispose: () => void;
  let editorRef: { current: { navigateHistory: (i: number) => void } | null };

  const Capture = () => {
    editorRef.current = useEditor() as never;
    return null;
  };

  beforeEach(async () => {
    localStorage.clear();
    // The retired photrez.facade=0 / photrez.facadeAuthority=wasm pins
    // used to select the emulator arms through the flag-driven dispatch;
    // production dispatch is native-only now, so the suite reaches the
    // same emulator arms explicitly (the shared route below mirrors the
    // Rust predicate).
    facadeRegistry.__resetFacadeRegistryForTests();
    bridge.__resetEmulatedForTests();
    installEmuBridgeSpies(bridge);
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation(async () => undefined);

    ws = new WorkspaceManager();
    renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn(), uploadSurfaceTiles: vi.fn() };
    scheduler = { requestRender: vi.fn() };
    container = document.createElement("div");
    document.body.appendChild(container);
    editorRef = { current: null };

    ws.addDocument(WorkspaceManager.createBlankDocument(DOC, "Nav", 8, 8));

    dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer as never} scheduler={scheduler as never}>
          <Capture />
        </EditorProvider>
      ),
      container,
    );
    await settle();
  });

  afterEach(() => {
    dispose();
    if (container.parentNode) container.parentNode.removeChild(container);
    vi.restoreAllMocks();
    localStorage.clear();
  });

  it("a 3-step jump lands the native cursor on the target entry and projects each step's delta", async () => {
    const session = ws.getActiveSession()!;
    const history = session.history;
    const facade = facadeRegistry.getFacade(DOC);
    await facade.addLayer("A");
    await facade.addLayer("B");
    await facade.addLayer("C");
    const before = await bridge.getHistoryQuery(DOC);
    expect(before.entries.length).toBe(3);
    expect(before.cursor).toBe(3);
    const applySpy = vi.spyOn(session.engine, "applyFacadeSnapshot");

    editorRef.current!.navigateHistory(0);
    await settle();
    await settle();

    const after = await bridge.getHistoryQuery(DOC);
    expect(after.cursor, "the native cursor landed on the jump target").toBe(0);
    expect(applySpy, "each of the three steps projected its delta").toHaveBeenCalledTimes(3);
    // Host stack re-synced from the query (not from its own pops):
    expect(history.canUndo(), "cursor at 0 -> nothing undoable").toBe(false);
    expect(history.canRedo()).toBe(true);

    // And back up again: the redo direction must move the cursor too.
    editorRef.current!.navigateHistory(3);
    await settle();
    await settle();
    const restored = await bridge.getHistoryQuery(DOC);
    expect(restored.cursor).toBe(3);
    expect(history.canUndo()).toBe(true);
    expect(history.canRedo()).toBe(false);
  });
});
