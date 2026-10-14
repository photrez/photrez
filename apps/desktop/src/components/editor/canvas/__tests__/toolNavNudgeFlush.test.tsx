import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider, useEditor } from "../../shell/EditorContext";
import { useCanvasKeyboard } from "../useCanvasKeyboard";
import { clearRegistry } from "../../keyboardRegistry";
import { WorkspaceManager } from "@/engine/workspace";
import type { CommandHistory } from "@/engine/history";
import type { HistoryEntryView, HistoryQueryResult } from "@/lib/protocol/types";

// Undo counts read the native history stream (nativeHistoryView.ts). Stub
// bridge_emu's getHistoryQuery to serve `nativeStream`, and append one entry
// per real history.commit so the count tracks the actual flush commits instead
// of the hardwired 0 an unauthoritative query yields.
vi.mock("@/lib/protocol/bridge_emu", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/protocol/bridge_emu")>()),
  getHistoryQuery: vi.fn(),
}));

import { getHistoryQuery } from "@/lib/protocol/bridge_emu";

const QUERY_MOCK = getHistoryQuery as unknown as ReturnType<typeof vi.fn>;
const nativeStream: HistoryEntryView[] = [];
let nextSeq = 1;

function appendNativeEntry(label: string) {
  const seq = nextSeq++;
  nativeStream.push({
    seq, groupId: seq, origin: "native", label,
    affectedLayerIds: [], versionBefore: seq - 1, versionAfter: seq, memoryCostBytes: 0,
  });
}

function nativeQuery(): HistoryQueryResult {
  return {
    cursor: nativeStream.length,
    lastSeq: nativeStream.length,
    degradedHint: false,
    pendingExternal: null,
    entries: [...nativeStream],
  };
}

// Append one native user entry per real commit and publish it synchronously so
// a getUndoCount() read in the same tick (the flush asserts run synchronously)
// sees the entry instead of a stale cache.
function armHistory(history: CommandHistory) {
  const realCommit = history.commit.bind(history);
  history.commit = ((snapshot, label, ...rest) => {
    appendNativeEntry(label ?? "Commit");
    (history as unknown as { nativeQuery: HistoryQueryResult | null }).nativeQuery = nativeQuery();
    return realCommit(snapshot, label, ...rest);
  }) as typeof history.commit;
}

function NudgeHarness(props: {
  capture: (editor: ReturnType<typeof useEditor>) => void;
}) {
  const editor = useEditor();
  props.capture(editor);
  useCanvasKeyboard({
    isSpacePressed: () => false,
    setIsSpacePressed: vi.fn(),
    isAltPressed: () => false,
    setIsAltPressed: vi.fn(),
    isPanning: () => false,
    setIsPanning: vi.fn(),
    stopMomentum: vi.fn(),
    fitToScreenAndRender: vi.fn(),
    syncViewport: vi.fn(),
    getCanvasContainerRef: () => undefined,
  });
  return null;
}

function setupNudge() {
  const session = WorkspaceManager.createBlankDocument(
    "nudge-flush",
    "Nudge",
    800,
    600,
  );
  const ws = new WorkspaceManager();
  const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
  const scheduler = { requestRender: vi.fn() };
  const container = document.createElement("div");
  document.body.appendChild(container);
  let captured: ReturnType<typeof useEditor> | undefined;
  const disposeRender = render(
    () => (
      <EditorProvider
        workspace={ws}
        renderer={renderer as never}
        scheduler={scheduler as never}
      >
        <NudgeHarness capture={(e) => { captured = e; }} />
      </EditorProvider>
    ),
    container,
  );
  ws.addDocument(session);
  const engine = session.engine;
  const history = ws.getActiveHistory()!;
  armHistory(history);
  const layer = engine.addLayer("Nudge Me");
  engine.setActiveLayer(layer.id);
  captured!.setActiveTool("move");
  const notifySpy = vi.spyOn(
    engine as unknown as { notifyChange: () => void },
    "notifyChange",
  );
  return {
    ws,
    engine,
    history,
    editor: captured!,
    layerId: layer.id,
    notifySpy,
    dispose: () => {
      notifySpy.mockRestore();
      disposeRender();
      container.parentNode?.removeChild(container);
    },
  };
}

function arrowDown(repeat: boolean) {
  window.dispatchEvent(
    new KeyboardEvent("keydown", {
      key: "ArrowRight",
      repeat,
      bubbles: true,
    }),
  );
}

function arrowUp() {
  window.dispatchEvent(
    new KeyboardEvent("keyup", { key: "ArrowRight", bubbles: true }),
  );
}

describe("legacy arrow-nudge silent burst + flush", () => {
  beforeEach(() => {
    clearRegistry();
    vi.restoreAllMocks();
    nativeStream.length = 0;
    nextSeq = 1;
    QUERY_MOCK.mockReset();
    QUERY_MOCK.mockImplementation(async () => nativeQuery());
  });

  it("repeats move the model with zero syncs, keyup flushes exactly once", () => {
    const t = setupNudge();
    const startX = t.engine.getLayer(t.layerId)!.transform.x;
    const undoBefore = t.history.getUndoCount();
    arrowDown(false);
    for (let i = 0; i < 4; i++) arrowDown(true);
    expect(t.engine.getLayer(t.layerId)!.transform.x).toBe(startX + 5);
    expect(t.notifySpy.mock.calls.length).toBe(0);
    arrowUp();
    expect(t.notifySpy.mock.calls.length).toBe(1);
    expect(t.engine.getLayer(t.layerId)!.transform.x).toBe(startX + 5);
    expect(t.history.getUndoCount()).toBe(undoBefore + 1);
    t.dispose();
  });

  it("window blur flushes an open burst so no sync is stranded", () => {
    const t = setupNudge();
    arrowDown(false);
    arrowDown(true);
    expect(t.notifySpy.mock.calls.length).toBe(0);
    window.dispatchEvent(new Event("blur"));
    expect(t.notifySpy.mock.calls.length).toBe(1);
    t.dispose();
  });

  it("a first-seen repeat still opens one history entry and flushes once", () => {
    const t = setupNudge();
    const startX = t.engine.getLayer(t.layerId)!.transform.x;
    const undoBefore = t.history.getUndoCount();
    // Focus regained while the key is held: the first keydown seen here is
    // already marked repeat because the real first press was swallowed.
    arrowDown(true);
    arrowDown(true);
    expect(t.engine.getLayer(t.layerId)!.transform.x).toBe(startX + 2);
    expect(t.history.getUndoCount()).toBe(undoBefore + 1);
    expect(t.notifySpy.mock.calls.length).toBe(0);
    arrowUp();
    expect(t.notifySpy.mock.calls.length).toBe(1);
    expect(t.history.getUndoCount()).toBe(undoBefore + 1);
    t.dispose();
  });

  it("a fresh press closes the previous burst before its own commit", () => {
    const t = setupNudge();
    const undoBefore = t.history.getUndoCount();
    arrowDown(false);
    arrowDown(true);
    arrowDown(false);
    expect(t.notifySpy.mock.calls.length).toBe(1);
    expect(t.history.getUndoCount()).toBe(undoBefore + 2);
    arrowUp();
    expect(t.notifySpy.mock.calls.length).toBe(2);
    t.dispose();
  });
});
