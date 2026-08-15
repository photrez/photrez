import i18n from "i18next";
import LanguageDetector from "i18next-browser-languagedetector";
import type { Resource } from "i18next";

// Auto-discover every locale catalog so adding a language is zero-code: a
// contributor only drops `locales/<code>/translation.json`. `en` is the
// reference/base locale and must always exist.
const localeModules = import.meta.glob("./locales/*/translation.json", { eager: true });

export const resources: Record<string, { translation: object }> = {};
for (const [path, mod] of Object.entries(localeModules)) {
  const code = path.match(/locales\/([^/]+)\/translation\.json$/)?.[1];
  if (code) resources[code] = { translation: (mod as { default: unknown }).default as object };
}

export const SUPPORTED_LOCALES = Object.keys(resources).sort();
export type Locale = string;

void i18n.use(LanguageDetector).init({
  resources: resources as unknown as Resource,
  fallbackLng: "en",
  supportedLngs: SUPPORTED_LOCALES,
  nonExplicitSupportedLngs: true,
  load: "languageOnly",
  interpolation: { escapeValue: false },
  detection: {
    order: ["localStorage", "navigator", "htmlTag"],
    caches: ["localStorage"],
    lookupLocalStorage: "photrez.locale",
  },
});

export default i18n;
