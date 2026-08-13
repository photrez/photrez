// SPDX-License-Identifier: AGPL-3.0-or-later
// Crop tool option bar. Section JSX extracted to CropOptionBarSections.tsx
// (report #20 phase 3); this file keeps state, handlers, and composition.

import {
  Show,
  createSignal,
  createEffect,
  onMount,
  on,
  untrack,
} from "solid-js";
import { EditableNumField } from "./primitives";
import { useEditor } from "./shell/EditorContext";
import { CROP_PRESETS } from "@/viewport/cropPresets";
import { toUnit, fromUnit } from "@/viewport/unitConversion";
import { fitCropRectToAspect } from "@/viewport/cropAutoFit";
import {
  getDefaultModernCropFrame,
  getModernCropApplyRotation,
  modernFrameToCropRect,
} from "@/viewport/modernCropGeometry";
import { ToolPill } from "./shell/OptionBarShared";
import { discardCropSession, applyCropPreview } from "./cropToolActions";
import {
  CropRatioPicker,
  CropRatioInputs,
  CropSizeInputs,
  CropFillControls,
  CropStraightenControl,
  CropRotateButtons,
  CropGuideSelect,
  CropClassicToggle,
  Divider,
  MoreDropdown,
} from "./CropOptionBarSections";

export function CropOptionBar() {
  const {
    workspace,
    renderer,
    setActiveTool,
    scheduler,
    syncViewport,
    bgColor,
    cropRect,
    setCropRect,
    cropInteractionMode,
    setCropInteractionMode,
    cropMode,
    setCropMode,
    cropGuideMode,
    setCropGuideMode,
    cropDeletePixels,
    setCropDeletePixels,
    cropFillEnabled,
    setCropFillEnabled,
    cropFillSource,
    setCropFillSource,
    cropFillCustomColor,
    setCropFillCustomColor,
    cropAspect,
    setCropAspect,
    cropSizeTarget,
    setCropSizeTarget,
    cropSizeUnit,
    setCropSizeUnit,
    cropRotation,
    setCropRotation,
    modernCropFrame,
    setModernCropFrame,
    modernCropImageTransform,
    setModernCropImageTransform,
    resetModernCrop,
    hiddenCropPreview,
    setHiddenCropPreview,
    setSelectedLayerId,
    docWidth,
    docHeight,
    viewportWidth,
    viewportHeight,
    zoom,
    pan,
    commitCropState,
    commitModernCropState,
  } = useEditor();

  const [showCustomRatio, setShowCustomRatio] = createSignal(false);
  const [recentRatios, setRecentRatios] = createSignal<
    { w: number; h: number }[]
  >([]);
  const [showRatiosDropdown, setShowRatiosDropdown] = createSignal(false);

  const handleLockCurrentShape = () => {
    let w = 0;
    let h = 0;
    if (cropInteractionMode() === "modern") {
      const frame = modernCropFrame();
      if (frame && frame.w > 0 && frame.h > 0) {
        w = Math.round(frame.w);
        h = Math.round(frame.h);
      }
    } else {
      const rect = cropRect();
      if (rect && rect.w > 0 && rect.h > 0) {
        w = Math.round(rect.w);
        h = Math.round(rect.h);
      }
    }

    if (w > 0 && h > 0) {
      setCropMode("ratio");
      setCropAspect({ w, h });
      setCropFrameToAspect({ w, h });
      pushRecentRatio(w, h);
    }
  };

  const pushRecentRatio = (w: number, h: number) => {
    setRecentRatios((prev) => {
      const next = prev.filter((r) => !(r.w === w && r.h === h));
      return [{ w, h }, ...next].slice(0, 3);
    });
  };

  const handleSwap = () => {
    if (cropMode() === "ratio" && cropAspect()) {
      const nextAspect = { w: cropAspect()!.h, h: cropAspect()!.w };
      setCropAspect(nextAspect);
      setCropFrameToAspect(nextAspect);
      const cw = customWVal(),
        ch = customHVal();
      setCustomWVal(ch);
      setCustomHVal(cw);
    } else if (cropMode() === "size" && cropSizeTarget()) {
      const nextTarget = { w: cropSizeTarget()!.h, h: cropSizeTarget()!.w };
      setCropSizeTarget(nextTarget);
      const sw = sizeWVal(),
        sh = sizeHVal();
      setSizeWVal(sh);
      setSizeHVal(sw);
      if (cropInteractionMode() === "modern") {
        setModernFrameToAspect({ w: nextTarget.w, h: nextTarget.h });
      } else if (cropRect()) {
        setCropRect(
          fitCropRectToAspect(
            nextTarget,
            docWidth(),
            docHeight(),
            cropRotation(),
          ),
        );
      }
    } else {
      if (cropInteractionMode() === "modern" && modernCropFrame()) {
        const fitted = fitFrameToMaxBounds(
          modernCropFrame()!.h,
          modernCropFrame()!.w,
        );
        setModernCropFrame(fitted);
      } else {
        const rect = cropRect();
        if (rect) {
          const cx = rect.x + rect.w / 2;
          const cy = rect.y + rect.h / 2;
          const nw = rect.h;
          const nh = rect.w;
          setCropRect({
            x: cx - nw / 2,
            y: cy - nh / 2,
            w: nw,
            h: nh,
          });
        }
      }
    }
  };

  const [customWVal, setCustomWVal] = createSignal(16);
  const [customHVal, setCustomHVal] = createSignal(9);

  // Local display values for Size mode — preserve user-entered physical values
  // without round-trip through pixel conversion (avoids drift like 3 cm → 2.99 cm)
  const [sizeWVal, setSizeWVal] = createSignal(800);
  const [sizeHVal, setSizeHVal] = createSignal(600);

  // Initialize from pixel state on mount; re-sync only when unit changes
  onMount(() => {
    const t = cropSizeTarget();
    const unit = cropSizeUnit();
    setSizeWVal(toUnit(t?.w ?? 800, unit));
    setSizeHVal(toUnit(t?.h ?? 600, unit));
  });

  createEffect(
    on(
      () => cropSizeUnit(),
      (unit) => {
        const t = cropSizeTarget();
        setSizeWVal(toUnit(t?.w ?? 800, unit));
        setSizeHVal(toUnit(t?.h ?? 600, unit));
      },
    ),
  );

  createEffect(() => {
    const aspect = cropAspect();
    if (aspect) {
      untrack(() => {
        setCustomWVal(aspect.w);
        setCustomHVal(aspect.h);
      });
    }
  });

  const isCustomActive = () => {
    if (cropMode() !== "ratio") return false;
    const aspect = cropAspect();
    if (!aspect) return false;
    return !CROP_PRESETS.some(
      (p) => p.aspect.w === aspect.w && p.aspect.h === aspect.h,
    );
  };

  const isActivePill = (preset: { w: number; h: number }) => {
    if (cropMode() !== "ratio") return false;
    const a = cropAspect();
    return a?.w === preset.w && a?.h === preset.h;
  };

  const handlePillClick = (preset: { w: number; h: number }) => {
    setCropMode("ratio");
    setCropAspect({ w: preset.w, h: preset.h });
    setCropFrameToAspect(preset);
  };

  const handleFreeClick = () => {
    setCropMode("free");
    if (cropInteractionMode() === "modern" && modernCropFrame()) {
      setModernCropFrame(
        fitFrameToMaxBounds(modernCropFrame()!.w, modernCropFrame()!.h),
      );
    }
  };

  const handleSizeModeClick = () => {
    setCropMode("size");
    const target = cropSizeTarget() ?? { w: 800, h: 600 };
    setCropSizeTarget(target);
    if (cropInteractionMode() === "modern") {
      setModernFrameToAspect({ w: target.w, h: target.h });
    } else if (cropRect()) {
      setCropRect(
        fitCropRectToAspect(target, docWidth(), docHeight(), cropRotation()),
      );
    }
  };

  const guideModeLabel = () => {
    const g = cropGuideMode();
    return g === "none"
      ? "None"
      : g === "thirds"
        ? "Thirds"
        : g === "grid"
          ? "Grid"
          : g === "diagonal"
            ? "Diagonal"
            : g === "golden"
              ? "Golden"
              : "None";
  };

  const currentRatioLabel = () => {
    if (cropMode() === "free") return "Free";
    if (cropMode() === "size") return "Size";
    const aspect = cropAspect();
    if (!aspect) return "Free";
    const preset = CROP_PRESETS.find(
      (p) => p.aspect.w === aspect.w && p.aspect.h === aspect.h,
    );
    if (preset) return preset.label;
    return `${aspect.w}:${aspect.h}`;
  };

  const unitLabel = () => cropSizeUnit();

  const resolvedCropFillColor = () =>
    cropFillSource() === "background"
      ? typeof bgColor === "function"
        ? bgColor()
        : "#ffffff"
      : cropFillCustomColor();

  const maxModernFrame = () => ({
    w: Math.min(viewportWidth(), docWidth() * zoom()),
    h: Math.min(viewportHeight(), docHeight() * zoom()),
  });

  const fitFrameToMaxBounds = (w: number, h: number) => {
    const max = maxModernFrame();
    let finalW = w;
    let finalH = h;
    if (w > max.w || h > max.h) {
      const scale = Math.min(max.w / w, max.h / h);
      finalW = w * scale;
      finalH = h * scale;
    }
    return {
      x: (viewportWidth() - finalW) / 2,
      y: (viewportHeight() - finalH) / 2,
      w: finalW,
      h: finalH,
    };
  };

  const setModernFrameToAspect = (aspect: { w: number; h: number }) => {
    const fitted = getDefaultModernCropFrame({
      viewportWidth: viewportWidth(),
      viewportHeight: viewportHeight(),
      docWidth: docWidth(),
      docHeight: docHeight(),
      zoom: zoom(),
      aspect,
      panX: pan().x,
      panY: pan().y,
    });
    setModernCropFrame(fitted);
  };

  const setCropFrameToAspect = (aspect: { w: number; h: number }) => {
    if (cropInteractionMode() === "modern") {
      setModernFrameToAspect(aspect);
    } else if (cropRect()) {
      setCropRect(
        fitCropRectToAspect(aspect, docWidth(), docHeight(), cropRotation()),
      );
    }
  };

  const applyCurrentCrop = () => {
    const modernFrame = modernCropFrame();
    const modernTransform = modernCropImageTransform();
    applyCropPreview({
      workspace,
      renderer,
      viewport: { width: viewportWidth(), height: viewportHeight() },
      cropRect:
        cropInteractionMode() === "modern" && modernFrame
          ? modernFrameToCropRect({
              frame: modernFrame,
              viewport: {
                width: viewportWidth(),
                height: viewportHeight(),
                panX: pan().x,
                panY: pan().y,
                zoom: zoom(),
              },
              transform: modernTransform,
            })
          : cropRect(),
      cropMode: cropMode(),
      cropSizeTarget: cropSizeTarget(),
      cropDeletePixels: cropDeletePixels(),
      cropFillColor: cropFillEnabled() ? resolvedCropFillColor() : null,
      cropRotation:
        cropInteractionMode() === "modern"
          ? getModernCropApplyRotation(modernTransform.rotation)
          : cropRotation(),
      scheduler,
      setCropRect,
      setCropRotation,
      setHiddenCropPreview,
      setActiveTool,
      setSelectedLayerId,
      recenterViewport: () => {
        const engine = workspace.getActiveEngine();
        if (!engine) return;
        engine.fitToScreen(viewportWidth(), viewportHeight());
        syncViewport();
      },
    });
    if (cropInteractionMode() === "modern") resetModernCrop();
  };

  // ─── Rotation helpers (shared by straighten slider + rotate buttons) ───

  const currentRotation = () =>
    cropInteractionMode() === "modern"
      ? modernCropImageTransform().rotation
      : cropRotation();

  const commitRotation = (v: number) => {
    if (cropInteractionMode() === "modern") {
      const frame = modernCropFrame();
      if (frame) commitModernCropState();
      setModernCropImageTransform((prev) => ({ ...prev, rotation: v }));
    } else {
      const r = cropRect();
      if (r) commitCropState(r, cropRotation());
      setCropRotation(v);
    }
  };

  const rotateBy = (delta: number) => {
    if (cropInteractionMode() === "modern") {
      const frame = modernCropFrame();
      if (frame) commitModernCropState();
      setModernCropImageTransform((prev) => ({
        ...prev,
        rotation: prev.rotation + delta,
      }));
    } else {
      const rect = cropRect();
      if (rect) {
        commitCropState(rect, cropRotation());
        const cx = rect.x + rect.w / 2;
        const cy = rect.y + rect.h / 2;
        setCropRect({
          x: cx - rect.h / 2,
          y: cy - rect.w / 2,
          w: rect.h,
          h: rect.w,
        });
      }
      setCropRotation(cropRotation() + delta);
    }
  };

  return (
    <>
      <ToolPill icon="crop" label="Crop" />

      <Divider />

      {/* Aspect Ratio Dropdown */}
      <CropRatioPicker
        ratioLabel={currentRatioLabel}
        open={showRatiosDropdown}
        setOpen={setShowRatiosDropdown}
        cropMode={cropMode}
        recents={recentRatios}
        onLockShape={() => {
          handleLockCurrentShape();
          setShowRatiosDropdown(false);
        }}
        onFree={() => {
          handleFreeClick();
          setShowRatiosDropdown(false);
        }}
        onCustom={() => {
          setCropMode("ratio");
          setShowRatiosDropdown(false);
          const cur = cropAspect() ?? { w: 16, h: 9 };
          setCropAspect(cur);
          setCropFrameToAspect(cur);
        }}
        onSize={() => {
          handleSizeModeClick();
          setShowRatiosDropdown(false);
        }}
        onRecent={(r) => {
          setCropMode("ratio");
          setShowCustomRatio(false);
          setCropAspect({ w: r.w, h: r.h });
          setCropFrameToAspect({ w: r.w, h: r.h });
          setShowRatiosDropdown(false);
        }}
        isCustomActive={isCustomActive}
        isActivePill={isActivePill}
        onPillClick={handlePillClick}
      />

      {/* W/H inputs beside the Ratio dropdown (ratio + size modes) */}
      <Show when={cropMode() === "ratio"}>
        <CropRatioInputs
          w={customWVal}
          h={customHVal}
          onWSubmit={(v) => {
            setCustomWVal(v);
            const aspect = { w: v, h: customHVal() };
            setCropAspect(aspect);
            pushRecentRatio(aspect.w, aspect.h);
            setCropFrameToAspect(aspect);
          }}
          onHSubmit={(v) => {
            setCustomHVal(v);
            const aspect = { w: customWVal(), h: v };
            setCropAspect(aspect);
            pushRecentRatio(aspect.w, aspect.h);
            setCropFrameToAspect(aspect);
          }}
          onSwap={handleSwap}
        />
      </Show>

      <Show when={cropMode() === "size"}>
        <CropSizeInputs
          w={sizeWVal}
          h={sizeHVal}
          unit={unitLabel}
          onWSubmit={(v) => {
            setSizeWVal(v);
            const valPx = fromUnit(v, cropSizeUnit());
            const nextTarget = { w: valPx, h: cropSizeTarget()?.h ?? 600 };
            setCropSizeTarget(nextTarget);
            if (cropInteractionMode() === "modern") {
              setModernFrameToAspect({ w: nextTarget.w, h: nextTarget.h });
            } else if (cropRect()) {
              setCropRect(
                fitCropRectToAspect(
                  nextTarget,
                  docWidth(),
                  docHeight(),
                  cropRotation(),
                ),
              );
            }
          }}
          onHSubmit={(v) => {
            setSizeHVal(v);
            const valPx = fromUnit(v, cropSizeUnit());
            const nextTarget = { w: cropSizeTarget()?.w ?? 800, h: valPx };
            setCropSizeTarget(nextTarget);
            if (cropInteractionMode() === "modern") {
              setModernFrameToAspect({ w: nextTarget.w, h: nextTarget.h });
            } else if (cropRect()) {
              setCropRect(
                fitCropRectToAspect(
                  nextTarget,
                  docWidth(),
                  docHeight(),
                  cropRotation(),
                ),
              );
            }
          }}
          onSwap={handleSwap}
          onUnitChange={setCropSizeUnit}
        />
      </Show>

      <Divider />

      {/* Delete pixels + Fill BG toggle + color */}
      <CropFillControls
        deletePixels={cropDeletePixels}
        onDeletePixelsChange={setCropDeletePixels}
        fillEnabled={cropFillEnabled}
        onFillEnabledChange={setCropFillEnabled}
        fillSource={cropFillSource}
        fillColor={resolvedCropFillColor}
        onPickColor={(v) => {
          setCropFillSource("custom");
          setCropFillCustomColor(v);
        }}
        onUseBackground={() => setCropFillSource("background")}
      />

      <Divider />

      {/* Advanced controls — inline when wide, collapse into More when narrow */}
      <div class="hidden @min-[880px]:flex items-center gap-1.5 shrink-0">
        {/* Straighten slider */}
        <CropStraightenControl
          rotation={currentRotation}
          onCommitRotation={commitRotation}
          onResetRotation={() => commitRotation(0)}
          layout="inline"
        />

        {/* Rotation Buttons */}
        <CropRotateButtons onRotate={rotateBy} layout="inline" />

        <Divider />

        {/* Composition Guide Mode */}
        <CropGuideSelect
          guideMode={cropGuideMode}
          onGuideModeChange={setCropGuideMode}
          label={guideModeLabel}
          layout="inline"
        />

        <Divider />

        {/* Classic crop toggle (checkbox) */}
        <CropClassicToggle
          checked={() => cropInteractionMode() === "classic"}
          onChange={(v: boolean) => setCropInteractionMode(v ? "classic" : "modern")}
          layout="inline"
        />
      </div>

      <MoreDropdown>
        {/* Straighten slider (overflow) */}
        <CropStraightenControl
          rotation={currentRotation}
          onCommitRotation={commitRotation}
          onResetRotation={() => commitRotation(0)}
          layout="menu"
        />

        {/* Angle Field */}
        <div class="flex flex-col gap-1.5">
          <span class="text-[10px] font-bold text-editor-text-dim uppercase tracking-wider">
            Angle
          </span>
          <EditableNumField
            value={currentRotation()}
            suffix="°"
            onSubmit={commitRotation}
            class="w-full"
          />
        </div>

        {/* Rotation Buttons */}
        <CropRotateButtons onRotate={rotateBy} layout="menu" />

        {/* Composition Guide Mode */}
        <CropGuideSelect
          guideMode={cropGuideMode}
          onGuideModeChange={setCropGuideMode}
          label={guideModeLabel}
          layout="menu"
        />

        <div class="h-px bg-editor-divider my-1.5" />

        {/* Classic crop toggle (checkbox) */}
        <CropClassicToggle
          checked={() => cropInteractionMode() === "classic"}
          onChange={(v: boolean) => setCropInteractionMode(v ? "classic" : "modern")}
          layout="menu"
        />
      </MoreDropdown>

      <Divider />

      <button
        onClick={() => {
          if (cropInteractionMode() === "modern") resetModernCrop();
          discardCropSession({
            cropRect: () => cropRect(),
            cropRotation: () => cropRotation(),
            hiddenCropPreview,
            setCropRect,
            setCropRotation,
            setHiddenCropPreview,
          });
        }}
        class="h-6 px-2.5 rounded-[4px] border border-editor-border bg-editor-surface-2 text-editor-text-dim text-[11px] font-semibold hover:text-editor-text"
      >
        Cancel
      </button>

      <button
        onClick={() => {
          applyCurrentCrop();
        }}
        class="h-6 px-2.5 rounded-[4px] border border-editor-accent/50 bg-editor-accent/15 text-editor-text text-[11px] font-semibold"
      >
        Apply
      </button>
    </>
  );
}
