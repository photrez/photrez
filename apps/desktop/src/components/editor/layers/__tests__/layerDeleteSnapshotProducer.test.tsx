// Delete Layer snapshot-bridge producer wiring test.
//
// Verifies the FIRST production `recordSnapshotHistory` call site:
// handleDeleteActiveLayer (legacy path) captures a before (pre-action, layer
// present) / after (post-action, layer gone) pair and routes it through
// `history.recordSnapshotHistory(before, after, "Delete Layer")`. With the
// bridge OFF this must be behavior-identical to the old `commit(snapshot)`; the
// flag-OFF inertness is asserted here too.
//
// The undo/re-do re-attach-by-token guard lives in useEditorCommands and is
// covered in useEditorCommands.snapshotBridge.test.ts; this file focuses on the
// producer + Model-A undo restore (no detach) + flag-OFF inertness.

import { describe, it, expect, vi, beforeEach, afterEach, beforeAll, afterAll } from "vitest";
import { renderHook } from "@solidjs/testing-library";
import { EditorProvider } from "../../shell/EditorContext";
import { useLayerActions } from "../useLayerActions";
import { WorkspaceManager } from "@/engine/workspace";
import { DialogProvider } from "../../dialogs/DialogProvider";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { invoke } from "@tauri-apps/api/core";
import type { WebGL2Backend } from "@/renderer/webgl2";

vi.mock("@/lib/desktop/tauriWindow", () => ({
  isTauriRuntime: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const GATE_KEY = "photrez.historyBridge";

const OriginalOffscreenCanvas = (globalThis as any).OffscreenCanvas;
beforeAll(() => {
  if (typeof OffscreenCanvas === "undefined") {
    (globalThis as any).OffscreenCanvas = class {
      width: number;
      height: number;
      constructor(w: number, h: number) {
        this.width = w;
        this.height = h;
      }
      getContext() {
        return {
          save: vi.fn(),
          restore: vi.fn(),
          translate: vi.fn(),
          rotate: vi.fn(),
          scale: vi.fn(),
          drawImage: vi.fn(),
          globalAlpha: 1,
          globalCompositeOperation: "source-over",
          canvas: this,
        } as any;
      }
      transferToImageBitmap() {
        return { width: this.width, height: this.height, close: vi.fn() } as unknown as ImageBitmap;
      }
    };
  }
});
afterAll(() => {
  if (OriginalOffscreenCanvas) {
    (globalThis as any).OffscreenCanvas = OriginalOffscreenCanvas;
  }
});

function makeMockRenderer(): WebGL2Backend {
  return {
    uploadImage: vi.fn(),
    destroyTexture: vi.fn(),
    render: vi.fn(),
    resizeToViewport: vi.fn(),
    getWebGLContext: vi.fn(),
  } as unknown as WebGL2Backend;
}

function fakeBitmap(): ImageBitmap {
  return { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
}

const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

function createWrapper() {
  const ws = new WorkspaceManager();
  ws.addDocument(WorkspaceManager.createBlankDocument("doc-a", "DocA", 800, 600));
  ws.switchDocument("doc-a");
  const engine = ws.getEngine("doc-a")!;
  const history = ws.getHistory("doc-a")!;
  const renderer = makeMockRenderer();
  const scheduler = { requestRender: vi.fn() };

  const wrapper = (props: { children: any }) => (
    <DialogProvider>
      <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
        {props.children}
      </EditorProvider>
    </DialogProvider>
  );

  return { ws, engine, history, renderer, scheduler, wrapper };
}

function gateOn(bridgeOn: boolean) {
  vi.mocked(isTauriRuntime).mockReturnValue(bridgeOn);
  localStorage.setItem(GATE_KEY, "1");
}

describe("Delete Layer snapshot-bridge producer (legacy path)", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.mocked(invoke).mockReset();
    vi.mocked(isTauriRuntime).mockReset();
    vi.mocked(isTauriRuntime).mockReturnValue(false);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("records recordSnapshotHistory(before, after) with before=layer present, after=layer gone, and fires the invoke", async () => {
    gateOn(true);
    vi.mocked(invoke).mockResolvedValue({ version: 1, epoch: 0 });

    const { engine, history, wrapper } = createWrapper();
    const layer = engine.addLayer("Layer1", 100, 100) as unknown as { id: string };
    const bm = fakeBitmap();
    engine.setLayerImageBitmap(layer.id, bm);
    engine.setActiveLayer(layer.id);

    const spy = vi.spyOn(history, "recordSnapshotHistory");
    const { result } = renderHook(() => useLayerActions(), { wrapper });
    result.handleDeleteActiveLayer();

    // Producer routed to recordSnapshotHistory, not plain commit.
    expect(spy).toHaveBeenCalledTimes(1);
    const [before, after, label] = spy.mock.calls[0];
    expect(label).toBe("Delete Layer");
    // before = pre-action state WITH the layer; after = post-action state WITHOUT it.
    expect(before.layers.some((l) => l.id === layer.id)).toBe(true);
    expect(after.layers.some((l) => l.id === layer.id)).toBe(false);
    expect(after.layers.length).toBe(before.layers.length - 1);
    // The post-action state is the live model (layer actually removed).
    expect(engine.getLayer(layer.id)).toBeUndefined();
    // The bridge fires rust_pixels_record_snapshot.
    await flush();
    const rec = vi.mocked(invoke).mock.calls.filter(([c]) => c === "rust_pixels_record_snapshot");
    expect(rec.length).toBe(1);
    expect((rec[0][1] as { docId: string }).docId).toBe("doc-a");
  });

  it("undo restores the deleted layer with the SAME ImageBitmap (no detach, no close)", () => {
    gateOn(true);
    vi.mocked(invoke).mockResolvedValue({ version: 1, epoch: 0 });

    const { engine, history, wrapper } = createWrapper();
    const layer = engine.addLayer("Layer1", 100, 100) as unknown as { id: string };
    const bm = fakeBitmap();
    engine.setLayerImageBitmap(layer.id, bm);
    engine.setActiveLayer(layer.id);

    const { result } = renderHook(() => useLayerActions(), { wrapper });
    result.handleDeleteActiveLayer();
    expect(engine.getLayer(layer.id)).toBeUndefined();

    // Model-A undo restore: regardless of the (flag-gated) token re-attach, TS
    // undo must return the pre-action state and re-attach the SAME bitmap object.
    const undone = history.undo(engine.snapshot());
    expect(undone).toBeTruthy();
    engine.restore(undone!);
    const restored = engine.getLayer(layer.id);
    expect(restored).toBeTruthy();
    expect(restored!.imageBitmap).toBe(bm); // SAME object, never detached
    expect(bm.close).not.toHaveBeenCalled();
  });

  it("flag OFF is inert: Delete Layer does NOT fire rust_pixels_record_snapshot, but still records a TS undo step", async () => {
    // Bridge OFF (non-Tauri runtime default). recordSnapshotHistory must only
    // push the pre-action state to the undo stack — no invoke.
    vi.mocked(isTauriRuntime).mockReturnValue(false);
    localStorage.removeItem(GATE_KEY);

    const { engine, history, wrapper } = createWrapper();
    const layer = engine.addLayer("Layer1", 100, 100) as unknown as { id: string };
    engine.setLayerImageBitmap(layer.id, fakeBitmap());
    engine.setActiveLayer(layer.id);

    const { result } = renderHook(() => useLayerActions(), { wrapper });
    result.handleDeleteActiveLayer();

    await flush();
    expect(invoke).not.toHaveBeenCalled();
    // The undo point is the pre-action state (layer present), so undo restores it.
    expect(history.getUndoCount()).toBe(1);
    const undone = history.undo(engine.snapshot());
    expect(undone?.layers.some((l) => l.id === layer.id)).toBe(true);
  });
});
