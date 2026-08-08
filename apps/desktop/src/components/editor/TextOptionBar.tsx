// SPDX-License-Identifier: AGPL-3.0-or-later
import { Show, For, createSignal, onMount } from "solid-js";
import { clsx } from "clsx";
import { useEditor } from "./shell/EditorContext";
import { ToolPill, Divider } from "./shell/OptionBarShared";
import { Tooltip } from "./Tooltip";
import { Icon } from "./icons";
import { getAvailableFonts } from "@/lib/fontEnumeration";
import type { FontFamily } from "@/lib/fontEnumeration";
import type { LayerNode } from "@/engine/types";
import type { TextData } from "@/engine/textTypes";

type TextLayer = LayerNode & { type: "text"; textData: TextData };

/** Common size presets (plan §9.1). Free-typed values 1..2000 also accepted. */
const FONT_SIZE_PRESETS = [
  6, 8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 32, 36, 42, 48,
  56, 64, 72, 80, 96, 120, 144, 192, 256,
];

/** Field-by-field no-op check: skips commit when the edit is a no-op (avoids
 *  ghost undo entries on repeated input events). Mirrors ShapeOptionBar. */
function shallowEqualTextData(cur: TextData, patch: Partial<TextData>): boolean {
  if (patch.fontFamily !== undefined && patch.fontFamily !== cur.fontFamily) return false;
  if (patch.fontSize !== undefined && patch.fontSize !== cur.fontSize) return false;
  if (patch.fontWeight !== undefined && patch.fontWeight !== cur.fontWeight) return false;
  if (patch.fontStyle !== undefined && patch.fontStyle !== cur.fontStyle) return false;
  if (patch.color !== undefined && patch.color !== cur.color) return false;
  if (patch.align !== undefined && patch.align !== cur.align) return false;
  if (patch.lineHeight !== undefined && patch.lineHeight !== cur.lineHeight) return false;
  if (patch.letterSpacing !== undefined && patch.letterSpacing !== cur.letterSpacing) return false;
  if (patch.boxMode !== undefined && patch.boxMode !== cur.boxMode) return false;
  if (patch.boxWidth !== undefined && patch.boxWidth !== cur.boxWidth) return false;
  return true;
}

/**
 * Text option bar — TWO modes:
 * - draw mode (text tool active, no text layer selected): writes the editor
 *   signals used for the NEXT created text layer (deterministic session
 *   defaults; color reads the shared editor foreground).
 * - edit mode (a text layer is selected, any active tool): binds live textData
 *   and calls engine.updateTextData (commit BEFORE mutation).
 * Research pain points addressed: always-visible bar (edit mode), WYSIWYG font
 * preview + search, color read from the same foreground used elsewhere.
 */
export function TextOptionBar() {
  const {
    workspace,
    layers,
    selectedLayerId,
    fgColor, setFgColor,
    textFontFamily, setTextFontFamily,
    textFontSize, setTextFontSize,
    textFontWeight, setTextFontWeight,
    textFontItalic, setTextFontItalic,
    textAlign, setTextAlign,
  } = useEditor();

  const [fonts, setFonts] = createSignal<FontFamily[]>([]);
  const [fontPickerOpen, setFontPickerOpen] = createSignal(false);
  const [fontSearch, setFontSearch] = createSignal("");

  // Lazy-load once per app session (module-level cache in fontEnumeration).
  onMount(() => {
    void getAvailableFonts().then((f) => setFonts(f));
  });

  // Read through the reactive `layers()` signal (same as ShapeOptionBar) so
  // the edit-mode binding updates immediately on engine change.
  const selectedText = (): TextLayer | null => {
    const id = selectedLayerId();
    if (!id) return null;
    const layer = layers().find((l) => l.id === id);
    return layer && layer.type === "text" && layer.textData ? (layer as TextLayer) : null;
  };

  const isEditMode = () => !!selectedText();
  const text = () => selectedText()!;

  const fontFamily = () => (isEditMode() ? text().textData.fontFamily : textFontFamily());
  const fontSize = () => (isEditMode() ? text().textData.fontSize : textFontSize());
  const fontWeight = () => (isEditMode() ? text().textData.fontWeight : textFontWeight());
  const fontItalic = () => (isEditMode() ? text().textData.fontStyle === "italic" : textFontItalic());
  const align = () => (isEditMode() ? text().textData.align : textAlign());
  const color = () => (isEditMode() ? text().textData.color : fgColor());

  const applyEdit = (patch: Partial<TextData>) => {
    const engine = workspace.getActiveEngine();
    const history = workspace.getActiveHistory();
    const layer = selectedText();
    if (!layer || !engine || !history) return;
    const next = { ...layer.textData, ...patch };
    // No-op guard: skip commit when the patch doesn't change the current
    // textData — prevents ghost undo entries from repeated input events.
    if (shallowEqualTextData(layer.textData, next)) return;
    // commit BEFORE mutation (AGENTS.md wiring rule)
    history.commit(engine.snapshot(), "Edit Text");
    engine.updateTextData(layer.id, next);
  };

  const setFamily = (family: string) => {
    setFontPickerOpen(false);
    if (isEditMode()) applyEdit({ fontFamily: family });
    else setTextFontFamily(family);
  };

  const setSize = (size: number) => {
    const v = Math.min(2000, Math.max(1, Math.round(size)));
    if (isEditMode()) applyEdit({ fontSize: v });
    else setTextFontSize(v);
  };

  const toggleBold = () => {
    const next = fontWeight() >= 600 ? 400 : 700;
    if (isEditMode()) applyEdit({ fontWeight: next });
    else setTextFontWeight(next);
  };

  const toggleItalic = () => {
    const next = !fontItalic();
    if (isEditMode()) applyEdit({ fontStyle: next ? "italic" : "normal" });
    else setTextFontItalic(next);
  };

  const setAlign = (a: "left" | "center" | "right") => {
    if (isEditMode()) applyEdit({ align: a });
    else setTextAlign(a);
  };

  const setColor = (c: string) => {
    if (isEditMode()) applyEdit({ color: c });
    else setFgColor(c);
  };

  const filteredFonts = () => {
    const q = fontSearch().toLowerCase().trim();
    if (!q) return fonts();
    return fonts().filter((f) => f.family.toLowerCase().includes(q));
  };

  const btnClass = clsx(
    "flex h-[24px] shrink-0 cursor-pointer items-center gap-1.5 rounded-[3px] border border-editor-field-border bg-editor-field px-1.5 select-none",
  );

  const iconBtnClass = (active: boolean) =>
    clsx(
      "flex h-[24px] w-[24px] shrink-0 cursor-pointer items-center justify-center rounded-[3px] border text-[11px] font-medium transition-colors",
      active
        ? "border-editor-accent/80 bg-editor-accent/15 text-editor-text shadow-sm"
        : "border-transparent text-editor-text-dim hover:border-editor-field-border hover:bg-editor-field/40 hover:text-editor-text",
    );

  return (
    <div data-text-option-bar class="flex items-center gap-1.5 text-[11px] select-none">
      <ToolPill icon="type" label="Text" />

      <Divider />

      {/* Font family — searchable dropdown with WYSIWYG preview (plan R4/R5) */}
      <div class="relative">
        <Tooltip content="Font family" placement="top">
          <button
            type="button"
            data-font-picker-trigger
            aria-label="Font family"
            onClick={() => {
              setFontPickerOpen((v) => !v);
              setFontSearch("");
            }}
            class={btnClass}
          >
            <span
              class="max-w-[110px] truncate text-editor-text"
              style={{ "font-family": `"${fontFamily()}", sans-serif` }}
            >
              {fontFamily()}
            </span>
            <Icon name="chevron-down" class="size-3 text-editor-text-dim" />
          </button>
        </Tooltip>
        <Show when={fontPickerOpen()}>
          <div
            class="absolute left-0 top-full z-50 mt-1 w-56 overflow-hidden rounded-md border border-editor-field-border bg-editor-panel shadow-lg"
            data-font-picker
          >
            <input
              type="search"
              placeholder="Search fonts..."
              aria-label="Search fonts"
              value={fontSearch()}
              onInput={(e) => setFontSearch(e.currentTarget.value)}
              class="w-full border-b border-editor-divider bg-transparent px-2.5 py-1.5 text-[11px] text-editor-text outline-none placeholder:text-editor-text-dim/50"
            />
            <div class="max-h-56 overflow-y-auto py-0.5">
              <For each={filteredFonts()}>
                {(f) => (
                  <button
                    type="button"
                    onClick={() => setFamily(f.family)}
                    class={clsx(
                      "flex w-full items-center gap-2 px-2.5 py-1 text-left text-[11px]",
                      fontFamily() === f.family
                        ? "bg-white/10 text-editor-text"
                        : "text-editor-text-dim hover:bg-white/5 hover:text-editor-text",
                    )}
                  >
                    <span class="w-8 shrink-0 text-sm" style={{ "font-family": `"${f.family}", sans-serif` }}>
                      Aa
                    </span>
                    <span class="truncate">{f.family}</span>
                  </button>
                )}
              </For>
            </div>
          </div>
          {/* Click-away backdrop (below the panel, above everything else) */}
          <div class="fixed inset-0 z-40" onClick={() => setFontPickerOpen(false)} />
        </Show>
      </div>

      <Divider />

      {/* Font size — free number input + preset dropdown */}
      <label class={btnClass}>
        <input
          type="number"
          aria-label="Font size"
          min={1}
          max={2000}
          value={fontSize()}
          onInput={(e) => setSize(Number(e.currentTarget.value))}
          class="w-10 bg-transparent text-center text-editor-text outline-none"
        />
        <select
          aria-label="Font size preset"
          value={fontSize()}
          onChange={(e) => setSize(Number(e.currentTarget.value))}
          class="w-4 cursor-pointer appearance-none bg-transparent text-editor-text-dim outline-none"
        >
          <For each={FONT_SIZE_PRESETS}>
            {(p) => <option value={p}>{p}</option>}
          </For>
        </select>
      </label>

      {/* Weight / style */}
      <button type="button" aria-label="Bold" aria-pressed={fontWeight() >= 600} onClick={toggleBold} class={iconBtnClass(fontWeight() >= 600)}>
        <span class="font-bold">B</span>
      </button>
      <button type="button" aria-label="Italic" aria-pressed={fontItalic()} onClick={toggleItalic} class={iconBtnClass(fontItalic())}>
        <span class="italic">I</span>
      </button>

      <Divider />

      {/* Alignment */}
      <div class="flex shrink-0 items-center gap-0.5">
        {(["left", "center", "right"] as const).map((a) => (
          <Tooltip content={a === "left" ? "Align left" : a === "center" ? "Align center" : "Align right"} placement="top">
            <button
              type="button"
              aria-label={a === "left" ? "Align left" : a === "center" ? "Align center" : "Align right"}
              aria-pressed={align() === a}
              onClick={() => setAlign(a)}
              class={iconBtnClass(align() === a)}
            >
              <Icon name={a === "left" ? "align-text-left" : a === "center" ? "align-text-center" : "align-text-right"} class="size-3.5" strokeWidth={1.6} />
            </button>
          </Tooltip>
        ))}
      </div>

      <Divider />

      {/* Color — reads/shared with the editor foreground in draw mode (R2) */}
      <Tooltip content="Text color" placement="top">
        <input
          type="color"
          aria-label="Text color"
          value={color()}
          onInput={(e) => setColor(e.currentTarget.value)}
          class="size-[22px] shrink-0 cursor-pointer rounded-[3px] border border-editor-field-border bg-transparent p-0"
        />
      </Tooltip>
    </div>
  );
}
