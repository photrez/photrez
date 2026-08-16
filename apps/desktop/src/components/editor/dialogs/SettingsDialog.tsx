import { useI18n } from "@/i18n/I18nProvider";
import { DesktopDialog, DesktopDialogButton } from "./DesktopDialog";
import { LanguageSwitcher } from "@/i18n/LanguageSwitcher";

export interface SettingsDialogProps {
  onDismiss: () => void;
}

export function SettingsDialog(props: SettingsDialogProps) {
  const { t } = useI18n();
  return (
    <DesktopDialog
      title={t("settings.title")}
      kind="settings"
      widthClass="w-[min(420px,calc(100vw-24px))]"
      onDismiss={props.onDismiss}
      actions={
        <DesktopDialogButton variant="primary" onClick={props.onDismiss}>
          {t("common.ok")}
        </DesktopDialogButton>
      }
    >
      <div class="flex flex-col gap-4 py-1">
        <section class="flex flex-col gap-2">
          <h3 class="text-xs font-semibold uppercase tracking-wide text-editor-icon">
            {t("settings.interface")}
          </h3>
          <LanguageSwitcher />
        </section>
      </div>
    </DesktopDialog>
  );
}
