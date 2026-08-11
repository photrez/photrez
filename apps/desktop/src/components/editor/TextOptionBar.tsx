// SPDX-License-Identifier: AGPL-3.0-or-later
import { Show, For, createSignal, createEffect, onMount } from "solid-js";
import { clsx } from "clsx";
import { useEditor } from "./shell/EditorContext";
import { ToolPill, Divider } from "./shell/OptionBarShared";
import { Tooltip } from "./Tooltip";
import { Icon } from "./icons";
import { getAvailableFonts, getInstantFonts } from "@/lib/fontEnumeration";
import type { FontFamily } from "@/lib/fontEnumeration";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import type { LayerNode } from "@/engine/types";
import type { TextData, TextStrokeAlign } from "@/engine/textTypes";

type TextLayer = LayerNode & { type: "text"; textData: TextData };

/** Common size presets (plan §9.1). Free-typed values 1..2000 also accepted. */
const FONT_SIZE_PRESETS = [
  6, 8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 32, 36, 42, 48,
  56, 64, 72, 80, 96, 120, 144, 192, 256,
];

/** Standard CSS font weights with human labels (100..900). The VALUE stays a
 *  numeric CSS weight (engine/undo contract); only the UI shows names. */
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

/** Label for any weight — preset names, numeric fallback for custom values. */
function weightLabel(value: number): string {
  return FONT_WEIGHT_PRESETS.find((p) => p.value === value)?.label ?? String(value);
}

/** Font family name with the searched substring highlighted in the accent color. */
function FontNameHighlight(props: { name: string; query: string }) {
  const parts = () => {
    const q = props.query.trim().toLowerCase();
    if (!q) return [props.name] as string[];
    const idx = props.name.toLowerCase().indexOf(q);
    if (idx === -1) return [props.name] as string[];
    return [
      props.name.slice(0, idx),
      props.name.slice(idx, idx + q.length),
      props.name.slice(idx + q.length),
    ] as string[];
  };
  return (
    <span class="truncate">
      <For each={parts()}>
        {(part, i) =>
          i() === 1 ? (
            <mark class="bg-transparent font-semibold text-editor-accent">{part}</mark>
          ) : (
            <span>{part}</span>
          )
        }
      </For>
    </span>
  );
}

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
  if (
    patch.stroke !== undefined &&
    ((patch.stroke.width ?? 0) !== (cur.stroke?.width ?? 0) ||
      (patch.stroke.color ?? "#000000") !== (cur.stroke?.color ?? "#000000"))
  )
    return false;
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
    textEditSession,
    textStrokeWidth, setTextStrokeWidth,
    textStrokeColor, setTextStrokeColor,
    textStrokeAlign, setTextStrokeAlign,
  } = useEditor();

  // Start from the instant WEB_SAFE placeholder so the dropdown is NEVER empty
  // while the (native) enumeration is in flight — the old empty-until-loaded
  // list read as a first-open lag. The full list replaces it when ready.
  const [fonts, setFonts] = createSignal<FontFamily[]>(getInstantFonts());
  const [fontPickerOpen, setFontPickerOpen] = createSignal(false);
  const [fontSearch, setFontSearch] = createSignal("");
  const [highlightIndex, setHighlightIndex] = createSignal(0);
  const [fontsLoading, setFontsLoading] = createSignal(false);
  // Drafts for the numeric inputs: while the user clears the box to retype a
  // value, the empty string is HELD instead of committing 0 (which used to
  // disable the stroke) or snapping to the clamp (font size). Commit happens
  // on the next valid keystroke; blur/Enter with an empty draft reverts.
  const [strokePopoverOpen, setStrokePopoverOpen] = createSignal(false);
  const [weightPickerOpen, setWeightPickerOpen] = createSignal(false);
  const [sizePickerOpen, setSizePickerOpen] = createSignal(false);
  const [strokeDraft, setStrokeDraft] = createSignal<string | null>(null);
  const [sizeDraft, setSizeDraft] = createSignal<string | null>(null);
  let searchRef: HTMLInputElement | undefined;
  let listRef: HTMLDivElement | undefined;

  // Load the full font list (cached module-level); the instant WEB_SAFE
  // placeholder stays visible until it resolves so the dropdown is never empty.
  const loadFonts = () => {
    setFontsLoading(true);
    void getAvailableFonts().then((f) => {
      setFonts(f);
      setFontsLoading(false);
    });
  };

  // Desktop (Tauri): native enumeration is prompt-free, so warm it at mount.
  // Web runtime: Local Font Access needs a user gesture — the full list loads
  // on the first dropdown open (loadFonts in the trigger onClick).
  onMount(() => {
    if (isTauriRuntime()) {
      loadFonts();
    }
  });

  // Auto-focus the search box when the dropdown opens (type-to-filter immediately).
  createEffect(() => {
    if (fontPickerOpen()) {
      searchRef?.focus();
    }
  });

  // Keep the keyboard highlight in range and scroll it into view. When the
  // filter shrinks the list the highlight clamps back to the top.
  createEffect(() => {
    const list = filteredFonts();
    if (list.length === 0) {
      setHighlightIndex(0);
      return;
    }
    const clamped = Math.min(highlightIndex(), list.length - 1);
    if (clamped !== highlightIndex()) setHighlightIndex(clamped);
    const el = listRef?.children[clamped];
    (el as HTMLElement | undefined)?.scrollIntoView?.({ block: "nearest" });
  });

  const handleFontPickerKeyDown = (e: KeyboardEvent) => {
    const list = filteredFonts();
    switch (e.key) {
      case "ArrowDown":
        if (list.length > 0) {
          e.preventDefault();
          setHighlightIndex((i) => Math.min(i + 1, list.length - 1));
        }
        break;
      case "ArrowUp":
        e.preventDefault();
        setHighlightIndex((i) => Math.max(i - 1, 0));
        break;
      case "Enter": {
        const f = list[highlightIndex()];
        if (f) {
          e.preventDefault();
          setFamily(f.family);
        }
        break;
      }
      case "Escape":
        e.preventDefault();
        setFontPickerOpen(false);
        break;
    }
  };

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
  const strokeWidth = () => (isEditMode() ? text().textData.stroke?.width ?? 0 : textStrokeWidth());
  const strokeColor = () => (isEditMode() ? text().textData.stroke?.color ?? "#000000" : textStrokeColor());
  const strokeAlign = () =>
    isEditMode()
      ? (text().textData.stroke?.align ?? "outside")
      : typeof textStrokeAlign === "function"
        ? textStrokeAlign()
        : "outside";

  const applyEdit = (patch: Partial<TextData>) => {
    const engine = workspace.getActiveEngine();
    const history = workspace.getActiveHistory();
    const layer = selectedText();
    if (!layer || !engine || !history) return;
    const next = { ...layer.textData, ...patch };
    // No-op guard: skip commit when the patch doesn't change the current
    // textData — prevents ghost undo entries from repeated input events.
    if (shallowEqualTextData(layer.textData, next)) return;
    // Session guard (B4): while a live text-edit session owns this SAME
    // layer, skip the history commit — the session commits exactly once at
    // close (one "Edit Text" undo step per session contract). Live-mutate
    // only, so option-bar changes appear in the overlay/engine immediately.
    // A session open on a DIFFERENT layer still commits normally.
    const session = textEditSession();
    if (session && session.layerId === layer.id) {
      engine.updateTextData(layer.id, next);
      return;
    }
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
    setSizeDraft(null); // any committed change clears a held draft
    const v = Math.min(2000, Math.max(1, Math.round(size)));
    if (isEditMode()) applyEdit({ fontSize: v });
    else setTextFontSize(v);
  };

  const onSizeInput = (raw: string) => {
    if (raw.trim() === "") {
      setSizeDraft(""); // hold empty while retyping — never snap mid-edit
      return;
    }
    setSizeDraft(null);
    const n = Math.round(Number(raw));
    if (Number.isFinite(n)) setSize(n);
  };

  const setWeight = (w: number) => {
    const v = Math.min(900, Math.max(100, Math.round(w)));
    if (isEditMode()) applyEdit({ fontWeight: v });
    else setTextFontWeight(v);
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

  const strokePatch = () => ({
    width: strokeWidth(),
    color: strokeColor(),
    align: strokeAlign(),
  });

  const setStrokeWidth = (w: number) => {
    setStrokeDraft(null); // any committed change clears a held draft
    const v = Math.min(100, Math.max(0, Math.round(w) || 0));
    if (isEditMode()) applyEdit({ stroke: { ...strokePatch(), width: v } });
    else setTextStrokeWidth(v);
  };

  const stepStroke = (delta: number) => {
    setStrokeDraft(null);
    // The stepper never disables the stroke — 1 is the floor (the toggle
    // button is the explicit off switch).
    setStrokeWidth(Math.min(100, Math.max(1, strokeWidth() + delta)));
  };

  const onStrokeInput = (raw: string) => {
    if (raw.trim() === "") {
      // Clearing the box to retype a value must NOT kill the stroke (empty
      // would otherwise commit Number("") = 0 and disable it).
      setStrokeDraft("");
      return;
    }
    setStrokeDraft(null);
    const n = Math.round(Number(raw));
    if (Number.isFinite(n)) setStrokeWidth(Math.min(100, Math.max(1, n)));
  };

  const setStrokeColor = (c: string) => {
    if (isEditMode()) applyEdit({ stroke: { ...strokePatch(), color: c } });
    else setTextStrokeColor(c);
  };

  const setStrokeAlign = (a: TextStrokeAlign) => {
    if (isEditMode()) applyEdit({ stroke: { ...strokePatch(), align: a } });
    else setTextStrokeAlign(a);
  };

  const toggleStroke = () => {
    setStrokeDraft(null);
    if (strokeWidth() > 0) setStrokeWidth(0);
    else if (isEditMode()) applyEdit({ stroke: { ...strokePatch(), width: 4 } });
    else setTextStrokeWidth(4);
  };

  const filteredFonts = () => {
    const q = fontSearch().toLowerCase().trim();
    if (!q) return fonts();
    return fonts().filter((f) => f.family.toLowerCase().includes(q));
  };

  const btnClass = clsx(
    "flex h-[24px] shrink-0 cursor-pointer items-center gap-1.5 rounded-[4px] border border-editor-field-border bg-editor-field px-2 select-none text-[11px] font-medium text-editor-text transition-colors hover:border-editor-text-dim/50",
  );

  const iconBtnClass = (active: boolean) =>
    clsx(
      "flex h-[24px] w-[24px] shrink-0 cursor-pointer items-center justify-center rounded-[4px] border text-[11px] font-medium transition-all duration-75",
      active
        ? "border-editor-accent bg-editor-accent/20 text-white shadow-xs"
        : "border-transparent text-editor-text-dim hover:border-editor-field-border hover:bg-editor-field/60 hover:text-editor-text",
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
            aria-haspopup="listbox"
            aria-expanded={fontPickerOpen()}
            onClick={() => {
              setFontPickerOpen((v) => !v);
              setFontSearch("");
              setHighlightIndex(0);
              // Web runtime: start the (prompted) enumeration from this user
              // gesture; Tauri: already cached — resolves instantly.
              loadFonts();
            }}
            class={btnClass}
          >
            <span class="text-[13px] leading-none text-editor-accent font-bold" style={{ "font-family": `"${fontFamily()}", sans-serif` }}>
              Aa
            </span>
            <span
              class="max-w-[100px] truncate text-editor-text font-medium"
              style={{ "font-family": `"${fontFamily()}", sans-serif` }}
            >
              {fontFamily()}
            </span>
            <Icon name="chevron-down" class="size-3 shrink-0 text-editor-text-dim ml-1" />
          </button>
        </Tooltip>
        <Show when={fontPickerOpen()}>
          <div
            class="absolute left-0 top-full z-50 mt-1.5 w-64 overflow-hidden rounded-[6px] border border-editor-field-border bg-[#1D2026] shadow-2xl"
            data-font-picker
            onKeyDown={handleFontPickerKeyDown}
          >
            <input
              ref={searchRef}
              type="search"
              placeholder="Search fonts..."
              aria-label="Search fonts"
              value={fontSearch()}
              onInput={(e) => {
                setFontSearch(e.currentTarget.value);
                setHighlightIndex(0);
              }}
              class="w-full border-b border-editor-divider bg-transparent px-2.5 py-1.5 text-[11px] text-editor-text outline-none placeholder:text-editor-text-dim/60"
            />
            <div
              ref={listRef}
              class="max-h-56 overflow-y-auto py-0.5"
              data-font-picker-list
              role="listbox"
              aria-label="Font families"
              aria-activedescendant={
                filteredFonts()[highlightIndex()] ? `font-opt-${highlightIndex()}` : undefined
              }
            >
              <Show
                when={filteredFonts().length > 0}
                fallback={
                  <div class="px-2.5 py-2 text-[11px] text-editor-text-dim/70">
                    No fonts match “{fontSearch()}”
                  </div>
                }
              >
                <For each={filteredFonts()}>
                  {(f, i) => (
                    <button
                      type="button"
                      id={`font-opt-${i()}`}
                      role="option"
                      aria-selected={fontFamily() === f.family}
                      onClick={() => setFamily(f.family)}
                      onMouseEnter={() => setHighlightIndex(i())}
                      class={clsx(
                        "flex w-full items-center gap-2 px-2.5 py-1 text-left text-[11px]",
                        i() === highlightIndex()
                          ? "bg-editor-accent/20 text-white"
                          : fontFamily() === f.family
                            ? "bg-white/10 text-white font-medium"
                            : "text-editor-text-dim hover:bg-white/5 hover:text-editor-text",
                      )}
                    >
                      <span class="w-8 shrink-0 text-sm" style={{ "font-family": `"${f.family}", sans-serif` }}>
                        Aa
                      </span>
                      <FontNameHighlight name={f.family} query={fontSearch()} />
                      <Show when={fontFamily() === f.family}>
                        <Icon name="check" class="ml-auto size-3 shrink-0 text-editor-accent" strokeWidth={3} />
                      </Show>
                    </button>
                  )}
                </For>
              </Show>
            </div>
            <Show when={fontsLoading()}>
              <div
                data-font-picker-loading
                class="border-t border-editor-divider px-2.5 py-1.5 text-[10px] text-editor-text-dim/70 animate-pulse"
              >
                Loading system fonts…
              </div>
            </Show>
          </div>
          {/* Click-away backdrop (below the panel, above everything else) */}
          <div class="fixed inset-0 z-40" onClick={() => setFontPickerOpen(false)} />
        </Show>
      </div>

      <Divider />

      {/* Font size — free number input + custom dark preset popover */}
      <div class="relative">
        <Tooltip content="Font size" placement="top">
          <div class="flex h-[24px] shrink-0 items-center rounded-[4px] border border-editor-field-border bg-editor-field px-1.5 transition-colors focus-within:border-editor-accent focus-within:ring-1 focus-within:ring-editor-accent/70">
            <input
              type="number"
              aria-label="Font size"
              min={1}
              max={2000}
              value={sizeDraft() ?? String(fontSize())}
              onInput={(e) => onSizeInput(e.currentTarget.value)}
              onBlur={() => setSizeDraft(null)}
              onKeyDown={(e) => {
                if (e.key === "Enter") (e.currentTarget as HTMLInputElement).blur();
              }}
              class="w-8 bg-transparent text-center font-mono text-[11px] font-semibold text-editor-text outline-none"
            />
            <span class="text-[10px] font-medium text-editor-text-dim">px</span>
            <button
              type="button"
              aria-label="Font size presets"
              aria-expanded={sizePickerOpen()}
              onClick={() => setSizePickerOpen(!sizePickerOpen())}
              class="ml-1 flex items-center p-0.5 text-editor-text-dim hover:text-editor-text transition-colors"
            >
              <Icon name="chevron-down" class="size-3" strokeWidth={1.75} />
            </button>
          </div>
        </Tooltip>
        <Show when={sizePickerOpen()}>
          <div
            class="absolute left-0 top-full z-50 mt-1.5 w-24 max-h-56 overflow-y-auto rounded-[6px] border border-editor-field-border bg-[#1D2026] py-1 shadow-2xl"
          >
            <For each={FONT_SIZE_PRESETS}>
              {(p) => (
                <button
                  type="button"
                  onClick={() => {
                    setSize(p);
                    setSizePickerOpen(false);
                  }}
                  class={clsx(
                    "flex w-full items-center justify-between px-2.5 py-1 text-left text-[11px] font-mono transition-colors",
                    fontSize() === p
                      ? "bg-editor-accent/20 text-white font-semibold"
                      : "text-editor-text-dim hover:bg-white/5 hover:text-editor-text",
                  )}
                >
                  <span>{p} px</span>
                  <Show when={fontSize() === p}>
                    <Icon name="check" class="size-3 text-editor-accent shrink-0" strokeWidth={3} />
                  </Show>
                </button>
              )}
            </For>
          </div>
          <div class="fixed inset-0 z-40" onClick={() => setSizePickerOpen(false)} />
        </Show>
      </div>

      {/* Weight — custom dark dropdown matching Font Family */}
      <div class="relative">
        <Tooltip content="Font weight" placement="top">
          <button
            type="button"
            aria-label="Font weight"
            aria-expanded={weightPickerOpen()}
            onClick={() => setWeightPickerOpen(!weightPickerOpen())}
            class="flex h-[24px] shrink-0 items-center gap-1 rounded-[4px] border border-editor-field-border bg-editor-field px-2 text-[11px] font-medium text-editor-text transition-colors hover:border-editor-text-dim/50 select-none cursor-pointer"
          >
            <span class="min-w-[56px] text-left">{weightLabel(fontWeight())}</span>
            <Icon name="chevron-down" class="size-3 text-editor-text-dim ml-1 shrink-0" strokeWidth={1.75} />
          </button>
        </Tooltip>
        <Show when={weightPickerOpen()}>
          <div
            class="absolute left-0 top-full z-50 mt-1.5 w-36 overflow-hidden rounded-[6px] border border-editor-field-border bg-[#1D2026] py-1 shadow-2xl"
          >
            <For each={FONT_WEIGHT_PRESETS}>
              {(p) => (
                <button
                  type="button"
                  onClick={() => {
                    setWeight(p.value);
                    setWeightPickerOpen(false);
                  }}
                  class={clsx(
                    "flex w-full items-center justify-between px-2.5 py-1.5 text-left text-[11px] transition-colors",
                    fontWeight() === p.value
                      ? "bg-editor-accent/20 text-white font-semibold"
                      : "text-editor-text-dim hover:bg-white/5 hover:text-editor-text",
                  )}
                >
                  <span>{p.label}</span>
                  <Show when={fontWeight() === p.value}>
                    <Icon name="check" class="size-3 text-editor-accent shrink-0" strokeWidth={3} />
                  </Show>
                </button>
              )}
            </For>
          </div>
          <div class="fixed inset-0 z-40" onClick={() => setWeightPickerOpen(false)} />
        </Show>
      </div>
      <Tooltip content="Italic" placement="top">
        <button type="button" aria-label="Italic" aria-pressed={fontItalic()} onClick={toggleItalic} class={iconBtnClass(fontItalic())}>
          <span class="italic font-serif font-bold text-xs">I</span>
        </button>
      </Tooltip>

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
          class="size-[24px] shrink-0 cursor-pointer rounded-[4px] border border-editor-field-border bg-transparent p-0 transition-transform hover:scale-105"
        />
      </Tooltip>

      <Divider />

      {/* Stroke Pill & Flyout Popover (B-stroke) */}
      <div class="relative flex items-center select-none" data-text-stroke>
        <Tooltip content="Stroke outline options" placement="top">
          <button
            type="button"
            aria-label="Toggle stroke options"
            aria-expanded={strokePopoverOpen()}
            onClick={() => setStrokePopoverOpen(!strokePopoverOpen())}
            class={clsx(
              "flex h-[24px] items-center gap-1.5 rounded-[4px] border px-2 text-[11px] font-medium transition-all duration-75 select-none",
              strokeWidth() > 0
                ? "border-editor-accent bg-editor-accent/20 text-white shadow-xs font-semibold"
                : "border-editor-field-border bg-editor-field text-editor-text-dim hover:border-editor-text-dim/50 hover:text-editor-text",
            )}
          >
            <Show
              when={strokeWidth() > 0}
              fallback={
                <>
                  <Icon name="square-pen" class="size-3.5 text-editor-text-dim" strokeWidth={1.75} />
                  <span class="text-editor-text-dim font-medium">Stroke</span>
                </>
              }
            >
              <span
                class="size-2.5 shrink-0 rounded-full border border-black/50 shadow-2xs"
                style={{ background: strokeColor() }}
              />
              <span class="text-editor-text-dim font-medium">Stroke:</span>
              <span class="font-mono text-white font-bold">{strokeWidth()}px</span>
            </Show>
            <Icon name="chevron-down" class="size-3 text-editor-text-dim ml-0.5 shrink-0" strokeWidth={1.75} />
          </button>
        </Tooltip>

        {/* Stroke Popover Panel */}
        <Show when={strokePopoverOpen()}>
          <div
            class="absolute right-0 top-full mt-1.5 z-50 w-56 rounded-[6px] border border-editor-field-border bg-[#1D2026] p-3 shadow-2xl"
            onPointerDown={(e) => e.stopPropagation()}
          >
            {/* Header & Main Toggle */}
            <div class="flex items-center justify-between border-b border-editor-field-border/60 pb-2 mb-2.5">
              <span class="text-[11px] font-semibold text-editor-text">Stroke Outline</span>
              <button
                type="button"
                aria-label="Toggle stroke"
                onClick={toggleStroke}
                class={clsx(
                  "rounded-[3px] px-2 py-0.5 text-[10px] font-bold transition-all",
                  strokeWidth() > 0
                    ? "bg-editor-accent text-white shadow-xs"
                    : "bg-editor-field text-editor-text-dim hover:text-editor-text border border-editor-field-border",
                )}
              >
                {strokeWidth() > 0 ? "ENABLED" : "OFF"}
              </button>
            </div>

            <Show when={strokeWidth() > 0}>
              {/* Width Slider & Stepper */}
              <div class="mb-3 space-y-1.5">
                <div class="flex items-center justify-between text-[10px]">
                  <span class="text-editor-text-dim font-medium">Width</span>
                  <div class="flex items-center gap-1">
                    <button
                      type="button"
                      aria-label="Decrease stroke width"
                      onClick={() => stepStroke(-1)}
                      class="flex size-4.5 items-center justify-center rounded-[2px] border border-editor-field-border bg-editor-field text-editor-text hover:bg-editor-field/80"
                    >
                      −
                    </button>
                    <input
                      type="number"
                      aria-label="Stroke width"
                      min={1}
                      max={100}
                      value={strokeDraft() ?? String(strokeWidth())}
                      onInput={(e) => onStrokeInput(e.currentTarget.value)}
                      onBlur={() => setStrokeDraft(null)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") (e.currentTarget as HTMLInputElement).blur();
                      }}
                      class="w-8 rounded-[2px] border border-editor-field-border bg-editor-field text-center font-mono text-[10px] text-editor-text outline-none focus:border-editor-accent"
                    />
                    <button
                      type="button"
                      aria-label="Increase stroke width"
                      onClick={() => stepStroke(1)}
                      class="flex size-4.5 items-center justify-center rounded-[2px] border border-editor-field-border bg-editor-field text-editor-text hover:bg-editor-field/80"
                    >
                      +
                    </button>
                  </div>
                </div>
                <input
                  type="range"
                  aria-label="Stroke width slider"
                  min={1}
                  max={100}
                  value={strokeWidth()}
                  onInput={(e) => setStrokeWidth(Number(e.currentTarget.value))}
                  class="w-full h-1.5 cursor-pointer appearance-none rounded-full bg-editor-field accent-editor-accent"
                />
              </div>

              {/* Color Swatch */}
              <div class="mb-3 flex items-center justify-between text-[10px]">
                <span class="text-editor-text-dim font-medium">Color</span>
                <div class="flex items-center gap-1.5">
                  <span class="font-mono text-[10px] text-editor-text">{strokeColor()}</span>
                  <input
                    type="color"
                    aria-label="Stroke color"
                    value={strokeColor()}
                    onInput={(e) => setStrokeColor(e.currentTarget.value)}
                    class="size-5 shrink-0 cursor-pointer rounded-[3px] border border-editor-field-border bg-transparent p-0"
                  />
                </div>
              </div>

              {/* Position Segmented Buttons */}
              <div>
                <span class="block mb-1 text-[10px] font-medium text-editor-text-dim">Position</span>
                <div class="flex rounded-[4px] border border-editor-field-border bg-editor-field p-0.5">
                  <button
                    type="button"
                    aria-label="Stroke position outside"
                    onClick={() => setStrokeAlign("outside")}
                    class={clsx(
                      "flex-1 rounded-[2px] py-1 text-[10px] font-semibold transition-all",
                      strokeAlign() === "outside"
                        ? "bg-editor-accent text-white shadow-xs"
                        : "text-editor-text-dim hover:text-editor-text",
                    )}
                  >
                    Outside
                  </button>
                  <button
                    type="button"
                    aria-label="Stroke position center"
                    onClick={() => setStrokeAlign("center")}
                    class={clsx(
                      "flex-1 rounded-[2px] py-1 text-[10px] font-semibold transition-all",
                      strokeAlign() === "center"
                        ? "bg-editor-accent text-white shadow-xs"
                        : "text-editor-text-dim hover:text-editor-text",
                    )}
                  >
                    Center
                  </button>
                  <button
                    type="button"
                    aria-label="Stroke position inside"
                    onClick={() => setStrokeAlign("inside")}
                    class={clsx(
                      "flex-1 rounded-[2px] py-1 text-[10px] font-semibold transition-all",
                      strokeAlign() === "inside"
                        ? "bg-editor-accent text-white shadow-xs"
                        : "text-editor-text-dim hover:text-editor-text",
                    )}
                  >
                    Inside
                  </button>
                </div>
              </div>
            </Show>
          </div>
          {/* Click-away backdrop for stroke popover */}
          <div class="fixed inset-0 z-40" onClick={() => setStrokePopoverOpen(false)} />
        </Show>
      </div>
    </div>
  );
}
