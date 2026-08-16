import { Show } from "solid-js";
import { useI18n } from "@/i18n/I18nProvider";

export function CropModeIndicator(props: { isActive: boolean }) {
  const { t } = useI18n();

  return (
    <Show when={props.isActive}>
      <div class="absolute top-4 left-1/2 -translate-x-1/2 z-[200] flex items-center gap-3 px-3 py-1.5 bg-editor-panel border border-zinc-800 rounded-[3px] shadow-2xl pointer-events-none">
        <div class="size-1.5 rounded-full bg-green-600 shadow-[0_0_8px_rgba(0,143,81,0.5)]" />
        <span class="text-[9px] font-bold text-white/90 uppercase tracking-widest">{t("crop.cropMode", "Crop Mode")}</span>
        <div class="w-[1px] h-3 bg-zinc-800 mx-1" />
        <span class="text-[9px] font-bold text-zinc-400"><kbd class="px-1.5 py-0.5 bg-zinc-800 border border-zinc-700 rounded-[2px] text-zinc-100 font-sans mr-1">Enter</kbd>{t("crop.crop", "Crop")}</span>
        <span class="text-[9px] font-bold text-zinc-400"><kbd class="px-1.5 py-0.5 bg-zinc-800 border border-zinc-700 rounded-[2px] text-zinc-100 font-sans mr-1">Esc</kbd>{t("common.cancel", "Cancel")}</span>
      </div>
    </Show>
  );
}
