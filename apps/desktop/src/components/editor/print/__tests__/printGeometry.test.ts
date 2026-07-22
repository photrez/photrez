// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, expect, it } from "vitest";
import {
  convertMmToUnit,
  convertUnitToMm,
  calculateEffectivePPI,
  getPPIQualityLevel,
  calculateScaleToFit,
  formatPhysicalDimensions,
} from "../printGeometry";

describe("printGeometry", () => {
  it("converts mm to units correctly", () => {
    expect(convertMmToUnit(25.4, "in")).toBe(1);
    expect(convertMmToUnit(10, "cm")).toBe(1);
    expect(convertMmToUnit(10, "mm")).toBe(10);
    expect(convertMmToUnit(25.4, "px")).toBe(300);
  });

  it("converts units to mm correctly", () => {
    expect(convertUnitToMm(1, "in")).toBe(25.4);
    expect(convertUnitToMm(1, "cm")).toBe(10);
    expect(convertUnitToMm(10, "mm")).toBe(10);
    expect(convertUnitToMm(300, "px")).toBe(25.4);
  });

  it("calculates effective PPI and quality rating correctly", () => {
    // 3000px on 10 inches (254mm) = 300 PPI -> Optimal
    const ppiOptimal = calculateEffectivePPI(3000, 2000, 254, 169.33);
    expect(ppiOptimal).toBeGreaterThanOrEqual(299);
    expect(getPPIQualityLevel(ppiOptimal).level).toBe("optimal");

    // 1500px on 10 inches = 150 PPI -> Draft
    const ppiDraft = calculateEffectivePPI(1500, 1000, 254, 169.33);
    expect(ppiDraft).toBe(150);
    expect(getPPIQualityLevel(ppiDraft).level).toBe("draft");

    // 720px on 10 inches = 72 PPI -> Low
    const ppiLow = calculateEffectivePPI(720, 480, 254, 169.33);
    expect(ppiLow).toBe(72);
    expect(getPPIQualityLevel(ppiLow).level).toBe("low");
  });

  it("calculates scale-to-fit correctly", () => {
    // A4 (210 x 297 mm) with 3000x2000px image
    const fit = calculateScaleToFit(3000, 2000, 210, 297, 10); // 10mm margin
    expect(fit.printWidthMm).toBeLessThanOrEqual(190);
    expect(fit.printHeightMm).toBeLessThanOrEqual(277);
    expect(fit.leftOffsetMm).toBeGreaterThan(0);
    expect(fit.topOffsetMm).toBeGreaterThan(0);
  });

  it("formats physical dimensions text correctly", () => {
    expect(formatPhysicalDimensions(210, 297, "cm")).toBe("21 cm × 29.7 cm");
  });
});
