// SPDX-License-Identifier: AGPL-3.0-or-later
import { Show, createSignal } from "solid-js";
import type { PrintOptions } from "./printTypes";
import { formatPhysicalDimensions } from "./printGeometry";

interface PrintPaperViewportProps {
  options: PrintOptions;
  setOptions: (updater: (prev: PrintOptions) => PrintOptions) => void;
  previewUrl: string | null;
  docWidthPx: number;
  docHeightPx: number;
  docName?: string;
}

export function PrintPaperViewport(props: PrintPaperViewportProps) {
  const [isDragging, setIsDragging] = createSignal(false);
  const [dragStartPos, setDragStartPos] = createSignal<{ x: number; y: number } | null>(null);
  const [initialOffsets, setInitialOffsets] = createSignal<{ left: number; top: number }>({ left: 0, top: 0 });

  // Calculate SVG paper dimensions (max preview box 480x480)
  const containerW = 440;
  const containerH = 440;

  const paperW = () => props.options.paperWidthMm;
  const paperH = () => props.options.paperHeightMm;
  const paperAspect = () => (paperH() > 0 ? paperW() / paperH() : 1);

  // SVG Paper display rect
  const svgPaper = () => {
    let w = containerW;
    let h = containerH;
    if (paperAspect() > 1) {
      h = containerW / paperAspect();
    } else {
      w = containerH * paperAspect();
    }
    return { width: w, height: h, scaleMmToSvg: w / paperW() };
  };

  // Image size on paper in mm
  const imageMm = () => {
    const scaleFactor = props.options.scalePercent / 100;
    // 300 DPI reference size in mm
    const refWm = (props.docWidthPx / 300) * 25.4;
    const refHm = (props.docHeightPx / 300) * 25.4;

    const wMm = refWm * scaleFactor;
    const hMm = refHm * scaleFactor;
    return { wMm, hMm };
  };

  // Image position in mm (center vs offset)
  const imagePosMm = () => {
    const { wMm, hMm } = imageMm();
    let left = props.options.leftOffsetMm;
    let top = props.options.topOffsetMm;

    if (props.options.centerImage) {
      left = (paperW() - wMm) / 2;
      top = (paperH() - hMm) / 2;
    }
    return { left, top, wMm, hMm };
  };

  // Convert mm to SVG coordinates
  const imageSvg = () => {
    const paper = svgPaper();
    const pos = imagePosMm();

    return {
      x: pos.left * paper.scaleMmToSvg,
      y: pos.top * paper.scaleMmToSvg,
      width: pos.wMm * paper.scaleMmToSvg,
      height: pos.hMm * paper.scaleMmToSvg,
    };
  };

  const handlePointerDown = (e: PointerEvent) => {
    setIsDragging(true);
    setDragStartPos({ x: e.clientX, y: e.clientY });
    setInitialOffsets({
      left: imagePosMm().left,
      top: imagePosMm().top,
    });
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  };

  const handlePointerMove = (e: PointerEvent) => {
    if (!isDragging() || !dragStartPos()) return;

    const dxPx = e.clientX - dragStartPos()!.x;
    const dyPx = e.clientY - dragStartPos()!.y;

    const paper = svgPaper();
    const dxMm = dxPx / paper.scaleMmToSvg;
    const dyMm = dyPx / paper.scaleMmToSvg;

    const newLeft = Number((initialOffsets().left + dxMm).toFixed(1));
    const newTop = Number((initialOffsets().top + dyMm).toFixed(1));

    props.setOptions((prev) => ({
      ...prev,
      centerImage: false,
      leftOffsetMm: newLeft,
      topOffsetMm: newTop,
    }));
  };

  const handlePointerUp = (e: PointerEvent) => {
    if (!isDragging()) return;
    setIsDragging(false);
    setDragStartPos(null);
    try {
      (e.target as HTMLElement).releasePointerCapture(e.pointerId);
    } catch {
      // Ignored if capture already lost
    }
  };

  return (
    <div class="flex flex-1 flex-col items-center justify-between bg-editor-bg p-4 select-none">
      {/* Header physical dimension badge */}
      <div class="mb-3 flex flex-col items-center justify-center gap-0.5 rounded-[6px] border border-editor-field-border bg-editor-panel px-4 py-2 text-[11.5px] font-semibold text-editor-text shadow-xs text-center">
        <div class="truncate font-semibold text-editor-text max-w-[340px]">
          {props.docName || "Untitled"} ({props.docWidthPx} × {props.docHeightPx} px)
        </div>
        <div class="text-[11px] font-normal text-editor-text-dim">
          Paper: {formatPhysicalDimensions(paperW(), paperH(), props.options.unit)} ({props.options.paperPreset})
        </div>
      </div>

      {/* Interactive Paper Viewport Canvas Container */}
      <div class="relative flex flex-1 items-center justify-center w-full max-h-[460px] p-2">
        <svg
          width={svgPaper().width}
          height={svgPaper().height}
          viewBox={`0 0 ${svgPaper().width} ${svgPaper().height}`}
          class="rounded-[4px] shadow-[0_4px_20px_rgba(0,0,0,0.4)] transition-all duration-150"
          style={{
            background: props.options.showPaperWhite ? "#ffffff" : "#222226",
          }}
        >
          {/* Outer Paper Border & Pattern */}
          <rect
            x="0"
            y="0"
            width={svgPaper().width}
            height={svgPaper().height}
            fill="none"
            stroke="#3f3f46"
            stroke-width="1"
            rx="2"
          />

          {/* Printable Area / Margin Guide */}
          <rect
            x={props.options.marginMm * svgPaper().scaleMmToSvg}
            y={props.options.marginMm * svgPaper().scaleMmToSvg}
            width={
              (paperW() - props.options.marginMm * 2) * svgPaper().scaleMmToSvg
            }
            height={
              (paperH() - props.options.marginMm * 2) * svgPaper().scaleMmToSvg
            }
            fill="none"
            stroke="#71717a"
            stroke-dasharray="4 4"
            stroke-width="0.8"
          />

          {/* Image Overlay */}
          <Show when={props.previewUrl}>
            <g
              class="cursor-grab active:cursor-grabbing"
              onPointerDown={handlePointerDown}
              onPointerMove={handlePointerMove}
              onPointerUp={handlePointerUp}
              onPointerCancel={handlePointerUp}
            >
              {/* Shadow under image */}
              <rect
                x={imageSvg().x + 2}
                y={imageSvg().y + 2}
                width={imageSvg().width}
                height={imageSvg().height}
                fill="rgba(0,0,0,0.2)"
                rx="1"
              />
              {/* Image element */}
              <image
                href={props.previewUrl!}
                x={imageSvg().x}
                y={imageSvg().y}
                width={imageSvg().width}
                height={imageSvg().height}
                preserveAspectRatio="none"
              />
              {/* Image Selection Outline */}
              <rect
                x={imageSvg().x}
                y={imageSvg().y}
                width={imageSvg().width}
                height={imageSvg().height}
                fill="none"
                stroke={isDragging() ? "#e15a17" : "#e15a17"}
                stroke-width={isDragging() ? "2" : "1.2"}
                stroke-dasharray={isDragging() ? "none" : "3 3"}
              />
            </g>
          </Show>
        </svg>
      </div>

      {/* Footer hint */}
      <div class="mt-2 text-[11px] text-editor-text-dim flex items-center gap-1.5 font-medium">
        <svg class="size-3 text-editor-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 15l-2 5L9 9l11 4-5 2zm0 0l5 5M7.188 2.239l.777 2.897M5.136 7.965l-2.898-.777M13.95 4.05l-2.122 2.122m-5.657 5.656l-2.12 2.122" />
        </svg>
        <span>Click and drag image on paper to reposition</span>
      </div>
    </div>
  );
}
