// Delete Layer snapshot-bridge producer wiring test.
//
// Verifies the FIRST production `recordSnapshotHistory` call site:
// handleDeleteActiveLayer (legacy path) captures a before (pre-action, layer
// present) / after (post-action, layer gone) pair and routes it through
// `history.recordSnapshotHistory(before, after, "Delete Layer")`. With no
// snapshot recorder this must be behavior-identical to the old
// `commit(snapshot)`; the no-IPC inertness is asserted here too.
//
// The undo/re-do re-attach-by-token guard lives in useEditorCommands and is
// covered in useEditorCommands.snapshotBridge.test.ts; this file focuses on the
// producer + Model-A undo restore (no detach) + no-IPC inertness.

import { describe, it, expect, vi, beforeEach, afterEach, beforeAll, afterAll } from "vitest";
import { renderHook } from "@solidjs/testing-library";
import { EditorProvider } from "../../shell/EditorContext";
import { useLayerActions } from "../useLayerActions";
import { WorkspaceManager } from "@/engine/workspace";
import { DialogProvider } from "../../dialogs/DialogProvider";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { invoke } from "@tauri-apps/api/core";
import { armWasmModule } from "@/lib/protocol/__tests__/wasmBridgeArm";
import type { WebGL2Backend } from "@/renderer/webgl2";

vi.mock("@/lib/desktop/tauriWindow", () => ({
  isTauriRuntime: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

// The retired `photrez.facadeAuthority=wasm` key used to select the wasm
// engine; the shared factory arms the same engine on the bridge surface this
// suite drives (the delete funnel dispatches through applyCommand). The helper
// must be imported INSIDE the factory: vi.mock is hoisted above imports.
vi.mock("@/lib/protocol/bridge", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/protocol/bridge")>();
  const { makeWasmArmedBridge } = await import("@/lib/protocol/__tests__/wasmBridgeArm");
  return makeWasmArmedBridge(actual);
});

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

beforeAll(async () => {
  await armWasmModule();
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

function tauriRuntime(on: boolean) {
  vi.mocked(isTauriRuntime).mockReturnValue(on);
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

  it("records recordSnapshotHistory(before, after) with before=layer present, after=layer gone", async () => {
    tauriRuntime(true);
    vi.mocked(invoke).mockResolvedValue({ version: 1, epoch: 0 });

    const { engine, history, wrapper } = createWrapper();
    const layer = engine.addLayer("Layer1", 100, 100) as unknown as { id: string };
    const bm = fakeBitmap();
    engine.setLayerImageBitmap(layer.id, bm);

    const { result } = renderHook(() => useLayerActions(), { wrapper });
    // The hook reads the active layer through the editor signal, so the
    // selection must go through the hook itself.
    result.handleSelectLayer(layer.id);

    const spy = vi.spyOn(history, "recordSnapshotHistory");
    await result.handleDeleteActiveLayer();

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
    // No snapshot invoke: `rust_pixels_record_snapshot` has no production caller.
    await flush();
    const rec = vi.mocked(invoke).mock.calls.filter(([c]) => c === "rust_pixels_record_snapshot");
    expect(rec.length).toBe(0);
  });

  it("undo restores the deleted layer with the SAME ImageBitmap (no detach, no close)", async () => {
    tauriRuntime(true);
    vi.mocked(invoke).mockResolvedValue({ version: 1, epoch: 0 });

    const { engine, history, wrapper } = createWrapper();
    const layer = engine.addLayer("Layer1", 100, 100) as unknown as { id: string };
    const bm = fakeBitmap();
    engine.setLayerImageBitmap(layer.id, bm);

    const { result } = renderHook(() => useLayerActions(), { wrapper });
    result.handleSelectLayer(layer.id);
    await result.handleDeleteActiveLayer();
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
});
