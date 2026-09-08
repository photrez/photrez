// Emulator parity for the metadata command arms.
//
// Drives the TS emulator (wasm NOT armed in this file, so applyCommand falls
// through to emulateApply) through each metadata arm and asserts the emulator
// mirrors the Rust ProtocolEngine arm semantics: silent no-op on an unknown id
// (mirroring DeleteLayer + the TS apply ops), and E_INVALID on an empty or
// duplicate addLayer id. The emulator layer shape is the flat RenderLayer, so
// metadata fields live top-level (flipH/flipV included).
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as bridge from "@/lib/protocol/bridge";
import { __resetEmulatedForTests } from "@/lib/protocol/bridge";
import { CONTRACT_VERSION } from "../types";
import type { RenderLayer } from "../types";

function mkLayer(id: string, name: string, opacity = 1): RenderLayer {
  return {
    id,
    name,
    visible: true,
    opacity,
    resourceId: 1,
    x: 0,
    y: 0,
    scaleX: 1,
    scaleY: 1,
    rotation: 0,
    dirtyRect: { x: 0, y: 0, width: 1, height: 1 },
  };
}

function apply(command: any): Promise<any> {
  return bridge.applyCommand({ contractVersion: CONTRACT_VERSION, command });
}
function addedId(result: any): string {
  return result.delta.changes[0].layer.id;
}
function addedLayer(result: any): RenderLayer {
  return result.delta.changes[0].layer as RenderLayer;
}

beforeEach(() => {
  __resetEmulatedForTests();
});
afterEach(() => {
  __resetEmulatedForTests();
});

describe("emulator metadata arms mirror the Rust arms", () => {
  it("addLayer then setVisible flips visible=false (delta reflects it)", async () => {
    const add = await apply({ type: "addLayer", id: "A", name: "A", width: 10, height: 10, index: 0 });
    const id = addedId(add);
    const res = await apply({ type: "setVisible", id, visible: false });
    expect(res.delta.changes).toHaveLength(1);
    expect(addedLayer(res).visible).toBe(false);
  });

  it("setLocked applies each of the 4 kinds to its named field", async () => {
    const add = await apply({ type: "addLayer", id: "A", name: "A", width: 10, height: 10, index: 0 });
    const id = addedId(add);

    const base = await apply({ type: "setLocked", id, kind: "base", locked: true });
    expect(addedLayer(base).locked).toBe(true);

    const trans = await apply({ type: "setLocked", id, kind: "transparency", locked: true });
    expect(addedLayer(trans).lockTransparency).toBe(true);

    const pos = await apply({ type: "setLocked", id, kind: "position", locked: true });
    expect(addedLayer(pos).lockPosition).toBe(true);

    const rot = await apply({ type: "setLocked", id, kind: "rotation", locked: true });
    expect(addedLayer(rot).lockRotation).toBe(true);
  });

  it("rename changes the layer name (delta reflects it)", async () => {
    const add = await apply({ type: "addLayer", id: "A", name: "A", width: 10, height: 10, index: 0 });
    const id = addedId(add);
    const res = await apply({ type: "rename", id, name: "Renamed" });
    expect(addedLayer(res).name).toBe("Renamed");
  });

  it("reorder moves a layer to the target index (delta order reflects it)", async () => {
    await apply({ type: "addLayer", id: "A", name: "A", width: 10, height: 10, index: 0 });
    await apply({ type: "addLayer", id: "B", name: "B", width: 10, height: 10, index: 0 });
    await apply({ type: "addLayer", id: "C", name: "C", width: 10, height: 10, index: 0 });
    // emulator order after three inserts-at-0 is [C, B, A]; move B (index 1) to top.
    const res = await apply({ type: "reorder", id: "B", to: 0 });
    const order = res.delta.changes.map((ch: any) => ch.layer.name);
    expect(order).toEqual(["B", "C", "A"]);
  });

  it("setBackgroundFlag sets isBackground + lockPosition + lockRotation", async () => {
    const add = await apply({ type: "addLayer", id: "A", name: "A", width: 10, height: 10, index: 0 });
    const id = addedId(add);
    const res = await apply({ type: "setBackgroundFlag", id });
    const l = addedLayer(res);
    expect(l.isBackground).toBe(true);
    expect(l.lockPosition).toBe(true);
    expect(l.lockRotation).toBe(true);
  });

  it("setBlendMode sets blendMode (delta reflects it)", async () => {
    const add = await apply({ type: "addLayer", id: "A", name: "A", width: 10, height: 10, index: 0 });
    const id = addedId(add);
    const res = await apply({ type: "setBlendMode", id, mode: "multiply" });
    expect(addedLayer(res).blendMode).toBe("multiply");
  });

  it("transformLayer carries flipH/flipV (delta reflects it)", async () => {
    const add = await apply({ type: "addLayer", id: "A", name: "A", width: 10, height: 10, index: 0 });
    const id = addedId(add);
    const res = await apply({ type: "transformLayer", id, transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: true, flipV: true } });
    const l = addedLayer(res);
    expect(l.flipH).toBe(true);
    expect(l.flipV).toBe(true);
  });

  it("unknown id is a silent no-op on every metadata arm (mirrors DeleteLayer)", async () => {
    const arms = [
      { type: "setVisible", id: "ghost", visible: false },
      { type: "setLocked", id: "ghost", kind: "base", locked: true },
      { type: "rename", id: "ghost", name: "x" },
      { type: "reorder", id: "ghost", to: 0 },
      { type: "setBackgroundFlag", id: "ghost" },
      { type: "setBlendMode", id: "ghost", mode: "multiply" },
    ];
    for (const cmd of arms) {
      const res = await apply(cmd);
      expect(res.delta.changes, `${cmd.type} unknown id must not mutate`).toHaveLength(0);
    }
  });

  it("addLayer rejects an empty id with E_INVALID", async () => {
    await expect(
      apply({ type: "addLayer", id: "", name: "A", width: 10, height: 10, index: 0 }),
    ).rejects.toThrow(/E_INVALID/);
  });

  it("addLayer rejects a duplicate id with E_INVALID", async () => {
    await apply({ type: "addLayer", id: "dup", name: "A", width: 10, height: 10, index: 0 });
    await expect(
      apply({ type: "addLayer", id: "dup", name: "B", width: 10, height: 10, index: 0 }),
    ).rejects.toThrow(/E_INVALID/);
  });

  it("background-layer reorder records no history entry but still bumps DV", async () => {
    await apply({ type: "addLayer", id: "A", name: "A", width: 10, height: 10, index: 0 });
    const bg = await apply({ type: "setBackgroundFlag", id: "A" });
    const before = await bridge.getHistoryQuery();
    const beforeDv = bg.documentVersion;
    // The Background is pinned to the bottom and never reordered (no-op for state).
    const res = await apply({ type: "reorder", id: "A", to: 0 });
    const after = await bridge.getHistoryQuery();
    expect(after.entries.length).toBe(before.entries.length);
    expect(res.documentVersion).toBe(beforeDv + 1);
    expect(res.delta.changes).toHaveLength(0);
  });

  it("reorder rejects an out-of-range target index with E_INVALID (no mutation)", async () => {
    await apply({ type: "addLayer", id: "A", name: "A", width: 10, height: 10, index: 0 });
    await apply({ type: "addLayer", id: "B", name: "B", width: 10, height: 10, index: 0 }); // [B, A]
    const before = await bridge.getHistoryQuery();
    await expect(apply({ type: "reorder", id: "A", to: 2 })).rejects.toThrow(/E_INVALID/); // to == len
    await expect(apply({ type: "reorder", id: "A", to: -1 })).rejects.toThrow(/E_INVALID/); // to == -1
    const after = await bridge.getHistoryQuery();
    expect(after).toEqual(before); // no new entry, no DV bump, state unchanged
  });

  it("setLocked transparency-only leaves the other lock kinds unchanged", async () => {
    const add = await apply({ type: "addLayer", id: "A", name: "A", width: 10, height: 10, index: 0 });
    const id = addedId(add);
    const res = await apply({ type: "setLocked", id, kind: "transparency", locked: true });
    const l = addedLayer(res);
    expect(l.lockTransparency).toBe(true);
    expect(l.locked).not.toBe(true);
    expect(l.lockPosition).not.toBe(true);
    expect(l.lockRotation).not.toBe(true);
  });
});

describe("emulator typed-add / setLayerParams / setAdjustment arms", () => {
  const shape = {
    kind: "star" as const, width: 120, height: 80, radius: 6,
    fill: { kind: "solid" as const, color: "#E15A17" },
    stroke: { enabled: true, color: "#000000", width: 2 },
    arrowHead: false,
  };
  const text = {
    content: "Hi", fontFamily: "Arial", fontSize: 32, fontWeight: 400, fontStyle: "normal" as const,
    color: "#000000", align: "left" as const, lineHeight: 1.2, letterSpacing: 0,
    boxMode: "point" as const, boxWidth: 0, boxHeight: 0,
    stroke: { width: 0, color: "#000000" },
  };

  it("typed addLayer (shape) projects layerType + blendMode + shapeParams", async () => {
    const res = await apply({ type: "addLayer", id: "S", name: "Star", width: 120, height: 80, index: 0, layerType: "shape", shapeParams: shape });
    const l = addedLayer(res);
    expect(l.layerType).toBe("shape");
    expect(l.blendMode).toBe("normal");
    expect(l.shapeParams).toEqual(shape);
    expect(l.textData).toBeUndefined();
    expect(l.visible).toBe(true);
    expect(l.opacity).toBe(1);
  });

  it("typed addLayer (text) projects layerType + textData", async () => {
    const res = await apply({ type: "addLayer", id: "T", name: "Text", width: 100, height: 20, index: 0, layerType: "text", textData: text });
    const l = addedLayer(res);
    expect(l.layerType).toBe("text");
    expect(l.textData).toEqual(text);
    expect(l.shapeParams).toBeUndefined();
  });

  it("addLayer without typed fields stays raster/normal (backward compatible)", async () => {
    const res = await apply({ type: "addLayer", id: "R", name: "R", width: 10, height: 10, index: 0 });
    const l = addedLayer(res);
    expect(l.layerType).toBe("raster");
    expect(l.blendMode).toBe("normal");
    expect(l.shapeParams).toBeUndefined();
    expect(l.textData).toBeUndefined();
  });

  it("setLayerParams sets shapeParams only (delta reflects it)", async () => {
    const add = await apply({ type: "addLayer", id: "L1", name: "L1", width: 10, height: 10, index: 0 });
    const id = addedId(add);
    const res = await apply({ type: "setLayerParams", id, shapeParams: shape });
    const l = addedLayer(res);
    expect(l.shapeParams).toEqual(shape);
    expect(l.textData).toBeUndefined();
  });

  it("setLayerParams sets textData only", async () => {
    const add = await apply({ type: "addLayer", id: "L1", name: "L1", width: 10, height: 10, index: 0 });
    const id = addedId(add);
    const res = await apply({ type: "setLayerParams", id, textData: text });
    const l = addedLayer(res);
    expect(l.textData).toEqual(text);
    expect(l.shapeParams).toBeUndefined();
  });

  it("setLayerParams with both absent rejects E_INVALID (no mutation, no DV bump)", async () => {
    const add = await apply({ type: "addLayer", id: "L1", name: "L1", width: 10, height: 10, index: 0 });
    const id = addedId(add);
    const before = await bridge.getHistoryQuery();
    await expect(apply({ type: "setLayerParams", id })).rejects.toThrow(/E_INVALID/);
    const after = await bridge.getHistoryQuery();
    expect(after).toEqual(before);
  });

  it("setLayerParams unknown id is a silent no-op", async () => {
    const res = await apply({ type: "setLayerParams", id: "ghost", shapeParams: shape });
    expect(res.delta.changes).toHaveLength(0);
  });

  it("setAdjustment sets basicAdjustment + derives hasAdjustments true", async () => {
    const add = await apply({ type: "addLayer", id: "L1", name: "L1", width: 10, height: 10, index: 0 });
    const id = addedId(add);
    const res = await apply({ type: "setAdjustment", id, adjustment: { brightness: 10, contrast: 0, saturation: 0 } });
    const l = addedLayer(res);
    expect(l.basicAdjustment).toEqual({ brightness: 10, contrast: 0, saturation: 0 });
    expect(l.hasAdjustments).toBe(true);
  });

  it("setAdjustment all-zero derives hasAdjustments false (still set)", async () => {
    const add = await apply({ type: "addLayer", id: "L1", name: "L1", width: 10, height: 10, index: 0 });
    const id = addedId(add);
    const res = await apply({ type: "setAdjustment", id, adjustment: { brightness: 0, contrast: 0, saturation: 0 } });
    const l = addedLayer(res);
    expect(l.basicAdjustment).toEqual({ brightness: 0, contrast: 0, saturation: 0 });
    expect(l.hasAdjustments).toBe(false);
  });

  it("setAdjustment clamps channels to [-100,100]", async () => {
    const add = await apply({ type: "addLayer", id: "L1", name: "L1", width: 10, height: 10, index: 0 });
    const id = addedId(add);
    const res = await apply({ type: "setAdjustment", id, adjustment: { brightness: 500, contrast: -500, saturation: 50 } });
    const l = addedLayer(res);
    expect(l.basicAdjustment).toEqual({ brightness: 100, contrast: -100, saturation: 50 });
  });

  it("setAdjustment with undefined clears and sets hasAdjustments false", async () => {
    const add = await apply({ type: "addLayer", id: "L1", name: "L1", width: 10, height: 10, index: 0 });
    const id = addedId(add);
    await apply({ type: "setAdjustment", id, adjustment: { brightness: 10, contrast: 0, saturation: 0 } });
    const res = await apply({ type: "setAdjustment", id });
    const l = addedLayer(res);
    expect(l.basicAdjustment).toBeUndefined();
    expect(l.hasAdjustments).toBe(false);
  });

  it("setAdjustment unknown id is a silent no-op", async () => {
    const res = await apply({ type: "setAdjustment", id: "ghost", adjustment: { brightness: 5, contrast: 0, saturation: 0 } });
    expect(res.delta.changes).toHaveLength(0);
  });
});
