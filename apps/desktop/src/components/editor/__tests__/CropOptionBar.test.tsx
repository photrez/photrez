import { describe, it, expect, vi, afterEach } from "vitest";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { render } from "solid-js/web";
import { createSignal } from "solid-js";
import { CropOptionBar } from "../CropOptionBar";

// CropOptionBar (via CropOptionBarSections) calls useDialog(); the test renders
// it under a mocked EditorContext (no real DialogProvider in the tree), so stub
// the dialog hooks. Mirrors the pattern in colorPickerCanvas/eyedropper tests.
// `colorPicker` is a hoisted spy so individual tests can drive its resolution.
const dialogMocks = vi.hoisted(() => ({
  colorPicker: vi.fn().mockResolvedValue(null),
}));
vi.mock("../dialogs/DialogProvider", () => ({
  useDialog: () => ({
    confirm: vi.fn().mockResolvedValue(false),
    alert: vi.fn().mockResolvedValue(undefined),
    quality: vi.fn().mockResolvedValue(null),
    confirmWithCheckbox: vi.fn().mockResolvedValue({ ok: false, checked: false }),
    confirmSave: vi.fn().mockResolvedValue(null),
    colorPicker: dialogMocks.colorPicker,
    newDocument: vi.fn().mockResolvedValue(null),
    about: vi.fn(),
  }),
}));

function clickPill(container: HTMLElement, label: string) {
  let searchLabel = label;
  if (label === "+") searchLabel = "Custom...";

  const buttonsBefore = container.querySelectorAll("button");
  let pill = Array.from(buttonsBefore).find(b => b.textContent?.trim() === searchLabel);
  
  if (!pill) {
    const ratioBtn = Array.from(buttonsBefore).find(b => {
      const text = b.textContent?.trim() || "";
      return text.startsWith("Ratio:") || text === "Ratio";
    });
    if (ratioBtn) {
      ratioBtn.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      const buttonsAfter = container.querySelectorAll("button");
      pill = Array.from(buttonsAfter).find(b => b.textContent?.trim() === searchLabel);
    }
  }
  
  if (pill) {
    pill.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  }
}

const MAX_W = 800; // min(viewportWidth=800, docWidth=1600 * zoom=1)
const MAX_H = 600; // min(viewportHeight=600, docHeight=1200 * zoom=1)

const modernContextBase = {
  workspace: {}, setActiveTool: vi.fn(), scheduler: {},
  bgColor: () => "#ffffff", setBgColor: vi.fn(),
  cropRect: () => null, setCropRect: vi.fn(),
  cropInteractionMode: () => "modern" as const, setCropInteractionMode: vi.fn(),
  cropMode: () => "free" as const, setCropMode: vi.fn(),
  cropAspect: () => null, setCropAspect: vi.fn(),
  cropSizeTarget: () => null, setCropSizeTarget: vi.fn(),
  cropGuideMode: () => "none", setCropGuideMode: vi.fn(),
  cropDeletePixels: () => false, setCropDeletePixels: vi.fn(),
  cropFillEnabled: () => true, setCropFillEnabled: vi.fn(),
  cropFillSource: () => "background" as const, setCropFillSource: vi.fn(),
  cropFillCustomColor: () => "#ffffff", setCropFillCustomColor: vi.fn(),
  cropSizeUnit: () => "px" as const, setCropSizeUnit: vi.fn(),
  cropRotation: () => 0, setCropRotation: vi.fn(),
  hiddenCropPreview: () => null, setHiddenCropPreview: vi.fn(),
  docWidth: () => 1600, docHeight: () => 1200,
  viewportWidth: () => MAX_W, viewportHeight: () => MAX_H,
  zoom: () => 1,
  pan: () => ({ x: 0, y: 0 }),
  modernCropFrame: () => ({ x: 200, y: 150, w: 400, h: 300 }), setModernCropFrame: vi.fn(),
  modernCropImageTransform: () => ({ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 }),
  setModernCropImageTransform: vi.fn(),
  resetModernCrop: vi.fn(),
  commitCropState: vi.fn(),
  activeDocumentId: () => "mock-doc-id",
};

function runWithContainer(fn: (container: HTMLElement, dispose: () => void) => void) {
  const container = document.createElement("div");
  document.body.appendChild(container);
  let disposed = false;
  const dispose = () => {
    if (!disposed) {
      disposed = true;
      container.parentNode?.removeChild(container);
    }
  };
  try {
    fn(container, dispose);
  } finally {
    dispose();
  }
}

function renderOptionBar(mockValue: any, container: HTMLElement) {
  mockUseEditor(mockValue);
  return render(() => <CropOptionBar />, container);
}

describe("CropOptionBar", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe("fill background controls", () => {
    it("defaults to the editor background color in Use Background Color mode", () => {
      runWithContainer((container, done) => {
        renderOptionBar({
          ...modernContextBase,
          bgColor: () => "#224466",
          cropFillEnabled: () => true,
          cropFillSource: () => "background",
          cropFillCustomColor: () => "#ff00ff",
        }, container);

        const colorBtn = container.querySelector("[data-crop-fill-color]") as HTMLButtonElement | null;
        expect(colorBtn).not.toBeNull();
        // The swatch is a styled button (background-color), not an <input>.
        expect(colorBtn!.style.backgroundColor).toBe("rgb(34, 68, 102)");
        expect(container.querySelector("[data-crop-fill-source='background']")).not.toBeNull();
        done();
      });
    });

    it("changing the background color updates crop fill while using background mode", () => {
      runWithContainer((container, done) => {
        const [bgColor, setBgColor] = createSignal("#111111");
        renderOptionBar({
          ...modernContextBase,
          bgColor,
          cropFillEnabled: () => true,
          cropFillSource: () => "background",
        }, container);

        const colorBtn = container.querySelector("[data-crop-fill-color]") as HTMLButtonElement | null;
        expect(colorBtn!.style.backgroundColor).toBe("rgb(17, 17, 17)");
        setBgColor("#445566");
        expect(colorBtn!.style.backgroundColor).toBe("rgb(68, 85, 102)");
        done();
      });
    });

    it("custom crop fill opens the picker and applies the chosen color without mutating the global swatch", async () => {
      const setCropFillSource = vi.fn();
      const setCropFillCustomColor = vi.fn();
      const setBgColor = vi.fn();
      const setColorPickerOpen = vi.fn();
      const setColorPickerTarget = vi.fn();
      dialogMocks.colorPicker.mockResolvedValueOnce("#778899");

      const container = document.createElement("div");
      document.body.appendChild(container);
      try {
        renderOptionBar({
          ...modernContextBase,
          bgColor: () => "#111111",
          setBgColor,
          cropFillEnabled: () => true,
          cropFillSource: () => "background",
          setCropFillSource,
          setCropFillCustomColor,
          setColorPickerOpen,
          setColorPickerTarget,
        }, container);

        const colorBtn = container.querySelector("[data-crop-fill-color]") as HTMLButtonElement | null;
        expect(colorBtn).not.toBeNull();
        colorBtn!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
        // allow the async colorPicker handler to resolve
        await new Promise((r) => setTimeout(r, 0));

        expect(dialogMocks.colorPicker).toHaveBeenCalled();
        expect(setColorPickerOpen).toHaveBeenCalledWith(true);
        expect(setCropFillSource).toHaveBeenCalledWith("custom");
        expect(setCropFillCustomColor).toHaveBeenCalledWith("#778899");
        expect(setBgColor).not.toHaveBeenCalled();
      } finally {
        container.parentNode?.removeChild(container);
      }
    });

    it("can return a custom fill to Use Background Color", () => {
      runWithContainer((container, done) => {
        const setCropFillSource = vi.fn();
        renderOptionBar({
          ...modernContextBase,
          bgColor: () => "#111111",
          cropFillEnabled: () => true,
          cropFillSource: () => "custom",
          cropFillCustomColor: () => "#778899",
          setCropFillSource,
        }, container);

        const useBgButton = container.querySelector("[data-crop-fill-use-bg]") as HTMLButtonElement | null;
        expect(useBgButton).not.toBeNull();
        useBgButton!.click();
        expect(setCropFillSource).toHaveBeenCalledWith("background");
        done();
      });
    });
  });

  describe("crop box always fits inside canvas after changes", () => {
    it("keeps frame within canvas bounds after repeated mode changes", () => {
      runWithContainer((container, done) => {
        const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("free");
        const [cropAspect, setCropAspect] = createSignal<{ w: number; h: number } | null>(null);
        const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>(null);
        const [modernFrame, setModernFrame] = createSignal<{ w: number; h: number }>({ w: 400, h: 300 });
        const setCropModeSpy = vi.fn((m) => setCropMode(m));
        const setCropAspectSpy = vi.fn((a) => setCropAspect(a));
        const setCropSizeTargetSpy = vi.fn((t) => setCropSizeTarget(t));
        const setModernFrameSpy = vi.fn((f) => setModernFrame(f));

        renderOptionBar({
          ...modernContextBase,
          cropMode, setCropMode: setCropModeSpy,
          cropAspect, setCropAspect: setCropAspectSpy,
          cropSizeTarget, setCropSizeTarget: setCropSizeTargetSpy,
          modernCropFrame: modernFrame, setModernCropFrame: setModernFrameSpy,
        }, container);

        // Free→Ratio: frame should be max at 16:9
        clickPill(container, "16:9");
        let frame = setModernFrameSpy.mock.lastCall?.[0];
        expect(frame.w).toBeLessThanOrEqual(MAX_W);
        expect(frame.h).toBeLessThanOrEqual(MAX_H);
        expect(frame.w / frame.h).toBeCloseTo(16 / 9, 1);

        // Ratio→Size (800x600): frame at target zoom
        clickPill(container, "Size");
        frame = setModernFrameSpy.mock.lastCall?.[0];
        expect(frame.w).toBeLessThanOrEqual(MAX_W);
        expect(frame.h).toBeLessThanOrEqual(MAX_H);

        // Size→Free: frame unchanged
        clickPill(container, "Free");
        frame = setModernFrameSpy.mock.lastCall?.[0];
        expect(frame.w).toBeLessThanOrEqual(MAX_W);
        expect(frame.h).toBeLessThanOrEqual(MAX_H);

        // Free→Ratio again: frame re-fitted
        clickPill(container, "16:9");
        frame = setModernFrameSpy.mock.lastCall?.[0];
        expect(frame.w).toBeLessThanOrEqual(MAX_W);
        expect(frame.h).toBeLessThanOrEqual(MAX_H);
        expect(frame.w / frame.h).toBeCloseTo(16 / 9, 1);

        done();
      });
    });

    it("clamps oversized frame when switching from Size to Free in Modern mode", () => {
      runWithContainer((container, done) => {
        // Start with an oversized frame (simulating a large size target that doesn't fit)
        const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("size");
        const [cropAspect, setCropAspect] = createSignal<{ w: number; h: number } | null>(null);
        const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>({ w: 5000, h: 4000 });
        const [modernFrame, setModernFrame] = createSignal<{ w: number; h: number }>({ w: 5000, h: 4000 });
        const setCropModeSpy = vi.fn((m) => setCropMode(m));
        const setModernFrameSpy = vi.fn((f) => setModernFrame(f));

        renderOptionBar({
          ...modernContextBase,
          cropMode, setCropMode: setCropModeSpy,
          cropAspect, setCropAspect: vi.fn(),
          cropSizeTarget, setCropSizeTarget: vi.fn(),
          modernCropFrame: modernFrame, setModernCropFrame: setModernFrameSpy,
        }, container);

        clickPill(container, "Free");

        const frame = setModernFrameSpy.mock.lastCall?.[0];
        expect(frame.w).toBeLessThanOrEqual(MAX_W);
        expect(frame.h).toBeLessThanOrEqual(MAX_H);
        // Aspect preserved from 5000:4000 = 5:4 = 1.25
        expect(frame.w / frame.h).toBeCloseTo(5 / 4, 1);
        // Verify frame coordinates are correctly centered in the viewport
        expect(frame.x).toBeCloseTo((MAX_W - frame.w) / 2, 1);
        expect(frame.y).toBeCloseTo((MAX_H - frame.h) / 2, 1);

        done();
      });
    });

    it("fills canvas at target aspect when switching to Size mode (Modern)", () => {
      runWithContainer((container, done) => {
        const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("free");
        const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>(null);
        const [modernFrame, setModernFrame] = createSignal<{ w: number; h: number }>({ w: 400, h: 300 });
        const setCropModeSpy = vi.fn((m) => setCropMode(m));
        const setCropSizeTargetSpy = vi.fn((t) => setCropSizeTarget(t));
        const setModernFrameSpy = vi.fn((f) => setModernFrame(f));

        renderOptionBar({
          ...modernContextBase,
          zoom: () => 1,
          cropMode, setCropMode: setCropModeSpy,
          cropAspect: () => null, setCropAspect: vi.fn(),
          cropSizeTarget, setCropSizeTarget: setCropSizeTargetSpy,
          modernCropFrame: modernFrame, setModernCropFrame: setModernFrameSpy,
        }, container);

        // Switch to Size mode — frame fills canvas at target 800x600 (4:3) aspect
        clickPill(container, "Size");
        let frame = setModernFrameSpy.mock.lastCall?.[0];
        expect(frame.w).toBeLessThanOrEqual(MAX_W);
        expect(frame.h).toBeLessThanOrEqual(MAX_H);

        done();
      });
    });

    it("fills canvas at target aspect under any zoom (Modern)", () => {
      runWithContainer((container, done) => {
        const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("free");
        const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>(null);
        const [modernFrame, setModernFrame] = createSignal<{ w: number; h: number }>({ w: 400, h: 300 });
        const setCropModeSpy = vi.fn((m) => setCropMode(m));
        const setCropSizeTargetSpy = vi.fn((t) => setCropSizeTarget(t));
        const setModernFrameSpy = vi.fn((f) => setModernFrame(f));

        renderOptionBar({
          ...modernContextBase,
          zoom: () => 0.5,
          cropMode, setCropMode: setCropModeSpy,
          cropAspect: () => null, setCropAspect: vi.fn(),
          cropSizeTarget, setCropSizeTarget: setCropSizeTargetSpy,
          modernCropFrame: modernFrame, setModernCropFrame: setModernFrameSpy,
        }, container);

        // Switch to Size mode — frame fills canvas at target 800x600 (4:3) aspect
        // Even at zoom=0.5, the frame should fill the max canvas bounds (in screen space)
        clickPill(container, "Size");
        let frame = setModernFrameSpy.mock.lastCall?.[0];
        // Frame is in doc coords; multiply by zoom to check screen-space bounds
        expect(frame.w * 0.5).toBeLessThanOrEqual(MAX_W);
        expect(frame.h * 0.5).toBeLessThanOrEqual(MAX_H);
        expect(frame.w / frame.h).toBeCloseTo(800 / 600, 1);

        done();
      });
    });

    it("re-fits frame after ratio preset change in Modern mode", () => {
      runWithContainer((container, done) => {
        const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("ratio");
        const [cropAspect, setCropAspect] = createSignal<{ w: number; h: number } | null>({ w: 16, h: 9 });
        const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>(null);
        const [modernFrame, setModernFrame] = createSignal<{ w: number; h: number }>({ w: 800, h: 450 });
        const setCropAspectSpy = vi.fn((a) => setCropAspect(a));
        const setModernFrameSpy = vi.fn((f) => setModernFrame(f));

        renderOptionBar({
          ...modernContextBase,
          cropMode, setCropMode: vi.fn(),
          cropAspect, setCropAspect: setCropAspectSpy,
          cropSizeTarget, setCropSizeTarget: vi.fn(),
          modernCropFrame: modernFrame, setModernCropFrame: setModernFrameSpy,
        }, container);

        // Change preset from 16:9 to 3:2
        clickPill(container, "3:2");

        expect(setCropAspectSpy).toHaveBeenCalledWith({ w: 3, h: 2 });
        const frame = setModernFrameSpy.mock.lastCall?.[0];
        expect(frame.w).toBeLessThanOrEqual(MAX_W);
        expect(frame.h).toBeLessThanOrEqual(MAX_H);
        // 3:2 aspect preserved
        expect(frame.w / frame.h).toBeCloseTo(3 / 2, 1);

        done();
      });
    });

    it("re-fits frame when switching to custom ratio 'preset' in Modern mode", () => {
      const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("ratio");
      const [cropAspect, setCropAspect] = createSignal<{ w: number; h: number } | null>(null);
      const [modernFrame, setModernFrame] = createSignal<{ w: number; h: number }>({ w: 600, h: 400 });
      const setCropAspectSpy = vi.fn((a) => setCropAspect(a));
      const setModernFrameSpy = vi.fn((f) => setModernFrame(f));

      const mockValue = {
        ...modernContextBase,
        cropMode, setCropMode: vi.fn(),
        cropAspect, setCropAspect: setCropAspectSpy,
        modernCropFrame: modernFrame, setModernCropFrame: setModernFrameSpy,
      };

      const container = document.createElement("div");
      document.body.appendChild(container);
      mockUseEditor(mockValue);
      const dispose = render(() => <CropOptionBar />, container);

      // Click "+" to expand custom W:H fields, then submit W=5 H=4
      clickPill(container, "+");

      // Query inputs after clicking "+" (custom W, custom H, Angle)
      const inputs = Array.from(container.querySelectorAll("input")).filter((i) => (i as HTMLInputElement).type === "text");
      const wInput = inputs[0] as HTMLInputElement;
      wInput.focus();
      wInput.value = "5";
      wInput.dispatchEvent(new InputEvent("input", { bubbles: true }));
      wInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, keyCode: 13 }));

      expect(setCropAspectSpy).toHaveBeenCalledWith({ w: 5, h: 9 });
      const frame = setModernFrameSpy.mock.lastCall?.[0];
      expect(frame.w).toBeLessThanOrEqual(MAX_W);
      expect(frame.h).toBeLessThanOrEqual(MAX_H);
      expect(frame.w / frame.h).toBeCloseTo(5 / 9, 1);

      dispose();
      document.body.removeChild(container);
    });

    it("keeps frame within canvas bounds in Classic mode after mode changes", () => {
      runWithContainer((container, done) => {
        const [cropRect, setCropRect] = createSignal<{ x: number; y: number; w: number; h: number } | null>({
          x: 0, y: 0, w: 1600, h: 1200,
        });
        const [cropAspect, setCropAspect] = createSignal<{ w: number; h: number } | null>(null);
        const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>(null);
        const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("free");
        const setCropRectSpy = vi.fn((r) => setCropRect(r));
        const setCropModeSpy = vi.fn((m) => setCropMode(m));

        renderOptionBar({
          workspace: {}, setActiveTool: vi.fn(), scheduler: {},
          cropRect, setCropRect: setCropRectSpy,
          cropInteractionMode: () => "classic" as const, setCropInteractionMode: vi.fn(),
          cropMode, setCropMode: setCropModeSpy,
          cropGuideMode: () => "none", setCropGuideMode: vi.fn(),
          cropDeletePixels: () => false, setCropDeletePixels: vi.fn(),
          cropFillEnabled: () => true, setCropFillEnabled: vi.fn(),
          cropFillSource: () => "background" as const, setCropFillSource: vi.fn(),
          cropFillCustomColor: () => "#ffffff", setCropFillCustomColor: vi.fn(),
          cropAspect, setCropAspect,
          cropSizeTarget, setCropSizeTarget,
          cropSizeUnit: () => "px" as const, setCropSizeUnit: vi.fn(),
          cropRotation: () => 0, setCropRotation: vi.fn(),
          hiddenCropPreview: () => null, setHiddenCropPreview: vi.fn(),
          docWidth: () => 1600, docHeight: () => 1200,
          activeDocumentId: () => "mock-doc-id",
        }, container);

        // Free→Ratio: fits to 16:9 within 1600x1200 doc
        clickPill(container, "16:9");
        let rect = setCropRectSpy.mock.lastCall?.[0];
        expect(rect.x).toBeGreaterThanOrEqual(0);
        expect(rect.y).toBeGreaterThanOrEqual(0);
        expect(rect.x + rect.w).toBeLessThanOrEqual(1600);
        expect(rect.y + rect.h).toBeLessThanOrEqual(1200);
        expect(rect.w / rect.h).toBeCloseTo(16 / 9, 1);

        // Ratio→Size (800x600): fits within doc
        clickPill(container, "Size");
        rect = setCropRectSpy.mock.lastCall?.[0];
        expect(rect.x).toBeGreaterThanOrEqual(0);
        expect(rect.x + rect.w).toBeLessThanOrEqual(1600);
        expect(rect.y + rect.h).toBeLessThanOrEqual(1200);

        // Size→Free: no rect change expected
        clickPill(container, "Free");
        // Only 2 calls so far (Ratio + Size), Free doesn't change rect in Classic
        expect(setCropRectSpy).toHaveBeenCalledTimes(2);

        done();
      });
    });
  });

  describe("mode select immediate application", () => {
    it("applies Free→Ratio immediately in Classic mode with existing cropRect", () => {
      const [cropRect, setCropRect] = createSignal<{ x: number; y: number; w: number; h: number } | null>({
        x: 0, y: 0, w: 1600, h: 1200,
      });
      const [cropAspect, setCropAspect] = createSignal<{ w: number; h: number } | null>(null);
      const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("free");
      const setCropRectSpy = vi.fn((r) => setCropRect(r));
      const setCropAspectSpy = vi.fn((a) => setCropAspect(a));
      const setCropModeSpy = vi.fn((m) => setCropMode(m));

      mockUseEditor({
        workspace: {}, setActiveTool: vi.fn(), scheduler: {},
        cropRect, setCropRect: setCropRectSpy,
        cropInteractionMode: () => "classic" as const, setCropInteractionMode: vi.fn(),
        cropMode, setCropMode: setCropModeSpy,
        cropGuideMode: () => "none", setCropGuideMode: vi.fn(),
        cropDeletePixels: () => false, setCropDeletePixels: vi.fn(),
          cropFillEnabled: () => true, setCropFillEnabled: vi.fn(),
          cropFillSource: () => "background" as const, setCropFillSource: vi.fn(),
          cropFillCustomColor: () => "#ffffff", setCropFillCustomColor: vi.fn(),
        cropAspect, setCropAspect: setCropAspectSpy,
        cropSizeTarget: () => null, setCropSizeTarget: vi.fn(),
        cropSizeUnit: () => "px" as const, setCropSizeUnit: vi.fn(),
        cropRotation: () => 0, setCropRotation: vi.fn(),
        hiddenCropPreview: () => null, setHiddenCropPreview: vi.fn(),
        docWidth: () => 1600, docHeight: () => 1200,
        activeDocumentId: () => "mock-doc-id",
      });

      const container = document.createElement("div");
      document.body.appendChild(container);
      const dispose = render(() => <CropOptionBar />, container);

      clickPill(container, "16:9");

      expect(setCropModeSpy).toHaveBeenCalledWith("ratio");
      expect(setCropAspectSpy).toHaveBeenCalledWith({ w: 16, h: 9 });
      expect(setCropRectSpy).toHaveBeenCalled();
      const rect = setCropRectSpy.mock.lastCall?.[0];
      // 1600x1200 canvas, 16:9 aspect (1.78): width-constrained → w=1600, h=900, centered y=150
      expect(rect.w).toBe(1600);
      expect(rect.h).toBeCloseTo(900, 1);
      expect(rect.y).toBeCloseTo(150, 1);
      expect(rect.x).toBe(0);

      dispose();
      container.parentNode?.removeChild(container);
    });

    it("applies Free→Ratio immediately in Modern mode", () => {
      const [cropAspect, setCropAspect] = createSignal<{ w: number; h: number } | null>(null);
      const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("free");
      const [modernFrame, setModernFrame] = createSignal<{ w: number; h: number }>({ w: 500, h: 500 });
      const setCropModeSpy = vi.fn((m) => setCropMode(m));
      const setCropAspectSpy = vi.fn((a) => setCropAspect(a));
      const setModernFrameSpy = vi.fn((f) => setModernFrame(f));
      const setCropRectSpy = vi.fn();

      mockUseEditor({
        workspace: {}, setActiveTool: vi.fn(), scheduler: {},
        cropRect: () => null, setCropRect: setCropRectSpy,
        cropInteractionMode: () => "modern" as const, setCropInteractionMode: vi.fn(),
        cropMode, setCropMode: setCropModeSpy,
        cropGuideMode: () => "none", setCropGuideMode: vi.fn(),
        cropDeletePixels: () => false, setCropDeletePixels: vi.fn(),
          cropFillEnabled: () => true, setCropFillEnabled: vi.fn(),
          cropFillSource: () => "background" as const, setCropFillSource: vi.fn(),
          cropFillCustomColor: () => "#ffffff", setCropFillCustomColor: vi.fn(),
        cropAspect, setCropAspect: setCropAspectSpy,
        cropSizeTarget: () => null, setCropSizeTarget: vi.fn(),
        cropSizeUnit: () => "px" as const, setCropSizeUnit: vi.fn(),
        cropRotation: () => 0, setCropRotation: vi.fn(),
        hiddenCropPreview: () => null, setHiddenCropPreview: vi.fn(),
        docWidth: () => 1600, docHeight: () => 1200,
        viewportWidth: () => 800, viewportHeight: () => 600,
        zoom: () => 1,
        pan: () => ({ x: 0, y: 0 }),
        modernCropFrame: modernFrame, setModernCropFrame: setModernFrameSpy,
        modernCropImageTransform: () => ({ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 }),
        setModernCropImageTransform: vi.fn(),
        resetModernCrop: vi.fn(),
        commitCropState: vi.fn(),
        activeDocumentId: () => "mock-doc-id",
      });

      const container = document.createElement("div");
      document.body.appendChild(container);
      const dispose = render(() => <CropOptionBar />, container);

      clickPill(container, "16:9");

      expect(setCropModeSpy).toHaveBeenCalledWith("ratio");
      expect(setCropAspectSpy).toHaveBeenCalledWith({ w: 16, h: 9 });
      expect(setModernFrameSpy).toHaveBeenCalled();
      const frame = setModernFrameSpy.mock.lastCall?.[0];
      // getDefaultModernCropFrame fits 16:9 inside min(viewport, projected canvas)
      // maxW = min(800, 1600) = 800, maxH = min(600, 1200) = 600
      // 16:9 → w=800, h=450 (fits within 600)
      expect(frame.w).toBe(800);
      expect(frame.h).toBe(450);

      dispose();
      container.parentNode?.removeChild(container);
    });

    it("fills canvas at target aspect when switching to Size mode (Modern)", () => {
      const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>(null);
      const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("free");
      const [modernFrame, setModernFrame] = createSignal<{ w: number; h: number }>({ w: 500, h: 500 });
      const setCropModeSpy = vi.fn((m) => setCropMode(m));
      const setCropSizeTargetSpy = vi.fn((t) => setCropSizeTarget(t));
      const setModernFrameSpy = vi.fn((f) => setModernFrame(f));
      const setCropRectSpy = vi.fn();

      mockUseEditor({
        workspace: {}, setActiveTool: vi.fn(), scheduler: {},
        cropRect: () => null, setCropRect: setCropRectSpy,
        cropInteractionMode: () => "modern" as const, setCropInteractionMode: vi.fn(),
        cropMode, setCropMode: setCropModeSpy,
        cropGuideMode: () => "none", setCropGuideMode: vi.fn(),
        cropDeletePixels: () => false, setCropDeletePixels: vi.fn(),
          cropFillEnabled: () => true, setCropFillEnabled: vi.fn(),
          cropFillSource: () => "background" as const, setCropFillSource: vi.fn(),
          cropFillCustomColor: () => "#ffffff", setCropFillCustomColor: vi.fn(),
        cropAspect: () => null, setCropAspect: vi.fn(),
        cropSizeTarget, setCropSizeTarget: setCropSizeTargetSpy,
        cropSizeUnit: () => "px" as const, setCropSizeUnit: vi.fn(),
        cropRotation: () => 0, setCropRotation: vi.fn(),
        hiddenCropPreview: () => null, setHiddenCropPreview: vi.fn(),
        docWidth: () => 1600, docHeight: () => 1200,
        viewportWidth: () => 800, viewportHeight: () => 600,
        zoom: () => 1,
        pan: () => ({ x: 0, y: 0 }),
        modernCropFrame: modernFrame, setModernCropFrame: setModernFrameSpy,
        modernCropImageTransform: () => ({ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 }),
        setModernCropImageTransform: vi.fn(),
        resetModernCrop: vi.fn(),
        commitCropState: vi.fn(),
        activeDocumentId: () => "mock-doc-id",
      });

      const container = document.createElement("div");
      document.body.appendChild(container);
      const dispose = render(() => <CropOptionBar />, container);

      clickPill(container, "Size");

      expect(setCropModeSpy).toHaveBeenCalledWith("size");
      expect(setCropSizeTargetSpy).toHaveBeenCalledWith({ w: 800, h: 600 });
      expect(setModernFrameSpy).toHaveBeenCalled();
      const frame = setModernFrameSpy.mock.lastCall?.[0];
      // target 800x600 (4:3) matches canvas aspect → frame fills viewport
      expect(frame.w).toBe(800);
      expect(frame.h).toBe(600);

      dispose();
      container.parentNode?.removeChild(container);
    });

    it("applies Free→Size immediately in Classic mode with existing cropRect", () => {
      const [cropRect, setCropRect] = createSignal<{ x: number; y: number; w: number; h: number } | null>({
        x: 0, y: 0, w: 1600, h: 1200,
      });
      const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>(null);
      const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("free");
      const setCropRectSpy = vi.fn((r) => setCropRect(r));
      const setCropSizeTargetSpy = vi.fn((t) => setCropSizeTarget(t));
      const setCropModeSpy = vi.fn((m) => setCropMode(m));

      mockUseEditor({
        workspace: {}, setActiveTool: vi.fn(), scheduler: {},
        cropRect, setCropRect: setCropRectSpy,
        cropInteractionMode: () => "classic" as const, setCropInteractionMode: vi.fn(),
        cropMode, setCropMode: setCropModeSpy,
        cropGuideMode: () => "none", setCropGuideMode: vi.fn(),
        cropDeletePixels: () => false, setCropDeletePixels: vi.fn(),
          cropFillEnabled: () => true, setCropFillEnabled: vi.fn(),
          cropFillSource: () => "background" as const, setCropFillSource: vi.fn(),
          cropFillCustomColor: () => "#ffffff", setCropFillCustomColor: vi.fn(),
        cropAspect: () => null, setCropAspect: vi.fn(),
        cropSizeTarget, setCropSizeTarget: setCropSizeTargetSpy,
        cropSizeUnit: () => "px" as const, setCropSizeUnit: vi.fn(),
        cropRotation: () => 0, setCropRotation: vi.fn(),
        hiddenCropPreview: () => null, setHiddenCropPreview: vi.fn(),
        docWidth: () => 1600, docHeight: () => 1200,
        activeDocumentId: () => "mock-doc-id",
      });

      const container = document.createElement("div");
      document.body.appendChild(container);
      const dispose = render(() => <CropOptionBar />, container);

      clickPill(container, "Size");

      expect(setCropModeSpy).toHaveBeenCalledWith("size");
      expect(setCropSizeTargetSpy).toHaveBeenCalledWith({ w: 800, h: 600 });
      expect(setCropRectSpy).toHaveBeenCalled();
      const rect = setCropRectSpy.mock.lastCall?.[0];
      // 1600x1200 canvas, size target 800x600 (aspect 1.33) matches canvas → full canvas
      expect(rect.w).toBe(1600);
      expect(rect.h).toBe(1200);

      dispose();
      container.parentNode?.removeChild(container);
    });

    it("applies Ratio→Free immediately without changing frame (Classic)", () => {
      const [cropRect, setCropRect] = createSignal<{ x: number; y: number; w: number; h: number } | null>({
        x: 50, y: 50, w: 400, h: 300,
      });
      const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("ratio");
      const setCropRectSpy = vi.fn((r) => setCropRect(r));
      const setCropModeSpy = vi.fn((m) => setCropMode(m));

      mockUseEditor({
        workspace: {}, setActiveTool: vi.fn(), scheduler: {},
        cropRect, setCropRect: setCropRectSpy,
        cropInteractionMode: () => "classic" as const, setCropInteractionMode: vi.fn(),
        cropMode, setCropMode: setCropModeSpy,
        cropGuideMode: () => "none", setCropGuideMode: vi.fn(),
        cropDeletePixels: () => false, setCropDeletePixels: vi.fn(),
          cropFillEnabled: () => true, setCropFillEnabled: vi.fn(),
          cropFillSource: () => "background" as const, setCropFillSource: vi.fn(),
          cropFillCustomColor: () => "#ffffff", setCropFillCustomColor: vi.fn(),
        cropAspect: () => ({ w: 16, h: 9 }), setCropAspect: vi.fn(),
        cropSizeTarget: () => null, setCropSizeTarget: vi.fn(),
        cropSizeUnit: () => "px" as const, setCropSizeUnit: vi.fn(),
        cropRotation: () => 0, setCropRotation: vi.fn(),
        hiddenCropPreview: () => null, setHiddenCropPreview: vi.fn(),
        docWidth: () => 1600, docHeight: () => 1200,
        activeDocumentId: () => "mock-doc-id",
      });

      const container = document.createElement("div");
      document.body.appendChild(container);
      const dispose = render(() => <CropOptionBar />, container);

      clickPill(container, "Free");

      expect(setCropModeSpy).toHaveBeenCalledWith("free");
      // Rect should NOT be changed when going to Free
      expect(setCropRectSpy).not.toHaveBeenCalled();

      dispose();
      container.parentNode?.removeChild(container);
    });

    it("applies Ratio→Free immediately without changing frame geometry (Modern)", () => {
      const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("ratio");
      const [modernFrame, setModernFrame] = createSignal<{ w: number; h: number }>({ w: 600, h: 400 });
      const setCropModeSpy = vi.fn((m) => setCropMode(m));
      const setModernFrameSpy = vi.fn((f) => setModernFrame(f));

      mockUseEditor({
        workspace: {}, setActiveTool: vi.fn(), scheduler: {},
        cropRect: () => null, setCropRect: vi.fn(),
        cropInteractionMode: () => "modern" as const, setCropInteractionMode: vi.fn(),
        cropMode, setCropMode: setCropModeSpy,
        cropGuideMode: () => "none", setCropGuideMode: vi.fn(),
        cropDeletePixels: () => false, setCropDeletePixels: vi.fn(),
          cropFillEnabled: () => true, setCropFillEnabled: vi.fn(),
          cropFillSource: () => "background" as const, setCropFillSource: vi.fn(),
          cropFillCustomColor: () => "#ffffff", setCropFillCustomColor: vi.fn(),
        cropAspect: () => ({ w: 16, h: 9 }), setCropAspect: vi.fn(),
        cropSizeTarget: () => null, setCropSizeTarget: vi.fn(),
        cropSizeUnit: () => "px" as const, setCropSizeUnit: vi.fn(),
        cropRotation: () => 0, setCropRotation: vi.fn(),
        hiddenCropPreview: () => null, setHiddenCropPreview: vi.fn(),
        docWidth: () => 1600, docHeight: () => 1200,
        viewportWidth: () => 800, viewportHeight: () => 600,
        zoom: () => 1,
        pan: () => ({ x: 0, y: 0 }),
        modernCropFrame: modernFrame, setModernCropFrame: setModernFrameSpy,
        modernCropImageTransform: () => ({ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 }),
        setModernCropImageTransform: vi.fn(),
        resetModernCrop: vi.fn(),
        commitCropState: vi.fn(),
        activeDocumentId: () => "mock-doc-id",
      });

      const container = document.createElement("div");
      document.body.appendChild(container);
      const dispose = render(() => <CropOptionBar />, container);

      clickPill(container, "Free");

      expect(setCropModeSpy).toHaveBeenCalledWith("free");
      // Free mode clamps frame to max bounds (600x400 already fits, so unchanged)
      expect(setModernFrameSpy).toHaveBeenCalled();
      const frame = setModernFrameSpy.mock.lastCall?.[0];
      expect(frame.w).toBe(600);
      expect(frame.h).toBe(400);

      dispose();
      container.parentNode?.removeChild(container);
    });

    it("applies Ratio→Size immediately in Classic mode", () => {
      const [cropRect, setCropRect] = createSignal<{ x: number; y: number; w: number; h: number } | null>({
        x: 0, y: 0, w: 1600, h: 900,
      });
      const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>(null);
      const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("ratio");
      const setCropRectSpy = vi.fn((r) => setCropRect(r));
      const setCropSizeTargetSpy = vi.fn((t) => setCropSizeTarget(t));
      const setCropModeSpy = vi.fn((m) => setCropMode(m));

      mockUseEditor({
        workspace: {}, setActiveTool: vi.fn(), scheduler: {},
        cropRect, setCropRect: setCropRectSpy,
        cropInteractionMode: () => "classic" as const, setCropInteractionMode: vi.fn(),
        cropMode, setCropMode: setCropModeSpy,
        cropGuideMode: () => "none", setCropGuideMode: vi.fn(),
        cropDeletePixels: () => false, setCropDeletePixels: vi.fn(),
          cropFillEnabled: () => true, setCropFillEnabled: vi.fn(),
          cropFillSource: () => "background" as const, setCropFillSource: vi.fn(),
          cropFillCustomColor: () => "#ffffff", setCropFillCustomColor: vi.fn(),
        cropAspect: () => ({ w: 16, h: 9 }), setCropAspect: vi.fn(),
        cropSizeTarget, setCropSizeTarget: setCropSizeTargetSpy,
        cropSizeUnit: () => "px" as const, setCropSizeUnit: vi.fn(),
        cropRotation: () => 0, setCropRotation: vi.fn(),
        hiddenCropPreview: () => null, setHiddenCropPreview: vi.fn(),
        docWidth: () => 1600, docHeight: () => 1200,
        activeDocumentId: () => "mock-doc-id",
      });

      const container = document.createElement("div");
      document.body.appendChild(container);
      const dispose = render(() => <CropOptionBar />, container);

      clickPill(container, "Size");

      expect(setCropModeSpy).toHaveBeenCalledWith("size");
      expect(setCropSizeTargetSpy).toHaveBeenCalledWith({ w: 800, h: 600 });
      expect(setCropRectSpy).toHaveBeenCalled();
      const rect = setCropRectSpy.mock.lastCall?.[0];
      // 1600x1200 canvas, size target 800x600 (aspect 1.33) = canvas aspect → full canvas
      expect(rect.w).toBe(1600);
      expect(rect.h).toBe(1200);

      dispose();
      container.parentNode?.removeChild(container);
    });

    it("applies Size→Free immediately in Classic mode without changing rect", () => {
      const [cropRect, setCropRect] = createSignal<{ x: number; y: number; w: number; h: number } | null>({
        x: 100, y: 100, w: 800, h: 600,
      });
      const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("size");
      const setCropRectSpy = vi.fn((r) => setCropRect(r));
      const setCropModeSpy = vi.fn((m) => setCropMode(m));

      mockUseEditor({
        workspace: {}, setActiveTool: vi.fn(), scheduler: {},
        cropRect, setCropRect: setCropRectSpy,
        cropInteractionMode: () => "classic" as const, setCropInteractionMode: vi.fn(),
        cropMode, setCropMode: setCropModeSpy,
        cropGuideMode: () => "none", setCropGuideMode: vi.fn(),
        cropDeletePixels: () => false, setCropDeletePixels: vi.fn(),
          cropFillEnabled: () => true, setCropFillEnabled: vi.fn(),
          cropFillSource: () => "background" as const, setCropFillSource: vi.fn(),
          cropFillCustomColor: () => "#ffffff", setCropFillCustomColor: vi.fn(),
        cropAspect: () => null, setCropAspect: vi.fn(),
        cropSizeTarget: () => ({ w: 800, h: 600 }), setCropSizeTarget: vi.fn(),
        cropSizeUnit: () => "px" as const, setCropSizeUnit: vi.fn(),
        cropRotation: () => 0, setCropRotation: vi.fn(),
        hiddenCropPreview: () => null, setHiddenCropPreview: vi.fn(),
        docWidth: () => 1600, docHeight: () => 1200,
        activeDocumentId: () => "mock-doc-id",
      });

      const container = document.createElement("div");
      document.body.appendChild(container);
      const dispose = render(() => <CropOptionBar />, container);

      clickPill(container, "Free");

      expect(setCropModeSpy).toHaveBeenCalledWith("free");
      expect(setCropRectSpy).not.toHaveBeenCalled();

      dispose();
      container.parentNode?.removeChild(container);
    });
  });

  it("calls setCropRect with centered auto-fit coordinates on custom aspect submit", async () => {
    const [cropRect, setCropRect] = createSignal<{ x: number; y: number; w: number; h: number } | null>({
      x: 10,
      y: 10,
      w: 100,
      h: 100,
    });
    const [cropAspect, setCropAspect] = createSignal<{ w: number; h: number } | null>({ w: 17, h: 13 });
    const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("ratio");

    const setCropRectSpy = vi.fn((rect) => setCropRect(rect));

    const mockValue = {
      workspace: {},
      setActiveTool: vi.fn(),
      scheduler: {},
      cropRect,
      setCropRect: setCropRectSpy,
      cropInteractionMode: () => "classic" as const,
      setCropInteractionMode: vi.fn(),
      cropMode,
      setCropMode,
      cropGuideMode: () => "none",
      setCropGuideMode: vi.fn(),
      cropDeletePixels: () => false,
      setCropDeletePixels: vi.fn(),
      cropFillEnabled: () => true,
      setCropFillEnabled: vi.fn(),
      cropFillSource: () => "background" as const,
      setCropFillSource: vi.fn(),
      cropFillCustomColor: () => "#ffffff",
      setCropFillCustomColor: vi.fn(),
      cropAspect,
      setCropAspect,
      cropSizeTarget: () => ({ w: 800, h: 600 }),
      setCropSizeTarget: vi.fn(),
      cropSizeUnit: () => "px" as const,
      setCropSizeUnit: vi.fn(),
      cropRotation: () => 0,
      setCropRotation: vi.fn(),
      hiddenCropPreview: () => null,
      setHiddenCropPreview: vi.fn(),
      docWidth: () => 1600,
      docHeight: () => 1200,
      activeDocumentId: () => "mock-doc-id",
      commitCropState: vi.fn(),
    };

    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(() => <CropOptionBar />, container);

    // Click "+" to expand custom W:H fields
    clickPill(container, "+");

    const inputs = Array.from(container.querySelectorAll("input")).filter((i) => (i as HTMLInputElement).type === "text");
    expect(inputs.length).toBeGreaterThan(0);

    const wInput = inputs[0]; // Custom W EditableNumField input
    wInput.focus();
    wInput.value = "3";
    wInput.dispatchEvent(new InputEvent("input", { bubbles: true }));
    wInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

    // Aspect changed from 17:13 to 3:13
    // Canvas is 1600x1200 (ratio 4/3 = 1.33)
    // New aspect ratio = 3/13 = 0.2307
    // Since 1600/1200 (1.33) > 3/13 (0.23), crop box will fit height: height=1200, width=1200 * (3/13) = 276.92
    // Centered: x = (1600 - 276.92) / 2 = 661.54, y = 0
    expect(setCropRectSpy).toHaveBeenCalled();
    const lastCall = setCropRectSpy.mock.lastCall?.[0];
    expect(lastCall).not.toBeNull();
    expect(lastCall.w).toBeCloseTo(276.92, 1);
    expect(lastCall.h).toBeCloseTo(1200, 1);
    expect(lastCall.x).toBeCloseTo(661.54, 1);
    expect(lastCall.y).toBe(0);

    dispose();
    container.parentNode?.removeChild(container);
  });

  it("calls setCropRect with centered auto-fit coordinates on custom size submit", async () => {
    const [cropRect, setCropRect] = createSignal<{ x: number; y: number; w: number; h: number } | null>({
      x: 10,
      y: 10,
      w: 100,
      h: 100,
    });
    const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>({ w: 800, h: 600 });
    const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("size");

    const setCropRectSpy = vi.fn((rect) => setCropRect(rect));

    const mockValue = {
      workspace: {},
      setActiveTool: vi.fn(),
      scheduler: {},
      cropRect,
      setCropRect: setCropRectSpy,
      cropInteractionMode: () => "classic" as const,
      setCropInteractionMode: vi.fn(),
      cropMode,
      setCropMode,
      cropGuideMode: () => "none",
      setCropGuideMode: vi.fn(),
      cropDeletePixels: () => false,
      setCropDeletePixels: vi.fn(),
      cropFillEnabled: () => true,
      setCropFillEnabled: vi.fn(),
      cropFillSource: () => "background" as const,
      setCropFillSource: vi.fn(),
      cropFillCustomColor: () => "#ffffff",
      setCropFillCustomColor: vi.fn(),
      cropAspect: () => null,
      setCropAspect: vi.fn(),
      cropSizeTarget,
      setCropSizeTarget,
      cropSizeUnit: () => "px" as const,
      setCropSizeUnit: vi.fn(),
      cropRotation: () => 0,
      setCropRotation: vi.fn(),
      hiddenCropPreview: () => null,
      setHiddenCropPreview: vi.fn(),
      docWidth: () => 1600,
      docHeight: () => 1200,
      activeDocumentId: () => "mock-doc-id",
    };

    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(() => <CropOptionBar />, container);

    const inputs = Array.from(container.querySelectorAll("input")).filter((i) => (i as HTMLInputElement).type === "text");
    expect(inputs.length).toBeGreaterThan(0);

    const wInput = inputs[0]; // W target size input
    wInput.focus();
    wInput.value = "400";
    wInput.dispatchEvent(new InputEvent("input", { bubbles: true }));
    wInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

    // Target changed to 400x600 (ratio 4/6 = 0.66)
    // Canvas is 1600x1200 (ratio 1.33)
    // crop box will fit height: height=1200, width=1200 * (4/6) = 800
    // Centered: x = (1600 - 800) / 2 = 400, y = 0
    expect(setCropRectSpy).toHaveBeenCalled();
    const lastCall = setCropRectSpy.mock.lastCall?.[0];
    expect(lastCall).not.toBeNull();
    expect(lastCall.w).toBe(800);
    expect(lastCall.h).toBe(1200);
    expect(lastCall.x).toBe(400);
    expect(lastCall.y).toBe(0);

    dispose();
    container.parentNode?.removeChild(container);
  });

  it("calls setCropRect with centered auto-fit coordinates on swap button click", async () => {
    const [cropRect, setCropRect] = createSignal<{ x: number; y: number; w: number; h: number } | null>({
      x: 10,
      y: 10,
      w: 100,
      h: 100,
    });
    const [cropAspect, setCropAspect] = createSignal<{ w: number; h: number } | null>({ w: 16, h: 9 });
    const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("ratio");

    const setCropRectSpy = vi.fn((rect) => setCropRect(rect));

    const mockValue = {
      workspace: {},
      setActiveTool: vi.fn(),
      scheduler: {},
      cropRect,
      setCropRect: setCropRectSpy,
      cropInteractionMode: () => "classic" as const,
      setCropInteractionMode: vi.fn(),
      cropMode,
      setCropMode,
      cropGuideMode: () => "none",
      setCropGuideMode: vi.fn(),
      cropDeletePixels: () => false,
      setCropDeletePixels: vi.fn(),
      cropFillEnabled: () => true,
      setCropFillEnabled: vi.fn(),
      cropFillSource: () => "background" as const,
      setCropFillSource: vi.fn(),
      cropFillCustomColor: () => "#ffffff",
      setCropFillCustomColor: vi.fn(),
      cropAspect,
      setCropAspect,
      cropSizeTarget: () => null,
      setCropSizeTarget: vi.fn(),
      cropSizeUnit: () => "px" as const,
      setCropSizeUnit: vi.fn(),
      cropRotation: () => 0,
      setCropRotation: vi.fn(),
      hiddenCropPreview: () => null,
      setHiddenCropPreview: vi.fn(),
      docWidth: () => 1600,
      docHeight: () => 1200,
      activeDocumentId: () => "mock-doc-id",
    };

    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(() => <CropOptionBar />, container);

    const swapBtn = container.querySelector('button[aria-label="Swap width and height"]') as HTMLButtonElement | null;
    expect(swapBtn).not.toBeNull();

    swapBtn!.click();

    // Swapped aspect: 9:16 = 0.5625
    // Canvas is 1600x1200 (ratio 1.33)
    // crop box will fit height: height=1200, width=1200 * (9/16) = 675
    // Centered: x = (1600 - 675) / 2 = 462.5, y = 0
    expect(setCropRectSpy).toHaveBeenCalled();
    const lastCall = setCropRectSpy.mock.lastCall?.[0];
    expect(lastCall).not.toBeNull();
    expect(lastCall.w).toBe(675);
    expect(lastCall.h).toBe(1200);
    expect(lastCall.x).toBe(462.5);
    expect(lastCall.y).toBe(0);

    dispose();
    container.parentNode?.removeChild(container);
  });

  it("cancel discards hidden crop preview and stays in Crop tool", () => {
    const [cropRect, setCropRect] = createSignal<{ x: number; y: number; w: number; h: number } | null>({
      x: 10,
      y: 20,
      w: 100,
      h: 80,
    });
    const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("free");
    const [hiddenCropPreview, setHiddenCropPreview] = createSignal<any>({
      rect: { x: 30, y: 40, w: 120, h: 90 },
      rotation: -8,
    });
    const setCropRectSpy = vi.fn((rect) => setCropRect(rect));
    const setHiddenCropPreviewSpy = vi.fn((preview) => setHiddenCropPreview(preview));
    const setActiveToolSpy = vi.fn();

    const mockValue = {
      workspace: {},
      setActiveTool: setActiveToolSpy,
      scheduler: {},
      cropRect,
      setCropRect: setCropRectSpy,
      cropInteractionMode: () => "classic" as const,
      setCropInteractionMode: vi.fn(),
      cropMode,
      setCropMode,
      cropGuideMode: () => "none",
      setCropGuideMode: vi.fn(),
      cropDeletePixels: () => false,
      setCropDeletePixels: vi.fn(),
      cropFillEnabled: () => true,
      setCropFillEnabled: vi.fn(),
      cropFillSource: () => "background" as const,
      setCropFillSource: vi.fn(),
      cropFillCustomColor: () => "#ffffff",
      setCropFillCustomColor: vi.fn(),
      cropAspect: () => null,
      setCropAspect: vi.fn(),
      cropSizeTarget: () => null,
      setCropSizeTarget: vi.fn(),
      cropSizeUnit: () => "px" as const,
      setCropSizeUnit: vi.fn(),
      cropRotation: () => 12,
      setCropRotation: vi.fn(),
      hiddenCropPreview,
      setHiddenCropPreview: setHiddenCropPreviewSpy,
      docWidth: () => 1600,
      docHeight: () => 1200,
      activeDocumentId: () => "mock-doc-id",
    };

    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(() => <CropOptionBar />, container);
    const cancelButton = Array.from(container.querySelectorAll("button")).find((button) =>
      button.textContent?.trim() === "Cancel"
    );

    expect(cancelButton).toBeDefined();
    cancelButton!.dispatchEvent(new MouseEvent("click", { bubbles: true }));

    expect(setCropRectSpy).toHaveBeenCalledWith(null);
    expect(setHiddenCropPreviewSpy).toHaveBeenCalledWith(null);
    expect(setActiveToolSpy).not.toHaveBeenCalled();

    dispose();
    container.parentNode?.removeChild(container);
  });

  describe("Size mode fills canvas at target aspect ratio", () => {
    const fitMaxW = 800;
    const fitMaxH = 600;

    it("small target (100×100) produces frame filling canvas at 1:1 aspect", () => {
      runWithContainer((container, done) => {
        const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("free");
        const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>(null);
        const [modernFrame, setModernFrame] = createSignal<{ w: number; h: number }>({ w: 400, h: 300 });
        const setCropModeSpy = vi.fn((m) => setCropMode(m));
        const setCropSizeTargetSpy = vi.fn((t) => setCropSizeTarget(t));
        const setModernFrameSpy = vi.fn((f) => setModernFrame(f));

        renderOptionBar({
          ...modernContextBase,
          cropMode, setCropMode: setCropModeSpy,
          cropAspect: () => null, setCropAspect: vi.fn(),
          cropSizeTarget: () => ({ w: 100, h: 100 }), setCropSizeTarget: setCropSizeTargetSpy,
          modernCropFrame: modernFrame, setModernCropFrame: setModernFrameSpy,
        }, container);

        clickPill(container, "Size");

        const frame = setModernFrameSpy.mock.lastCall?.[0];
        expect(frame.w).toBeLessThanOrEqual(fitMaxW);
        expect(frame.h).toBeLessThanOrEqual(fitMaxH);
        // 100:100 = 1:1 aspect, height fills canvas (600), width = 600
        expect(frame.w / frame.h).toBeCloseTo(1, 1);
        expect(frame.w).toBeGreaterThan(100); // frame is larger than raw target
        done();
      });
    });

    it("very tall target (1000×2000) produces frame filling canvas at 1:2 aspect", () => {
      runWithContainer((container, done) => {
        const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("free");
        const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>(null);
        const [modernFrame, setModernFrame] = createSignal<{ w: number; h: number }>({ w: 400, h: 300 });
        const setCropModeSpy = vi.fn((m) => setCropMode(m));
        const setCropSizeTargetSpy = vi.fn((t) => setCropSizeTarget(t));
        const setModernFrameSpy = vi.fn((f) => setModernFrame(f));

        renderOptionBar({
          ...modernContextBase,
          cropMode, setCropMode: setCropModeSpy,
          cropAspect: () => null, setCropAspect: vi.fn(),
          cropSizeTarget: () => ({ w: 1000, h: 2000 }), setCropSizeTarget: setCropSizeTargetSpy,
          modernCropFrame: modernFrame, setModernCropFrame: setModernFrameSpy,
        }, container);

        clickPill(container, "Size");

        const frame = setModernFrameSpy.mock.lastCall?.[0];
        expect(frame.w).toBeLessThanOrEqual(fitMaxW);
        expect(frame.h).toBeLessThanOrEqual(fitMaxH);
        // 1000:2000 = 1:2 aspect, height fills canvas (600), width = 300
        expect(frame.w / frame.h).toBeCloseTo(0.5, 1);
        done();
      });
    });

    it("very wide target (2000×1000) produces frame filling canvas at 2:1 aspect", () => {
      runWithContainer((container, done) => {
        const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("free");
        const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>(null);
        const [modernFrame, setModernFrame] = createSignal<{ w: number; h: number }>({ w: 400, h: 300 });
        const setCropModeSpy = vi.fn((m) => setCropMode(m));
        const setCropSizeTargetSpy = vi.fn((t) => setCropSizeTarget(t));
        const setModernFrameSpy = vi.fn((f) => setModernFrame(f));

        renderOptionBar({
          ...modernContextBase,
          cropMode, setCropMode: setCropModeSpy,
          cropAspect: () => null, setCropAspect: vi.fn(),
          cropSizeTarget: () => ({ w: 2000, h: 1000 }), setCropSizeTarget: setCropSizeTargetSpy,
          modernCropFrame: modernFrame, setModernCropFrame: setModernFrameSpy,
        }, container);

        clickPill(container, "Size");

        const frame = setModernFrameSpy.mock.lastCall?.[0];
        expect(frame.w).toBeLessThanOrEqual(fitMaxW);
        expect(frame.h).toBeLessThanOrEqual(fitMaxH);
        // 2000:1000 = 2:1 aspect, width fills canvas (800), height = 400
        expect(frame.w / frame.h).toBeCloseTo(2, 1);
        done();
      });
    });

    it("size W input submit refits frame to new aspect (Modern)", () => {
      runWithContainer((container, done) => {
        const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>({ w: 800, h: 600 });
        const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("size");
        const [modernFrame, setModernFrame] = createSignal<{ w: number; h: number }>({ w: 800, h: 600 });
        const setCropSizeTargetSpy = vi.fn((t) => setCropSizeTarget(t));
        const setModernFrameSpy = vi.fn((f) => setModernFrame(f));

        renderOptionBar({
          ...modernContextBase,
          cropMode, setCropMode: vi.fn(),
          cropAspect: () => null, setCropAspect: vi.fn(),
          cropSizeUnit: () => "px" as const, setCropSizeUnit: vi.fn(),
          cropSizeTarget, setCropSizeTarget: setCropSizeTargetSpy,
          modernCropFrame: modernFrame, setModernCropFrame: setModernFrameSpy,
        }, container);

        const inputs = Array.from(container.querySelectorAll("input")).filter((i) => (i as HTMLInputElement).type === "text");
        const wInput = inputs[0];
        wInput.focus();
        wInput.value = "400";
        wInput.dispatchEvent(new InputEvent("input", { bubbles: true }));
        wInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

        // Target changed to 400x600 (aspect 2:3) → frame should fill canvas at 2:3
        const frame = setModernFrameSpy.mock.lastCall?.[0];
        expect(frame.w).toBeLessThanOrEqual(fitMaxW);
        expect(frame.h).toBeLessThanOrEqual(fitMaxH);
        expect(frame.w / frame.h).toBeCloseTo(400 / 600, 1);
        done();
      });
    });

    it("size H input submit refits frame to new aspect (Modern)", () => {
      runWithContainer((container, done) => {
        const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>({ w: 800, h: 600 });
        const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("size");
        const [modernFrame, setModernFrame] = createSignal<{ w: number; h: number }>({ w: 800, h: 600 });
        const setCropSizeTargetSpy = vi.fn((t) => setCropSizeTarget(t));
        const setModernFrameSpy = vi.fn((f) => setModernFrame(f));

        renderOptionBar({
          ...modernContextBase,
          cropMode, setCropMode: vi.fn(),
          cropAspect: () => null, setCropAspect: vi.fn(),
          cropSizeUnit: () => "px" as const, setCropSizeUnit: vi.fn(),
          cropSizeTarget, setCropSizeTarget: setCropSizeTargetSpy,
          modernCropFrame: modernFrame, setModernCropFrame: setModernFrameSpy,
        }, container);

        const inputs = Array.from(container.querySelectorAll("input")).filter((i) => (i as HTMLInputElement).type === "text");
        // W input is inputs[0], H input is inputs[1]
        const hInput = inputs[1];
        hInput.focus();
        hInput.value = "300";
        hInput.dispatchEvent(new InputEvent("input", { bubbles: true }));
        hInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

        // Target changed to 800x300 (aspect 8:3) → frame should fill canvas at 8:3
        const frame = setModernFrameSpy.mock.lastCall?.[0];
        expect(frame.w).toBeLessThanOrEqual(fitMaxW);
        expect(frame.h).toBeLessThanOrEqual(fitMaxH);
        expect(frame.w / frame.h).toBeCloseTo(800 / 300, 1);
        done();
      });
    });

    it("swap button in Size mode refits frame to swapped aspect (Modern)", () => {
      runWithContainer((container, done) => {
        const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>({ w: 300, h: 600 });
        const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("size");
        const [modernFrame, setModernFrame] = createSignal<{ w: number; h: number }>({ w: 400, h: 800 });
        const setCropSizeTargetSpy = vi.fn((t) => setCropSizeTarget(t));
        const setModernFrameSpy = vi.fn((f) => setModernFrame(f));

        renderOptionBar({
          ...modernContextBase,
          cropMode, setCropMode: vi.fn(),
          cropAspect: () => null, setCropAspect: vi.fn(),
          cropSizeTarget, setCropSizeTarget: setCropSizeTargetSpy,
          cropSizeUnit: () => "px" as const, setCropSizeUnit: vi.fn(),
          modernCropFrame: modernFrame, setModernCropFrame: setModernFrameSpy,
        }, container);

        // Find the swap button (has aria-label "Swap Width/Height")
        const swapBtn = Array.from(container.querySelectorAll("button")).find(
          (b) => b.getAttribute("aria-label") === "Swap width and height"
        );
        expect(swapBtn).toBeDefined();
        swapBtn!.dispatchEvent(new MouseEvent("click", { bubbles: true }));

        // Target swapped to 600x300 (aspect 2:1) → frame should fill canvas at 2:1
        expect(setCropSizeTargetSpy).toHaveBeenCalledWith({ w: 600, h: 300 });
        const frame = setModernFrameSpy.mock.lastCall?.[0];
        expect(frame.w).toBeLessThanOrEqual(fitMaxW);
        expect(frame.h).toBeLessThanOrEqual(fitMaxH);
        expect(frame.w / frame.h).toBeCloseTo(2, 1);
        done();
      });
    });
  });

  describe("Size mode physical unit value stability (no drift)", () => {
    it("3×4 cm stays stable across repeated submits", () => {
      runWithContainer((container, done) => {
        const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>(null);
        const [sizeUnit, setSizeUnit] = createSignal<"px" | "cm" | "mm" | "in">("cm");
        const setCropSizeTargetSpy = vi.fn((t) => setCropSizeTarget(t));

        renderOptionBar({
          ...modernContextBase,
          cropMode: () => "size" as const, setCropMode: vi.fn(),
          cropAspect: () => null, setCropAspect: vi.fn(),
          cropSizeTarget, setCropSizeTarget: setCropSizeTargetSpy,
          cropSizeUnit: sizeUnit, setCropSizeUnit: vi.fn((u) => setSizeUnit(u)),
          modernCropFrame: () => null, setModernCropFrame: vi.fn(),
        }, container);

        const inputs = Array.from(container.querySelectorAll("input")).filter((i) => (i as HTMLInputElement).type === "text");
        const wInput = inputs[0] as HTMLInputElement;
        const hInput = inputs[1] as HTMLInputElement;

        // Enter W=3 cm
        wInput.focus();
        wInput.value = "3";
        wInput.dispatchEvent(new InputEvent("input", { bubbles: true }));
        wInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
        expect(wInput.value).toBe("3");

        // Enter H=4 cm
        hInput.focus();
        hInput.value = "4";
        hInput.dispatchEvent(new InputEvent("input", { bubbles: true }));
        hInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
        expect(hInput.value).toBe("4");

        // Re-submit W=3 again — should not drift to 2.99
        wInput.focus();
        wInput.value = "3";
        wInput.dispatchEvent(new InputEvent("input", { bubbles: true }));
        wInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
        expect(wInput.value).toBe("3");

        // Re-submit H=4 again — should not drift
        hInput.focus();
        hInput.value = "4";
        hInput.dispatchEvent(new InputEvent("input", { bubbles: true }));
        hInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
        expect(hInput.value).toBe("4");

        done();
      });
    });

    it("4×6 in stays stable across repeated submits", () => {
      runWithContainer((container, done) => {
        const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>(null);
        const [sizeUnit, setSizeUnit] = createSignal<"px" | "cm" | "mm" | "in">("in");
        const setCropSizeTargetSpy = vi.fn((t) => setCropSizeTarget(t));

        renderOptionBar({
          ...modernContextBase,
          cropMode: () => "size" as const, setCropMode: vi.fn(),
          cropAspect: () => null, setCropAspect: vi.fn(),
          cropSizeTarget, setCropSizeTarget: setCropSizeTargetSpy,
          cropSizeUnit: sizeUnit, setCropSizeUnit: vi.fn((u) => setSizeUnit(u)),
          modernCropFrame: () => null, setModernCropFrame: vi.fn(),
        }, container);

        const inputs = Array.from(container.querySelectorAll("input")).filter((i) => (i as HTMLInputElement).type === "text");
        const wInput = inputs[0] as HTMLInputElement;
        const hInput = inputs[1] as HTMLInputElement;

        wInput.focus();
        wInput.value = "4";
        wInput.dispatchEvent(new InputEvent("input", { bubbles: true }));
        wInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
        expect(wInput.value).toBe("4");

        hInput.focus();
        hInput.value = "6";
        hInput.dispatchEvent(new InputEvent("input", { bubbles: true }));
        hInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
        expect(hInput.value).toBe("6");

        // Re-submit should not drift
        wInput.focus();
        wInput.value = "4";
        wInput.dispatchEvent(new InputEvent("input", { bubbles: true }));
        wInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
        expect(wInput.value).toBe("4");

        done();
      });
    });

    it("unit switch (cm→px→cm) preserves displayed value", () => {
      runWithContainer((container, done) => {
        const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>(null);
        const [sizeUnit, setSizeUnit] = createSignal<"px" | "cm" | "mm" | "in">("cm");
        const setCropSizeTargetSpy = vi.fn((t) => setCropSizeTarget(t));

        renderOptionBar({
          ...modernContextBase,
          cropMode: () => "size" as const, setCropMode: vi.fn(),
          cropAspect: () => null, setCropAspect: vi.fn(),
          cropSizeTarget, setCropSizeTarget: setCropSizeTargetSpy,
          cropSizeUnit: sizeUnit, setCropSizeUnit: vi.fn((u) => setSizeUnit(u)),
          modernCropFrame: () => null, setModernCropFrame: vi.fn(),
        }, container);

        const inputs = Array.from(container.querySelectorAll("input")).filter((i) => (i as HTMLInputElement).type === "text");
        const wInput = inputs[0] as HTMLInputElement;
        const hInput = inputs[1] as HTMLInputElement;

        // Enter 3×4 cm
        wInput.focus();
        wInput.value = "3";
        wInput.dispatchEvent(new InputEvent("input", { bubbles: true }));
        wInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

        hInput.focus();
        hInput.value = "4";
        hInput.dispatchEvent(new InputEvent("input", { bubbles: true }));
        hInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

        // Switch to px
        setSizeUnit("px");
        // Switch back to cm
        setSizeUnit("cm");

        // After round-trip, display should still show 3 and 4
        expect(wInput.value).toBe("3");
        expect(hInput.value).toBe("4");

        done();
      });
    });

    it("mode switch (Size→Free→Size) preserves value", () => {
      runWithContainer((container, done) => {
        const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("size");
        const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>(null);
        const setCropSizeTargetSpy = vi.fn((t) => setCropSizeTarget(t));

        renderOptionBar({
          ...modernContextBase,
          cropMode, setCropMode: vi.fn((m) => setCropMode(m)),
          cropAspect: () => null, setCropAspect: vi.fn(),
          cropSizeTarget, setCropSizeTarget: setCropSizeTargetSpy,
          cropSizeUnit: () => "cm" as const, setCropSizeUnit: vi.fn(),
          modernCropFrame: () => null, setModernCropFrame: vi.fn(),
        }, container);

        // Enter W=3 cm in Size mode
        let inputs = Array.from(container.querySelectorAll("input")).filter((i) => (i as HTMLInputElement).type === "text");
        let wInput = inputs[0] as HTMLInputElement;
        wInput.focus();
        wInput.value = "3";
        wInput.dispatchEvent(new InputEvent("input", { bubbles: true }));
        wInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

        // Switch to Free mode — Size section unmounts
        setCropMode("free");

        // Switch back to Size — Size section remounts
        setCropMode("size");

        // After remount, display should still show 3
        inputs = Array.from(container.querySelectorAll("input")).filter((i) => (i as HTMLInputElement).type === "text");
        wInput = inputs[0] as HTMLInputElement;
        expect(wInput.value).toBe("3");

        done();
      });
    });

    it("repeated re-renders cause no value drift (3×4 cm)", () => {
      runWithContainer((container, done) => {
        const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>(null);
        const [sizeUnit, setSizeUnit] = createSignal<"px" | "cm" | "mm" | "in">("cm");
        const [renderTick, setRenderTick] = createSignal(0);
        const setCropSizeTargetSpy = vi.fn((t) => {
          setCropSizeTarget(t);
          setRenderTick((n) => n + 1);
        });

        renderOptionBar({
          ...modernContextBase,
          cropMode: () => "size" as const, setCropMode: vi.fn(),
          cropAspect: () => null, setCropAspect: vi.fn(),
          cropSizeTarget, setCropSizeTarget: setCropSizeTargetSpy,
          cropSizeUnit: sizeUnit, setCropSizeUnit: vi.fn((u) => setSizeUnit(u)),
          modernCropFrame: () => null, setModernCropFrame: vi.fn(),
        }, container);

        const inputs = Array.from(container.querySelectorAll("input")).filter((i) => (i as HTMLInputElement).type === "text");
        const wInput = inputs[0] as HTMLInputElement;
        const hInput = inputs[1] as HTMLInputElement;

        // 10 rounds of entering 3×4 cm — should never drift
        for (let i = 0; i < 10; i++) {
          wInput.focus();
          wInput.value = "3";
          wInput.dispatchEvent(new InputEvent("input", { bubbles: true }));
          wInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
          expect(wInput.value).toBe(`3`);

          hInput.focus();
          hInput.value = "4";
          hInput.dispatchEvent(new InputEvent("input", { bubbles: true }));
          hInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
          expect(hInput.value).toBe(`4`);
        }

        done();
      });
    });

    it("3×4 cm converts to ~354×472 px at 300 DPI (not 113×151)", () => {
      runWithContainer((container, done) => {
        const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>(null);
        const setCropSizeTargetSpy = vi.fn((t) => setCropSizeTarget(t));

        renderOptionBar({
          ...modernContextBase,
          cropMode: () => "size" as const, setCropMode: vi.fn(),
          cropAspect: () => null, setCropAspect: vi.fn(),
          cropSizeTarget, setCropSizeTarget: setCropSizeTargetSpy,
          cropSizeUnit: () => "cm" as const, setCropSizeUnit: vi.fn(),
          modernCropFrame: () => null, setModernCropFrame: vi.fn(),
        }, container);

        const inputs = Array.from(container.querySelectorAll("input")).filter((i) => (i as HTMLInputElement).type === "text");
        const wInput = inputs[0] as HTMLInputElement;
        const hInput = inputs[1] as HTMLInputElement;

        wInput.focus();
        wInput.value = "3";
        wInput.dispatchEvent(new InputEvent("input", { bubbles: true }));
        wInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

        // 3 cm at 300 DPI = 354 px (not 113 at 96 DPI)
        const lastW = setCropSizeTargetSpy.mock.lastCall?.[0];
        expect(lastW.w).toBeCloseTo(354.33, 1);

        hInput.focus();
        hInput.value = "4";
        hInput.dispatchEvent(new InputEvent("input", { bubbles: true }));
        hInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

        // 4 cm at 300 DPI = 472 px
        const lastH = setCropSizeTargetSpy.mock.lastCall?.[0];
        expect(lastH.h).toBeCloseTo(472.44, 1);

        done();
      });
    });

    it("4×6 in converts to ~1200×1800 px at 300 DPI (not 384×576)", () => {
      runWithContainer((container, done) => {
        const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>(null);
        const setCropSizeTargetSpy = vi.fn((t) => setCropSizeTarget(t));

        renderOptionBar({
          ...modernContextBase,
          cropMode: () => "size" as const, setCropMode: vi.fn(),
          cropAspect: () => null, setCropAspect: vi.fn(),
          cropSizeTarget, setCropSizeTarget: setCropSizeTargetSpy,
          cropSizeUnit: () => "in" as const, setCropSizeUnit: vi.fn(),
          modernCropFrame: () => null, setModernCropFrame: vi.fn(),
        }, container);

        const inputs = Array.from(container.querySelectorAll("input")).filter((i) => (i as HTMLInputElement).type === "text");
        const wInput = inputs[0] as HTMLInputElement;
        const hInput = inputs[1] as HTMLInputElement;

        wInput.focus();
        wInput.value = "4";
        wInput.dispatchEvent(new InputEvent("input", { bubbles: true }));
        wInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

        // 4 in at 300 DPI = 1200 px (not 384 at 96 DPI)
        const lastW = setCropSizeTargetSpy.mock.lastCall?.[0];
        expect(lastW.w).toBe(1200);

        hInput.focus();
        hInput.value = "6";
        hInput.dispatchEvent(new InputEvent("input", { bubbles: true }));
        hInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

        // 6 in at 300 DPI = 1800 px
        const lastH = setCropSizeTargetSpy.mock.lastCall?.[0];
        expect(lastH.h).toBe(1800);

        done();
      });
    });

    it("applied crop passes rounded pixel values at 300 DPI (not silently downscaled)", () => {
      runWithContainer((container, done) => {
        const [cropSizeTarget, setCropSizeTarget] = createSignal<{ w: number; h: number } | null>(null);
        const setCropSizeTargetSpy = vi.fn((t) => setCropSizeTarget(t));

        renderOptionBar({
          ...modernContextBase,
          cropMode: () => "size" as const, setCropMode: vi.fn(),
          cropAspect: () => null, setCropAspect: vi.fn(),
          cropSizeTarget, setCropSizeTarget: setCropSizeTargetSpy,
          cropSizeUnit: () => "cm" as const, setCropSizeUnit: vi.fn(),
          modernCropFrame: () => null, setModernCropFrame: vi.fn(),
        }, container);

        const inputs = Array.from(container.querySelectorAll("input")).filter((i) => (i as HTMLInputElement).type === "text");
        const wInput = inputs[0] as HTMLInputElement;
        const hInput = inputs[1] as HTMLInputElement;

        wInput.focus();
        wInput.value = "3";
        wInput.dispatchEvent(new InputEvent("input", { bubbles: true }));
        wInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

        // Verify the last setCropSizeTarget call has ~354 px, not 113
        const lastTarget = setCropSizeTargetSpy.mock.lastCall?.[0];
        expect(lastTarget.w).toBeGreaterThan(300); // 300+ DPI, not 96 DPI
        expect(lastTarget.w).toBeCloseTo(354.33, 1);

        done();
      });
    });

    it("locks modern crop frame shape in modern interaction mode", () => {
      runWithContainer((container, done) => {
        const [cropMode, setCropMode] = createSignal<"free" | "ratio" | "size">("free");
        const [cropAspect, setCropAspect] = createSignal<{ w: number; h: number } | null>(null);
        const setCropModeSpy = vi.fn((m) => setCropMode(m));
        const setCropAspectSpy = vi.fn((a) => setCropAspect(a));
        const setModernFrameSpy = vi.fn();

        renderOptionBar({
          ...modernContextBase,
          cropInteractionMode: () => "modern" as const,
          cropMode, setCropMode: setCropModeSpy,
          cropAspect, setCropAspect: setCropAspectSpy,
          modernCropFrame: () => ({ x: 100, y: 100, w: 400, h: 300 }),
          setModernCropFrame: setModernFrameSpy,
        }, container);

        clickPill(container, "Lock Current Shape");

        expect(setCropModeSpy).toHaveBeenCalledWith("ratio");
        expect(setCropAspectSpy).toHaveBeenCalledWith({ w: 400, h: 300 });
        expect(setModernFrameSpy).toHaveBeenCalled();

        done();
      });
    });

  });

  describe("crop tool UI redesign (2026-07-15)", () => {
    it("removes the Modern paradigm toggle (no standalone 'Modern' button)", () => {
      runWithContainer((container, done) => {
        renderOptionBar({ ...modernContextBase }, container);
        const hasModern = Array.from(container.querySelectorAll("button")).some(
          (b) => b.textContent?.trim() === "Modern"
        );
        expect(hasModern).toBe(false);
        done();
      });
    });

    it("straighten slider updates modern image rotation", () => {
      const setModernCropImageTransform = vi.fn();
      const commitModernCropState = vi.fn();
      runWithContainer((container, done) => {
        renderOptionBar({ ...modernContextBase, setModernCropImageTransform, commitModernCropState }, container);
        const slider = container.querySelector('input[type="range"]') as HTMLInputElement | null;
        expect(slider).not.toBeNull();
        slider!.value = "12.5";
        slider!.dispatchEvent(new Event("input", { bubbles: true }));
        expect(setModernCropImageTransform).toHaveBeenCalled();
        done();
      });
    });

    it("straighten slider updates classic rotation when in classic mode", () => {
      const setCropRotation = vi.fn();
      runWithContainer((container, done) => {
        renderOptionBar({ ...modernContextBase, cropInteractionMode: () => "classic" as const, setCropRotation }, container);
        const slider = container.querySelector('input[type="range"]') as HTMLInputElement | null;
        expect(slider).not.toBeNull();
        slider!.value = "-8";
        slider!.dispatchEvent(new Event("input", { bubbles: true }));
        expect(setCropRotation).toHaveBeenCalled();
        done();
      });
    });

    it("inline Classic toggle is reachable and switches interaction mode to classic", () => {
      const setCropInteractionMode = vi.fn();
      runWithContainer((container, done) => {
        renderOptionBar({ ...modernContextBase, setCropInteractionMode }, container);
        const classicLabel = Array.from(container.querySelectorAll("label")).find(
          (l) => l.textContent?.trim() === "Classic Crop"
        );
        expect(classicLabel).not.toBeUndefined();
        const input = classicLabel!.querySelector("input[type='checkbox']") as HTMLInputElement;
        input.click();
        expect(setCropInteractionMode).toHaveBeenCalledWith("classic");
        done();
      });
    });

    it("Classic crop toggle in More menu switches interaction mode to classic", () => {
      const setCropInteractionMode = vi.fn();
      runWithContainer((container, done) => {
        renderOptionBar({ ...modernContextBase, setCropInteractionMode }, container);
        const moreTrigger = container.querySelector(".relative.hidden")?.querySelector("button") as HTMLButtonElement | null;
        expect(moreTrigger).not.toBeNull();
        moreTrigger!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
        const moreContainer = container.querySelector(".relative.hidden")!;
        const classicLabel = Array.from(moreContainer.querySelectorAll("label")).find(
          (l) => l.textContent?.trim() === "Classic Crop"
        );
        expect(classicLabel).not.toBeUndefined();
        const input = classicLabel!.querySelector("input[type='checkbox']") as HTMLInputElement;
        input.click();
        expect(setCropInteractionMode).toHaveBeenCalledWith("classic");
        done();
      });
    });
  });
});
