// SPDX-License-Identifier: AGPL-3.0-or-later
//
// commitFacadeParams: the routing funnel for parametric layer payloads
// (textData / shapeParams).
//
// The funnel is cloned from commitFacadeAdjustment, so the ownership policy is
// the shared one: an id set that is entirely facade-owned routes ONE command per
// id, an id set with no owned member falls through to the legacy caller, and a
// mixed set is rejected with ZERO commands so no half-applied state can exist.
//
// Real wasm arm, real DocumentEngine, real facade projection. Command counts are
// taken at the single boundary every protocol command crosses (bridge
// applyCommand), not at a facade stub.

import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { DocumentEngine } from "@/engine/document";
import { DEFAULT_TEXT_DATA, type TextData } from "@/engine/textTypes";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import {
  commitFacadeParams,
  getFacade,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";
import { applyCommand, getSnapshot } from "@/lib/protocol/bridge";
import { CONTRACT_VERSION, type CommandEnvelope } from "@/lib/protocol/types";

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

// jsdom has no OffscreenCanvas; stub the seam the text rasterizer uses so a typed
// add produces a bitmap instead of falling through to a null 2d context.
function stubOffscreenCanvas(): void {
  const Mock = function (this: any, w: number, h: number) {
    this.width = w;
    this.height = h;
    const ctx: any = {
      font: "",
      fillStyle: "",
      strokeStyle: "",
      lineWidth: 0,
      lineJoin: "miter",
      miterLimit: 10,
      globalAlpha: 1,
      globalCompositeOperation: "source-over",
      textBaseline: "alphabetic",
      letterSpacing: undefined,
      measureText: (s: string) => ({
        width: s.length * 10,
        actualBoundingBoxAscent: 80,
        actualBoundingBoxDescent: 24,
        fontBoundingBoxAscent: 80,
        fontBoundingBoxDescent: 24,
      }),
      fillText: () => {},
      strokeText: () => {},
      drawImage: () => {},
      save: () => {},
      restore: () => {},
      translate: () => {},
      scale: () => {},
      rotate: () => {},
      fillRect: () => {},
    };
    this.getContext = () => ctx;
    this.transferToImageBitmap = () => ({ width: this.width, height: this.height, close: () => {} });
  } as unknown as typeof OffscreenCanvas;
  vi.stubGlobal("OffscreenCanvas", Mock);
}

let wasm: { protocol_reset: (docId: string) => void } | null = null;
const usedDocs: string[] = [];

beforeAll(async () => {
  wasm = await getWasmExportModule();
});

beforeEach(() => {
  localStorage.setItem("photrez.facade", "1");
  localStorage.removeItem("photrez.facadeAuthority");
  stubOffscreenCanvas();
});

afterEach(() => {
  for (const id of usedDocs) wasm?.protocol_reset(id);
  usedDocs.length = 0;
  localStorage.removeItem("photrez.facade");
  localStorage.removeItem("photrez.facadeAuthority");
  __resetFacadeRegistryForTests();
  (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

/**
 * A document with one facade-owned text layer and one plain legacy layer.
 * Ownership is established by one projection (applyFacadeSnapshot marks every
 * layer it carries as owned). The text layer goes in through the typed-add arm
 * first: a layer the TS engine creates on its own is invisible to the protocol
 * arms, and an arm that does not hold the layer restates nothing, so the params
 * command would report success while the projection carried no textData.
 */
async function setupDoc(id: string) {
  usedDocs.push(id);
  const engine = new DocumentEngine(id, id, 800, 600);
  const textData = {
    ...DEFAULT_TEXT_DATA,
    content: "hello",
    boxMode: "area" as const,
    boxWidth: 200,
    boxHeight: 100,
  };
  const layer = engine.addTextLayer("Text", textData);
  await applyCommand({
    contractVersion: CONTRACT_VERSION,
    expectedVersion: 0,
    docId: id,
    command: {
      type: "addLayer",
      id: layer.id,
      name: "Text",
      width: 200,
      height: 100,
      index: 0,
      layerType: "text",
      textData,
    } as never,
  });
  const snapshot = await getSnapshot(id);
  const facade = getFacade(id);
  facade.syncRenderedVersionTo(snapshot.version);
  engine.applyFacadeSnapshot(snapshot as never);
  const legacy = engine.addLayer("Legacy", 40, 40);
  commandLog.types.length = 0;
  return { engine, ownedId: layer.id, legacyId: legacy.id };
}

const boxed = (content: string, boxWidth: number, boxHeight: number): TextData => ({
  ...DEFAULT_TEXT_DATA,
  content,
  boxMode: "area",
  boxWidth,
  boxHeight,
});

describe("commitFacadeParams", () => {
  it("owned target: exactly one command and the projection writes textData back", async () => {
    const { engine, ownedId } = await setupDoc("docParamsOwned");
    const next = boxed("resized", 320, 160);

    const res = await commitFacadeParams(engine as never, [ownedId], { textData: next });

    expect(res).toEqual({ status: "applied", count: 1 });
    expect(commandLog.types).toEqual(["setLayerParams"]);
    expect(engine.getLayer(ownedId)!.textData).toEqual(next);
  });

  it("legacy target: no command, status legacy, model untouched", async () => {
    const { engine, legacyId } = await setupDoc("docParamsLegacy");

    const res = await commitFacadeParams(engine as never, [legacyId], { textData: boxed("nope", 10, 10) });

    expect(res).toEqual({ status: "legacy" });
    expect(commandLog.types).toEqual([]);
  });

  it("mixed ownership: rejected with zero commands, so nothing is half-applied", async () => {
    const { engine, ownedId, legacyId } = await setupDoc("docParamsMixed");
    const before = engine.getLayer(ownedId)!.textData;

    const res = await commitFacadeParams(engine as never, [ownedId, legacyId], { textData: boxed("nope", 10, 10) });

    expect(res).toEqual({ status: "mixed-rejected" });
    expect(commandLog.types).toEqual([]);
    expect(engine.getLayer(ownedId)!.textData).toEqual(before);
  });

  it("empty selection: status empty, zero commands", async () => {
    const { engine } = await setupDoc("docParamsEmpty");

    const res = await commitFacadeParams(engine as never, [], { textData: boxed("nope", 10, 10) });

    expect(res).toEqual({ status: "empty" });
    expect(commandLog.types).toEqual([]);
  });

  it("unknown or blank id is not owned, so it falls through to legacy with zero commands", async () => {
    const { engine } = await setupDoc("docParamsUnknown");

    expect(await commitFacadeParams(engine as never, ["no-such-layer"], { textData: boxed("x", 10, 10) })).toEqual({
      status: "legacy",
    });
    expect(await commitFacadeParams(engine as never, [""], { textData: boxed("x", 10, 10) })).toEqual({
      status: "legacy",
    });
    expect(commandLog.types).toEqual([]);
  });

  // The native arm (crates/core document_core_apply.rs SetLayerParams) rejects an
  // envelope that carries neither half BEFORE it mutates anything, so the funnel
  // rejects rather than reporting a silent success.
  it("both halves absent: the native arm rejects E_INVALID before any mutation", async () => {
    const { engine, ownedId } = await setupDoc("docParamsBothAbsent");

    await expect(commitFacadeParams(engine as never, [ownedId], {})).rejects.toThrow(/E_INVALID/);
    expect(engine.getLayer(ownedId)!.textData!.content).toBe("hello");
  });

  // Mock-fidelity note: JSON has no NaN. Number.NaN serializes to null on the
  // wire, so the native arm's typed deserialization rejects the whole envelope
  // (E_ENVELOPE_PARSE). It does NOT clamp the value, and it is NOT E_INVALID.
  it("non-finite boxWidth is a wire-level parse rejection, not a clamp", async () => {
    const { engine, ownedId } = await setupDoc("docParamsNaN");

    await expect(
      commitFacadeParams(engine as never, [ownedId], {
        textData: { ...boxed("x", 100, 100), boxWidth: Number.NaN },
      }),
    ).rejects.toThrow(/E_ENVELOPE_PARSE/);
    expect(engine.getLayer(ownedId)!.textData!.boxWidth).toBe(200);
  });

  // Real behavior, reported rather than wished away: neither the native arm nor
  // applyFacadeSnapshot normalizes the payload - clamping lives in
  // normalizeTextData, which only the TS rasterizer path calls. So a routed
  // commit CAN land a degenerate box in the model.
  it("zero box size is stored verbatim (no clamping on this path)", async () => {
    const { engine, ownedId } = await setupDoc("docParamsZero");

    const res = await commitFacadeParams(engine as never, [ownedId], { textData: boxed("x", 0, 0) });

    expect(res.status).toBe("applied");
    expect(engine.getLayer(ownedId)!.textData!.boxWidth).toBe(0);
  });
});
