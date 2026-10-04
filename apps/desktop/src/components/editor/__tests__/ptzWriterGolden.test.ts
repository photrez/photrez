// apps/desktop/src/components/editor/__tests__/ptzWriterGolden.test.ts
//
// BLOCKING proof: the TypeScript stand-in for the Rust `.ptz` writer produces
// the SAME BYTES the real writer produces.
//
// `ptzWriterContract.ts` is a hand-written second implementation, so on its own
// it would only prove itself. `ptzWriterGolden.json` holds the real output of
// `PtzDocument::to_json` (crates/core/src/ptz_document.rs) for three fixtures:
// a real dumped v3 file, a document carrying all four layer kinds, and a legacy
// text layer missing fields added after it was written.
//
// This test is the join. It fails the moment the two disagree -- including on
// the axes that are invisible to a parsed comparison: key ORDER (serde_json's
// `Map` is a `BTreeMap`, so output is alphabetical), the `.0` on floats, and
// nested defaults for absent fields.
//
// Regenerate the golden ONLY when the writer intentionally changes:
//   cargo test -p photrez-core --lib -- --ignored ptz_writer_golden
// The Rust-side `ptz_writer_golden_is_up_to_date` gate fails first if you forget.

import { describe, expect, it } from "vitest";
import {
  assertProjectionStaysNarrow,
  emulateRustSerialization,
  loadWriterGolden,
} from "./ptzWriterContract";

const golden = loadWriterGolden();

/** First index where two strings differ, for a readable failure message. */
function firstDifference(actual: string, expected: string): string {
  const limit = Math.min(actual.length, expected.length);
  for (let i = 0; i < limit; i++) {
    if (actual[i] !== expected[i]) {
      return `at index ${i}:\n  actual:   ...${actual.slice(Math.max(0, i - 50), i + 70)}\n  expected: ...${expected.slice(Math.max(0, i - 50), i + 70)}`;
    }
  }
  return `length differs: actual ${actual.length} vs expected ${expected.length}`;
}

// Scoped to the three golden fixtures, NOT a universal claim about the writer.
// Coverage is bounded by what `ptz_fixtures.rs` defines; a layer shape absent
// from those fixtures is unpinned on both sides.
describe("the TypeScript writer model matches the real Rust writer on the golden fixtures", () => {
  it("has a golden for every fixture", () => {
    expect(Object.keys(golden).sort()).toEqual([
      "legacyTextLayer", "mixedLayerTypes", "realDumpedModel",
    ]);
  });

  // TOTAL BYTE COMPARISON. No tolerance, no carve-out.
  //
  // It was tempting to compare structurally with a float tolerance, because
  // `serde_json`'s default float parsing is approximate and can be 1 ULP off
  // the correctly-rounded value on some literals -- on this fixture's `panY`
  // it was. That is fixed at the source instead: `crates/core/Cargo.toml`
  // enables serde_json's `float_roundtrip` feature, so Rust parses decimals
  // exactly as `str::parse::<f64>` and as JavaScript's `JSON.parse` do
  // (measured: all agree on `0x404fccccaaaaaaa8` for `63.599995930989564`).
  // So a byte difference here is a real difference in the writer's output.
  for (const [name, testCase] of Object.entries(golden)) {
    it(`reproduces the real bytes for ${name}`, () => {
      const actual = emulateRustSerialization(JSON.parse(testCase.input));
      expect(
        actual,
        `projection diverged from the Rust writer ${firstDifference(actual, testCase.expected)}`,
      ).toBe(testCase.expected);
    });
  }

  // The specific divergence this round introduced: Rust's `Default` impls emit
  // CONCRETE values for absent nested fields, where the previous stand-in
  // emitted `null`. The legacy fixture omits most TextData fields, so it is the
  // case that catches it.
  it("fills absent nested fields with the Rust defaults, not null", () => {
    const written = JSON.parse(
      emulateRustSerialization(JSON.parse(golden.legacyTextLayer.input)),
    );
    const td = written.layers[0].textData;
    expect(td.letterSpacing).toBe(0);
    expect(td.boxMode).toBe("point");
    expect(td.boxWidth).toBe(0);
    expect(td.underline).toBe(false);
    expect(td.strikethrough).toBe(false);
    expect(td.uppercase).toBe(false);
    // A nested object default, not null and not a dropped key.
    expect(td.stroke).toEqual({ align: "outside", color: "#000000", width: 0 });
  });

  // Ordering is invisible to a parsed comparison, so assert it on raw bytes.
  it("emits keys alphabetically, as serde_json's BTreeMap does", () => {
    const raw = emulateRustSerialization(JSON.parse(golden.realDumpedModel.input));
    expect(raw.startsWith('{"activeLayerId":')).toBe(true);
    const dirty = raw.indexOf('"dirty"');
    const format = raw.indexOf('"format"');
    const version = raw.indexOf('"version"');
    expect(dirty).toBeLessThan(format);
    expect(format).toBeLessThan(version);
    // Every layer key set is sorted too.
    const layersAt = raw.indexOf('"layers":[') + '"layers":['.length;
    expect(raw.slice(layersAt, layersAt + 20)).toMatch(/^\{"baseImageBitmap":/);
  });

  // Float SPELLING, where it is deterministic: an integral f64 gets `.0`, and
  // the u32 header does not.
  it("writes f64 fields with an explicit decimal point but the u32 version without", () => {
    const raw = emulateRustSerialization(JSON.parse(golden.realDumpedModel.input));
    expect(raw).toContain('"height":200.0,');
    expect(raw.endsWith('"width":300.0}')).toBe(true);
    expect(raw).toContain('"version":3,');
    expect(raw).not.toContain('"version":3.0');
  });

  /**
   * Guards the property the total byte comparison above now depends on: the
   * golden must echo the fixture's decimal literals back unchanged, so the
   * projection and Rust cannot both be "right" about a value neither of them
   * round-trips identically. With serde_json's `float_roundtrip` enabled this
   * holds for the one non-exactly-representable value in the fixtures.
   */
  it("round-trips the fixtures' float literals without perturbation", () => {
    const raw = emulateRustSerialization(JSON.parse(golden.realDumpedModel.input));
    expect(raw).toContain('"panY":63.599995930989564');
    // The other two fixtures use only exactly-representable values, so their
    // bytes must survive untouched.
    const mixed = emulateRustSerialization(JSON.parse(golden.mixedLayerTypes.input));
    expect(mixed).toContain('"panY":2.5');
    expect(mixed).toContain('"contrast":-33.25');
    const legacy = emulateRustSerialization(JSON.parse(golden.legacyTextLayer.input));
    expect(legacy).toContain('"fontSize":36.0');
    expect(legacy).toContain('"lineHeight":1.2');
  });

  it("carries every nested payload through unchanged", () => {
    const written = JSON.parse(emulateRustSerialization(JSON.parse(golden.mixedLayerTypes.input)));
    const byName = Object.fromEntries(written.layers.map((l: any) => [l.name, l]));
    expect(byName.Adjusted.basicAdjustment).toEqual({
      brightness: 12.5, contrast: -33.25, saturation: 44.75,
    });
    expect(byName.Star.shapeParams.fill).toEqual({ color: "#E15A17", kind: "solid" });
    expect(byName.Star.shapeParams.stroke).toEqual({
      color: "#00FF00", enabled: true, width: 3.5,
    });
    expect(byName.Title.textData.stroke).toEqual({
      align: "inside", color: "#ABCDEF", width: 2.5,
    });
    expect(byName.Title.textData.content).toBe("Hello");
    expect(byName.Grade.basicAdjustment).toEqual({
      brightness: -1.5, contrast: 2.5, saturation: -3.5,
    });
  });
});

describe("the projection stays narrow", () => {
  it("drops unmodelled keys and rejects out-of-contract documents", () => {
    expect(() => assertProjectionStaysNarrow()).not.toThrow();
  });

  it("drops a document field the typed writer does not model", () => {
    const written = JSON.parse(emulateRustSerialization({
      id: "d", name: "n", width: 1, height: 1, dirty: false,
      someBrandNewField: "must not survive",
      layers: [],
    }));
    expect(written.someBrandNewField).toBeUndefined();
  });

  it("rejects an out-of-contract document instead of writing it", () => {
    expect(() => emulateRustSerialization({
      id: "d", name: "n", width: 1, height: 1, dirty: false,
      layers: [{
        id: "l", name: "L", type: "raster", visible: true, opacity: "opaque",
        locked: false, blendMode: "normal", width: 1, height: 1,
      }],
    })).toThrow(/does not match the .ptz format/);
  });

  it("rejects an unknown blend mode rather than downgrading it", () => {
    expect(() => emulateRustSerialization({
      id: "d", name: "n", width: 1, height: 1, dirty: false,
      layers: [{
        id: "l", name: "L", type: "raster", visible: true, opacity: 1,
        locked: false, blendMode: "not-a-mode", width: 1, height: 1,
      }],
    })).toThrow(/does not match the .ptz format/);
  });

  it("rejects a selection missing a required field", () => {
    // Rust's SelectionState requires x/y/width/height/angle.
    const base = {
      id: "d", name: "n", width: 1, height: 1, dirty: false, layers: [],
    };
    expect(() => emulateRustSerialization({ ...base, selection: { x: 1 } }))
      .toThrow(/does not match the .ptz format/);
  });

  it("rejects a transform missing a required field", () => {
    // Rust's Transform2D requires all seven fields.
    expect(() => emulateRustSerialization({
      id: "d", name: "n", width: 1, height: 1, dirty: false,
      layers: [{
        id: "l", name: "L", type: "raster", visible: true, opacity: 1,
        locked: false, blendMode: "normal", width: 1, height: 1,
        transform: { x: 0 },
      }],
    })).toThrow(/does not match the .ptz format/);
  });

  it("rejects an unknown enum inside a nested payload", () => {
    // `#[serde(default)]` covers an ABSENT field, not an invalid present one.
    expect(() => emulateRustSerialization({
      id: "d", name: "n", width: 1, height: 1, dirty: false,
      layers: [{
        id: "l", name: "L", type: "text", visible: true, opacity: 1,
        locked: false, blendMode: "normal", width: 1, height: 1,
        transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
        textData: { content: "x", boxMode: "sideways" },
      }],
    })).toThrow(/does not match the .ptz format/);
  });
});