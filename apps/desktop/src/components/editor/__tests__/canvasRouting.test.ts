// Routing of the two document-size ops (resize canvas, apply crop). Mirrors
// facadeStructuralOps.test.ts but exercises the CANVAS arms under native
// authority: the routes gate on the facade flag plus native authority, run the
// host floors (finite/positive/device max), the resize memory budget, and the
// pixel-baking crop variants (which stay on the host path), then dispatch the
// facade command and project the projected document size back into the model.
//
// The arms already exist in the Rust engine and are proven MEASURED-EQUAL in the
// parity matrix; this file proves the PRODUCTION dispatch (EditorFacade methods +
// commitFacadeX funnels) issues the correct envelope (camel at the seam, snake on
// the wire), adopts the returned document size, and defers to legacy when asked.

import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from "vitest";
import { DocumentEngine } from "@/engine/document";
import { getEffectiveMaxDim } from "@/engine/types";
import * as bridge from "@/lib/protocol/bridge";
import {
  getFacade,
  seedFacadeFromEngine,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";
import { routeResizeCanvas, routeApplyCrop } from "../canvasRouting";
import { installCanvasRouteEmulator, type CanvasRouteEmulator } from "@/__tests__/canvasRouteEmulator";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as Mock<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>;

let emulator: CanvasRouteEmulator;

const makeRenderer = () => ({ uploadImage: vi.fn() });
const makeScheduler = () => ({ requestRender: vi.fn() });

beforeEach(() => {
  localStorage.setItem("photrez.facade", "1");
  localStorage.setItem("photrez.facadeAuthority", "native");
  bridge.__resetNativeAuthorityForTests();
  invokeMock.mockReset();
  emulator = installCanvasRouteEmulator(invokeMock);
});

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.removeItem("photrez.facade");
  localStorage.removeItem("photrez.facadeAuthority");
  bridge.__resetNativeAuthorityForTests();
  invokeMock.mockReset();
  __resetFacadeRegistryForTests();
  emulator.reset();
});

async function newDoc(id: string, withLayer = false) {
  const engine = new DocumentEngine(id, id, 800, 600);
  if (withLayer) {
    const layer = engine.addLayer("A", 100, 100);
    layer.imageBitmap = { width: 4, height: 4 } as unknown as ImageBitmap;
  }
  const facade = getFacade(id);
  await seedFacadeFromEngine(engine as never, facade);
  return { engine, facade };
}

function typeOf(call: unknown): string {
  return (call as { command: { type: string } }).command.type;
}

describe("routeResizeCanvas", () => {
  it("flag OFF: legacy status, zero applyCommand", async () => {
    localStorage.removeItem("photrez.facade");
    const { engine } = await newDoc("canvas1");
    const spy = vi.spyOn(bridge, "applyCommand");
    const status = await routeResizeCanvas(
      engine,
      {},
      makeRenderer() as never,
      makeScheduler() as never,
      400,
      300,
    );
    expect(status).toBe("legacy");
    expect(spy).not.toHaveBeenCalled();
  });

  it("applied: camel envelope + expectedVersion, engine and snapshot adopt the new size, no TS history commit", async () => {
    const { engine, facade } = await newDoc("canvas1", true);
    const renderer = makeRenderer();
    const scheduler = makeScheduler();
    const history = { commit: vi.fn() };
    const vBefore = facade.renderedVersion;
    const spy = vi.spyOn(bridge, "applyCommand");

    const status = await routeResizeCanvas(
      engine,
      history,
      renderer as never,
      scheduler as never,
      1024,
      768,
    );

    expect(status).toBe("applied");
    expect(history.commit).not.toHaveBeenCalled();
    expect(spy).toHaveBeenCalledTimes(1);
    const env = spy.mock.calls[0][0] as unknown as {
      expectedVersion?: number;
      command: { type: string; width: number; height: number };
    };
    expect(env.command.type).toBe("resizeCanvas");
    expect(env.command.width).toBe(1024);
    expect(env.command.height).toBe(768);
    expect(env.expectedVersion).toBe(vBefore);
    expect(facade.snapshot.width).toBe(1024);
    expect(facade.snapshot.height).toBe(768);
    expect(engine.getWidth()).toBe(1024);
    expect(engine.getHeight()).toBe(768);
    // The route re-uploads layer textures and asks for a redraw.
    expect(renderer.uploadImage).toHaveBeenCalledTimes(1);
    expect(scheduler.requestRender).toHaveBeenCalledOnce();
  });

  it("device max-dim rejects with error and zero applyCommand", async () => {
    const { engine } = await newDoc("canvas1");
    const spy = vi.spyOn(bridge, "applyCommand");
    const status = await routeResizeCanvas(
      engine,
      {},
      makeRenderer() as never,
      makeScheduler() as never,
      getEffectiveMaxDim() + 1,
      100,
    );
    expect(status).toBe("error");
    expect(spy).not.toHaveBeenCalled();
  });

  it("memory budget rejects with error and zero applyCommand", async () => {
    const { engine } = await newDoc("canvas1");
    const big = engine.addLayer("Big", 10, 10);
    big.width = 20000; // RGBA bytes alone exceed MAX_PIXEL_BUDGET (1 GB)
    big.height = 20000;
    const spy = vi.spyOn(bridge, "applyCommand");
    const status = await routeResizeCanvas(
      engine,
      {},
      makeRenderer() as never,
      makeScheduler() as never,
      800,
      600,
    );
    expect(status).toBe("error");
    expect(spy).not.toHaveBeenCalled();
  });
});

describe("routeApplyCrop", () => {
  it("pixel-baking variants return legacy with zero applyCommand", async () => {
    const { engine } = await newDoc("canvas1");
    const spy = vi.spyOn(bridge, "applyCommand");
    const deletePixels = await routeApplyCrop(engine, {}, makeRenderer() as never, makeScheduler() as never, 0, 0, 100, 100, { deleteCroppedPixels: true });
    const fill = await routeApplyCrop(engine, {}, makeRenderer() as never, makeScheduler() as never, 0, 0, 100, 100, { fillBackgroundColor: "#ffffff" });
    expect(deletePixels).toBe("legacy");
    expect(fill).toBe("legacy");
    expect(spy).not.toHaveBeenCalled();
  });

  it("target size above device max rejects with error and zero applyCommand", async () => {
    const { engine } = await newDoc("canvas1");
    const spy = vi.spyOn(bridge, "applyCommand");
    const status = await routeApplyCrop(
      engine,
      {},
      makeRenderer() as never,
      makeScheduler() as never,
      0,
      0,
      100,
      100,
      { targetSize: { w: getEffectiveMaxDim() + 1, h: 100 } },
    );
    expect(status).toBe("error");
    expect(spy).not.toHaveBeenCalled();
  });

  it("applied: camel envelope at the seam, snake target fields on the wire, dims adopted", async () => {
    const { engine, facade } = await newDoc("canvas1", true);
    const history = { commit: vi.fn() };
    const spy = vi.spyOn(bridge, "applyCommand");

    const status = await routeApplyCrop(
      engine,
      history,
      makeRenderer() as never,
      makeScheduler() as never,
      10,
      20,
      50,
      50,
      { rotation: -5, targetSize: { w: 100, h: 200 } },
    );

    expect(status).toBe("applied");
    expect(history.commit).not.toHaveBeenCalled();
    expect(spy).toHaveBeenCalledTimes(1);
    const env = spy.mock.calls[0][0] as unknown as {
      command: { type: string; x: number; rotation?: number; targetWidth?: number; targetHeight?: number };
    };
    expect(env.command.type).toBe("applyCrop");
    expect(env.command.x).toBe(10);
    expect(env.command.rotation).toBe(-5);
    expect(env.command.targetWidth).toBe(100);
    expect(env.command.targetHeight).toBe(200);

    const wire = emulator.wireEnvelopes.find((e) => e.command.type === "applyCrop");
    expect(wire).toBeTruthy();
    expect(wire!.command.target_width).toBe(100);
    expect(wire!.command.target_height).toBe(200);

    expect(facade.snapshot.width).toBe(100);
    expect(facade.snapshot.height).toBe(200);
    expect(engine.getWidth()).toBe(100);
    expect(engine.getHeight()).toBe(200);
  });

  it("applied crop clears the TS model selection (legacy parity) so a later fill is unmasked", async () => {
    const { engine } = await newDoc("canvas1", true);
    engine.createSelection(10, 10, 20, 20);
    expect(engine.getSelection()).not.toBeNull();

    const status = await routeApplyCrop(
      engine,
      {},
      makeRenderer() as never,
      makeScheduler() as never,
      10,
      20,
      50,
      50,
      { rotation: 0 },
    );

    expect(status).toBe("applied");
    // No selection means the fill mask covers the whole layer (unmasked).
    expect(engine.getSelection()).toBeNull();
  });

  it("whole-canvas crop is a no-op: applied without dispatch and no phantom history entry", async () => {
    const { engine } = await newDoc("canvas1", true);
    engine.createSelection(10, 10, 20, 20);
    const spy = vi.spyOn(bridge, "applyCommand");

    const status = await routeApplyCrop(
      engine,
      {},
      makeRenderer() as never,
      makeScheduler() as never,
      0,
      0,
      engine.getWidth(),
      engine.getHeight(),
      { rotation: 0 },
    );

    expect(status).toBe("applied");
    // No native entry is recorded, so there is no phantom step to undo.
    expect(spy).not.toHaveBeenCalled();
    // The visible effect still matches the legacy crop.
    expect(engine.getSelection()).toBeNull();
  });

  it("divergence guard: a stale native size is rebased with a resize BEFORE the crop command", async () => {
    const { engine, facade } = await newDoc("canvas1");
    await facade.resizeCanvas(500, 500);
    engine.applyFacadeSnapshot(facade.snapshot as never);
    // A legacy TS-only size change: the native engine still stores 500x500.
    engine.resizeCanvas(300, 300);
    const spy = vi.spyOn(bridge, "applyCommand");

    const status = await routeApplyCrop(
      engine,
      {},
      makeRenderer() as never,
      makeScheduler() as never,
      0,
      0,
      100,
      100,
      { rotation: 0 },
    );

    expect(status).toBe("applied");
    const types = spy.mock.calls.map((c) => typeOf(c[0]));
    expect(types).toEqual(["resizeCanvas", "applyCrop"]);
    const rebase = spy.mock.calls[0][0] as unknown as { command: { width: number; height: number } };
    expect(rebase.command.width).toBe(300);
    expect(rebase.command.height).toBe(300);
  });
});

describe("route undo of a canvas entry", () => {
  it("undo restores the document size into the model and is reported as a handled step", async () => {
    const { engine, facade } = await newDoc("canvas1");
    await facade.resizeCanvas(1000, 800);
    engine.applyFacadeSnapshot(facade.snapshot as never);
    await facade.resizeCanvas(800, 600);
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getWidth()).toBe(800);

    await facade.undo();
    // Empty layer delta BUT a dims-carrying delta: a handled step, not a no-op.
    expect(facade.lastHistoryDeltaWasEmpty).toBe(false);
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getWidth()).toBe(1000);
    expect(engine.getHeight()).toBe(800);

    await facade.undo();
    expect(facade.lastHistoryDeltaWasEmpty).toBe(true);
  });
});
