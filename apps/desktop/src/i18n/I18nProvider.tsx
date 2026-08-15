import { createContext, useContext, createSignal, onCleanup, type JSX } from "solid-js";
import i18n, { SUPPORTED_LOCALES, type Locale } from "./index";

export interface I18nValue {
  locale: () => Locale;
  locales: readonly Locale[];
  setLocale: (l: Locale) => void;
  t: (key: string, opts?: Record<string, unknown>) => string;
}

const I18nContext = createContext<I18nValue>();

function normalizeLocale(l: string | undefined): Locale {
  const base = (l ?? "en").split("-")[0];
  return (SUPPORTED_LOCALES as readonly string[]).includes(base) ? (base as Locale) : "en";
}

// Safe default used when no I18nProvider is mounted (e.g. standalone component
// tests that render <AppTitleBar/> without the full provider tree). Keeps
// useI18n() callable everywhere without crashing.
const fallbackI18n: I18nValue = {
  locale: () => normalizeLocale(i18n.resolvedLanguage),
  locales: SUPPORTED_LOCALES,
  setLocale: (l) => {
    void i18n.changeLanguage(l);
  },
  t: (key, opts) => i18n.t(key, opts),
};

export function I18nProvider(props: { children: JSX.Element }) {
  const [locale, setLocaleSignal] = createSignal<Locale>(normalizeLocale(i18n.resolvedLanguage));

  const onLanguageChanged = (l: string) => setLocaleSignal(normalizeLocale(l));
  i18n.on("languageChanged", onLanguageChanged);
  onCleanup(() => i18n.off("languageChanged", onLanguageChanged));

  const setLocale = (l: Locale) => {
    void i18n.changeLanguage(l);
  };

  // Track the locale signal so any component calling t() re-renders when the
  // active language changes.
  const t = (key: string, opts?: Record<string, unknown>) => {
    locale();
    return i18n.t(key, opts);
  };

  const value: I18nValue = {
    locale,
    locales: SUPPORTED_LOCALES,
    setLocale,
    t,
  };

  return <I18nContext.Provider value={value}>{props.children}</I18nContext.Provider>;
}

export function useI18n(): I18nValue {
  return useContext(I18nContext) ?? fallbackI18n;
}
