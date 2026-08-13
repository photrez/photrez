import { createSignal, onMount } from "solid-js";
import { getVersion } from "@tauri-apps/api/app";
import { DesktopDialog, DesktopDialogButton } from "./DesktopDialog";

export interface AboutDialogProps {
  onDismiss: () => void;
}

export function AboutDialog(props: AboutDialogProps) {
  const [version, setVersion] = createSignal("0.1.0");

  onMount(() => {
    void getVersion()
      .then((v) => setVersion(v))
      .catch(() => setVersion("0.1.0"));
  });

  const openLink = (url: string) => {
    window.open(url, "_blank", "noopener,noreferrer");
  };

  return (
    <DesktopDialog
      title="About Photrez"
      kind="about"
      widthClass="w-[min(400px,calc(100vw-24px))]"
      onDismiss={props.onDismiss}
      actions={
        <DesktopDialogButton variant="primary" onClick={props.onDismiss}>
          Close
        </DesktopDialogButton>
      }
    >
      <div class="flex flex-col gap-4 py-1">
        {/* Header & Logo */}
        <div class="flex items-center gap-3.5">
          <div class="flex size-12 shrink-0 items-center justify-center rounded-[8px] border border-editor-divider bg-[#151516] p-2 shadow-inner">
            <svg viewBox="0 0 512 512" class="size-8 shrink-0" aria-hidden="true">
              <defs>
                <linearGradient id="brandGradientAbout" x1="0%" y1="0%" x2="0%" y2="100%">
                  <stop offset="0%" stop-color="#FFB31A" />
                  <stop offset="100%" stop-color="var(--color-editor-accent)" />
                </linearGradient>
                <linearGradient id="bgDarkAbout" x1="0%" y1="0%" x2="0%" y2="100%">
                  <stop offset="0%" stop-color="#2C2C2E" />
                  <stop offset="100%" stop-color="#151516" />
                </linearGradient>
                <mask id="mountainMaskAbout" maskUnits="userSpaceOnUse" x="-500" y="-500" width="2000" height="2000">
                  <rect x="-500" y="-500" width="2000" height="2000" fill="white" />
                  <polygon points="250.4,320 300.86,127 307,127 357.6,320" fill="black" />
                </mask>
                <filter id="pShadowAbout" x="-20%" y="-20%" width="150%" height="150%">
                  <feDropShadow dx="0" dy="16" stdDeviation="16" flood-color="#000000" flood-opacity="0.5" />
                </filter>
              </defs>
              <rect x="24" y="24" width="464" height="464" rx="100" fill="url(#bgDarkAbout)" />
              <g transform="translate(8, 48) scale(0.85)" filter="url(#pShadowAbout)">
                <path d="M 240 50 L 460 50 L 390 310 L 253 310 L 219 440 L 136 440 Z" fill="url(#brandGradientAbout)" mask="url(#mountainMaskAbout)" />
                <circle cx="333" cy="175" r="30" fill="#FFE57F" mask="url(#mountainMaskAbout)" />
              </g>
            </svg>
          </div>
          <div>
            <div class="flex items-center gap-2">
              <h3 class="text-[15px] font-bold tracking-tight text-white">Photrez</h3>
              <span class="rounded-[4px] border border-editor-accent/40 bg-editor-accent/10 px-1.5 py-0.5 text-[10px] font-semibold text-editor-accent">
                v{version()}
              </span>
            </div>
            <p class="mt-0.5 text-[11px] leading-tight text-editor-text-subtle">
              Native desktop image studio — Lean. Offline. Under 80 MB.
            </p>
          </div>
        </div>

        {/* Technical Architecture Specs */}
        <div class="rounded-[6px] border border-editor-field-border bg-[#151516] p-2.5 text-[11px]">
          <div class="grid grid-cols-2 gap-y-1.5 text-editor-text-subtle">
            <div>
              Core Engine: <span class="font-medium text-editor-text">Rust (photrez-core)</span>
            </div>
            <div>
              Rasterizer: <span class="font-medium text-editor-text">WebGL 2D</span>
            </div>
            <div>
              Runtime Shell: <span class="font-medium text-editor-text">Tauri v2 + SolidJS</span>
            </div>
            <div>
              License: <span class="font-medium text-editor-text">AGPLv3 Open-Source</span>
            </div>
          </div>
        </div>

        {/* Quick Link Buttons */}
        <div class="flex items-center gap-2">
          <button
            type="button"
            onClick={() => openLink("https://github.com/photrez/photrez")}
            class="flex h-7 flex-1 items-center justify-center gap-1.5 rounded-[4px] border border-editor-field-border bg-editor-field text-[11px] font-medium text-editor-text transition-colors hover:bg-white/[0.08] hover:border-editor-accent/40 cursor-pointer"
          >
            <svg class="size-3.5 text-editor-icon" viewBox="0 0 24 24" fill="currentColor">
              <path fill-rule="evenodd" clip-rule="evenodd" d="M12 2C6.477 2 2 6.484 2 12.017c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.008-.868-.013-1.703-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.466-1.11-1.466-.908-.62.069-.608.069-.608 1.003.07 1.53 1.032 1.53 1.032.892 1.53 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.113-4.555-4.951 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0112 6.844c.85.004 1.705.115 2.504.337 1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.943.359.309.678.92.678 1.855 0 1.338-.012 2.419-.012 2.747 0 .268.18.58.688.482A10.019 10.019 0 0022 12.017C22 6.484 17.522 2 12 2z" />
            </svg>
            GitHub Repository
          </button>
          <button
            type="button"
            onClick={() => openLink("https://photrez.github.io")}
            class="flex h-7 flex-1 items-center justify-center gap-1.5 rounded-[4px] border border-editor-field-border bg-editor-field text-[11px] font-medium text-editor-text transition-colors hover:bg-white/[0.08] hover:border-editor-accent/40 cursor-pointer"
          >
            <svg class="size-3.5 text-editor-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="12" cy="12" r="10" />
              <line x1="2" y1="12" x2="22" y2="12" />
              <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
            </svg>
            Official Website
          </button>
        </div>
      </div>
    </DesktopDialog>
  );
}
