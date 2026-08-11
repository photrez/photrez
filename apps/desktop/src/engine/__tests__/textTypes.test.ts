import { describe, it, expect } from "vitest";
import { DEFAULT_TEXT_DATA, normalizeTextData } from "../textTypes";
import type { TextData } from "../textTypes";
import type { LayerNode } from "../types";

describe("TextData contract", () => {
  it("is JSON-serializable (no functions, no doc-space x/y)", () => {
    const data: TextData = {
      content: "Hello",
      fontFamily: "Arial",
      fontSize: 48,
      fontWeight: 700,
      fontStyle: "italic",
      color: "#A1b2C3",
      align: "center",
      lineHeight: 1.6,
      letterSpacing: 2,
      boxMode: "area",
      boxWidth: 320,
      stroke: { width: 8, color: "#00Aa00" },
    };
    const round = JSON.parse(JSON.stringify(data)) as TextData;
    expect(round).toEqual(data);
  });

  it("LayerNode carries the 'text' type and textData field", () => {
    // Compile-time contract: the union must accept "text" and the field must
    // accept a TextData. Construction proves both without a full LayerNode.
    const layerType: LayerNode["type"] = "text";
    const layerTextData: LayerNode["textData"] = DEFAULT_TEXT_DATA;
    expect(layerType).toBe("text");
    expect(layerTextData).toEqual(DEFAULT_TEXT_DATA);
  });
});

describe("normalizeTextData", () => {
  it("is idempotent on DEFAULT_TEXT_DATA", () => {
    expect(normalizeTextData(DEFAULT_TEXT_DATA)).toEqual(DEFAULT_TEXT_DATA);
  });

  it("clamps fontSize to 1..2000 and treats NaN/undefined/string as default", () => {
    expect(normalizeTextData({ fontSize: 0 }).fontSize).toBe(1);
    expect(normalizeTextData({ fontSize: 9999 }).fontSize).toBe(2000);
    expect(normalizeTextData({ fontSize: NaN }).fontSize).toBe(DEFAULT_TEXT_DATA.fontSize);
    expect(normalizeTextData({}).fontSize).toBe(DEFAULT_TEXT_DATA.fontSize);
    expect(normalizeTextData({ fontSize: "48" }).fontSize).toBe(DEFAULT_TEXT_DATA.fontSize);
  });

  it("clamps fontWeight to 100..900 in 100 steps", () => {
    expect(normalizeTextData({ fontWeight: 50 }).fontWeight).toBe(100);
    expect(normalizeTextData({ fontWeight: 950 }).fontWeight).toBe(900);
    expect(normalizeTextData({ fontWeight: 400 }).fontWeight).toBe(400);
  });

  it("rounds fontWeight half-up at the step boundary (150 -> 200, 149 -> 100)", () => {
    expect(normalizeTextData({ fontWeight: 150 }).fontWeight).toBe(200);
    expect(normalizeTextData({ fontWeight: 149 }).fontWeight).toBe(100);
    expect(normalizeTextData({ fontWeight: 250 }).fontWeight).toBe(300);
    expect(normalizeTextData({ fontWeight: 849 }).fontWeight).toBe(800);
  });

  it("clamps lineHeight to 0.5..5.0 and letterSpacing to -100..500", () => {
    expect(normalizeTextData({ lineHeight: 0 }).lineHeight).toBe(0.5);
    expect(normalizeTextData({ lineHeight: 99 }).lineHeight).toBe(5.0);
    expect(normalizeTextData({ letterSpacing: -500 }).letterSpacing).toBe(-100);
    expect(normalizeTextData({ letterSpacing: 9999 }).letterSpacing).toBe(500);
  });

  it("switches boxWidth with boxMode: point -> 0, area -> >= 1", () => {
    expect(normalizeTextData({ boxMode: "point", boxWidth: 500 }).boxWidth).toBe(0);
    expect(normalizeTextData({ boxMode: "area", boxWidth: 320 }).boxWidth).toBe(320);
    expect(normalizeTextData({ boxMode: "area", boxWidth: 0 }).boxWidth).toBe(1);
    expect(normalizeTextData({ boxMode: "area" }).boxWidth).toBe(1);
  });

  it("preserves valid hex colors and falls back to #000000 otherwise", () => {
    expect(normalizeTextData({ color: "#A1b2C3" }).color).toBe("#A1b2C3");
    expect(normalizeTextData({ color: "red" }).color).toBe("#000000");
    expect(normalizeTextData({ color: "#123" }).color).toBe("#000000");
    expect(normalizeTextData({ color: "" }).color).toBe("#000000");
    expect(normalizeTextData({}).color).toBe("#000000");
  });

  it("falls back align/fontStyle/boxMode to defaults when not in union", () => {
    expect(normalizeTextData({ align: "justify" }).align).toBe("left");
    expect(normalizeTextData({ fontStyle: "bold" }).fontStyle).toBe("normal");
    expect(normalizeTextData({ boxMode: "box" }).boxMode).toBe("point");
  });

  it("normalizes arbitrary invalid inputs without throwing", () => {
    expect(normalizeTextData(null)).toEqual(DEFAULT_TEXT_DATA);
    expect(normalizeTextData(undefined)).toEqual(DEFAULT_TEXT_DATA);
    expect(normalizeTextData("junk")).toEqual(DEFAULT_TEXT_DATA);
    expect(normalizeTextData({ fontSize: "48", fontWeight: 400.5, content: 42 })).toEqual({
      ...DEFAULT_TEXT_DATA,
      content: "",
    });
  });

  describe("stroke", () => {
    it("defaults to width 0 (off) with a valid color", () => {
      expect(DEFAULT_TEXT_DATA.stroke).toEqual({ width: 0, color: "#000000", align: "outside" });
      expect(normalizeTextData({}).stroke).toEqual(DEFAULT_TEXT_DATA.stroke);
    });

    it("keeps valid stroke width/color", () => {
      expect(normalizeTextData({ stroke: { width: 12, color: "#00ff00" } }).stroke).toEqual({
        width: 12,
        color: "#00ff00",
        align: "outside",
      });
    });

    it("clamps width to 0..100 and rejects bad colors", () => {
      expect(normalizeTextData({ stroke: { width: -5 } }).stroke.width).toBe(0);
      expect(normalizeTextData({ stroke: { width: 999 } }).stroke.width).toBe(100);
      expect(normalizeTextData({ stroke: { width: NaN } }).stroke.width).toBe(0);
      expect(normalizeTextData({ stroke: { color: "red" } }).stroke.color).toBe("#000000");
      expect(normalizeTextData({ stroke: "junk" }).stroke).toEqual(DEFAULT_TEXT_DATA.stroke);
    });

    it("old textData without stroke normalizes to stroke off (lenient migration)", () => {
      const legacy = { ...DEFAULT_TEXT_DATA } as Record<string, unknown>;
      delete legacy.stroke;
      expect(normalizeTextData(legacy).stroke).toEqual({ width: 0, color: "#000000", align: "outside" });
    });
  });
});
