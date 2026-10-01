// The FIELD -> PREDICATE seam: what a typed size actually reaches the crop.
//
// WHY THIS FILE EXISTS
//
// Every other predicate test calls `resolveCropDocumentSize` DIRECTLY, so they
// would all still pass if production stopped calling it. Nothing in the suite
// pinned the seam between the size FIELD and the predicate - and that seam is
// where the measured defect entered: the field has no min/max, so a typed `0`
// became a real target size.
//
// The chain under test, with the real components:
//
//   EditableNumField (primitives.tsx)  - the real commit path: parseFloat,
//                                          isNaN -> revert, else onSubmit
//     -> fromUnit (viewport/unitConversion.ts)  - cm/mm/in scale the value
//     -> setCropSizeTarget
//       -> cropToolActions.ts:99   cropMode === "size" gates the target
//       -> Math.round at :100
//       -> resolveCropDocumentSize at :157
//       -> engine.applyCrop + history.commit (6th argument)
//
// `CropSizeInputs` is mounted for real (it renders the real `EditableNumField`),
// and `fromUnit` is the real imported function, so a regression in either is
// caught rather than assumed away.
//
// Several expectations here were hand-traced before writing and are pinned as
// found, not as wished-for:
//   - "" / "abc" / whitespace NEVER reach the predicate. parseFloat yields NaN,
//     the field reverts to its prior value and onSubmit never fires - so no crop
//     happens at all. That is a stronger property than "the predicate rejects it".
//   - a typed 0 in FREE mode stays inert: the size target is only read when
//     cropMode === "size".
//   - cm is NOT x300. UNIT_TO_PX.cm is 300/2.54; only `in` is 300. `-5 cm`
//     converts to -590.55, which stays negative and is therefore rejected - but
//     that rests on the multiply preserving the sign, which is what is pinned.

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render } from "solid-js/web";
import { createSignal } from "solid-js";
import { CropSizeInputs } from "../CropOptionBarSections";
import { applyCropPreview } from "../cropToolActions";
import { fromUnit, UNIT_TO_PX, toUnit } from "@/viewport/unitConversion";

function tick(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

/**
 * Mount the real CropSizeInputs wired the way CropOptionBar.tsx:504-521 wires it,
 * then drive applyCropPreview with whatever the field produced.
 *
 * Returns the applyCrop arguments and the 6th argument history.commit received.
 */
async function driveSizeField(
  typed: string,
  opts: { cropMode?: "free" | "ratio" | "size"; unit?: string } = {},
): Promise<{
  applyCropArgs: unknown[] | null;
  commitSizePair: unknown;
  targetAfterField: { w: number; h: number } | null;
  onSubmitFired: boolean;
}> {
  const cropMode = opts.cropMode ?? "size";
  const unit = opts.unit ?? "px";

  // Reactive stand-ins for the crop session signals CropOptionBar owns.
  const [sizeWVal, setSizeWVal] = createSignal(800);
  const [sizeHVal, setSizeHVal] = createSignal(600);
  const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>({
    w: 800,
    h: 600,
  });

  let onSubmitFired = false;

  const container = document.createElement("div");
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <CropSizeInputs
        w={sizeWVal}
        h={sizeHVal}
        unit={() => unit}
        // Same shape as CropOptionBar's handler: scale to px, then publish the
        // target. The real fromUnit is imported, so a conversion regression shows.
        onWSubmit={(v) => {
          onSubmitFired = true;
          setSizeWVal(v);
          const valPx = fromUnit(v, unit);
          setCropSizeTarget({ w: valPx, h: cropSizeTarget()?.h ?? 600 });
        }}
        onHSubmit={(v) => {
          onSubmitFired = true;
          setSizeHVal(v);
          const valPx = fromUnit(v, unit);
          setCropSizeTarget({ w: cropSizeTarget()?.w ?? 800, h: valPx });
        }}
        onSwap={() => {}}
        onUnitChange={() => {}}
      />
    ),
    container,
  );

  try {
    const input = container.querySelector("input") as HTMLInputElement;
    expect(input, "CropSizeInputs renders the W field").toBeTruthy();

    // The real commit path requires focus to enter editing state (primitives.tsx:284).
    input.dispatchEvent(new FocusEvent("focus", { bubbles: true }));
    input.value = typed;
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await tick();

    const targetAfterField = cropSizeTarget();

    // Now run the crop exactly as the tool does.
    const history = { commit: vi.fn() };
    const engine = {
      snapshot: () => ({}),
      applyCrop: vi.fn(),
      setActiveLayer: vi.fn(),
      getWidth: () => 128,
      getHeight: () => 128,
      getViewport: () => ({ zoom: 1 }),
      getLayers: () => [],
    };
    applyCropPreview({
      workspace: {
        getActiveEngine: () => engine,
        getActiveHistory: () => history,
      } as never,
      renderer: { uploadImage: vi.fn(), resize: vi.fn(), resizeToViewport: vi.fn() } as never,
      viewport: { width: 800, height: 600 },
      cropRect: { x: 10, y: 20, w: 100, h: 100 },
      cropMode,
      cropSizeTarget: targetAfterField,
      cropDeletePixels: true,
      cropRotation: 0,
      scheduler: { requestRender: vi.fn() } as never,
      setCropRect: vi.fn(),
      setCropRotation: vi.fn(),
      setHiddenCropPreview: vi.fn(),
      setActiveTool: vi.fn(),
      setSelectedLayerId: vi.fn(),
      recenterViewport: vi.fn(),
    });

    const applyArgs = engine.applyCrop.mock.calls[0] as unknown[] | undefined;
    const commitCall = history.commit.mock.calls[0] as unknown[] | undefined;
    return {
      applyCropArgs: applyArgs ? [...applyArgs] : null,
      commitSizePair: commitCall ? commitCall[5] : undefined,
      targetAfterField: targetAfterField ? { ...targetAfterField } : null,
      onSubmitFired,
    };
  } finally {
    dispose();
    container.parentNode?.removeChild(container);
  }
}

describe("field -> predicate seam: a typed 0 never becomes an accepted crop", () => {
  beforeEach(() => {
    localStorage.clear();
    // isFacadeEnabled() is `getItem("photrez.facade") !== "0"`, so an UNSET key
    // means ENABLED - which routes the crop asynchronously through the facade.
    // The default (synchronous) path is what the field seam feeds, so opt out.
    localStorage.setItem("photrez.facade", "0");
    localStorage.setItem("photrez.facadeAuthority", "wasm");
  });
  afterEach(() => localStorage.clear());

  it('a typed "0" reaches applyCrop as a 0 target and is REJECTED', async () => {
    const r = await driveSizeField("0");
    // The value really does get through the field and the unit conversion.
    expect(r.onSubmitFired).toBe(true);
    expect(r.targetAfterField).toEqual({ w: 0, h: 600 });
    // applyCrop is called with the 0 - it is the engine that refuses it.
    expect(r.applyCropArgs?.[4]).toMatchObject({ targetSize: { w: 0, h: 600 } });
    // And the history entry records NO size pair, so a later REDO cannot write 0.
    expect(r.commitSizePair).toBeUndefined();
  });

  it('a typed "-5" reaches applyCrop and is REJECTED', async () => {
    const r = await driveSizeField("-5");
    expect(r.onSubmitFired).toBe(true);
    expect(r.targetAfterField).toEqual({ w: -5, h: 600 });
    expect(r.applyCropArgs?.[4]).toMatchObject({ targetSize: { w: -5, h: 600 } });
    expect(r.commitSizePair).toBeUndefined();
  });

  it('a typed "1e9" reaches applyCrop and is REJECTED by the ceiling', async () => {
    const r = await driveSizeField("1e9");
    expect(r.onSubmitFired).toBe(true);
    expect(r.targetAfterField?.w).toBe(1e9);
    expect(r.applyCropArgs?.[4]).toMatchObject({ targetSize: { w: 1e9, h: 600 } });
    expect(r.commitSizePair).toBeUndefined();
  });

  // Not "the predicate rejects it" - the field never produces a value at all.
  it.each([["", "empty"], ["abc", "non-numeric"], ["   ", "whitespace"]])(
    'a typed %j (%s) makes the field REVERT and never starts a crop',
    async (typed) => {
      const r = await driveSizeField(typed);
      // parseFloat -> NaN -> the field reverts and onSubmit never fires.
      expect(r.onSubmitFired).toBe(false);
      // The target is untouched, so the crop that runs carries the prior target.
      expect(r.targetAfterField).toEqual({ w: 800, h: 600 });
      // And the prior target is a legal size, so this is an ordinary accepted crop.
      expect(r.commitSizePair).toEqual({
        before: { width: 128, height: 128 },
        after: { width: 800, height: 600 },
      });
    },
  );

  it("a typed 0 in FREE mode stays inert and never reaches the size path", async () => {
    const r = await driveSizeField("0", { cropMode: "free" });
    // The field DOES publish a target - but cropToolActions only reads it when
    // cropMode === "size", so with no target the crop uses the crop rect.
    expect(r.onSubmitFired).toBe(true);
    expect(r.applyCropArgs?.[4]).toMatchObject({ targetSize: null });
    expect(r.commitSizePair).toEqual({
      before: { width: 128, height: 128 },
      after: { width: 100, height: 100 },
    });
  });
});

describe("field -> predicate seam: the smallest legal size still works", () => {
  beforeEach(() => {
    localStorage.clear();
    localStorage.setItem("photrez.facade", "0");
    localStorage.setItem("photrez.facadeAuthority", "wasm");
  });
  afterEach(() => localStorage.clear());

  it('a typed "1" is accepted and records its size pair', async () => {
    const r = await driveSizeField("1");
    expect(r.onSubmitFired).toBe(true);
    expect(r.applyCropArgs?.[4]).toMatchObject({ targetSize: { w: 1, h: 600 } });
    // The pair IS recorded here - this is the direction a guard must not break.
    expect(r.commitSizePair).toEqual({
      before: { width: 128, height: 128 },
      after: { width: 1, height: 600 },
    });
  });
});

describe("unit conversion: a negative stays negative across the multiply", () => {
  beforeEach(() => {
    localStorage.clear();
    localStorage.setItem("photrez.facade", "0");
    localStorage.setItem("photrez.facadeAuthority", "wasm");
  });
  afterEach(() => localStorage.clear());
  // The rejection of "-5 cm" rests on fromUnit preserving the SIGN. If the
  // conversion ever regressed to a magnitude-only form, a negative could cross to
  // positive and slip past the predicate - so the converted value is pinned, not
  // just the outcome.
  it("cm is 300/2.54, not 300, and -5 cm converts to a large negative", () => {
    expect(UNIT_TO_PX.cm).toBeCloseTo(300 / 2.54, 6);
    expect(UNIT_TO_PX.cm).not.toBe(300);
    expect(fromUnit(-5, "cm")).toBe(-590.55);
    expect(fromUnit(-5, "cm")).toBeLessThan(0);
  });

  it("in is the only unit that is a plain 300x", () => {
    expect(UNIT_TO_PX.in).toBe(300);
    expect(fromUnit(-5, "in")).toBe(-1500);
    expect(fromUnit(-5, "in")).toBeLessThan(0);
  });

  it("round-trips a negative through toUnit/fromUnit without changing its sign", () => {
    for (const unit of ["px", "cm", "mm", "in"]) {
      expect(fromUnit(toUnit(-128, unit), unit)).toBeLessThan(0);
    }
  });

  it("a typed negative in cm is rejected end to end", async () => {
    const r = await driveSizeField("-5", { unit: "cm" });
    expect(r.onSubmitFired).toBe(true);
    // The px value is the multiplied negative, not the typed -5.
    expect(r.targetAfterField).toEqual({ w: -590.55, h: 600 });
    // applyCrop receives the ROUNDED target: cropToolActions rounds the size
    // fields on the way in. -590.55 -> -591, still negative, still rejected.
    expect(r.applyCropArgs?.[4]).toMatchObject({ targetSize: { w: -591, h: 600 } });
    expect(r.commitSizePair).toBeUndefined();
  });
});