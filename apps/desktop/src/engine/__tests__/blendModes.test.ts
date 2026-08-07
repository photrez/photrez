import { describe, expect, it } from "vitest";
import {
  BLEND_MODE_OPTIONS,
  BLEND_MODE_SHADER_IDS,
  blendModeToShaderId,
  getCanvasCompositeOperation,
  isBlendMode,
} from "../blendModes";
import type { BlendMode } from "../types";

describe("blend mode parity registry", () => {
  it("only exposes blend modes covered by the engine BlendMode contract", () => {
    const values = BLEND_MODE_OPTIONS.map((option) => option.value);

    expect(values).toEqual([
      "normal",
      "multiply",
      "screen",
      "overlay",
      "darken",
      "lighten",
      "color-dodge",
      "color-burn",
      "soft-light",
      "hard-light",
      "difference",
      "exclusion",
    ]);
  });

  it("maps every exposed mode to an explicit Canvas2D export operation", () => {
    const operations: Record<BlendMode, GlobalCompositeOperation> = {
      normal: "source-over",
      multiply: "multiply",
      screen: "screen",
      overlay: "overlay",
      darken: "darken",
      lighten: "lighten",
      "color-dodge": "color-dodge",
      "color-burn": "color-burn",
      "soft-light": "soft-light",
      "hard-light": "hard-light",
      difference: "difference",
      exclusion: "exclusion",
    };

    for (const option of BLEND_MODE_OPTIONS) {
      expect(getCanvasCompositeOperation(option.value)).toBe(operations[option.value]);
    }
  });

  it("rejects modes that are implemented in shaders but not approved for UI/export parity", () => {
    // Every W3C/Canvas2D blend mode is now exposed; only non-blend modes
    // (e.g. hue/saturation from other pipelines) must be rejected.
    expect(isBlendMode("hue")).toBe(false);
    expect(isBlendMode("saturation")).toBe(false);
    expect(isBlendMode("luminosity")).toBe(false);
  });

  it("maps every BlendMode to its WebGL shader id (parity with the preview renderer)", () => {
    const expectedIds: Record<BlendMode, number> = {
      normal: 0,
      multiply: 1,
      screen: 2,
      overlay: 3,
      darken: 4,
      lighten: 5,
      "color-dodge": 6,
      "color-burn": 7,
      "hard-light": 8,
      "soft-light": 9,
      difference: 10,
      exclusion: 11,
    };

    // Record<BlendMode, number> forces all union members present at compile
    // time, so a newly added BlendMode without a shader id fails the build.
    expect(BLEND_MODE_SHADER_IDS).toEqual(expectedIds);

    for (const option of BLEND_MODE_OPTIONS) {
      expect(blendModeToShaderId(option.value)).toBe(expectedIds[option.value]);
      // Non-normal modes must never silently fall back to the normal id (0),
      // which would make the WebGL preview differ from the Canvas2D export.
      if (option.value !== "normal") {
        expect(blendModeToShaderId(option.value)).not.toBe(0);
      }
    }
  });
});
