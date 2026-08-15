import { SelectDropdown } from "@/components/editor/shell/OptionBarShared";
import { useI18n } from "./I18nProvider";
import { languageNativeName } from "./languages";

export function LanguageSwitcher() {
  const { locale, locales, setLocale, t } = useI18n();
  const options = locales.map((code) => ({ value: code, label: languageNativeName(code) }));
  return (
    <SelectDropdown<string>
      value={locale()}
      options={options}
      onChange={(v) => setLocale(v)}
      labelPrefix={t("settings.language")}
      class="ml-2"
    />
  );
}
