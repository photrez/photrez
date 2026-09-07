// Unit tests for the canonical-document seed payload builder. Verifies the JSON
// shape the native Rust engine deserializes (crates/core/src/canonical_model.rs):
// camelCase keys, no transient pixel refs, no resourceId, optional fields omitted
// when absent, and selection present/absent behavior.

import { describe, it, expect } from "vitest";
import { buildCanonicalDocumentPayload } from "../canonicalSeed";
import type { DocumentEngine } from "@/engine/document";
import type { LayerNode, SelectionState } from "@/engine/types";
import { getWasmExportModule } from "@/components/editor/wasmExport";

function rasterLayer(): LayerNode {
  return {
    id: "L-raster",
    name: "Bg",
    type: "raster",
    visible: true,
    opacity: 1,
    locked: false,
    isBackground: true,
    lockTransparency: true,
    blendMode: "multiply",
    transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    width: 800,
    height: 600,
    // Transient pixel references: must NOT appear in the canonical output.
    imageBitmap: {} as ImageBitmap,
    baseImageBitmap: null,
    bitmapEpoch: 7,
  } as LayerNode;
}

function textLayer(): LayerNode {
  return {
    id: "L-text",
    name: "Title",
    type: "text",
    visible: true,
    opacity: 1,
    locked: true,
    blendMode: "normal",
    transform: { x: 10, y: 20, scaleX: 1.5, scaleY: 1.5, rotation: 0, flipH: false, flipV: false },
    width: 300,
    height: 60,
    imageBitmap: null,
    baseImageBitmap: null,
    bitmapEpoch: undefined,
    textData: {
      content: "Hi",
      fontFamily: "Arial",
      fontSize: 32,
      fontWeight: 700,
      fontStyle: "italic",
      color: "#000000",
      align: "center",
      lineHeight: 1.2,
      letterSpacing: 0,
      boxMode: "area",
      boxWidth: 300,
      boxHeight: 60,
      stroke: { width: 2, color: "#FF0000", align: "outside" },
      underline: false,
      strikethrough: false,
      uppercase: true,
    },
  } as LayerNode;
}

function shapeLayer(): LayerNode {
  return {
    id: "L-shape",
    name: "Star",
    type: "shape",
    visible: true,
    opacity: 0.8,
    locked: false,
    blendMode: "screen",
    transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 15, flipH: true, flipV: false },
    width: 200,
    height: 200,
    imageBitmap: null,
    baseImageBitmap: null,
    bitmapEpoch: undefined,
    shapeParams: {
      kind: "star",
      width: 200,
      height: 200,
      radius: 12,
      fill: { kind: "solid", color: "#E15A17" },
      stroke: { enabled: true, color: "#000000", width: 2 },
      arrowHead: false,
    },
  } as LayerNode;
}

function makeEngine(selection: SelectionState | null): DocumentEngine {
  return {
    getId: () => "doc-1",
    getName: () => "Doc",
    getWidth: () => 800,
    getHeight: () => 600,
    getLayers: () => [rasterLayer(), textLayer(), shapeLayer()],
    getSelection: () => selection,
  } as unknown as DocumentEngine;
}

const EXCLUDED = [
  "imageBitmap",
  "baseImageBitmap",
  "bitmapEpoch",
  "resourceId",
  "activeLayerId",
  "viewport",
  "dirty",
  "format",
  "version",
];

function collectKeys(node: unknown, acc: Set<string>): void {
  if (Array.isArray(node)) {
    node.forEach((n) => collectKeys(n, acc));
  } else if (node && typeof node === "object") {
    for (const [k, v] of Object.entries(node as Record<string, unknown>)) {
      acc.add(k);
      collectKeys(v, acc);
    }
  }
}

describe("buildCanonicalDocumentPayload", () => {
  it("excludes all transient pixel / resource / session keys", () => {
    const json = buildCanonicalDocumentPayload(makeEngine(null));
    for (const key of EXCLUDED) {
      expect(json).not.toContain(`"${key}"`);
    }
    const parsed = JSON.parse(json);
    const keys = new Set<string>();
    collectKeys(parsed, keys);
    for (const key of EXCLUDED) {
      expect(keys.has(key)).toBe(false);
    }
  });

  it("uses camelCase top-level keys matching the canonical model", () => {
    const parsed = JSON.parse(buildCanonicalDocumentPayload(makeEngine(null)));
    expect(Object.keys(parsed).sort()).toEqual(["height", "id", "layers", "name", "width"]);
    expect(parsed.id).toBe("doc-1");
    expect(parsed.name).toBe("Doc");
    expect(parsed.width).toBe(800);
    expect(parsed.height).toBe(600);
  });

  it("keeps text layer textData and shape layer shapeParams", () => {
    const parsed = JSON.parse(buildCanonicalDocumentPayload(makeEngine(null)));
    const byId = Object.fromEntries(parsed.layers.map((l: { id: string }) => [l.id, l]));
    expect(byId["L-text"].type).toBe("text");
    expect(byId["L-text"].locked).toBe(true);
    expect(byId["L-text"].textData.content).toBe("Hi");
    expect(byId["L-text"].textData.fontFamily).toBe("Arial");
    expect(byId["L-text"].textData.stroke.align).toBe("outside");
    expect(byId["L-shape"].type).toBe("shape");
    expect(byId["L-shape"].shapeParams.kind).toBe("star");
    expect(byId["L-shape"].shapeParams.fill.kind).toBe("solid");
  });

  it("serializes transform with scaleX/scaleY (not scale_x)", () => {
    const parsed = JSON.parse(buildCanonicalDocumentPayload(makeEngine(null)));
    const text = parsed.layers.find((l: { id: string }) => l.id === "L-text");
    expect(text.transform.scaleX).toBe(1.5);
    expect(text.transform.scaleY).toBe(1.5);
    expect(text.transform).not.toHaveProperty("scale_x");
  });

  it("omits resourceId and only includes selection when present", () => {
    const withoutSel = JSON.parse(buildCanonicalDocumentPayload(makeEngine(null)));
    expect(withoutSel).not.toHaveProperty("selection");

    const sel: SelectionState = {
      x: 5,
      y: 5,
      width: 20,
      height: 15,
      angle: 0,
      shape: "rect",
      inverted: false,
    };
    const withSelection = JSON.parse(buildCanonicalDocumentPayload(makeEngine(sel)));
    expect(withSelection.selection).toEqual(sel);
  });

  it("omits optional layer fields that are undefined on the source node", () => {
    const parsed = JSON.parse(buildCanonicalDocumentPayload(makeEngine(null)));
    const raster = parsed.layers.find((l: { id: string }) => l.id === "L-raster");
    // isBackground / lockTransparency were set; the absent ones must be omitted.
    expect(raster.isBackground).toBe(true);
    expect(raster.lockTransparency).toBe(true);
    expect(raster).not.toHaveProperty("lockPosition");
    expect(raster).not.toHaveProperty("lockRotation");
    expect(raster).not.toHaveProperty("hasAdjustments");
    expect(raster).not.toHaveProperty("basicAdjustment");
    expect(raster).not.toHaveProperty("shapeParams");
    expect(raster).not.toHaveProperty("textData");
  });
});

// Validity against the REAL Rust parser (crates/core/src/canonical_bridge.rs
// canonical_validate_json). The builder output must deserialize through the exact
// serde path the native seed uses - a JSON-shape guess is not a contract test.
describe("canonical seed payload validity (real Rust parser)", () => {
  it("fails the whole shadow seed when a legacy layer is missing a required field", async () => {
    const wasm = (await getWasmExportModule()) as unknown as {
      canonical_validate_json: (j: string) => string;
    };
    // Legacy file-restored LayerNodes (editorOpenImage.ts / snapshot.ts) may omit
    // `locked` on old files. The builder MUST NOT backfill it; the missing required
    // key must fail the entire document validation loudly, not silently fake data.
    const missingLocked = { ...rasterLayer(), locked: undefined } as unknown as LayerNode;
    const engine = {
      getId: () => "doc-1",
      getName: () => "Doc",
      getWidth: () => 800,
      getHeight: () => 600,
      getLayers: () => [missingLocked],
      getSelection: () => null,
    } as unknown as DocumentEngine;
    const err = wasm.canonical_validate_json(buildCanonicalDocumentPayload(engine));
    expect(err).not.toBe("");
  });
});
