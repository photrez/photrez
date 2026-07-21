import { describe, it, expect } from "vitest";
import { floodFill, gradientFill, type ColorStop } from "../fillOperations";

function makeTestImage(w: number, h: number, fill: [number, number, number, number]): ImageData {
  const data = new Uint8ClampedArray(w * h * 4);
  for (let i = 0; i < data.length; i += 4) {
    data[i] = fill[0];
    data[i + 1] = fill[1];
    data[i + 2] = fill[2];
    data[i + 3] = fill[3];
  }
  // Note: ImageData requires colorSpace in TS types, but our pure functions
  // only access .data, .width, .height — the cast is safe for pixel-math tests.
  return { data, width: w, height: h } as unknown as ImageData;
}

function getPixel(img: ImageData, x: number, y: number): [number, number, number, number] {
  const idx = (y * img.width + x) * 4;
  return [img.data[idx], img.data[idx + 1], img.data[idx + 2], img.data[idx + 3]];
}

describe("floodFill", () => {
  it("fills entire uniform region when no mask", () => {
    const img = makeTestImage(10, 10, [255, 0, 0, 255]);
    floodFill(img, 5, 5, 0, 255, 0, 255, 32);
    expect(getPixel(img, 0, 0)).toEqual([0, 255, 0, 255]);
    expect(getPixel(img, 9, 9)).toEqual([0, 255, 0, 255]);
  });

  it("does not fill pixels outside tolerance", () => {
    const img = makeTestImage(10, 10, [255, 0, 0, 255]);
    // Change pixel (9,9) to blue
    const idx = (9 * 10 + 9) * 4;
    img.data[idx] = 0; img.data[idx + 1] = 0; img.data[idx + 2] = 255;
    floodFill(img, 5, 5, 0, 255, 0, 255, 32);
    // (9,9) should still be blue
    expect(getPixel(img, 9, 9)).toEqual([0, 0, 255, 255]);
    // (0,0) should be filled (it's red, within tolerance of start)
    expect(getPixel(img, 0, 0)).toEqual([0, 255, 0, 255]);
  });

  it("respects selection mask (clamp to AABB)", () => {
    const img = makeTestImage(10, 10, [255, 0, 0, 255]);
    floodFill(img, 5, 5, 0, 255, 0, 255, 32, { x: 3, y: 3, w: 4, h: 4 });
    // Pixel inside mask should be filled
    expect(getPixel(img, 4, 4)).toEqual([0, 255, 0, 255]);
    // Pixel outside mask should remain red
    expect(getPixel(img, 1, 1)).toEqual([255, 0, 0, 255]);
    expect(getPixel(img, 8, 8)).toEqual([255, 0, 0, 255]);
  });

  it("replaces non-contiguous matching pixels when contiguous is false", () => {
    const img = makeTestImage(10, 10, [255, 0, 0, 255]);
    // Create a black barrier wall at column 5 separating left and right halves
    for (let y = 0; y < 10; y++) {
      const idx = (y * 10 + 5) * 4;
      img.data[idx] = 0; img.data[idx + 1] = 0; img.data[idx + 2] = 0; img.data[idx + 3] = 255;
    }
    // When contiguous is false (global replace), click at (1,1) should replace red on BOTH sides of the wall
    floodFill(img, 1, 1, 0, 255, 0, 255, 32, null, false);
    // Left side filled
    expect(getPixel(img, 1, 1)).toEqual([0, 255, 0, 255]);
    // Right side ALSO filled (global replace)
    expect(getPixel(img, 8, 8)).toEqual([0, 255, 0, 255]);
    // Black barrier wall left untouched
    expect(getPixel(img, 5, 5)).toEqual([0, 0, 0, 255]);
  });

  it("respects ellipse selection (skip outside ellipse)", () => {
    const img = makeTestImage(10, 10, [255, 0, 0, 255]);
    floodFill(img, 5, 5, 0, 255, 0, 255, 32, { x: 0, y: 0, w: 10, h: 10, shape: "ellipse" });
    // Corner of 10×10 ellipse should NOT be filled
    expect(getPixel(img, 0, 0)).toEqual([255, 0, 0, 255]);
    // Center should be filled
    expect(getPixel(img, 5, 5)).toEqual([0, 255, 0, 255]);
  });

  it("returns early when start is out of bounds", () => {
    const img = makeTestImage(10, 10, [255, 0, 0, 255]);
    floodFill(img, -1, 5, 0, 255, 0, 255, 32);
    expect(getPixel(img, 5, 5)).toEqual([255, 0, 0, 255]);
  });

  it("returns early when start pixel already matches fill color", () => {
    const img = makeTestImage(10, 10, [0, 255, 0, 255]);
    floodFill(img, 5, 5, 0, 255, 0, 255, 32);
    expect(getPixel(img, 0, 0)).toEqual([0, 255, 0, 255]);
  });
});

describe("gradientFill", () => {
  it("fills linear gradient from red to blue across full width", () => {
    const img = makeTestImage(10, 10, [0, 0, 0, 255]);
    const stops: ColorStop[] = [
      { offset: 0, r: 255, g: 0, b: 0, a: 255 },
      { offset: 1, r: 0, g: 0, b: 255, a: 255 },
    ];
    gradientFill(img, "linear", 0, 0, 10, 0, stops);
    // Left edge should be red
    expect(getPixel(img, 0, 5)[0]).toBeGreaterThan(200);
    expect(getPixel(img, 0, 5)[2]).toBeLessThan(50);
    // Right edge should be blue
    expect(getPixel(img, 9, 5)[0]).toBeLessThan(50);
    expect(getPixel(img, 9, 5)[2]).toBeGreaterThan(200);
    // Center should be purple-ish (middle of gradient)
    const mid = getPixel(img, 5, 5);
    expect(mid[0]).toBeGreaterThan(50);
    expect(mid[2]).toBeGreaterThan(50);
  });

  it("fills radial gradient from center outward", () => {
    const img = makeTestImage(10, 10, [255, 255, 255, 255]);
    const stops: ColorStop[] = [
      { offset: 0, r: 255, g: 0, b: 0, a: 255 },
      { offset: 1, r: 0, g: 0, b: 255, a: 255 },
    ];
    gradientFill(img, "radial", 5, 5, 5, 0, stops); // center (5,5), radius 5
    // Center should be red
    expect(getPixel(img, 5, 5)[0]).toBeGreaterThan(200);
    // Corner (0,0) distance ~7px → beyond radius → should be blue
    const corner = getPixel(img, 0, 0);
    expect(corner[2]).toBeGreaterThan(200);
  });

  it("respects selection mask", () => {
    const img = makeTestImage(10, 10, [100, 100, 100, 255]);
    const stops: ColorStop[] = [
      { offset: 0, r: 255, g: 0, b: 0, a: 255 },
      { offset: 1, r: 0, g: 0, b: 255, a: 255 },
    ];
    gradientFill(img, "linear", 0, 0, 10, 0, stops, { x: 2, y: 2, w: 6, h: 6 });
    // Pixel inside mask at (5,5) should be gradient-filled
    expect(getPixel(img, 5, 5)).not.toEqual([100, 100, 100, 255]);
    // Pixel outside mask at (0,0) should remain gray
    expect(getPixel(img, 0, 0)).toEqual([100, 100, 100, 255]);
  });
});
