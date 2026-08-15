// Native (endonym) display name for a locale code, e.g. "en" -> "English",
// "id" -> "Indonesia", "fr" -> "Français". Used by the language switcher so
// contributors never have to hardcode language names — drop a locale file and
// the switcher labels it automatically via Intl.
export function languageNativeName(code: string): string {
  try {
    const displayNames = new Intl.DisplayNames([code], { type: "language" });
    const name = displayNames.of(code);
    if (name) return name;
  } catch {
    /* unsupported/unrecognized code — fall back to the raw code below */
  }
  return code;
}
