// Phase E pilot — Delete Layer EditorClient wiring + parity oracle.
//
// Wiring tests drive the REAL handler (handleDeleteActiveLayer) via a real
// WorkspaceManager + EditorProvider (the same production funnel the Delete
// button and the layer.delete keybind call). They assert:
//   - facade flag ON  -> routes through the facade client (deleteLayer command,
//                        snapshot projected into the engine);
//   - facade flag OFF -> routes to the byte-identical legacy TS path
//                        (engine.deleteLayer + recordSnapshotHistory, no facade);
//   - a Rust Err (surfaces as a throw from wasm.protocol_apply_command ->
//     applyCommand THROWS) fails closed: no half-mutation, TS state preserved,
//     toast shown.
//
// The parity oracle computes an INDEPENDENT-copy legacy reference (the engine's
// own snapshot serialization with the victim removed — NOT the facade's delete
// logic) and asserts the facade delete projection is byte-identical to it on
// the comparable layer set/metadata.

import { describe, it, expect, vi, beforeEach, afterEach, beforeAll, afterAll } from "vitest";
import { renderHook } from "@solidjs/testing-library";
import { EditorProvider } from "@/components/editor/shell/EditorContext";
import { DialogProvider } from "@/components/editor/dialogs/DialogProvider";
import { useLayerActions } from "@/components/editor/layers/useLayerActions";
import { WorkspaceManager } from "@/engine/workspace";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { invoke } from "@tauri-apps/api/core";
import type { WebGL2Backend } from "@/renderer/webgl2";
import { getFacade, seedFacadeFromEngine, __resetFacadeRegistryForTests } from "@/lib/protocol/facadeRegistry";
import { createEditorClient } from "@/lib/protocol/editorClient";
import * as bridge from "@/lib/protocol/bridge";
import { showToast } from "@/components/editor/Toast";

vi.mock("@/lib/desktop/tauriWindow", () => ({ isTauriRuntime: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@/components/editor/Toast", () => ({ showToast: vi.fn() }));

const toastMock = showToast as unknown as ReturnType<typeof vi.fn>;

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
      getContext(): any {
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
      transferToImageBitmap(): unknown {
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

type CanonicalLayer = {
  id: string;
  name: string;
  visible: boolean;
  opacity: number;
  x: number;
  y: number;
  scaleX: number;
  scaleY: number;
  rotation: number;
};

// Comparable layer metadata shared by the facade RenderLayer and the engine's
// snapshot LayerNode (transform flattened). Byte-identical on the user-visible
// layer set/metadata; resourceId is intentionally excluded (facade-only).
function canonical(l: any): CanonicalLayer {
  return {
    id: l.id,
    name: l.name,
    visible: l.visible,
    opacity: l.opacity,
    x: l.x ?? l.transform?.x ?? 0,
    y: l.y ?? l.transform?.y ?? 0,
    scaleX: l.scaleX ?? l.transform?.scaleX ?? 1,
    scaleY: l.scaleY ?? l.transform?.scaleY ?? 1,
    rotation: l.rotation ?? l.transform?.rotation ?? 0,
  };
}
function canonicalSorted(layers: readonly any[]): string {
  return JSON.stringify(layers.map(canonical).sort((a, b) => a.id.localeCompare(b.id)));
}

// Seeds a facade from the engine (Background), then adds the named ordinary
// layers through the facade and projects each snapshot into the engine so the
// facade owns them. Returns the last projected snapshot.
function seedFacadeWithLayers(engine: any, facade: any, names: string[]) {
  seedFacadeFromEngine(engine, facade);
  let snap: any = facade.snapshot;
  for (const name of names) {
    snap = facade.addLayer(name);
    engine.applyFacadeSnapshot(snap);
  }
  return snap;
}

beforeEach(() => {
  localStorage.clear();
  __resetFacadeRegistryForTests();
  (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
  vi.mocked(isTauriRuntime).mockReset();
  vi.mocked(invoke).mockReset();
  toastMock.mockClear();
});
afterEach(() => {
  localStorage.removeItem("photrez.facade");
  __resetFacadeRegistryForTests();
  (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
  vi.restoreAllMocks();
});

describe("Delete Layer EditorClient routing", () => {
  it("facade ON: routes Delete Layer through the facade client -> snapshot projected, no legacy history", () => {
    localStorage.setItem("photrez.facade", "1");
    const { engine, history, renderer, wrapper } = createWrapper();
    const facade = getFacade("doc-a");
    const snap = seedFacadeWithLayers(engine, facade, ["Victim"]);
    const victim = snap.layers[snap.layers.length - 1];
    engine.setActiveLayer(victim.id);

    const delSpy = vi.spyOn(facade, "deleteLayer");
    const { result } = renderHook(() => useLayerActions(), { wrapper });
    result.handleDeleteActiveLayer();

    // routed to facade delete, ONCE, with the right id
    expect(delSpy).toHaveBeenCalledTimes(1);
    expect(delSpy).toHaveBeenCalledWith(victim.id);
    // projection applied: renderer-visible model no longer has the layer
    expect(engine.getLayer(victim.id)).toBeUndefined();
    expect(renderer.destroyTexture).toHaveBeenCalledWith(victim.id);
    // NO legacy TS history entry for the migrated delete
    expect(history.getUndoCount()).toBe(0);
  });

  it("facade OFF: routes to the byte-identical legacy TS path (no facade, no applyCommand)", () => {
    localStorage.setItem("photrez.facade", "0");
    const { engine, history, wrapper } = createWrapper();
    const layer = engine.addLayer("Extra");
    engine.setActiveLayer(layer.id);
    const facadeSpy = vi.spyOn(bridge, "applyCommand");

    const { result } = renderHook(() => useLayerActions(), { wrapper });
    result.handleDeleteActiveLayer();

    expect(engine.getLayer(layer.id)).toBeUndefined();
    expect(history.getUndoCount()).toBe(1);
    expect(facadeSpy).not.toHaveBeenCalled(); // no facade command issued
  });

  it("ERR SAFE: a Rust Err (applyCommand throws) fails closed — no half-mutation, toast shown", () => {
    localStorage.setItem("photrez.facade", "1");
    const { engine, wrapper } = createWrapper();
    const facade = getFacade("doc-a");
    const snap = seedFacadeWithLayers(engine, facade, ["Victim"]);
    const victim = snap.layers[snap.layers.length - 1];
    engine.setActiveLayer(victim.id);

    // A Rust Err surfaces as a throw from wasm.protocol_apply_command; the
    // bridge applyCommand wrapper wraps that envelope and re-throws. Exactly
    // what fails here.
    vi.spyOn(bridge, "applyCommand").mockImplementation(() => {
      throw new Error("E_VERSION_MISMATCH: expected 1 got 2");
    });

    const { result } = renderHook(() => useLayerActions(), { wrapper });
    result.handleDeleteActiveLayer();

    // fail-closed: the layer is NOT deleted, facade snapshot unchanged
    expect(engine.getLayer(victim.id)).toBeTruthy();
    expect(facade.snapshot.layers.find((l: any) => l.id === victim.id)).toBeTruthy();
    // user-facing error surfaced, not swallowed
    expect(toastMock).toHaveBeenCalledTimes(1);
    expect(String(toastMock.mock.calls[0][0])).toContain("Cannot delete layer");
  });

  it("GHOST GUARD: a facade no-op delete (victim left present) is blocked — texture NOT destroyed, toast shown", () => {
    localStorage.setItem("photrez.facade", "1");
    const { engine, renderer, wrapper } = createWrapper();
    const facade = getFacade("doc-a");
    const snap = seedFacadeWithLayers(engine, facade, ["Victim"]);
    const victim = snap.layers[snap.layers.length - 1];
    engine.setActiveLayer(victim.id);

    // The facade delete becomes a NO-OP: the returned delta carries no Remove
    // change, so the facade snapshot STILL contains the victim (an unknown-id
    // delete leaves the layer present). The EditorClient ghost guard must then
    // surface blocked so the caller does NOT destroy the texture of a layer
    // that still exists.
    const v = facade.renderedVersion;
    vi.spyOn(bridge, "applyCommand").mockImplementation(() => {
      return {
        documentVersion: v + 1,
        delta: { baseVersion: v, version: v + 1, changes: [] },
      } as never;
    });

    const { result } = renderHook(() => useLayerActions(), { wrapper });
    result.handleDeleteActiveLayer();

    // blocked: layer NOT removed, texture NOT destroyed, failure surfaced.
    expect(engine.getLayer(victim.id)).toBeTruthy();
    expect(facade.snapshot.layers.find((l: any) => l.id === victim.id)).toBeTruthy();
    expect(renderer.destroyTexture).not.toHaveBeenCalled();
    expect(toastMock).toHaveBeenCalledTimes(1);
    expect(String(toastMock.mock.calls[0][0])).toContain("Cannot delete layer");
  });
});

describe("Delete Layer parity oracle (independent-copy legacy reference)", () => {
  it("facade delete projection == independent-copy legacy reference on layer set/metadata", () => {
    localStorage.setItem("photrez.facade", "1");
    const { engine } = createWrapper();
    const facade = getFacade("doc-a");
    seedFacadeWithLayers(engine, facade, ["A", "B"]); // Background + A + B
    const victim = engine.getLayers().find((l: any) => l.name === "A");
    const victimId = victim!.id;

    // INDEPENDENT-copy legacy reference: the engine's OWN snapshot serialization
    // (pre-state) with the victim removed — exactly what the legacy TS path
    // (engine.deleteLayer -> engine.snapshot()) yields. NOT derived from the
    // facade's delete/emulator logic.
    const pre = engine.snapshot();
    const legacyExpected = canonicalSorted(
      pre.layers.filter((l: any) => l.id !== victimId),
    );

    // FACADE path via EditorClient.
    const client = createEditorClient(
      { applyFacadeSnapshot: (s) => engine.applyFacadeSnapshot(s as never) },
      facade,
    );
    const res = client.deleteLayer(victimId);
    expect(res.status).toBe("facade");
    expect(facade.snapshot.layers.find((l: any) => l.id === victimId)).toBeUndefined();

    const actual = canonicalSorted(facade.snapshot.layers);
    // byte-identical layer set/metadata: the projection equals the legacy
    // reference result exactly.
    expect(actual).toBe(legacyExpected);

    // The projection reached the engine model (renderer-visible) identically.
    expect(canonicalSorted(engine.getLayers())).toBe(legacyExpected);
  });
});
