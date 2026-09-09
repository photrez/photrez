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
import { __resetEmulatedForTests, setEmuDocumentDims, getEmuSelection } from "@/lib/protocol/bridge";
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

describe("emulator selection arms", () => {
  beforeEach(() => {
    __resetEmulatedForTests();
  });
  afterEach(() => {
    __resetEmulatedForTests();
  });

  it("setSelection stores selection, bumps DV, commits no history entry", async () => {
    const before = await bridge.getHistoryQuery();
    const res = await apply({ type: "setSelection", selection: { x: 10, y: 20, width: 30, height: 40, angle: 5, shape: "ellipse", inverted: false } });
    // selection surfaces via the emu state hook (CommandResult has no selection field)
    expect(getEmuSelection()).toEqual({ x: 10, y: 20, width: 30, height: 40, angle: 5, shape: "ellipse", inverted: false });
    expect(res.delta.changes).toHaveLength(0);
    const after = await bridge.getHistoryQuery();
    expect(after.entries.length).toBe(before.entries.length);
    expect(res.documentVersion).toBe(before.cursor + 1);
  });

  it("clearSelection resets the emulator selection", async () => {
    await apply({ type: "setSelection", selection: { x: 1, y: 2, width: 3, height: 4, angle: 0 } });
    expect(getEmuSelection()).not.toBeNull();
    const res = await apply({ type: "clearSelection" });
    expect(getEmuSelection()).toBeNull();
    expect(res.delta.changes).toHaveLength(0);
  });

  it("selectAll fills the full canvas from the emu doc dims hook", async () => {
    setEmuDocumentDims(200, 150);
    const res = await apply({ type: "selectAll" });
    expect(getEmuSelection()).toEqual({ x: 0, y: 0, width: 200, height: 150, angle: 0, shape: undefined, inverted: undefined });
    expect(res.delta.changes).toHaveLength(0);
  });

  it("invertSelection toggles inverted and bumps DV without an entry", async () => {
    const before = await bridge.getHistoryQuery();
    await apply({ type: "setSelection", selection: { x: 1, y: 2, width: 3, height: 4, angle: 0, inverted: false } });
    const res = await apply({ type: "invertSelection" });
    expect(getEmuSelection().inverted).toBe(true);
    const after = await bridge.getHistoryQuery();
    expect(after.entries.length).toBe(before.entries.length);
    expect(res.documentVersion).toBe(before.cursor + 2);
  });

  it("invertSelection without a selection falls back to full-canvas selection", async () => {
    // Dims hook set + no selection -> falls back to the same full-canvas rect
    // selectAll builds (mirrors the host op). DV bumps, no history entry.
    setEmuDocumentDims(200, 150);
    const before = await bridge.getHistoryQuery();
    const res = await apply({ type: "invertSelection" });
    expect(getEmuSelection()).toEqual({ x: 0, y: 0, width: 200, height: 150, angle: 0, shape: undefined, inverted: undefined });
    const after = await bridge.getHistoryQuery();
    expect(after.entries.length).toBe(before.entries.length);
    expect(res.documentVersion).toBe(before.cursor + 1);

    // No hook + no selection -> E_INVALID (mirrors host select-all requirement).
    __resetEmulatedForTests();
    let threw: any = null;
    try {
      await apply({ type: "invertSelection" });
    } catch (e) {
      threw = e;
    }
    expect(threw).not.toBeNull();
    expect(threw.code).toBe("E_INVALID");
  });
});

// --- Structural arms (Duplicate / MergeDown / MergeSelected / Flatten / Rasterize) ---
// Each arm is metadata-only in the emulator (no pixel work). The delta after an
// arm carries every remaining layer as an upsert plus removes for vanished ids,
// so the final ordered layer set is readable straight from the delta.
async function addTop(id: string, opts: any = {}): Promise<any> {
  return apply({ type: "addLayer", id, name: id, width: 10, height: 10, index: 0, ...opts });
}
// ids[0] is the TOP of stack. addLayer inserts at index 0 (newest on top), so we
// add in reverse to land in the requested order.
async function stack(ids: string[]): Promise<void> {
  for (let i = ids.length - 1; i >= 0; i--) await addTop(ids[i]);
}
function upsertedIds(res: any): string[] {
  return res.delta.changes.filter((c: any) => c.kind === "upsert").map((c: any) => c.layer.id);
}
function removedIds(res: any): string[] {
  return res.delta.changes.filter((c: any) => c.kind === "remove").map((c: any) => c.id);
}
async function entryCount(): Promise<number> {
  return (await bridge.getHistoryQuery()).entries.length;
}

describe("emulator structural arms mirror the Rust arms", () => {
  it("duplicateLayer clones above source, mints a fresh resource id, and clears locks/bg", async () => {
    await stack(["A", "B"]); // [A, B] (A top)
    const before = await entryCount();
    const res = await apply({ type: "duplicateLayer", id: "A", newId: "A2" });
    const clone = res.delta.changes.find((c: any) => c.kind === "upsert" && c.layer.id === "A2").layer;
    expect(clone).toBeDefined();
    expect(clone.id).toBe("A2");
    expect(clone.name).toBe("A 2"); // nextDuplicateName bumps the numeric suffix
    expect(clone.resourceId).not.toBe(2); // fresh resource id, not the source's (A = rid 2)
    expect(clone.locked).toBe(false);
    expect(clone.isBackground).toBeUndefined();
    expect(clone.lockPosition).toBeUndefined();
    expect(clone.lockRotation).toBeUndefined();
    // Clone placed above the source (A2 at top, source A below it).
    expect(upsertedIds(res)).toEqual(["A2", "A", "B"]);
    expect(res.documentVersion).toBe(3);
    expect(await entryCount()).toBe(before + 1);
  });

  it("duplicateLayer unknown id is a silent no-op", async () => {
    await stack(["A", "B"]);
    const res = await apply({ type: "duplicateLayer", id: "ghost", newId: "G" });
    expect(res.delta.changes).toHaveLength(0);
  });

  it("duplicateLayer rejects an empty newId with E_INVALID", async () => {
    await stack(["A", "B"]);
    await expect(apply({ type: "duplicateLayer", id: "A", newId: "" })).rejects.toThrow(/E_INVALID/);
  });

  it("duplicateLayer rejects a duplicate newId with E_INVALID", async () => {
    await stack(["A", "B"]);
    await expect(apply({ type: "duplicateLayer", id: "A", newId: "B" })).rejects.toThrow(/E_INVALID/);
  });

  it("duplicateLayer undo removes the clone, redo re-adds it", async () => {
    await stack(["A", "B"]);
    await apply({ type: "duplicateLayer", id: "A", newId: "A2" });
    const u = await apply({ type: "undo" });
    expect(removedIds(u)).toEqual(["A2"]);
    const r = await apply({ type: "redo" });
    expect(upsertedIds(r)).toEqual(["A2"]);
  });

  it("mergeDown fuses top+bottom into 'top + bottom', inheriting bottom blend + either-lock", async () => {
    await stack(["A", "B"]); // [A, B] (A top, B below)
    await apply({ type: "setBlendMode", id: "B", mode: "multiply" });
    await apply({ type: "setLocked", id: "B", kind: "base", locked: true });
    setEmuDocumentDims(100, 100);
    const before = await entryCount();
    const res = await apply({ type: "mergeDown", id: "A", mergedId: "M" });
    expect(removedIds(res)).toEqual(["A", "B"]);
    const merged = upsertedIds(res);
    expect(merged).toEqual(["M"]);
    const m = res.delta.changes.find((c: any) => c.kind === "upsert" && c.layer.id === "M").layer;
    expect(m.name).toBe("A + B");
    expect(m.blendMode).toBe("multiply"); // inherited from bottom (B)
    expect(m.locked).toBe(true); // either source locked
    expect(m.layerType).toBe("raster");
    expect(m.width).toBe(100);
    expect(m.height).toBe(100);
    expect(res.documentVersion).toBe(5);
    expect(await entryCount()).toBe(before + 1);
  });

  it("mergeDown merged node mints a fresh resource id distinct from both sources", async () => {
    await stack(["A", "B"]); // [A, B] (A top, B below)
    setEmuDocumentDims(100, 100);
    const snapBefore = await bridge.getSnapshot();
    const srcRids = snapBefore.layers.map((l: any) => l.resourceId);
    const res = await apply({ type: "mergeDown", id: "A", mergedId: "M" });
    const m = res.delta.changes.find((c: any) => c.kind === "upsert" && c.layer.id === "M").layer;
    // The merged node must own an independent resource id - not any source's.
    expect(srcRids.includes(m.resourceId)).toBe(false);
  });

  it("mergeDown on the bottom-most layer is a silent no-op", async () => {
    await stack(["A", "B"]); // B is bottom
    setEmuDocumentDims(100, 100);
    const res = await apply({ type: "mergeDown", id: "B", mergedId: "M" });
    expect(res.delta.changes).toHaveLength(0);
  });

  it("mergeDown unknown id is a silent no-op", async () => {
    await stack(["A", "B"]);
    setEmuDocumentDims(100, 100);
    const res = await apply({ type: "mergeDown", id: "ghost", mergedId: "M" });
    expect(res.delta.changes).toHaveLength(0);
  });

  it("mergeDown rejects empty / present mergedId with E_INVALID", async () => {
    await stack(["A", "B"]);
    setEmuDocumentDims(100, 100);
    await expect(apply({ type: "mergeDown", id: "A", mergedId: "" })).rejects.toThrow(/E_INVALID/);
    await expect(apply({ type: "mergeDown", id: "A", mergedId: "B" })).rejects.toThrow(/E_INVALID/);
  });

  it("mergeDown requires seeded document dims (E_INVALID otherwise)", async () => {
    await stack(["A", "B"]);
    await expect(apply({ type: "mergeDown", id: "A", mergedId: "M" })).rejects.toThrow(/E_INVALID/);
  });

  it("mergeDown undo restores the pair, redo re-merges", async () => {
    await stack(["A", "B"]);
    setEmuDocumentDims(100, 100);
    await apply({ type: "mergeDown", id: "A", mergedId: "M" });
    const u = await apply({ type: "undo" });
    expect(upsertedIds(u).sort()).toEqual(["A", "B"]);
    expect(removedIds(u)).toEqual(["M"]);
    const r = await apply({ type: "redo" });
    expect(removedIds(r).sort()).toEqual(["A", "B"]);
    expect(upsertedIds(r)).toEqual(["M"]);
  });

  it("mergeSelected fuses 2 into 'A + B' and 3 into 'A (+2 merged)', locked = any", async () => {
    await stack(["A", "B", "C"]); // [A, B, C]
    await apply({ type: "setLocked", id: "C", kind: "base", locked: true });
    setEmuDocumentDims(50, 50);
    const two = await apply({ type: "mergeSelected", ids: ["A", "B"], mergedId: "M2" });
    const m2 = two.delta.changes.find((c: any) => c.kind === "upsert" && c.layer.id === "M2").layer;
    expect(m2.name).toBe("A + B");
    expect(m2.locked).toBe(false); // neither A nor B locked
    expect(upsertedIds(two)).toEqual(["M2", "C"]); // placed at A's (top) index

    await apply({ type: "undo" });
    setEmuDocumentDims(50, 50);
    const three = await apply({ type: "mergeSelected", ids: ["A", "B", "C"], mergedId: "M3" });
    const m3 = three.delta.changes.find((c: any) => c.kind === "upsert" && c.layer.id === "M3").layer;
    expect(m3.name).toBe("A (+2 merged)");
    expect(m3.locked).toBe(true); // C was locked
    expect(m3.blendMode).toBe("normal");
  });

  it("mergeSelected with <2 matched ids is a silent no-op", async () => {
    await stack(["A", "B"]);
    setEmuDocumentDims(50, 50);
    const one = await apply({ type: "mergeSelected", ids: ["A"], mergedId: "M" });
    expect(one.delta.changes).toHaveLength(0);
    const none = await apply({ type: "mergeSelected", ids: ["ghost", "phantom"], mergedId: "M" });
    expect(none.delta.changes).toHaveLength(0);
  });

  it("mergeSelected rejects empty / present mergedId with E_INVALID", async () => {
    await stack(["A", "B"]);
    setEmuDocumentDims(50, 50);
    await expect(apply({ type: "mergeSelected", ids: ["A", "B"], mergedId: "" })).rejects.toThrow(/E_INVALID/);
    await expect(apply({ type: "mergeSelected", ids: ["A", "B"], mergedId: "A" })).rejects.toThrow(/E_INVALID/);
  });

  it("mergeSelected requires seeded document dims (E_INVALID otherwise)", async () => {
    await stack(["A", "B"]);
    await expect(apply({ type: "mergeSelected", ids: ["A", "B"], mergedId: "M" })).rejects.toThrow(/E_INVALID/);
  });

  it("flatten collapses >1 layer into one Background node (bg flag + pos/rot locks, not locked)", async () => {
    await stack(["A", "B", "C"]);
    setEmuDocumentDims(80, 60);
    const before = await entryCount();
    const res = await apply({ type: "flatten", mergedId: "BG" });
    expect(removedIds(res).sort()).toEqual(["A", "B", "C"]);
    expect(upsertedIds(res)).toEqual(["BG"]);
    const bg = res.delta.changes.find((c: any) => c.kind === "upsert" && c.layer.id === "BG").layer;
    expect(bg.name).toBe("Background");
    expect(bg.isBackground).toBe(true);
    expect(bg.lockPosition).toBe(true);
    expect(bg.lockRotation).toBe(true);
    expect(bg.locked).toBe(false);
    expect(bg.layerType).toBe("raster");
    expect(bg.width).toBe(80);
    expect(bg.height).toBe(60);
    expect(res.documentVersion).toBe(4);
    expect(await entryCount()).toBe(before + 1);
  });

  it("flatten on a single layer is a silent no-op", async () => {
    await stack(["A"]);
    setEmuDocumentDims(80, 60);
    const res = await apply({ type: "flatten", mergedId: "BG" });
    expect(res.delta.changes).toHaveLength(0);
  });

  it("flatten rejects empty / present mergedId with E_INVALID", async () => {
    await stack(["A", "B"]);
    setEmuDocumentDims(80, 60);
    await expect(apply({ type: "flatten", mergedId: "" })).rejects.toThrow(/E_INVALID/);
    await expect(apply({ type: "flatten", mergedId: "A" })).rejects.toThrow(/E_INVALID/);
  });

  it("flatten requires seeded document dims (E_INVALID otherwise)", async () => {
    await stack(["A", "B"]);
    await expect(apply({ type: "flatten", mergedId: "BG" })).rejects.toThrow(/E_INVALID/);
  });

  it("rasterizeLayer drops shape/text params; non-parametric and unknown ids no-op", async () => {
    await addTop("S", { layerType: "shape", shapeParams: { kind: "star" } });
    await addTop("T", { layerType: "text", textData: { content: "x" } });
    await addTop("R"); // raster by default
    const s = await apply({ type: "rasterizeLayer", id: "S" });
    const sl = s.delta.changes.find((c: any) => c.layer.id === "S").layer;
    expect(sl.layerType).toBe("raster");
    expect(sl.shapeParams).toBeUndefined();
    const t = await apply({ type: "rasterizeLayer", id: "T" });
    const tl = t.delta.changes.find((c: any) => c.layer.id === "T").layer;
    expect(tl.layerType).toBe("raster");
    expect(tl.textData).toBeUndefined();
    const r = await apply({ type: "rasterizeLayer", id: "R" });
    expect(r.delta.changes).toHaveLength(0); // non-parametric: no-op
    const g = await apply({ type: "rasterizeLayer", id: "ghost" });
    expect(g.delta.changes).toHaveLength(0); // unknown id: no-op
  });
});
