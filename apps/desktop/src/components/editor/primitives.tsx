import type { JSX } from "solid-js";
import { Show, createSignal, createEffect } from "solid-js";
import { clsx } from "clsx";
import { Icon } from "./icons";

export function NumField(props: {
  label?: string;
  value: string;
  suffix?: string;
  class?: string;
  labelClass?: string;
}) {
  return (
    <div
      class={clsx(
        "flex h-[24px] items-center gap-1 rounded-[3px] border border-editor-field-border bg-editor-field px-1.5",
        props.class,
      )}
    >
      <Show when={props.label}>
        <span
          class={clsx(
            "text-[10px] font-medium text-editor-text-dim",
            props.labelClass,
          )}
        >
          {props.label}
        </span>
      </Show>
      <span class="text-[11px] text-editor-text">{props.value}</span>
      <Show when={props.suffix}>
        <span class="text-[10px] text-editor-text-dim">{props.suffix}</span>
      </Show>
    </div>
  );
}

export function SelectField(props: { value: string; class?: string }) {
  return (
    <div
      class={clsx(
        "flex h-[26px] items-center justify-between rounded-[4px] border border-editor-field-border bg-editor-field px-2.5",
        props.class,
      )}
    >
      <span class="text-[12px] text-editor-text">{props.value}</span>
      <Icon
        name="chevron-down"
        class="size-3.5 text-editor-text-dim"
        strokeWidth={1.75}
      />
    </div>
  );
}

export function PropRow(props: { label: string; children: JSX.Element }) {
  return (
    <div class="flex items-center gap-2">
      <span class="w-[50px] shrink-0 text-[11px] text-editor-text-dim">
        {props.label}
      </span>
      <div class="flex flex-1 items-center gap-1.5">{props.children}</div>
    </div>
  );
}

export type SliderType =
  | "default"
  | "opacity"
  | "brightness"
  | "contrast"
  | "saturation"
  | "zoom"
  | "brush-size"
  | "brush-hardness"
  | "brush-strength"
  | "straighten";

export function Slider(props: {
  percent: number;
  value?: number; // Raw value for center-origin calculations (-100..100)
  type?: SliderType;
  accent?: boolean;
}) {
  const type = () => props.type || "default";

  // Center-origin active fill calculation:
  const getCenterFillStyle = () => {
    const rawVal = props.value !== undefined ? props.value : 0;
    if (rawVal >= 0) {
      return {
        left: "50%",
        width: `${rawVal / 2}%`,
      };
    } else {
      const pos = (rawVal + 100) / 2;
      return {
        left: `${pos}%`,
        width: `${50 - pos}%`,
      };
    }
  };

  const getBackgroundStyle = () => {
    switch (type()) {
      case "opacity":
      case "brush-strength":
        return {
          "background-image": `linear-gradient(to right, transparent, var(--color-editor-accent, #E15A17)), conic-gradient(#2d3139 0.25turn, #1a1d24 0.25turn 0.5turn, #2d3139 0.5turn 0.75turn, #1a1d24 0.75turn)`,
          "background-size": "auto, 8px 8px",
        };
      case "brightness":
        return {
          "background-image":
            "linear-gradient(to right, #09090b, #52525b 50%, #ffffff)",
        };
      case "contrast":
        return {
          "background-image":
            "linear-gradient(to right, #09090b, #71717a 50%, #ffffff)",
        };
      case "saturation":
        return {
          "background-image":
            "linear-gradient(to right, #52525b, #71717a 35%, #ef4444 60%, #eab308 80%, #3b82f6 100%)",
        };
      case "brush-hardness":
        return {
          "background-image":
            "linear-gradient(to right, color-mix(in srgb, var(--color-editor-accent) 15%, transparent), var(--color-editor-accent, #E15A17))",
        };
      case "brush-size":
        return {
          "clip-path": "polygon(0% 65%, 100% 15%, 100% 85%, 0% 65%)",
          background: "var(--color-editor-field-border, #343941)",
        };
      default:
        return undefined;
    }
  };

  const isCenterOrigin = () => {
    return ["brightness", "contrast", "saturation", "straighten"].includes(type());
  };

  return (
    <div
      class={clsx(
        "relative flex-1 select-none",
        type() === "brush-size" ? "h-[8px]" : "h-[4px]",
      )}
    >
      {/* Track Background & Fills */}
      <div
        class={clsx(
          "absolute inset-0 overflow-hidden",
          type() !== "brush-size" && "rounded-full",
          !getBackgroundStyle() && "bg-editor-field-border",
        )}
        style={getBackgroundStyle()}
      >
        {/* Center Origin Fills */}
        <Show when={isCenterOrigin()}>
          <div
            class={clsx(
              "absolute inset-y-0 rounded-full",
              type() === "brightness" && "bg-zinc-100 shadow-[0_0_8px_rgba(255,255,255,0.45)]",
              type() === "contrast" && "bg-zinc-300 shadow-[0_0_8px_rgba(212,212,216,0.35)]",
              type() === "saturation" && "bg-gradient-to-r from-pink-500 to-rose-500 shadow-[0_0_8px_rgba(244,63,94,0.45)]",
              type() === "straighten" && "bg-editor-accent shadow-[0_0_8px_rgba(225,90,23,0.45)]",
            )}
            style={getCenterFillStyle()}
          />
          {/* Neutral center tick mark */}
          <div class="absolute left-1/2 top-0 bottom-0 w-px bg-white/45 -translate-x-1/2" />
        </Show>

        {/* Left-Aligned Fills */}
        <Show
          when={
            !isCenterOrigin() &&
            type() !== "opacity" &&
            type() !== "brush-strength"
          }
        >
          <div
            class={clsx(
              "absolute inset-y-0 left-0 rounded-full",
              props.accent !== false ? "bg-editor-accent" : "bg-editor-text/55",
            )}
            style={{ width: `${props.percent}%` }}
          />
        </Show>
      </div>

      {/* Unified Thumb — left uses parent-relative %, translate centers the 12px dot */}
      <div
        class="absolute top-1/2 size-[12px] rounded-full border border-black/55 bg-[#d8dce2] shadow-[0_1px_2px_rgba(0,0,0,0.65),0_0_0_1px_rgba(255,255,255,0.18)] hover:scale-110 pointer-events-none"
        style={{
          left: `${props.percent}%`,
          transform: `translate(-50%, -50%)`,
        }}
      />
    </div>
  );
}

export function EditableNumField(props: {
  label?: string;
  value: number;
  suffix?: string;
  class?: string;
  labelClass?: string;
  disabled?: boolean;
  min?: number;
  max?: number;
  step?: number;
  onSubmit: (val: number) => void;
}) {
  let inputRef: HTMLInputElement | undefined;

  const formatValue = (val: number) => {
    return `${Math.round(val * 100) / 100}`;
  };

  const [text, setText] = createSignal(formatValue(props.value));
  const [editing, setEditing] = createSignal(false);

  createEffect(() => {
    if (!editing()) {
      setText(formatValue(props.value));
    }
  });

  const commit = () => {
    if (!editing()) return;
    const val = text();
    setEditing(false);
    inputRef?.blur();

    let parsed = parseFloat(val);
    if (!isNaN(parsed)) {
      if (props.min !== undefined) parsed = Math.max(props.min, parsed);
      if (props.max !== undefined) parsed = Math.min(props.max, parsed);
      if (Math.abs(parsed - props.value) > 0.0001) {
        props.onSubmit(parsed);
      }
    } else {
      setText(formatValue(props.value));
    }
  };

  const revert = () => {
    setEditing(false);
    setText(formatValue(props.value));
    inputRef?.blur();
  };

  return (
    <div
      class={clsx(
        "flex h-[24px] items-center gap-1 rounded-[3px] border border-editor-field-border bg-editor-field px-1.5 transition-colors",
        "focus-within:border-editor-accent focus-within:ring-1 focus-within:ring-editor-accent/70",
        props.disabled && "opacity-40 pointer-events-none",
        props.class,
      )}
    >
      <Show when={props.label}>
        <span
          class={clsx(
            "text-[10px] font-medium text-editor-text-dim",
            props.labelClass,
          )}
        >
          {props.label}
        </span>
      </Show>

      <input
        ref={inputRef}
        type="text"
        value={text()}
        disabled={props.disabled}
        onFocus={() => {
          if (props.disabled) return;
          setEditing(true);
          setText(formatValue(props.value));
          inputRef?.select();
        }}
        onInput={(e) => {
          setText(e.currentTarget.value);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            commit();
          }
          if (e.key === "Escape") {
            e.preventDefault();
            revert();
          }
          if (e.key === "ArrowUp" || e.key === "ArrowDown") {
            e.preventDefault();
            const parsed = parseFloat(text());
            if (!isNaN(parsed)) {
              const baseStep = props.step ?? 1;
              const step = e.shiftKey ? baseStep * 10 : baseStep;
              let nextVal = e.key === "ArrowUp" ? parsed + step : parsed - step;
              if (props.min !== undefined) nextVal = Math.max(props.min, nextVal);
              if (props.max !== undefined) nextVal = Math.min(props.max, nextVal);
              const formatted = `${Math.round(nextVal * 100) / 100}`;
              setText(formatted);
              props.onSubmit(nextVal);
            }
          }
        }}
        onBlur={commit}
        class="w-full min-w-0 bg-transparent text-[11px] text-editor-text outline-none"
      />
      <Show when={props.suffix}>
        <span class="text-[10px] text-editor-text-dim">{props.suffix}</span>
      </Show>
    </div>
  );
}
