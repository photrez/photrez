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
});
