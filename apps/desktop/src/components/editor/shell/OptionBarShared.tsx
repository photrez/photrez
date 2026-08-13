import { Icon, IconName } from "../icons";
import { clsx } from "clsx";
import { Show, For, createSignal, JSX } from "solid-js";
import { Tooltip } from "../Tooltip";

export function ToggleBtn(props: { active: boolean; onChange: (v: boolean) => void; icon: IconName; label: string; labelClass?: string; class?: string }) {
  return (
    <button
      type="button"
      onClick={() => props.onChange(!props.active)}
      class={clsx(
        "flex h-[24px] shrink-0 items-center gap-1.5 rounded-[4px] border px-2 text-[11px] font-medium transition-all duration-75 select-none cursor-pointer focus:outline-none focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-editor-accent/70",
        props.active
          ? "border-editor-accent bg-editor-accent/20 text-white shadow-xs font-semibold"
          : "border-editor-field-border/60 bg-editor-field/40 text-[#A1A1AA] hover:border-editor-field-border hover:bg-editor-field hover:text-white",
        props.class
      )}
    >
      <Icon name={props.icon} class={clsx("size-3", props.active ? "text-editor-accent" : "text-[#A1A1AA]")} strokeWidth={1.5} />
      <span class={props.labelClass}>{props.label}</span>
    </button>
  );
}

export function OptionCheckbox(props: { checked: boolean; onChange: (v: boolean) => void; label: string; class?: string; labelClass?: string }) {
  return (
    <label class={clsx("flex h-[24px] cursor-pointer select-none items-center gap-1.5 rounded-[4px] border border-transparent px-1.5 text-[11px] font-medium transition-colors hover:bg-editor-field/60 hover:border-editor-field-border/60", props.class)}>
      <input
        type="checkbox"
        class="peer sr-only"
        checked={props.checked}
        onChange={(e) => props.onChange(e.currentTarget.checked)}
      />
      <div class={clsx(
        "flex size-3.5 shrink-0 items-center justify-center rounded-[3px] border transition-colors",
        props.checked 
          ? "border-editor-accent bg-editor-accent text-white shadow-xs" 
          : "border-editor-field-border bg-editor-field/80 shadow-inner peer-focus-visible:outline peer-focus-visible:outline-1 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-editor-accent/70"
      )}>
        <Show when={props.checked}>
          <Icon name="check" class="size-3 text-white" strokeWidth={3.5} />
        </Show>
      </div>
      <span class={clsx("select-none transition-colors", props.checked ? "text-white font-semibold" : "text-[#A1A1AA]", props.labelClass)}>{props.label}</span>
    </label>
  );
}


export function Divider() {
  return <div class="h-5 w-px shrink-0 bg-[#2D323C]" />;
}

export function ToolPill(props: { icon: IconName; label: string }) {
  return (
    <div class="flex h-[24px] shrink-0 items-center gap-1.5 rounded-[4px] border border-editor-field-border bg-editor-field px-2 text-[11px] font-semibold text-[#A1A1AA] capitalize select-none">
      <Icon name={props.icon} class="size-3 text-[#A1A1AA]" strokeWidth={1.75} />
      <span class="text-white">{props.label}</span>
    </div>
  );
}

export function MoreDropdown(props: { children: JSX.Element }) {
  const [isOpen, setIsOpen] = createSignal(false);
  return (
    <div class="relative hidden @max-[880px]:flex">
      <Tooltip content="More Options" placement="top">
        <button
          type="button"
          aria-label="More Options"
          onClick={() => setIsOpen(!isOpen())}
          class="flex size-[24px] shrink-0 items-center justify-center rounded-[3px] border border-editor-field-border bg-editor-field text-[#A1A1AA] hover:border-[#4B515D] hover:text-white transition-colors cursor-pointer"
        >
          <Icon name="more" class="size-4" strokeWidth={1.5} />
        </button>
      </Tooltip>
      <Show when={isOpen()}>
        <div class="absolute right-0 top-full z-50 mt-1 flex flex-col gap-2 rounded-[6px] border border-[#363B44] bg-[#1D2026] p-2 shadow-2xl min-w-[150px]">
          <div class="fixed inset-0 z-[-1]" onClick={() => setIsOpen(false)} />
          {props.children}
        </div>
      </Show>
    </div>
  );
}

export interface DropdownOption<T extends string = string> {
  value: T;
  label: string;
  icon?: IconName;
  description?: string;
}

export function SelectDropdown<T extends string = string>(props: {
  value: T;
  options: readonly DropdownOption<T>[];
  onChange: (value: T) => void;
  labelPrefix?: string;
  icon?: IconName;
  class?: string;
  menuWidth?: string;
  disabled?: boolean;
}) {
  const [open, setOpen] = createSignal(false);
  const activeOpt = () => props.options.find((o) => o.value === props.value) ?? props.options[0];

  return (
    <div class={clsx("relative shrink-0 select-none", props.class)}>
      <button
        type="button"
        disabled={props.disabled}
        aria-expanded={open()}
        onClick={() => !props.disabled && setOpen(!open())}
        class={clsx(
          "flex h-[24px] cursor-pointer items-center gap-1.5 rounded-[4px] border border-editor-field-border bg-editor-field px-2 text-[11px] font-medium text-white transition-colors hover:border-[#4B515D] disabled:opacity-40 disabled:pointer-events-none",
          open() && "border-editor-accent bg-editor-field/90"
        )}
      >
        <Show when={props.icon || activeOpt()?.icon}>
          <Icon name={props.icon || activeOpt()!.icon!} class="size-3.5 text-editor-accent" strokeWidth={1.6} />
        </Show>
        <span class="font-medium text-[#A1A1AA]">
          {props.labelPrefix ? `${props.labelPrefix}: ` : ""}
          <span class="font-semibold text-white">{activeOpt()?.label ?? props.value}</span>
        </span>
        <Icon name="chevron-down" class="size-3 text-[#A1A1AA] transition-transform duration-100" strokeWidth={1.6} />
      </button>

      <Show when={open()}>
        <div class="fixed inset-0 z-50" onClick={() => setOpen(false)} />
        <div
          class={clsx(
            "absolute left-0 top-full z-51 mt-1 rounded-[6px] border border-[#363B44] bg-[#1B1D22] p-1 shadow-2xl min-w-[140px] max-h-[280px] overflow-y-auto",
            props.menuWidth
          )}
        >
          <For each={props.options}>
            {(opt) => {
              const isSelected = () => opt.value === props.value;
              return (
                <button
                  type="button"
                  onClick={() => {
                    props.onChange(opt.value);
                    setOpen(false);
                  }}
                  class={clsx(
                    "flex w-full cursor-pointer items-center justify-between gap-2 rounded-[4px] px-2 py-1.5 text-left text-[11px] font-medium transition-colors select-none",
                    isSelected()
                      ? "bg-editor-accent/20 text-white font-semibold"
                      : "text-[#A1A1AA] hover:bg-white/10 hover:text-white"
                  )}
                >
                  <div class="flex items-center gap-2 min-w-0">
                    <Show when={opt.icon}>
                      <Icon
                        name={opt.icon!}
                        class={clsx("size-3.5 shrink-0", isSelected() ? "text-editor-accent" : "text-[#A1A1AA]")}
                        strokeWidth={1.6}
                      />
                    </Show>
                    <span class="truncate">{opt.label}</span>
                  </div>
                  <Show when={isSelected()}>
                    <Icon name="check" class="size-3 text-editor-accent shrink-0" strokeWidth={2.5} />
                  </Show>
                </button>
              );
            }}
          </For>
        </div>
      </Show>
    </div>
  );
}

