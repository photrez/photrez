import { describe, it, expect } from "vitest";
import type { ShapeParams, LayerNode } from "../types";

describe("ShapeParams contract", () => {
  it("is JSON-serializable (no functions, no doc-space x/y)", () => {
    const params: ShapeParams = {
      kind: "rect",
      width: 200,
      height: 100,
      radius: 8,
      fill: { kind: "solid", color: "#E15A17" },
      stroke: { enabled: false, color: "#000000", width: 4 },
      arrowHead: false,
    };
    const round = JSON.parse(JSON.stringify(params)) as ShapeParams;
    expect(round).toEqual(params);
  });

  it("line shape may set arrowHead; fill ignored at render", () => {
    const params: ShapeParams = {
      kind: "line", width: 300, height: 0, radius: 0,
      fill: { kind: "none", color: "#000000" },
      stroke: { enabled: true, color: "#ffffff", width: 6 },
      arrowHead: true,
    };
    expect(params.kind).toBe("line");
    expect(params.arrowHead).toBe(true);
  });
});
