// Contract test: the TS builder output must deserialize through the REAL Rust
// parser (crates/core/src/canonical_model.rs `canonical_validate_json`), not a
// JSON-shape guess. A key drift between the TS builder and the Rust model would
// surface as a parse error here - exactly the failure the fire-and-forget seed
// would otherwise swallow in production.
//
// Loads the REAL .wasm via the production getWasmExportModule (wasmTestShim alias
// -> same bytes the app arms). The engine + layers are REAL (DocumentEngine +
// layerFactory defaults), so the LayerNodes come from the actual creation paths,
// not a hand-written stub. After `bun run build:wasm` the pkg exposes
// `canonical_validate_json`; if it is missing the beforeAll assertion fails LOUDLY
// (no silent shape-only substitute).

import { describe, it, expect, beforeAll } from "vitest";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import { DocumentEngine } from "@/engine/document";
import type { ShapeParams } from "@/engine/types";
import type { TextData } from "@/engine/textTypes";
import { buildCanonicalDocumentPayload } from "../canonicalSeed";

type WasmModule = {
  canonical_validate_json: (json: string) => string;
};

describe("canonical seed payload parses in the REAL wasm parser", () => {
  let wasm: WasmModule | null = null;

  beforeAll(async () => {
    const m = (await getWasmExportModule()) as unknown as Partial<WasmModule>;
    // Hard gate: the rebuilt pkg MUST expose the validator. No fallback.
    expect(typeof m.canonical_validate_json).toBe("function");
    wasm = m as WasmModule;
  });

  it("builder output from a real engine (raster + text + shape + selection) parses cleanly", () => {
    const engine = new DocumentEngine("doc-contract", "Contract", 800, 600);
    // Background raster layer (real factory defaults).
    engine.addLayer("Background");

    // Text layer via the real addTextLayer path.
    const textData: TextData = {
      content: "Hi",
      fontFamily: "Arial",
      fontSize: 32,
      fontWeight: 700,
      fontStyle: "normal",
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
    };
    engine.addTextLayer("Title", textData);

    // Shape layer via the real addShapeLayer path.
    const shapeParams: ShapeParams = {
      kind: "star",
      width: 200,
      height: 200,
      radius: 12,
      fill: { kind: "solid", color: "#E15A17" },
      stroke: { enabled: true, color: "#000000", width: 2 },
      arrowHead: false,
    };
    engine.addShapeLayer("Star", shapeParams);

    // Selection via the real engine path.
    engine.createSelection(10, 20, 100, 200);

    const json = buildCanonicalDocumentPayload(engine);

    // Sanity: the builder pulled real layer ids/types from the live model.
    const parsed = JSON.parse(json);
    expect(parsed.layers.length).toBe(3);
    expect(parsed.layers.some((l: { type: string }) => l.type === "text")).toBe(true);
    expect(parsed.layers.some((l: { type: string }) => l.type === "shape")).toBe(true);
    expect(parsed).toHaveProperty("selection");

    // The REAL parser: the exact serde path the native seed will deserialize.
    expect(wasm!.canonical_validate_json(json)).toBe("");
  });

  it("rejects a malformed payload via the real parser (proves it is not a stub)", () => {
    expect(wasm!.canonical_validate_json("{ not canonical json")).not.toBe("");
  });
});
