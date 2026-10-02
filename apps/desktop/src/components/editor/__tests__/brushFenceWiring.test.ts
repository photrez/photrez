// WIRING TEST: the real brush path must actually CALL the store-presence fence.
//
// THE DEBT THIS CLOSES. `hostLayerPaintable.test.ts` proves the fence PREDICATE
// behaves correctly when called. It does NOT prove the production brush path calls
// it. Reverting `strokeBlockedByRust` to `!== true` left that file at 6/6 green,
// because it restates the decision rule in its own helper rather than driving the
// hook. So deleting the call site, wiring it to the wrong variable, or dropping one
// of the per-stroke resets would all pass every existing test - the
// "pure function tests pass, app fails" class this repo has been bitten by
// repeatedly.
//
// This file drives the REAL hook (`useBrushOverlay()`), through its REAL exported
// entry points (`onPaintStroke` for the composite path, `commitBrushStroke` for the
// commit path), so the fence call, the once-per-stroke caching and the teardown
// resets are all exercised as shipped.
//
// It pins FOUR properties, and no more:
//   1. the pointer path consults the fence at all (removing the call reddens);
//   2. the decision is taken ONCE and read by BOTH paths (a re-read that disagrees
//      would show as a differing call count / outcome between the two paths);
//   3. the per-stroke decision is RESET between strokes, on BOTH teardown entry
//      points that are reachable from the public hook - a completed commit and
//      cancelActiveStroke;
//   4. UNKNOWN (no / unreadable mirror) passes through the WIRING, not merely
//      through the predicate.
//
// COVERAGE BOUNDARY - MEASURED, and it is narrower than "every reset is pinned".
// There are 7 reset sites in useBrushOverlay.ts (cancelActiveStroke, five
// teardown branches inside commitBrushStroke, and the test-only
// clearPrevStrokePointCount). They are MUTUALLY REDUNDANT, and that was proven by
// defeat, not assumed:
//   - removing ALL 7 reddens both reset tests here (2 failed / 7);
//   - removing any ONE of them leaves all 7 tests GREEN - including the
//     commit-path success reset, which is the one a reader would most expect to
//     matter.
// So a surviving sibling site masks a dropped one, and PER-SITE coverage is not
// achievable through the public hook. What these two tests actually pin is the
// aggregate contract ("the verdict never survives a teardown") plus the two
// semantically distinct teardown ENTRY points, which is the strongest claim the
// hook surface supports. Do not read this as "each of the 7 is individually
// covered" - it is not, and a future drop of a single site will not be caught
// here. It WILL be caught by removing them all, which is what the two tests were
// written to detect.
//
// No dev flag and no production-conditional behaviour is involved: the harness
// supplies an engine double, exactly as the existing brush suites do, and the
// fence's answer is read off what the stroke actually did.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { useBrushOverlay, flushC4Commits } from "../useBrushOverlay";
import type { DocumentEngine } from "@/engine/document";
import type { CommandHistory } from "@/engine/history";

const { showToast } = vi.hoisted(() => ({ showToast: vi.fn() }));
vi.mock("../Toast", () => ({ showToast }));

// useBrushOverlay calls useDialog(); the harness renders the hook with no real
// DialogProvider in the tree, so the dialog hooks are stubbed - the same pattern
// the existing brush and CropOptionBar suites use.
vi.mock("../dialogs/DialogProvider", () => ({
  useDialog: () => ({
    confirm: vi.fn().mockResolvedValue(false),
    alert: vi.fn().mockResolvedValue(undefined),
    quality: vi.fn().mockResolvedValue(null),
    confirmWithCheckbox: vi.fn().mockResolvedValue({ ok: false, checked: false }),
    confirmSave: vi.fn().mockResolvedValue(null),
    colorPicker: vi.fn().mockResolvedValue(null),
    newDocument: vi.fn().mockResolvedValue(null),
    about: vi.fn(),
  }),
}));

function mockBitmap(w = 512, h = 512): HTMLCanvasElement {
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  (c as unknown as { close: () => void }).close = () => {};
  return c;
}
if (typeof globalThis.createImageBitmap === "undefined") {
  (globalThis as unknown as Record<string, unknown>).createImageBitmap = async () =>
    mockBitmap() as unknown as ImageBitmap;
}
if (typeof globalThis.OffscreenCanvas === "undefined") {
  (globalThis as unknown as Record<string, unknown>).OffscreenCanvas = function (w: number, h: number) {
    const c = document.createElement("canvas");
    c.width = w;
    c.height = h;
    (c as unknown as Record<string, unknown>).transferToImageBitmap = () => mockBitmap(w, h);
    return c;
  };
}
if (typeof (globalThis as unknown as Record<string, unknown>).ImageData === "undefined") {
  (globalThis as unknown as Record<string, unknown>).ImageData = class {
    data: Uint8ClampedArray;
    width: number;
    height: number;
    constructor(d: ArrayLike<number> | number, w?: number, h?: number) {
      if (typeof d === "number") {
        this.width = d;
        this.height = h as number;
        this.data = new Uint8ClampedArray(d * (h as number) * 4);
      } else {
        this.data = Uint8ClampedArray.from(d);
        this.width = w as number;
        this.height = h as number;
      }
    }
  };
}

const SETTINGS = { size: 20, hardness: 1, opacity: 1, flow: 1, smoothing: 0.5 };

/**
 * A harness whose engine double answers `rustHoldsLayer` with whatever the test
 * needs, so the fence's WIRING is exercised without reimplementing it.
 */
function harness(opts: { holds: (id: string) => boolean | null }) {
  const layer = {
    id: "layer-1",
    name: "L",
    visible: true,
    locked: false,
    lockTransparency: false,
    isBackground: false,
    hasAdjustments: false,
    basicAdjustment: undefined,
    transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    width: 512,
    height: 512,
    imageBitmap: document.createElement("canvas") as unknown as ImageBitmap,
  };
  const rustHoldsLayer = vi.fn(opts.holds);
  let lastBitmap: unknown = null;
  const engine = {
    getActiveLayerId: () => layer.id,
    getLayer: () => layer,
    getWidth: () => 512,
    getHeight: () => 512,
    getViewport: () => ({ zoom: 1 }),
    getLayers: () => [layer],
    snapshot: vi.fn(() => ({ layers: [{ id: layer.id, imageBitmap: lastBitmap }] })),
    setLayerImageBitmap: vi.fn((_id: string, b: unknown) => {
      lastBitmap = b;
    }),
    getPaintSurface: () => null,
    // The fence's input. Real production calls this exact method.
    rustHoldsLayer,
  };
  const commit = vi.fn();
  const history = {
    commit,
    setLastPaintCoords: () => {},
    getLastPaintCoords: () => null,
  };
  mockUseEditor({
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => history,
      getActiveDocumentId: () => "doc-fence",
    },
    renderer: { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn() },
    scheduler: { requestRender: vi.fn() },
    fgColor: () => "#ff0000",
    bgColor: () => "#ffffff",
    docWidth: () => 512,
    docHeight: () => 512,
    activeTool: () => "brush",
    brushSize: () => 20,
    brushHardness: () => 1,
    eraserSize: () => 20,
    eraserHardness: () => 1,
  } as never);

  const canvas = document.createElement("canvas");
  canvas.width = 512;
  canvas.height = 512;
  const overlay = useBrushOverlay();
  overlay.setOverlayCanvasRef(canvas);
  return {
    overlay,
    layer,
    engine: engine as unknown as DocumentEngine,
    history: history as unknown as CommandHistory,
    commit,
    rustHoldsLayer,
  };
}

describe("brush fence WIRING: the real pointer path consults the store-presence check", () => {
  beforeEach(() => {
    localStorage.clear();
    showToast.mockClear();
  });
  afterEach(async () => {
    await flushC4Commits();
    localStorage.clear();
  });

  // Property 1 - the pointer path consults the fence. If the call site were
  // removed or short-circuited, rustHoldsLayer would never be called.
  it("calls the fence on the composite path before any dab is recorded", () => {
    const h = harness({ holds: () => true });
    h.overlay.onPaintStroke([{ x: 30, y: 30 }], false, SETTINGS, false);
    expect(
      h.rustHoldsLayer,
      "the composite path must consult the fence - a stroke that never asks is " +
        "the exact wiring hole this test closes",
    ).toHaveBeenCalled();
    expect(h.rustHoldsLayer.mock.calls[0][0]).toBe("layer-1");
  });

  // Property 1 (negative half) - a layer Rust does NOT hold is blocked, and the
  // block is visible rather than a silent drop.
  it("a layer Rust does not hold is refused, and says so", async () => {
    const h = harness({ holds: () => false });
    h.overlay.onPaintStroke([{ x: 30, y: 30 }], false, SETTINGS, false);
    expect(h.rustHoldsLayer).toHaveBeenCalled();
    // A refused stroke produces no committed history entry.
    await h.overlay.commitBrushStroke(h.engine, h.history, "layer-1", false);
    await flushC4Commits();
    expect(h.commit, "a blocked stroke must not commit").not.toHaveBeenCalled();
    // And it is VISIBLE - a silent drop is the bug class being fixed.
    expect(showToast.mock.calls.length).toBeGreaterThan(0);
  });

  // Property 1 (commit half) - the COMMIT path has its OWN consult, and it is
  // only reachable when no composite ran first. When the composite has already
  // refused, the stroke records no dabs and the commit returns on its own count
  // check, so the two gates are indistinguishable - measured: removing the
  // commit-path call leaves every other test in this file green. Invoking the
  // commit on its own is what separates them.
  it("the commit path consults the fence on its own, not only via the composite", async () => {
    const h = harness({ holds: () => false });
    // No preceding onPaintStroke: this is the only shape that isolates the
    // commit-path consult from the composite one.
    await h.overlay.commitBrushStroke(h.engine, h.history, "layer-1", false);
    await flushC4Commits();
    expect(
      h.rustHoldsLayer,
      "the commit path must consult the fence itself - if it relied solely on the " +
        "composite gate, a commit reached without one would replay the model " +
        "unfenced",
    ).toHaveBeenCalled();
    expect(showToast.mock.calls.length).toBeGreaterThan(0);
    expect(h.commit).not.toHaveBeenCalled();
  });

  // Property 2 - the decision is taken ONCE per stroke and shared. Two
  // onPaintStroke calls plus a commit in one stroke must consult the mirror once.
  it("takes the decision ONCE per stroke and both paths share it", async () => {
    const h = harness({ holds: () => true });
    // Several move events within one stroke, then the commit.
    h.overlay.onPaintStroke([{ x: 30, y: 30 }], false, SETTINGS, false);
    h.overlay.onPaintStroke([{ x: 32, y: 30 }], false, SETTINGS, false);
    h.overlay.onPaintStroke([{ x: 34, y: 30 }], false, SETTINGS, false);
    await h.overlay.commitBrushStroke(h.engine, h.history, "layer-1", false);
    await flushC4Commits();
    // Cached per stroke: NOT once per event. If the commit path re-decided, or the
    // composite path did, this count would exceed one.
    expect(
      h.rustHoldsLayer.mock.calls.length,
      "the fence must be consulted once per stroke, not once per event",
    ).toBe(1);
  });

  // Property 3 - the per-stroke reset, pinned on the COMMIT teardown. Two
  // consecutive strokes, the second on the opposite answer, must each consult the
  // mirror. Deliberately NO cancelActiveStroke between them: a cancel also resets,
  // so including one would mask a missing commit-side reset. Measured - that is
  // exactly how the first version of this test went green with the commit reset
  // removed.
  it("a committed stroke resets the decision, so the NEXT stroke re-asks", async () => {
    const answers: Array<boolean | null> = [true, false];
    let call = 0;
    const h = harness({ holds: () => answers[Math.min(call++, answers.length - 1)] });

    // Stroke 1: Rust holds it -> allowed and committed.
    h.overlay.onPaintStroke([{ x: 30, y: 30 }], false, SETTINGS, false);
    await h.overlay.commitBrushStroke(h.engine, h.history, "layer-1", false);
    await flushC4Commits();
    expect(h.rustHoldsLayer).toHaveBeenCalledTimes(1);
    expect(h.commit).toHaveBeenCalledTimes(1);

    // Stroke 2: Rust no longer holds it -> must be re-asked and blocked. Reusing
    // stroke 1's cached `true` would paint on a layer Rust no longer holds.
    h.overlay.onPaintStroke([{ x: 30, y: 30 }], false, SETTINGS, false);
    expect(
      h.rustHoldsLayer.mock.calls.length,
      "a committed stroke must reset the per-stroke verdict - a stale one would " +
        "silently paint on a layer Rust no longer holds",
    ).toBe(2);
    await h.overlay.commitBrushStroke(h.engine, h.history, "layer-1", false);
    await flushC4Commits();
    // Stroke 2 was refused, so it added no history.
    expect(h.commit.mock.calls.length).toBe(1);
    expect(showToast.mock.calls.length).toBeGreaterThan(0);
  });

  // Property 3, the CANCEL teardown specifically. The sequence above cannot pin
  // it: commitBrushStroke resets the decision on its own way out, so dropping the
  // reset inside cancelActiveStroke still left that test green - measured, and the
  // reason this case exists. Cancelling WITHOUT a prior commit is the only way to
  // reach the cancel teardown with a cached verdict still in hand.
  it("cancelActiveStroke alone resets the decision (no commit in between)", () => {
    const answers: Array<boolean | null> = [true, false];
    let call = 0;
    const h = harness({ holds: () => answers[Math.min(call++, answers.length - 1)] });

    // Stroke 1 caches `true`. Deliberately NOT committed.
    h.overlay.onPaintStroke([{ x: 30, y: 30 }], false, SETTINGS, false);
    expect(h.rustHoldsLayer).toHaveBeenCalledTimes(1);

    h.overlay.cancelActiveStroke();

    // Stroke 2 must re-consult. Without the reset in cancelActiveStroke it would
    // reuse the cached `true` and paint on a layer Rust no longer holds.
    h.overlay.onPaintStroke([{ x: 30, y: 30 }], false, SETTINGS, false);
    expect(
      h.rustHoldsLayer.mock.calls.length,
      "cancelActiveStroke must reset the per-stroke verdict - a stale one after a " +
        "cancel would silently paint on a layer Rust no longer holds",
    ).toBe(2);
  });

  // Property 4 - the "only a positive false blocks" semantic must survive the
  // WIRING, not just the predicate: an unknown mirror must let the stroke through.
  it.each([
    ["null (no mirror)", null],
    ["undefined (partial engine)", undefined],
  ] as Array<[string, boolean | null]>)(
    "an UNKNOWN mirror (%s) lets the stroke through the wiring",
    async (_label, value) => {
      const h = harness({ holds: () => value });
      h.overlay.onPaintStroke([{ x: 30, y: 30 }], false, SETTINGS, false);
      await h.overlay.commitBrushStroke(h.engine, h.history, "layer-1", false);
      await flushC4Commits();
      expect(
        h.commit,
        "unknown must paint: with no readable mirror the replay cannot run, so " +
          "blocking here would kill painting outright",
      ).toHaveBeenCalled();
    },
  );
});