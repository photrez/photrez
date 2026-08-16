import { Show, For, createSignal, createMemo, onMount } from "solid-js";
import { clsx } from "clsx";
import { Icon } from "./icons";
import { EditableNumField, NumField, PropRow, Slider } from "./primitives";
import { SelectDropdown } from "./shell/OptionBarShared";
import { Tooltip } from "./Tooltip";
import { useEditor } from "./shell/EditorContext";
import { useDialog } from "./dialogs/DialogProvider";
import { SectionHeader } from "./layers/SectionHeader";
import { CanvasProperties } from "./canvas/CanvasProperties";
import { LayerThumb } from "./layers/LayerThumb";
import { normalizeRotation, getLayerAabb } from "@/viewport/transformGeometry";
import { getAvailableFonts, getInstantFonts, type FontFamily } from "@/lib/fontEnumeration";
import type { TextData, TextStrokeAlign } from "@/engine/textTypes";
import { useLayerActions } from "./layers/useLayerActions";
import type { LayerNode, Transform2D } from "@/engine/types";
import { useI18n } from "@/i18n/I18nProvider";

const FONT_WEIGHT_PRESETS: { value: number; label: string }[] = [
  { value: 100, label: "Thin" },
  { value: 200, label: "Extra Light" },
  { value: 300, label: "Light" },
  { value: 400, label: "Regular" },
  { value: 500, label: "Medium" },
  { value: 600, label: "Semibold" },
  { value: 700, label: "Bold" },
  { value: 800, label: "Extra Bold" },
  { value: 900, label: "Black" },
];

export function PropertiesPanel() {
  const { t } = useI18n();
  const { workspace, layers, selectedLayerId, selectedLayerIds, scheduler, activeDocumentId, docWidth, docHeight, constrainRatio, setConstrainRatio, textEditSession, setColorPickerOpen, setColorPickerTarget } = useEditor();
  const dialogs = useDialog();
  const layerActions = useLayerActions();
  const [opacityEditLayerId, setOpacityEditLayerId] = createSignal<string | null>(null);
  const [fontPickerOpen, setFontPickerOpen] = createSignal(false);
  const [fontSearch, setFontSearch] = createSignal("");
  const [fonts, setFonts] = createSignal<FontFamily[]>(getInstantFonts());

  const multiSelectionGroupAabb = createMemo(() => {
    const ids = typeof selectedLayerIds === "function" ? selectedLayerIds() : [];
    if (ids.length <= 1) return null;
    const all = typeof layers === "function" ? layers() : [];
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    let count = 0;
    for (const id of ids) {
      const l = all.find((item) => item.id === id);
      if (!l || !l.visible) continue;
      const aabb = getLayerAabb(l.transform, l.width, l.height);
      if (aabb.x < minX) minX = aabb.x;
      if (aabb.y < minY) minY = aabb.y;
      if (aabb.x + aabb.width > maxX) maxX = aabb.x + aabb.width;
      if (aabb.y + aabb.height > maxY) maxY = aabb.y + aabb.height;
      count++;
    }
    if (count <= 1 || minX === Infinity) return null;
    return {
      x: Math.round(minX * 100) / 100,
      y: Math.round(minY * 100) / 100,
      width: Math.round((maxX - minX) * 100) / 100,
      height: Math.round((maxY - minY) * 100) / 100,
    };
  });

  const handleAlign = (type: "left" | "center-h" | "right" | "top" | "center-v" | "bottom") => {
    const engine = workspace.getActiveEngine();
    const multiIds = typeof selectedLayerIds === "function" ? selectedLayerIds() : [];
    const targetIds = multiIds.length > 1 ? multiIds : (selectedLayerId() ? [selectedLayerId()!] : []);
    if (!engine || targetIds.length === 0) return;

    const layersToAlign = targetIds
      .map((id) => ({ id, layer: engine.getLayer(id) }))
      .filter((item): item is { id: string; layer: LayerNode } => Boolean(item.layer) && !item.layer!.locked && !item.layer!.lockPosition && !item.layer!.isBackground);

    if (layersToAlign.length === 0) return;

    const docW = docWidth();
    const docH = docHeight();
    const history = workspace.getActiveHistory();
    const preSnapshot = engine.snapshot();
    let anyChanged = false;

    for (const { id: targetId, layer } of layersToAlign) {
      const next = { ...layer.transform };
      const layerW = Math.round(layer.width * layer.transform.scaleX);
      const layerH = Math.round(layer.height * layer.transform.scaleY);

      switch (type) {
        case "left":
          next.x = 0;
          break;
        case "center-h":
          next.x = Math.round((docW - layerW) / 2);
          break;
        case "right":
          next.x = docW - layerW;
          break;
        case "top":
          next.y = 0;
          break;
        case "center-v":
          next.y = Math.round((docH - layerH) / 2);
          break;
        case "bottom":
          next.y = docH - layerH;
          break;
      }

      if (next.x !== layer.transform.x || next.y !== layer.transform.y) {
        anyChanged = true;
        engine.transformLayer(targetId, next);
      }
    }

    if (anyChanged && history) {
      history.commit(preSnapshot, `Align ${type}`);
      scheduler.requestRender();
      workspace.notifyVisualChange();
    }
  };

  const handleDistribute = (axis: "h" | "v") => {
    const engine = workspace.getActiveEngine();
    const multiIds = typeof selectedLayerIds === "function" ? selectedLayerIds() : [];
    if (!engine || multiIds.length < 3) return;

    const layersToDistribute = multiIds
      .map((id) => ({ id, layer: engine.getLayer(id) }))
      .filter((item): item is { id: string; layer: LayerNode } => Boolean(item.layer) && !item.layer!.locked && !item.layer!.lockPosition && !item.layer!.isBackground)
      .map((item) => {
        const aabb = getLayerAabb(item.layer.transform, item.layer.width, item.layer.height);
        return {
          id: item.id,
          layer: item.layer,
          aabb,
        };
      });

    if (layersToDistribute.length < 3) return;

    const history = workspace.getActiveHistory();
    const preSnapshot = engine.snapshot();

    if (axis === "h") {
      layersToDistribute.sort((a, b) => a.aabb.x - b.aabb.x);
      const first = layersToDistribute[0];
      const last = layersToDistribute[layersToDistribute.length - 1];
      const totalSpan = (last.aabb.x + last.aabb.width) - first.aabb.x;
      const totalLayersWidth = layersToDistribute.reduce((sum, item) => sum + item.aabb.width, 0);
      const freeSpace = totalSpan - totalLayersWidth;
      const gap = freeSpace / (layersToDistribute.length - 1);

      let currentX = first.aabb.x;
      for (const item of layersToDistribute) {
        const dx = Math.round(currentX - item.aabb.x);
        if (dx !== 0) {
          engine.transformLayer(item.id, {
            ...item.layer.transform,
            x: item.layer.transform.x + dx,
          });
        }
        currentX += item.aabb.width + gap;
      }
      history?.commit(preSnapshot, "Distribute Horizontally");
    } else {
      layersToDistribute.sort((a, b) => a.aabb.y - b.aabb.y);
      const first = layersToDistribute[0];
      const last = layersToDistribute[layersToDistribute.length - 1];
      const totalSpan = (last.aabb.y + last.aabb.height) - first.aabb.y;
      const totalLayersHeight = layersToDistribute.reduce((sum, item) => sum + item.aabb.height, 0);
      const freeSpace = totalSpan - totalLayersHeight;
      const gap = freeSpace / (layersToDistribute.length - 1);

      let currentY = first.aabb.y;
      for (const item of layersToDistribute) {
        const dy = Math.round(currentY - item.aabb.y);
        if (dy !== 0) {
          engine.transformLayer(item.id, {
            ...item.layer.transform,
            y: item.layer.transform.y + dy,
          });
        }
        currentY += item.aabb.height + gap;
      }
      history?.commit(preSnapshot, "Distribute Vertically");
    }

    scheduler.requestRender();
    workspace.notifyVisualChange();
  };

  const loadFonts = () => {
    void getAvailableFonts().then((f) => setFonts(f));
  };

  onMount(() => {
    loadFonts();
  });

  const filteredFonts = () => {
    const q = fontSearch().toLowerCase().trim();
    if (!q) return fonts();
    return fonts().filter((f) => f.family.toLowerCase().includes(q));
  };

  const displayedFonts = createMemo(() => {
    const list = filteredFonts();
    return fontSearch().trim() ? list : list.slice(0, 100);
  });

  const activeLayer = () => {
    const id = selectedLayerId();
    if (!id) return null;
    return layers().find(l => l.id === id) || null;
  };

  // Stable memo — use inside <Show> render prop to avoid Solid's stale-getter error
  // that occurs when a render-prop getter is accessed after the Show has begun unmounting.
  // This happens when a canvas interaction (e.g. pasteboard click → setSelectedLayerId(null))
  // triggers reactive cleanup while a child EditableNumField is still reading props.value.
  const safeLayer = createMemo(() => activeLayer());

  const safeText = createMemo(() => {
    const l = safeLayer();
    return l && l.type === "text" && l.textData ? (l as LayerNode & { type: "text"; textData: TextData }) : null;
  });

  const commitTextDataEdit = (patch: Partial<TextData>, label: string) => {
    const engine = workspace.getActiveEngine();
    const id = selectedLayerId();
    if (!engine || !id) return;
    const layer = engine.getLayer(id);
    if (!layer || layer.locked || layer.type !== "text" || !layer.textData) return;

    const next = { ...layer.textData, ...patch };
    const session = typeof textEditSession === "function" ? textEditSession() : null;
    if (session && session.layerId === layer.id) {
      engine.updateTextData(layer.id, next);
      scheduler.requestRender();
      workspace.notifyVisualChange();
      return;
    }

    const history = workspace.getActiveHistory();
    history?.commit(engine.snapshot(), label);
    engine.updateTextData(layer.id, next);
    scheduler.requestRender();
    workspace.notifyVisualChange();
  };

  const handlePickTextColor = async (currentColor: string) => {
    setColorPickerOpen(true);
    setColorPickerTarget("foreground");
    const chosen = await dialogs.colorPicker({
      title: "Text Color",
      initialColor: currentColor,
      target: "foreground",
      onChange: (c) => commitTextDataEdit({ color: c }, "Change Text Color"),
    });
    if (chosen) {
      commitTextDataEdit({ color: chosen }, "Change Text Color");
    }
    setColorPickerOpen(false);
  };

  const handlePickTextStrokeColor = async (currentColor: string, strokeObj: any) => {
    setColorPickerOpen(true);
    setColorPickerTarget("foreground");
    const chosen = await dialogs.colorPicker({
      title: "Text Stroke Color",
      initialColor: currentColor,
      target: "foreground",
      onChange: (c) => commitTextDataEdit({ stroke: { ...strokeObj, color: c } }, "Change Stroke Color"),
    });
    if (chosen) {
      commitTextDataEdit({ stroke: { ...strokeObj, color: chosen } }, "Change Stroke Color");
    }
    setColorPickerOpen(false);
  };

  const handleOpacityChange = (val: number) => {
    const engine = workspace.getActiveEngine();
    const id = selectedLayerId();
    if (engine && id) {
      const layer = engine.getLayer(id);
      if (!layer || layer.locked) return;
      if (Math.abs(layer.opacity - val / 100) < 0.0001) return;
      if (opacityEditLayerId() !== id) {
        workspace.getActiveHistory()?.commit(engine.snapshot(), "Adjust Opacity");
        setOpacityEditLayerId(id);
      }
      engine.setLayerOpacity(id, val / 100);
      scheduler.requestRender();
      workspace.notifyVisualChange();
    }
  };

  const finishOpacityEdit = () => {
    setOpacityEditLayerId(null);
  };

  const commitTransform = (patch: Partial<Transform2D>, label: string) => {
    const engine = workspace.getActiveEngine();
    const id = selectedLayerId();
    if (!engine || !id) return false;
    const layer = engine.getLayer(id);
    if (!layer || layer.locked) return false;

    const next = { ...layer.transform, ...patch };
    if (
      next.x === layer.transform.x &&
      next.y === layer.transform.y &&
      next.scaleX === layer.transform.scaleX &&
      next.scaleY === layer.transform.scaleY &&
      next.rotation === layer.transform.rotation &&
      next.flipH === layer.transform.flipH &&
      next.flipV === layer.transform.flipV
    ) {
      return false;
    }

    const history = workspace.getActiveHistory();
    history?.commit(engine.snapshot(), label);
    engine.transformLayer(id, next);
    scheduler.requestRender();
    workspace.notifyVisualChange();
    return true;
  };

  const handlePositionField = (axis: "x" | "y") => (val: number) => {
    const layer = activeLayer();
    if (!layer || layer.lockPosition) return;
    commitTransform({ [axis]: val }, "Move Layer");
  };

  const handleSizeField = (axis: "w" | "h") => (val: number) => {
    const layer = activeLayer();
    if (!layer || val <= 0) return;
    const nextScale = axis === "w" ? val / layer.width : val / layer.height;
    const patch: Partial<Transform2D> = axis === "w" ? { scaleX: nextScale } : { scaleY: nextScale };
    if (constrainRatio()) {
      const ratioScale = Math.sign(
        axis === "w" ? (layer.transform.scaleY || 1) : (layer.transform.scaleX || 1)
      ) * Math.abs(nextScale);
      if (axis === "w") patch.scaleY = ratioScale;
      else patch.scaleX = ratioScale;
    }
    commitTransform(patch, "Resize Layer");
  };

  const handleRotationField = (val: number) => {
    const layer = activeLayer();
    if (!layer || layer.lockRotation) return;
    commitTransform({ rotation: val }, "Rotate Layer");
  };

  const handleFlip = (axis: "h" | "v") => {
    const layer = activeLayer();
    if (!layer || layer.locked) return;
    const patch = axis === "h"
      ? { flipH: !layer.transform.flipH }
      : { flipV: !layer.transform.flipV };
    commitTransform(patch, axis === "h" ? "Flip Horizontal" : "Flip Vertical");
  };

  const handleResetTransform = () => {
    const layer = activeLayer();
    if (!layer || layer.locked) return;
    commitTransform(
      { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      "Reset Transform",
    );
  };

  const handleCenterHorizontal = () => {
    const layer = activeLayer();
    if (!layer || layer.locked || layer.lockPosition) return;
    const effW = layer.width * Math.abs(layer.transform.scaleX);
    commitTransform({ x: (docWidth() - effW) / 2 }, "Center Horizontal");
  };

  const handleCenterVertical = () => {
    const layer = activeLayer();
    if (!layer || layer.locked || layer.lockPosition) return;
    const effH = layer.height * Math.abs(layer.transform.scaleY);
    commitTransform({ y: (docHeight() - effH) / 2 }, "Center Vertical");
  };

  const handleFitToCanvas = () => {
    const layer = activeLayer();
    if (!layer || layer.locked || layer.width <= 0 || layer.height <= 0) return;
    const dw = docWidth();
    const dh = docHeight();
    const ratio = Math.min(dw / layer.width, dh / layer.height);
    const signX = Math.sign(layer.transform.scaleX) || 1;
    const signY = Math.sign(layer.transform.scaleY) || 1;
    const effW = layer.width * ratio;
    const effH = layer.height * ratio;
    commitTransform(
      { x: (dw - effW) / 2, y: (dh - effH) / 2, scaleX: signX * ratio, scaleY: signY * ratio },
      "Fit to Canvas",
    );
  };

  const handleRotate90 = (dir: "cw" | "ccw") => {
    const layer = activeLayer();
    if (!layer || layer.locked || layer.lockRotation) return;
    const next = normalizeRotation(layer.transform.rotation + (dir === "cw" ? 90 : -90));
    commitTransform({ rotation: next }, dir === "cw" ? "Rotate 90° CW" : "Rotate 90° CCW");
  };

  const transformStatusText = () => {
    const layer = activeLayer();
    if (!layer) return null;
    if (layer.locked) return t("properties.layerLockedTransform", "Layer is locked. Unlock it in Layers to edit transform values.");
    if (layer.lockPosition && layer.lockRotation) return t("properties.posRotLocked", "Position and rotation are locked for this layer.");
    if (layer.lockPosition) return t("properties.posLocked", "Position fields are locked for this layer.");
    if (layer.lockRotation) return t("properties.rotLocked", "Rotation is locked for this layer.");
    return null;
  };

  return (
    <section class="flex flex-1 shrink-0 flex-col overflow-hidden bg-editor-panel">
      <div class="flex-1 overflow-y-auto">
        <Show
          when={activeDocumentId()}
          fallback={
            <div class="flex h-full flex-col items-center justify-center gap-3 text-center px-6">
              <Icon name="sliders" class="size-6 text-editor-text-dim opacity-50" strokeWidth={1.5} />
              <div class="space-y-1">
                <p class="text-[13px] font-medium text-editor-text">{t("properties.noImageOpen", "No image open")}</p>
                <p class="text-[12px] text-editor-text-dim leading-snug">{t("properties.noImageOpenDesc", "Open or create an image to view and edit properties.")}</p>
              </div>
            </div>
          }
        >
          <Show
            when={typeof selectedLayerIds === "function" && selectedLayerIds().length > 1}
            fallback={
              <Show
                when={safeLayer()}
                fallback={<CanvasProperties />}
              >
                <>
                  <div class="border-b border-editor-divider px-3 py-2.5">
                    <SectionHeader
                      icon="layers"
                      iconClass="text-editor-text-dim"
                      label={t("properties.selectedLayer", "Selected Layer")}
                    />
                    <div class="mt-2 flex items-center gap-2.5 rounded-[4px] border border-editor-divider bg-editor-field p-2">
                      <LayerThumb layer={safeLayer()!} isActive={true} />
                      <div class="min-w-0 flex-1">
                        <p class="truncate text-[11.5px] font-medium text-editor-text leading-tight" title={safeLayer()!.name}>
                          {safeLayer()!.name}
                        </p>
                        <p class="truncate text-[10.5px] text-editor-text-dim leading-snug mt-0.5">
                          {safeLayer()!.type === "raster" ? "Image layer" : `${safeLayer()!.type.charAt(0).toUpperCase()}${safeLayer()!.type.slice(1)} layer`} · {safeLayer()!.width} × {safeLayer()!.height} px
                        </p>
                      </div>
                    </div>
                  </div>

                {/* Typography Section for Text Layers */}
                <Show when={safeText()}>
                  {(textLayer) => (
                    <div class="border-b border-editor-divider px-3 py-2.5" data-typography-section>
                      <SectionHeader
                        icon="type"
                        iconClass="text-editor-text-dim"
                        label={t("properties.typography", "Typography")}
                      />

                      <div class="mt-2 flex flex-col gap-2">
                        {/* Font Family Dropdown */}
                        <PropRow label={t("properties.font", "Font")}>
                          <div class="relative flex-1">
                            <button
                              type="button"
                              data-font-picker-trigger-inspector
                              aria-label="Font family"
                              disabled={safeLayer()!.locked}
                              onClick={() => {
                                setFontPickerOpen((v) => !v);
                                setFontSearch("");
                                loadFonts();
                              }}
                              class="flex h-[24px] w-full items-center justify-between gap-1.5 rounded-[3px] border border-editor-field-border bg-editor-field px-1.5 text-[11px] text-editor-text select-none disabled:opacity-40"
                            >
                              <span class="truncate" style={{ "font-family": `"${textLayer().textData.fontFamily}", sans-serif` }}>
                                {textLayer().textData.fontFamily}
                              </span>
                              <Icon name="chevron-down" class="size-3 shrink-0 text-editor-text-dim" />
                            </button>
                            <Show when={fontPickerOpen()}>
                              <div class="absolute left-0 top-full z-50 mt-1 w-full overflow-hidden rounded-[6px] border border-[#363B44] bg-[#1B1D22] shadow-2xl">
                                <input
                                  type="search"
                                  placeholder={t("tools.options.searchFonts", "Search fonts...")}
                                  aria-label={t("tools.options.searchFonts", "Search fonts in inspector")}
                                  value={fontSearch()}
                                  onInput={(e) => setFontSearch(e.currentTarget.value)}
                                  class="w-full border-b border-[#2D323C] bg-transparent px-2.5 py-1.5 text-[11px] text-white outline-none placeholder:text-[#A1A1AA]/60"
                                />
                                <div class="max-h-56 overflow-y-auto py-1" role="listbox">
                                  <For each={displayedFonts()}>
                                    {(f) => {
                                      const isSelected = () => textLayer().textData.fontFamily === f.family;
                                      return (
                                        <button
                                          type="button"
                                          role="option"
                                          aria-selected={isSelected()}
                                          onClick={() => {
                                            setFontPickerOpen(false);
                                            commitTextDataEdit({ fontFamily: f.family }, "Change Font Family");
                                          }}
                                          class={clsx(
                                            "flex w-full items-center justify-between px-2.5 py-1.5 text-left text-[11px] font-medium transition-colors select-none",
                                            isSelected()
                                              ? "bg-editor-accent/20 text-white font-semibold"
                                              : "text-[#D1D5DB] hover:bg-white/10 hover:text-white"
                                          )}
                                        >
                                          <span style={{ "font-family": `"${f.family}", sans-serif` }}>{f.family}</span>
                                          <Show when={isSelected()}>
                                            <Icon name="check" class="size-3 text-editor-accent shrink-0" strokeWidth={2.5} />
                                          </Show>
                                        </button>
                                      );
                                    }}
                                  </For>
                                </div>
                              </div>
                              <div class="fixed inset-0 z-40" onClick={() => setFontPickerOpen(false)} />
                            </Show>
                          </div>
                        </PropRow>

                        {/* Size & Weight */}
                        <PropRow label={t("properties.sizeAndWeight", "Size & Weight")}>
                          <div class="flex flex-1 items-center gap-1 min-w-0">
                            <EditableNumField
                              value={textLayer().textData.fontSize}
                              suffix="px"
                              min={1}
                              max={2000}
                              onSubmit={(v) => commitTextDataEdit({ fontSize: Math.max(1, Math.min(2000, Math.round(v))) }, "Change Font Size")}
                              disabled={safeLayer()!.locked}
                              class="w-20 shrink-0"
                            />
                            <SelectDropdown
                              value={String(textLayer().textData.fontWeight)}
                              options={FONT_WEIGHT_PRESETS.map((p) => ({ value: String(p.value), label: p.label }))}
                              onChange={(v) => commitTextDataEdit({ fontWeight: Number(v) }, "Change Font Weight")}
                              disabled={safeLayer()!.locked}
                              class="flex-1 min-w-0"
                            />
                          </div>
                        </PropRow>

                        {/* Style & Align */}
                        <PropRow label={t("properties.styleAndAlign", "Style & Align")}>
                          <button
                            type="button"
                            aria-label="Italic"
                            aria-pressed={textLayer().textData.fontStyle === "italic"}
                            disabled={safeLayer()!.locked}
                            onClick={() => commitTextDataEdit({ fontStyle: textLayer().textData.fontStyle === "italic" ? "normal" : "italic" }, "Toggle Italic")}
                            class={clsx(
                              "flex h-[24px] w-[28px] shrink-0 items-center justify-center rounded-[3px] border text-[11px] font-medium transition-colors disabled:opacity-40",
                              textLayer().textData.fontStyle === "italic"
                                ? "border-editor-accent/80 bg-editor-accent/15 text-editor-text"
                                : "border-editor-field-border bg-editor-field text-editor-text-dim hover:text-editor-text",
                            )}
                          >
                            <span class="italic font-serif">I</span>
                          </button>

                          <div class="flex flex-1 items-center gap-0.5 min-w-0">
                            {(["left", "center", "right"] as const).map((a) => (
                              <button
                                type="button"
                                aria-label={`Align ${a}`}
                                aria-pressed={textLayer().textData.align === a}
                                disabled={safeLayer()!.locked}
                                onClick={() => commitTextDataEdit({ align: a }, `Align Text ${a}`)}
                                class={clsx(
                                  "flex h-[24px] flex-1 items-center justify-center rounded-[3px] border text-[11px] transition-colors disabled:opacity-40",
                                  textLayer().textData.align === a
                                    ? "border-editor-accent/80 bg-editor-accent/15 text-editor-text"
                                    : "border-editor-field-border bg-editor-field text-editor-text-dim hover:text-editor-text",
                                )}
                              >
                                <Icon name={a === "left" ? "align-text-left" : a === "center" ? "align-text-center" : "align-text-right"} class="size-3.5" strokeWidth={1.6} />
                              </button>
                            ))}
                          </div>
                        </PropRow>

                        {/* Box Mode */}
                        <PropRow label={t("properties.boxMode", "Box Mode")}>
                          <div class="flex flex-1 items-center gap-1 min-w-0">
                            <SelectDropdown
                              value={textLayer().textData.boxMode}
                              options={[
                                { value: "point", label: "Auto Width (Point)" },
                                { value: "area", label: "Fixed Box (Area)" },
                              ]}
                              onChange={(v) => {
                                const m = v as "point" | "area";
                                if (m === "point") commitTextDataEdit({ boxMode: "point", boxWidth: 0, boxHeight: 0 }, "Point Text Mode");
                                else {
                                  const curW = textLayer().textData.boxWidth;
                                  commitTextDataEdit({ boxMode: "area", boxWidth: curW > 0 ? curW : 200 }, "Area Text Mode");
                                }
                              }}
                              disabled={safeLayer()!.locked}
                              class="flex-1 min-w-0"
                            />
                            <Show when={textLayer().textData.boxMode === "area"}>
                              <EditableNumField
                                label="W"
                                value={textLayer().textData.boxWidth}
                                suffix="px"
                                min={1}
                                max={10000}
                                onSubmit={(w) => commitTextDataEdit({ boxWidth: Math.max(1, Math.round(w)) }, "Change Box Width")}
                                disabled={safeLayer()!.locked}
                                class="w-20 shrink-0"
                              />
                            </Show>
                          </div>
                        </PropRow>

                        {/* Spacing & Line Height */}
                        <PropRow label={t("properties.spacing", "Spacing")}>
                          <div class="flex flex-1 items-center gap-1 min-w-0">
                            <EditableNumField
                              label="Line H"
                              value={Math.round(textLayer().textData.lineHeight * 10) / 10}
                              step={0.1}
                              min={0.5}
                              max={5.0}
                              onSubmit={(v) => commitTextDataEdit({ lineHeight: Math.max(0.5, Math.min(5.0, v)) }, "Change Line Height")}
                              disabled={safeLayer()!.locked}
                              class="flex-1 min-w-0"
                            />
                            <EditableNumField
                              label="Letter"
                              value={textLayer().textData.letterSpacing}
                              suffix="px"
                              min={-100}
                              max={500}
                              onSubmit={(v) => commitTextDataEdit({ letterSpacing: Math.max(-100, Math.min(500, Math.round(v))) }, "Change Letter Spacing")}
                              disabled={safeLayer()!.locked}
                              class="flex-1 min-w-0"
                            />
                          </div>
                        </PropRow>

                        {/* Color & Stroke */}
                        <PropRow label={t("properties.colorAndStroke", "Color & Stroke")}>
                          <div class="flex flex-1 items-center gap-1.5">
                            <button
                              type="button"
                              aria-label="Text color"
                              disabled={safeLayer()!.locked}
                              onClick={() => handlePickTextColor(textLayer().textData.color)}
                              class="size-[24px] shrink-0 cursor-pointer rounded-[3px] border border-editor-field-border p-0 disabled:opacity-40"
                              style={{ "background-color": textLayer().textData.color }}
                            />
                            <button
                              type="button"
                              aria-label="Toggle stroke"
                              aria-pressed={(textLayer().textData.stroke?.width ?? 0) > 0}
                              disabled={safeLayer()!.locked}
                              onClick={() => commitTextDataEdit({ stroke: { width: (textLayer().textData.stroke?.width ?? 0) > 0 ? 0 : 4, color: textLayer().textData.stroke?.color ?? "#000000" } }, "Toggle Text Stroke")}
                              class={clsx(
                                "flex h-[24px] flex-1 items-center justify-center gap-1 rounded-[3px] border text-[11px] font-bold transition-colors disabled:opacity-40",
                                (textLayer().textData.stroke?.width ?? 0) > 0
                                  ? "border-editor-accent/80 bg-editor-accent/15 text-editor-text"
                                  : "border-editor-field-border bg-editor-field text-editor-text-dim hover:text-editor-text",
                              )}
                            >
                              {t("properties.stroke", "Stroke")}
                            </button>
                            <Show when={(textLayer().textData.stroke?.width ?? 0) > 0}>
                              <EditableNumField
                                label="W"
                                value={textLayer().textData.stroke.width}
                                suffix="px"
                                min={1}
                                max={100}
                                onSubmit={(w) => commitTextDataEdit({ stroke: { ...textLayer().textData.stroke, width: Math.max(1, Math.min(100, Math.round(w))) } }, "Change Stroke Width")}
                                disabled={safeLayer()!.locked}
                                class="w-16"
                              />
                              <button
                                type="button"
                                aria-label="Stroke color"
                                disabled={safeLayer()!.locked}
                                onClick={() => handlePickTextStrokeColor(textLayer().textData.stroke.color, textLayer().textData.stroke)}
                                class="size-[24px] shrink-0 cursor-pointer rounded-[3px] border border-editor-field-border p-0 disabled:opacity-40"
                                style={{ "background-color": textLayer().textData.stroke.color }}
                              />
                            </Show>
                          </div>
                        </PropRow>

                        {/* Stroke Position Segmented Control (when outline enabled) */}
                        <Show when={(textLayer().textData.stroke?.width ?? 0) > 0}>
                          <PropRow label={t("properties.position", "Position")}>
                            <div class="flex h-[24px] flex-1 rounded-[3px] border border-editor-field-border bg-editor-field p-0.5 select-none">
                              <button
                                type="button"
                                aria-label="Stroke position outside"
                                disabled={safeLayer()!.locked}
                                onClick={() => commitTextDataEdit({ stroke: { ...textLayer().textData.stroke, align: "outside" } }, "Change Stroke Position")}
                                class={clsx(
                                  "flex-1 rounded-[2px] text-[10px] font-semibold transition-colors disabled:opacity-40",
                                  (textLayer().textData.stroke.align ?? "outside") === "outside"
                                    ? "bg-editor-accent text-white shadow-xs"
                                    : "text-editor-text-dim hover:text-editor-text",
                                )}
                              >
                                Outside
                              </button>
                              <button
                                type="button"
                                aria-label="Stroke position center"
                                disabled={safeLayer()!.locked}
                                onClick={() => commitTextDataEdit({ stroke: { ...textLayer().textData.stroke, align: "center" } }, "Change Stroke Position")}
                                class={clsx(
                                  "flex-1 rounded-[2px] text-[10px] font-semibold transition-colors disabled:opacity-40",
                                  textLayer().textData.stroke.align === "center"
                                    ? "bg-editor-accent text-white shadow-xs"
                                    : "text-editor-text-dim hover:text-editor-text",
                                )}
                              >
                                Center
                              </button>
                              <button
                                type="button"
                                aria-label="Stroke position inside"
                                disabled={safeLayer()!.locked}
                                onClick={() => commitTextDataEdit({ stroke: { ...textLayer().textData.stroke, align: "inside" } }, "Change Stroke Position")}
                                class={clsx(
                                  "flex-1 rounded-[2px] text-[10px] font-semibold transition-colors disabled:opacity-40",
                                  textLayer().textData.stroke.align === "inside"
                                    ? "bg-editor-accent text-white shadow-xs"
                                    : "text-editor-text-dim hover:text-editor-text",
                                )}
                              >
                                Inside
                              </button>
                            </div>
                          </PropRow>
                        </Show>
                      </div>
                    </div>
                  )}
                </Show>

                <div class="border-b border-editor-divider px-3 py-2.5">
                  <SectionHeader
                    icon="move"
                    iconClass="text-editor-text-dim"
                    label={t("properties.transform", "Transform")}
                  />

                  <div class="mt-2 flex flex-col gap-2">
                    <Show when={transformStatusText()}>
                      {(message) => <StatusHint>{message()}</StatusHint>}
                    </Show>
                    <PropRow label={t("properties.position", "Position")}>
                      <EditableNumField label="X" value={safeLayer()!.transform.x} suffix="px" onSubmit={handlePositionField("x")} disabled={safeLayer()!.lockPosition || safeLayer()!.locked} class="flex-1" />
                      <EditableNumField label="Y" value={safeLayer()!.transform.y} suffix="px" onSubmit={handlePositionField("y")} disabled={safeLayer()!.lockPosition || safeLayer()!.locked} class="flex-1" />
                    </PropRow>
                    <PropRow label={t("tools.options.size", "Size")}>
                      <EditableNumField label="W" value={safeLayer()!.width * safeLayer()!.transform.scaleX} suffix="px" onSubmit={handleSizeField("w")} disabled={safeLayer()!.locked} class="flex-1" />
                      <EditableNumField label="H" value={safeLayer()!.height * safeLayer()!.transform.scaleY} suffix="px" onSubmit={handleSizeField("h")} disabled={safeLayer()!.locked} class="flex-1" />
                      <button
                        class={`flex size-[26px] shrink-0 items-center justify-center ${constrainRatio() ? "text-editor-accent" : "text-editor-text-dim"}`}
                        aria-label="Constrain proportions"
                        aria-pressed={constrainRatio()}
                        onClick={() => setConstrainRatio(!constrainRatio())}
                      >
                        <Icon name={constrainRatio() ? "link" : "unlink"} class="size-3.5" strokeWidth={1.75} />
                      </button>
                    </PropRow>
                    <PropRow label={t("properties.rotation", "Rotation")}>
                      <EditableNumField label="R" value={safeLayer()!.transform.rotation} suffix="deg" onSubmit={handleRotationField} disabled={safeLayer()!.lockRotation || safeLayer()!.locked} class="flex-1" />
                    </PropRow>
                    <PropRow label={t("properties.scale", "Scale")}>
                      <NumField label="X" value={`${Math.round(safeLayer()!.transform.scaleX * 100)}`} suffix="%" class="flex-1" />
                      <NumField label="Y" value={`${Math.round(safeLayer()!.transform.scaleY * 100)}`} suffix="%" class="flex-1" />
                    </PropRow>
                    <PropRow label={t("tools.options.opacity", "Opacity")}>
                      <div class="flex-grow flex items-center gap-2.5">
                        <div class="relative flex-grow flex items-center h-[24px]">
                          <Slider
                            percent={Math.round(safeLayer()!.opacity * 100)}
                            type="opacity"
                          />
                          <input
                            aria-label="Opacity"
                            type="range"
                            min="0"
                            max="100"
                            value={Math.round(safeLayer()!.opacity * 100)}
                            disabled={safeLayer()!.locked}
                            onInput={(e) => handleOpacityChange(parseInt(e.currentTarget.value))}
                            onPointerUp={finishOpacityEdit}
                            onBlur={finishOpacityEdit}
                            onChange={finishOpacityEdit}
                            class="absolute inset-0 w-full h-[24px] opacity-0 cursor-pointer disabled:pointer-events-none"
                          />
                        </div>
                        <span class="w-[44px] shrink-0 text-right text-[12px] text-editor-text">
                          {Math.round(safeLayer()!.opacity * 100)} %
                        </span>
                      </div>
                    </PropRow>

                    <PropRow label={t("properties.actions", "Actions")}>
                      <button
                        type="button"
                        aria-label="Flip horizontal"
                        disabled={safeLayer()!.locked}
                        onClick={() => handleFlip("h")}
                        class="flex h-[26px] flex-1 items-center justify-center gap-1.5 rounded-[4px] border border-editor-field-border bg-editor-field px-2 text-[11px] text-editor-text transition-colors hover:bg-editor-field-border disabled:pointer-events-none disabled:opacity-40"
                      >
                        <Icon name="flip-h" class="size-3.5" strokeWidth={1.75} />
                        {t("properties.flipH", "Flip H")}
                      </button>
                      <button
                        type="button"
                        aria-label="Flip vertical"
                        disabled={safeLayer()!.locked}
                        onClick={() => handleFlip("v")}
                        class="flex h-[26px] flex-1 items-center justify-center gap-1.5 rounded-[4px] border border-editor-field-border bg-editor-field px-2 text-[11px] text-editor-text transition-colors hover:bg-editor-field-border disabled:pointer-events-none disabled:opacity-40"
                      >
                        <Icon name="flip-v" class="size-3.5" strokeWidth={1.75} />
                        {t("properties.flipV", "Flip V")}
                      </button>
                      <button
                        type="button"
                        aria-label="Reset transform"
                        disabled={safeLayer()!.locked}
                        onClick={handleResetTransform}
                        class="flex h-[26px] flex-1 items-center justify-center gap-1.5 rounded-[4px] border border-editor-field-border bg-editor-field px-2 text-[11px] text-editor-text transition-colors hover:bg-editor-field-border disabled:pointer-events-none disabled:opacity-40"
                      >
                        <Icon name="rotate-ccw" class="size-3.5" strokeWidth={1.75} />
                        {t("common.reset", "Reset")}
                      </button>
                    </PropRow>

                    <PropRow label={t("canvasProps.quickActions", "Quick")}>
                      <Tooltip content={safeLayer()!.lockPosition ? t("properties.posLockedShort", "Position locked for this layer") : t("properties.centerH", "Center horizontally on canvas")}>
                        <button
                        type="button"
                        aria-label={t("properties.centerH", "Center horizontally on canvas")}
                        disabled={safeLayer()!.locked || safeLayer()!.lockPosition}
                        onClick={handleCenterHorizontal}
                        class="flex h-[26px] flex-1 items-center justify-center gap-1 rounded-[4px] border border-editor-field-border bg-editor-field px-2 text-[11px] text-editor-text transition-colors hover:bg-editor-field-border disabled:pointer-events-none disabled:opacity-40"
                        >
                          <Icon name="align-h" class="size-3.5" strokeWidth={1.75} />
                        </button>
                      </Tooltip>
                      <Tooltip content={safeLayer()!.lockPosition ? t("properties.posLockedShort", "Position locked for this layer") : t("properties.centerV", "Center vertically on canvas")}>
                        <button
                        type="button"
                        aria-label={t("properties.centerV", "Center vertically on canvas")}
                        disabled={safeLayer()!.locked || safeLayer()!.lockPosition}
                        onClick={handleCenterVertical}
                        class="flex h-[26px] flex-1 items-center justify-center gap-1 rounded-[4px] border border-editor-field-border bg-editor-field px-2 text-[11px] text-editor-text transition-colors hover:bg-editor-field-border disabled:pointer-events-none disabled:opacity-40"
                        >
                          <Icon name="align-v" class="size-3.5" strokeWidth={1.75} />
                        </button>
                      </Tooltip>
                      <Tooltip content={safeLayer()!.lockPosition ? t("properties.posLockedShort", "Position locked for this layer") : t("properties.fitCanvas", "Fit to canvas (scale + center)")}>
                        <button
                        type="button"
                        aria-label={t("properties.fitCanvas", "Fit to canvas (scale + center)")}
                        disabled={safeLayer()!.locked || safeLayer()!.lockPosition}
                        onClick={handleFitToCanvas}
                        class="flex h-[26px] flex-1 items-center justify-center gap-1 rounded-[4px] border border-editor-field-border bg-editor-field px-2 text-[11px] text-editor-text transition-colors hover:bg-editor-field-border disabled:pointer-events-none disabled:opacity-40"
                        >
                          <Icon name="maximize" class="size-3.5" strokeWidth={1.75} />
                        </button>
                      </Tooltip>
                      <Tooltip content={safeLayer()!.lockRotation ? t("properties.rotLockedShort", "Rotation locked for this layer") : t("properties.rotateCCW", "Rotate 90° counterclockwise")}>
                        <button
                        type="button"
                        aria-label={t("properties.rotateCCW", "Rotate 90° counterclockwise")}
                        disabled={safeLayer()!.locked || safeLayer()!.lockRotation}
                        onClick={() => handleRotate90("ccw")}
                        class="flex h-[26px] flex-1 items-center justify-center gap-1 rounded-[4px] border border-editor-field-border bg-editor-field px-2 text-[11px] text-editor-text transition-colors hover:bg-editor-field-border disabled:pointer-events-none disabled:opacity-40"
                        >
                          <Icon name="rotate-ccw" class="size-3.5" strokeWidth={1.75} />
                        </button>
                      </Tooltip>
                      <Tooltip content={safeLayer()!.lockRotation ? t("properties.rotLockedShort", "Rotation locked for this layer") : t("properties.rotateCW", "Rotate 90° clockwise")}>
                        <button
                        type="button"
                        aria-label={t("properties.rotateCW", "Rotate 90° clockwise")}
                        disabled={safeLayer()!.locked || safeLayer()!.lockRotation}
                        onClick={() => handleRotate90("cw")}
                        class="flex h-[26px] flex-1 items-center justify-center gap-1 rounded-[4px] border border-editor-field-border bg-editor-field px-2 text-[11px] text-editor-text transition-colors hover:bg-editor-field-border disabled:pointer-events-none disabled:opacity-40"
                        >
                          <Icon name="rotate-cw" class="size-3.5" strokeWidth={1.75} />
                        </button>
                      </Tooltip>
                    </PropRow>

                  </div>
                </div>
              </>
            </Show>
          }
        >
          {/* Multiple Layers Selected Inspector */}
          <div class="flex flex-col">
            {/* Header Card */}
            <div class="border-b border-editor-divider px-3 py-2.5">
              <SectionHeader
                icon="layers"
                iconClass="text-editor-text-dim"
                label={t("properties.multipleLayers", "Multiple Layers")}
              />
              <div class="mt-2 flex items-center gap-2.5 rounded-[4px] border border-editor-divider bg-editor-field p-2">
                <div class="flex size-8 shrink-0 items-center justify-center rounded-[3px] bg-editor-panel-bg text-editor-accent border border-editor-divider">
                  <Icon name="layers" class="size-4" />
                </div>
                <div class="min-w-0 flex-1">
                  <p class="truncate text-[11.5px] font-medium text-editor-text leading-tight">
                    {t("properties.layersSelected", { count: selectedLayerIds().length })}
                  </p>
                  <p class="truncate text-[10.5px] text-editor-text-dim leading-snug mt-0.5">
                    {selectedLayerIds().map(id => layers().find(l => l.id === id)?.name).filter(Boolean).join(", ")}
                  </p>
                </div>
              </div>
            </div>

            {/* Combined Transform Stats */}
            <Show when={multiSelectionGroupAabb()}>
              {(group) => (
                <div class="border-b border-editor-divider px-3 py-2.5">
                  <SectionHeader
                    icon="move"
                    iconClass="text-editor-text-dim"
                    label={t("properties.combinedBounds", "Combined Bounds")}
                  />
                  <div class="mt-2 flex flex-col gap-2">
                    <PropRow label={t("properties.position", "Position")}>
                      <div class="flex items-center gap-1.5">
                        <NumField label="X" value={String(Math.round(group().x * 10) / 10)} suffix="px" />
                        <NumField label="Y" value={String(Math.round(group().y * 10) / 10)} suffix="px" />
                      </div>
                    </PropRow>
                    <PropRow label={t("tools.options.size", "Size")}>
                      <div class="flex items-center gap-1.5">
                        <NumField label="W" value={String(Math.round(group().width * 10) / 10)} suffix="px" />
                        <NumField label="H" value={String(Math.round(group().height * 10) / 10)} suffix="px" />
                      </div>
                    </PropRow>
                  </div>
                </div>
              )}
            </Show>

            {/* Align to Canvas */}
            <div class="border-b border-editor-divider px-3 py-2.5">
              <SectionHeader
                icon="grid-3"
                iconClass="text-editor-text-dim"
                label={t("properties.alignToCanvas", "Align to Canvas")}
              />
              <div class="mt-2 grid grid-cols-6 gap-1">
                <Tooltip content={t("tools.options.alignLeft", "Align Left")}>
                  <button
                    type="button"
                    onClick={() => handleAlign("left")}
                    class="flex h-7 items-center justify-center rounded-[3px] border border-editor-divider bg-editor-field text-editor-text-dim hover:bg-editor-hover hover:text-editor-text transition-colors"
                  >
                    <Icon name="align-left" class="size-3.5" />
                  </button>
                </Tooltip>
                <Tooltip content={t("tools.options.alignCenterH", "Align Horizontal Center")}>
                  <button
                    type="button"
                    onClick={() => handleAlign("center-h")}
                    class="flex h-7 items-center justify-center rounded-[3px] border border-editor-divider bg-editor-field text-editor-text-dim hover:bg-editor-hover hover:text-editor-text transition-colors"
                  >
                    <Icon name="align-h" class="size-3.5" />
                  </button>
                </Tooltip>
                <Tooltip content={t("tools.options.alignRight", "Align Right")}>
                  <button
                    type="button"
                    onClick={() => handleAlign("right")}
                    class="flex h-7 items-center justify-center rounded-[3px] border border-editor-divider bg-editor-field text-editor-text-dim hover:bg-editor-hover hover:text-editor-text transition-colors"
                  >
                    <Icon name="align-right" class="size-3.5" />
                  </button>
                </Tooltip>
                <Tooltip content={t("tools.options.alignTop", "Align Top")}>
                  <button
                    type="button"
                    onClick={() => handleAlign("top")}
                    class="flex h-7 items-center justify-center rounded-[3px] border border-editor-divider bg-editor-field text-editor-text-dim hover:bg-editor-hover hover:text-editor-text transition-colors"
                  >
                    <Icon name="align-top" class="size-3.5" />
                  </button>
                </Tooltip>
                <Tooltip content={t("tools.options.alignCenterV", "Align Vertical Center")}>
                  <button
                    type="button"
                    onClick={() => handleAlign("center-v")}
                    class="flex h-7 items-center justify-center rounded-[3px] border border-editor-divider bg-editor-field text-editor-text-dim hover:bg-editor-hover hover:text-editor-text transition-colors"
                  >
                    <Icon name="align-v" class="size-3.5" />
                  </button>
                </Tooltip>
                <Tooltip content={t("tools.options.alignBottom", "Align Bottom")}>
                  <button
                    type="button"
                    onClick={() => handleAlign("bottom")}
                    class="flex h-7 items-center justify-center rounded-[3px] border border-editor-divider bg-editor-field text-editor-text-dim hover:bg-editor-hover hover:text-editor-text transition-colors"
                  >
                    <Icon name="align-bottom" class="size-3.5" />
                  </button>
                </Tooltip>
              </div>
            </div>

            {/* Distribute Spacing (when >= 3 layers selected) */}
            <Show when={typeof selectedLayerIds === "function" && selectedLayerIds().length >= 3}>
              <div class="border-b border-editor-divider px-3 py-2.5">
                <SectionHeader
                  icon="split-h"
                  iconClass="text-editor-text-dim"
                  label={t("properties.distributeSpacing", "Distribute Spacing")}
                />
                <div class="mt-2 grid grid-cols-2 gap-1.5">
                  <button
                    type="button"
                    onClick={() => handleDistribute("h")}
                    class="flex h-7 items-center justify-center gap-1.5 rounded-[3px] border border-editor-divider bg-editor-field text-[11px] font-medium text-editor-text-dim hover:bg-editor-hover hover:text-editor-text transition-colors"
                  >
                    <Icon name="distribute-h" class="size-3.5" />
                    <span>{t("properties.distributeH", "Horizontal")}</span>
                  </button>
                  <button
                    type="button"
                    onClick={() => handleDistribute("v")}
                    class="flex h-7 items-center justify-center gap-1.5 rounded-[3px] border border-editor-divider bg-editor-field text-[11px] font-medium text-editor-text-dim hover:bg-editor-hover hover:text-editor-text transition-colors"
                  >
                    <Icon name="distribute-v" class="size-3.5" />
                    <span>{t("properties.distributeV", "Vertical")}</span>
                  </button>
                </div>
              </div>
            </Show>

            {/* Batch Actions */}
            <div class="border-b border-editor-divider px-3 py-2.5">
              <SectionHeader
                icon="sliders"
                iconClass="text-editor-text-dim"
                label={t("properties.batchActions", "Batch Actions")}
              />
              <div class="mt-2 flex flex-col gap-1.5">
                <button
                  type="button"
                  onClick={layerActions.handleMergeActiveLayerDown}
                  class="flex h-7 w-full items-center justify-center gap-1.5 rounded-[3px] border border-editor-divider bg-editor-field text-[11px] font-medium text-editor-text hover:bg-editor-hover transition-colors"
                >
                  <Icon name="layers" class="size-3.5 text-editor-text-dim" />
                  <span>{t("properties.mergeSelectedLayers", "Merge Selected Layers")}</span>
                </button>
                <div class="grid grid-cols-2 gap-1.5">
                  <button
                    type="button"
                    onClick={layerActions.handleDuplicateActiveLayer}
                    class="flex h-7 items-center justify-center gap-1.5 rounded-[3px] border border-editor-divider bg-editor-field text-[11px] font-medium text-editor-text hover:bg-editor-hover transition-colors"
                  >
                    <Icon name="copy" class="size-3.5 text-editor-text-dim" />
                    <span>{t("layers.duplicateLayer", "Duplicate")}</span>
                  </button>
                  <button
                    type="button"
                    onClick={layerActions.handleDeleteActiveLayer}
                    class="flex h-7 items-center justify-center gap-1.5 rounded-[3px] border border-red-500/20 bg-red-500/10 text-[11px] font-medium text-red-400 hover:bg-red-500/20 transition-colors"
                  >
                    <Icon name="trash" class="size-3.5 text-red-400" />
                    <span>{t("layers.deleteLayer", "Delete")}</span>
                  </button>
                </div>
              </div>
            </div>
          </div>
        </Show>
        </Show>
      </div>
    </section>
  );
}

function StatusHint(props: { children: string }) {
  return (
    <div class="flex items-start gap-2 rounded-[4px] border border-editor-divider bg-editor-field px-2.5 py-2 text-[11px] leading-snug text-editor-text-dim">
      <Icon name="sliders" class="mt-0.5 size-3.5 shrink-0 text-editor-text-dim" strokeWidth={1.75} />
      <span>{props.children}</span>
    </div>
  );
}

