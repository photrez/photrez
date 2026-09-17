// SPDX-License-Identifier: AGPL-3.0-or-later
//
// ShapeOptionBar call-site wiring while the edited shape layer is owned by the
// native editor state. The protocol-level tests prove the funnel and the
// projection; this file proves the OPTION BAR drives them, which is the half a
// pure-function test cannot see.
//
// Contracts pinned here:
//  1. Edit mode on an owned layer: ONE setLayerParams command, no TS history
//     entry (the native engine records the step), and the model ends on the value
//     the arm holds.
//  2. Flag OFF is untouched: one history entry committed BEFORE the engine write,
//     the engine write carries the locally computed params, zero commands.
//  3. An unowned layer keeps the legacy path on both sides of the flag: the
//     funnel is never reached, so nothing is partially applied.
//  4. A rejected command leaves the model on the previous params and reports the
//     failure instead of keeping a change the authority never took.
//  5. Invalid payloads on the real arm: the arm rejects them at its own boundary
//     (see the last describe).
//
// Real component, real DocumentEngine, real facade projection, real protocol arm
// (wasm). Command counts are taken at the single boundary every protocol command
// crosses (bridge applyCommand).
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { ShapeOptionBar } from "../ShapeOptionBar";
import { DocumentEngine, isFacadeOwnedLayer } from "@/engine/document";
import { CommandHistory } from "@/engine/history";
import { renderShapeToBitmap } from "@/engine/shapeRaster";
import type { ShapeParams } from "@/engine/types";
import {
  __resetFacadeRegistryForTests,
  commitFacadeParams,
  getFacade,
  seedFacadeFromEngine,
} from "@/lib/protocol/facadeRegistry";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import { applyCommand, getSnapshot } from "@/lib/protocol/bridge";
import { CONTRACT_VERSION } from "@/lib/protocol/types";
import type { CommandEnvelope } from "@/lib/protocol/types";
import { showToast } from "../Toast";

vi.mock("../Toast", () => ({ showToast: vi.fn() }));

vi.mock("../dialogs/DialogProvider", () => ({
  useDialog: () => ({ colorPicker: async () => null }),
}));

// applyCommand is the one function every protocol command crosses, so its call
// list IS the dispatch count.
const { commandLog } = vi.hoisted(() => ({ commandLog: { types: [] as string[] } }));
vi.mock("@/lib/protocol/bridge", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/protocol/bridge")>();
  return {
    ...actual,
    applyCommand: (envelope: CommandEnvelope) => {
      commandLog.types.push(envelope.command.type);
      return actual.applyCommand(envelope);
    },
  };
});

// jsdom has no OffscreenCanvas; the shape rasterizer (creation and the routed
// re-raster) needs the 2d context it would otherwise fail to get.
function stubOffscreenCanvas(): void {
  const Mock = function (this: any, w: number, h: number) {
    this.width = w;
    this.height = h;
    const ctx = {
      fillStyle: undefined,
      strokeStyle: undefined,
      lineWidth: undefined,
      lineCap: undefined,
      lineJoin: undefined,
      translate: () => {},
      beginPath: () => {},
      rect: () => {},
      roundRect: () => {},
      ellipse: () => {},
      moveTo: () => {},
      lineTo: () => {},
      bezierCurveTo: () => {},
      quadraticCurveTo: () => {},
      closePath: () => {},
      fill: () => {},
      stroke: () => {},
    };
    this.getContext = () => ctx;
    this.transferToImageBitmap = () => ({ width: this.width, height: this.height, close: () => {} });
  } as unknown as typeof OffscreenCanvas;
  vi.stubGlobal("OffscreenCanvas", Mock);
}

const DOC = "shape-option-bar-routing";

const PARAMS: ShapeParams = {
  kind: "rect",
  width: 100,
  height: 50,
  radius: 8,
  fill: { kind: "solid", color: "#ff0000" },
  stroke: { enabled: true, color: "#00ff00", width: 6 },
  arrowHead: false,
};

let wasm: { protocol_reset: (docId: string) => void } | null = null;

beforeAll(async () => {
  wasm = await getWasmExportModule();
});

beforeEach(() => {
  localStorage.setItem("photrez.facade", "1");
  localStorage.setItem("photrez.facadeAuthority", "wasm");
  stubOffscreenCanvas();
});

afterEach(() => {
  wasm?.protocol_reset(DOC);
  localStorage.removeItem("photrez.facade");
  localStorage.removeItem("photrez.facadeAuthority");
  __resetFacadeRegistryForTests();
  (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  document.body.replaceChildren();
});

/** Microtask drain for the fire-and-forget command dispatches. */
async function settle(): Promise<void> {
  for (let i = 0; i < 8; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

interface Harness {
  engine: DocumentEngine;
  history: CommandHistory;
  layerId: string;
  uploadImage: ReturnType<typeof vi.fn>;
  requestRender: ReturnType<typeof vi.fn>;
  container: HTMLElement;
  cleanup: () => void;
}

async function setup(opts: { owned: boolean; unheld?: boolean }): Promise<Harness> {
  const engine = new DocumentEngine(DOC, "Doc", 800, 600);
  const layer = engine.addShapeLayer("Shape", { ...PARAMS });

  if (opts.unheld) {
    // Ownership without the native engine ever receiving the layer: the facade
    // snapshot carries it (so a projection keeps it) while the protocol engine
    // holds nothing. applyFacadeSnapshot marks every id in the cache vector, so
    // the layer counts as owned even though no arm can restate it.
    const facade = getFacade(DOC);
    await seedFacadeFromEngine(engine as never, facade);
    engine.applyFacadeSnapshot(facade.snapshot as never);
  }

  if (opts.owned) {
    // The native engine only learns a layer through a protocol arm, so the shape
    // layer goes in through the typed-add arm under the id the model already
    // holds: the projection then marks the layer owned.
    await applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: 0,
      docId: DOC,
      command: {
        type: "addLayer",
        id: layer.id,
        name: "Shape",
        width: layer.width,
        height: layer.height,
        index: 0,
        layerType: "shape",
        shapeParams: { ...PARAMS },
      } as never,
    });
    const snapshot = await getSnapshot(DOC);
    engine.applyFacadeSnapshot(snapshot as never);
    getFacade(DOC).syncRenderedVersionTo(snapshot.version);
  }

  const history = new CommandHistory();
  const uploadImage = vi.fn();
  const requestRender = vi.fn();
  mockUseEditor({
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => history,
    },
    renderer: { uploadImage },
    scheduler: { requestRender },
    layers: () => engine.getLayers(),
    selectedLayerId: () => layer.id,
    fgColor: () => "#E15A17",
    setFgColor: vi.fn(),
    shapeKind: () => "rect",
    setShapeKind: vi.fn(),
    shapeFillEnabled: () => true,
    setShapeFillEnabled: vi.fn(),
    shapeStrokeEnabled: () => true,
    setShapeStrokeEnabled: vi.fn(),
    shapeStrokeColor: () => "#00ff00",
    setShapeStrokeColor: vi.fn(),
    shapeStrokeWidth: () => 6,
    setShapeStrokeWidth: vi.fn(),
    shapeRadius: () => 8,
    setShapeRadius: vi.fn(),
    shapeArrowHead: () => false,
    setShapeArrowHead: vi.fn(),
    colorPickerOpen: () => false,
    setColorPickerOpen: vi.fn(),
    colorPickerTarget: () => "foreground",
    setColorPickerTarget: vi.fn(),
  } as never);

  const container = document.createElement("div");
  document.body.appendChild(container);
  const dispose = render(() => <ShapeOptionBar />, container);
  await settle();

  commandLog.types.length = 0;
  return {
    engine,
    history,
    layerId: layer.id,
    uploadImage,
    requestRender,
    container,
    cleanup: () => {
      dispose();
      container.parentNode?.removeChild(container);
    },
  };
}

/** Drive the corner-radius input the way the user does (input event per tick). */
function typeRadius(container: HTMLElement, value: string): void {
  const input = container.querySelector('input[aria-label="Corner radius"]') as HTMLInputElement;
  if (!input) throw new Error("corner radius input not rendered");
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

const shapeParamsOf = (ref: { engine: DocumentEngine; layerId: string }): ShapeParams =>
  ref.engine.getLayer(ref.layerId)!.shapeParams!;

describe("ShapeOptionBar edit mode - owned layer", () => {
  it("dispatches ONE params command, writes no TS history, and lands the model on the arm's value", async () => {
    const h = await setup({ owned: true });
    expect(isFacadeOwnedLayer(h.layerId)).toBe(true);

    const engineWrite = vi.spyOn(h.engine, "updateShapeParams");
    typeRadius(h.container, "30");
    await settle();

    // The edited params travel as one command; no TS history entry is written
    // because the native engine records the undo step.
    expect(commandLog.types).toEqual(["setLayerParams"]);
    expect(h.history.getUndoCount()).toBe(0);

    // The authoritative side holds the edit, and the model agrees with it.
    const snapshot = await getSnapshot(DOC);
    const native = (snapshot as { layers: Array<{ id: string; layerType?: string; shapeParams?: ShapeParams }> }).layers.find(
      (l) => l.id === h.layerId,
    )!;
    expect(native.layerType).toBe("shape");
    expect(native.shapeParams!.radius).toBe(30);
    expect(shapeParamsOf(h)).toEqual(native.shapeParams);

    // Geometry follows the params: the layer is re-rasterized host-side from the
    // projected value, so its dims match a raster of exactly those params. The
    // native params arm does not recompute shape dims, so without this the quad
    // would keep the previous shape's size.
    const settled = h.engine.getLayer(h.layerId)!;
    expect(settled.width).toBe(renderShapeToBitmap(settled.shapeParams!).width);
    expect(settled.height).toBe(renderShapeToBitmap(settled.shapeParams!).height);
    expect(h.uploadImage).toHaveBeenCalledWith(h.layerId, expect.anything());
    expect(h.requestRender).toHaveBeenCalled();

    // The only model write is that re-raster, and it carries the value the arm
    // already holds - never a locally computed patch. No mismatch toast: a false
    // "does not hold this layer" would mean the equality check is too strict.
    expect(engineWrite).toHaveBeenCalledTimes(1);
    expect(engineWrite.mock.calls[0][1]).toEqual(native.shapeParams);
    expect(vi.mocked(showToast)).not.toHaveBeenCalled();
    h.cleanup();
  });

  it("a second edit of the same control dispatches again (no stale value carried)", async () => {
    const h = await setup({ owned: true });

    typeRadius(h.container, "30");
    await settle();
    typeRadius(h.container, "40");
    await settle();

    expect(commandLog.types).toEqual(["setLayerParams", "setLayerParams"]);
    const snapshot = await getSnapshot(DOC);
    const native = (snapshot as { layers: Array<{ id: string; shapeParams?: ShapeParams }> }).layers.find(
      (l) => l.id === h.layerId,
    )!;
    expect(native.shapeParams!.radius).toBe(40);
    expect(shapeParamsOf(h).radius).toBe(40);
    h.cleanup();
  });

  it("a rejected command leaves the previous params in place and reports the failure", async () => {
    const h = await setup({ owned: true });
    const facade = getFacade(DOC);
    vi.spyOn(facade, "setLayerParams").mockRejectedValueOnce(new Error("E_INVALID: boom"));
    const before = shapeParamsOf(h);

    typeRadius(h.container, "30");
    await settle();

    expect(shapeParamsOf(h)).toEqual(before);
    expect(h.history.getUndoCount()).toBe(0);
    expect(vi.mocked(showToast)).toHaveBeenCalledWith(
      expect.stringContaining("E_INVALID: boom"),
      "error",
    );
    h.cleanup();
  });

  it("a layer the native engine never received: the funnel fails loud, the old params stay, the miss is toasted", async () => {
    const h = await setup({ owned: false, unheld: true });
    expect(isFacadeOwnedLayer(h.layerId)).toBe(true);
    // Precondition: the model holds the shape at radius 8 and the native engine
    // holds nothing, so the params arm restates no layer at all.
    expect(shapeParamsOf(h).radius).toBe(8);

    typeRadius(h.container, "30");
    await settle();

    expect(commandLog.types).toEqual(["setLayerParams"]);
    // The funnel's settled-value check throws before the caller can report
    // applied, so the model keeps the pre-edit params and the miss is toasted.
    expect(shapeParamsOf(h).radius).toBe(8);
    expect(h.history.getUndoCount()).toBe(0);
    expect(vi.mocked(showToast)).toHaveBeenCalledWith(
      expect.stringContaining("does not hold this layer"),
      "error",
    );
    h.cleanup();
  });
});

describe("ShapeOptionBar edit mode - keeps photrez.facade=0 opt-out behavior (and unowned layers)", () => {
  it("opted out: one history entry committed BEFORE the engine write, no commands", async () => {
    const h = await setup({ owned: true });
    localStorage.setItem("photrez.facade", "0");
    const commit = vi.spyOn(h.history, "commit");
    const engineWrite = vi.spyOn(h.engine, "updateShapeParams");

    typeRadius(h.container, "30");
    await settle();

    expect(commandLog.types).toEqual([]);
    expect(commit).toHaveBeenCalledTimes(1);
    expect(engineWrite).toHaveBeenCalledTimes(1);
    expect(engineWrite.mock.calls[0][1]).toEqual(expect.objectContaining({ radius: 30 }));
    expect(commit.mock.invocationCallOrder[0]).toBeLessThan(
      engineWrite.mock.invocationCallOrder[0],
    );
    expect(h.history.getUndoCount()).toBe(1);
    expect(shapeParamsOf(h).radius).toBe(30);
    h.cleanup();
  });

  it("flag ON but the layer is not owned: legacy path, zero commands, nothing partially applied", async () => {
    // Single-target call site: the funnel is only reached for an owned id, so its
    // mixed-ownership rejection (covered in facadeParamsFunnel.test.ts) cannot be
    // produced from here and no half-applied state is reachable.
    const h = await setup({ owned: false });
    expect(isFacadeOwnedLayer(h.layerId)).toBe(false);
    const commit = vi.spyOn(h.history, "commit");

    typeRadius(h.container, "30");
    await settle();

    expect(commandLog.types).toEqual([]);
    expect(commit).toHaveBeenCalledTimes(1);
    expect(h.history.getUndoCount()).toBe(1);
    expect(shapeParamsOf(h).radius).toBe(30);
    h.cleanup();
  });
});

// The option bar always sends a complete merged payload, so these malformed
// payloads are not reachable from the UI. They are pinned here because they are
// reachable from any other caller of the funnel, and because the answer has to
// come from the arm, not from an assumption: crates/core/src/model.rs declares
// ShapeParams fields WITHOUT serde defaults, so a missing field, a JSON null in
// a number slot (what NaN stringifies to), or a null in a string slot fails
// envelope deserialization ("E_ENVELOPE_PARSE") and the WHOLE command is
// rejected. Nothing is stored, so the previous params survive. The TS emulator
// cannot reproduce this class: it receives already-parsed objects and only
// rejects the both-absent case (bridge_emu.ts setLayerParams).
describe("shape params invalid payloads (real arm)", () => {
  const cases: Array<{ label: string; payload: unknown; message: RegExp }> = [
    { label: "empty params object", payload: {}, message: /E_ENVELOPE_PARSE: missing field `kind`/ },
    {
      label: "NaN radius (stringifies to null)",
      payload: { ...PARAMS, radius: Number.NaN },
      message: /E_ENVELOPE_PARSE: invalid type: null, expected f64/,
    },
    {
      label: "null fill color",
      payload: { ...PARAMS, fill: { kind: "solid", color: null } },
      message: /E_ENVELOPE_PARSE: invalid type: null, expected a string/,
    },
  ];

  for (const c of cases) {
    it(`${c.label}: rejected, and the arm keeps its previous params`, async () => {
      const h = await setup({ owned: true });
      const before = shapeParamsOf(h);

      await expect(
        commitFacadeParams(h.engine, [h.layerId], { shapeParams: c.payload as never }),
      ).rejects.toThrow(c.message);

      const snapshot = await getSnapshot(DOC);
      const native = (snapshot as { layers: Array<{ id: string; shapeParams?: ShapeParams }> }).layers.find(
        (l) => l.id === h.layerId,
      )!;
      expect(native.shapeParams).toEqual(PARAMS);
      expect(shapeParamsOf(h)).toEqual(before);
      h.cleanup();
    });
  }
});
