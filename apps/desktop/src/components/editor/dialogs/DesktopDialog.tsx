import { createUniqueId, onCleanup, onMount, Show, type JSX, type ParentProps } from "solid-js";
import { clsx } from "clsx";

interface DesktopDialogProps extends ParentProps {
  title: string;
  actions?: JSX.Element;
  role?: "dialog" | "alertdialog";
  kind?: string;
  tone?: "default" | "danger";
  widthClass?: string;
  bodyClass?: string;
  manageFocus?: boolean;
  dismissible?: boolean;
  /** When false, the dialog is non-modal: the backdrop is click-through
   *  (pointer-events:none) and does NOT cancel on outside click. Used by
   *  the color picker so the canvas behind it stays interactive. */
  modal?: boolean;
  onDismiss?: () => void;
  dialogRef?: (element: HTMLDivElement) => void;
  onKeyDown?: JSX.EventHandler<HTMLDivElement, KeyboardEvent>;
  onBackdropPointerDown?: () => void;
}

export function DesktopDialog(props: DesktopDialogProps) {
  const id = createUniqueId();
  const titleId = `photrez-dialog-title-${id}`;
  const descriptionId = `photrez-dialog-description-${id}`;
  let dialogElement!: HTMLDivElement;
  let restoreFocusTo: HTMLElement | null = null;

  onMount(() => {
    if (!props.manageFocus) return;
    restoreFocusTo = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    queueMicrotask(() => {
      const initial = dialogElement.querySelector<HTMLElement>(
        "[data-dialog-initial-focus], button:not([aria-label='Close']):not(:disabled), input:not(:disabled), select:not(:disabled)",
      );
      initial?.focus();
    });
  });

  onCleanup(() => {
    if (props.manageFocus) restoreFocusTo?.focus();
  });

  const handleKeyDown: JSX.EventHandler<HTMLDivElement, KeyboardEvent> = (event) => {
    props.onKeyDown?.(event);
    if (event.defaultPrevented || !props.manageFocus) return;
    if (event.key === "Escape" && props.dismissible !== false) {
      event.preventDefault();
      props.onDismiss?.();
      return;
    }
    if (event.key !== "Tab") return;
    const focusable = Array.from(dialogElement.querySelectorAll<HTMLElement>(
      "button:not([aria-label='Close']):not(:disabled), input:not(:disabled), select:not(:disabled), [tabindex]:not([aria-label='Close']):not([tabindex='-1'])",
    ));
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };

  return (
    <>
      <div
        class={clsx("fixed inset-0 z-[89]", props.modal === false ? "pointer-events-none" : "bg-transparent")}
        data-dialog-backdrop
        onPointerDown={props.modal === false ? undefined : props.onBackdropPointerDown}
      />
      <div
        ref={(element) => {
          dialogElement = element;
          props.dialogRef?.(element);
        }}
        role={props.role ?? "dialog"}
        aria-modal={props.modal === false ? "false" : "true"}
        aria-labelledby={titleId}
        aria-describedby={descriptionId}
        data-dialog-kind={props.kind}
        data-dialog-tone={props.tone ?? "default"}
        data-photrez-dialog
        class={clsx(
          "fixed left-1/2 top-1/2 z-[90] -translate-x-1/2 -translate-y-1/2 overflow-hidden rounded-[8px] border border-editor-field-border bg-editor-panel shadow-[0_18px_50px_rgba(0,0,0,0.55)] animate-dialog-fade-in",
          props.widthClass ?? "w-[min(360px,calc(100vw-24px))]",
        )}
        onKeyDown={handleKeyDown}
      >
        <div
          data-dialog-titlebar
          class="flex h-9 items-center justify-between border-b border-editor-divider bg-editor-topbar px-3"
        >
          <div class="flex items-center gap-2">
            <svg viewBox="0 0 512 512" class="size-[18px] shrink-0" aria-hidden="true">
              <defs>
                <linearGradient id="brandGradientDialog" x1="0%" y1="0%" x2="0%" y2="100%">
                  <stop offset="0%" stop-color="#FFB31A" />
                  <stop offset="100%" stop-color="#E15A17" />
                </linearGradient>
                <linearGradient id="bgDarkDialog" x1="0%" y1="0%" x2="0%" y2="100%">
                  <stop offset="0%" stop-color="#2C2C2E" />
                  <stop offset="100%" stop-color="#151516" />
                </linearGradient>
                <mask id="mountainMaskDialog" maskUnits="userSpaceOnUse" x="-500" y="-500" width="2000" height="2000">
                  <rect x="-500" y="-500" width="2000" height="2000" fill="white" />
                  <polygon points="250.4,320 300.86,127 307,127 357.6,320" fill="black" />
                </mask>
                <filter id="pShadowDialog" x="-20%" y="-20%" width="150%" height="150%">
                  <feDropShadow dx="0" dy="16" stdDeviation="16" flood-color="#000000" flood-opacity="0.5" />
                </filter>
              </defs>
              <rect x="24" y="24" width="464" height="464" rx="100" fill="url(#bgDarkDialog)" />
              <g transform="translate(8, 48) scale(0.85)" filter="url(#pShadowDialog)">
                <path d="M 240 50 L 460 50 L 390 310 L 253 310 L 219 440 L 136 440 Z" fill="url(#brandGradientDialog)" mask="url(#mountainMaskDialog)" />
                <circle cx="333" cy="175" r="30" fill="#FFE57F" mask="url(#mountainMaskDialog)" />
              </g>
            </svg>
            <h2 id={titleId} class="text-[12px] font-medium tracking-[0.01em] text-editor-text">
              {props.title}
            </h2>
          </div>
          <button
            onClick={(e) => {
              if (props.dismissible !== false) {
                // Split the call — onDismiss takes no args while
                // onBackdropPointerDown takes the event (avoids a union cast).
                if (props.onDismiss) {
                  props.onDismiss();
                } else {
                  props.onBackdropPointerDown?.();
                }
              }
            }}
            class="flex size-5 items-center justify-center rounded-[4px] text-editor-icon hover:bg-white/[0.08] hover:text-editor-text transition-colors cursor-pointer disabled:opacity-30"
            aria-label="Close"
            tabindex="-1"
            disabled={props.dismissible === false}
          >
            <svg class="size-2.5" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
              <line x1="3" y1="3" x2="13" y2="13" />
              <line x1="13" y1="3" x2="3" y2="13" />
            </svg>
          </button>
        </div>
        <div
          id={descriptionId}
          data-dialog-body
          class={clsx("px-4 py-4 text-[12px] leading-[18px] text-editor-text", props.bodyClass)}
        >
          {props.children}
        </div>
        <Show when={props.actions}>
          <div
            data-dialog-actions
            class="flex min-h-11 items-center justify-end gap-2 border-t border-editor-divider bg-editor-topbar/65 px-3 py-2"
          >
            {props.actions}
          </div>
        </Show>
      </div>
    </>
  );
}

interface DesktopDialogButtonProps extends ParentProps {
  onClick: JSX.EventHandler<HTMLButtonElement, MouseEvent>;
  type?: "button" | "submit";
  variant?: "primary" | "secondary";
  disabled?: boolean;
  ref?: (element: HTMLButtonElement) => void;
  class?: string;
  [key: `data-${string}`]: string | boolean | undefined;
}

export function DesktopDialogButton(props: DesktopDialogButtonProps) {
  return (
    <button
      ref={props.ref}
      type={props.type ?? "button"}
      disabled={props.disabled}
      class={clsx(
        "h-7 min-w-16 rounded-[6px] border px-3 text-[11px] font-medium outline-none transition-colors duration-75 disabled:cursor-not-allowed disabled:opacity-40 focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-editor-accent/70",
        props.variant === "primary"
          ? "border-editor-accent bg-editor-accent text-black/90 font-semibold hover:bg-editor-accent/90"
          : "border-editor-field-border bg-editor-field text-editor-text hover:bg-white/[0.06] hover:border-editor-accent/50 focus-visible:border-editor-accent",
        props.class,
      )}
      onClick={props.onClick}
      {...Object.fromEntries(Object.entries(props).filter(([key]) => key.startsWith("data-")))}
    >
      {props.children}
    </button>
  );
}

export const desktopDialogFieldClass =
  "h-7 w-full rounded-[4px] border border-editor-field-border bg-editor-field px-2 text-[12px] text-editor-text outline-none shadow-[inset_0_1px_2px_rgba(0,0,0,0.35)] focus:border-editor-accent focus:ring-1 focus:ring-editor-accent/30 disabled:cursor-not-allowed disabled:opacity-40";
