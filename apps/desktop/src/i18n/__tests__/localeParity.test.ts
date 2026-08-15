import { describe, expect, it } from "vitest";
import { resources } from "../index";

// Contributor safety net: every non-English locale must contain the exact same
// set of keys as the English reference. Missing keys are caught here (and in CI)
// instead of silently falling back to English at runtime.
function collectKeys(obj: unknown, prefix = ""): string[] {
  if (!obj || typeof obj !== "object" || Array.isArray(obj)) return [];
  const keys: string[] = [];
  for (const [k, v] of Object.entries(obj)) {
    const full = prefix ? `${prefix}.${k}` : k;
    if (v && typeof v === "object" && !Array.isArray(v)) {
      keys.push(...collectKeys(v, full));
    } else {
      keys.push(full);
    }
  }
  return keys;
}

const baseKeys = new Set(collectKeys((resources.en as { translation: object }).translation));

describe("locale key parity", () => {
  it("English reference locale exists and is non-empty", () => {
    expect(baseKeys.size).toBeGreaterThan(0);
  });

  for (const locale of SUPPORTED_LOCALES_EXCLUDING_EN()) {
    it(`locale '${locale}' has no missing keys vs English`, () => {
      const translation = (resources[locale] as { translation: object }).translation;
      const keys = new Set(collectKeys(translation));
      const missing = [...baseKeys].filter((k) => !keys.has(k));
      expect(missing, `missing keys in ${locale}: ${missing.join(", ")}`).toEqual([]);
    });
  }
});

function SUPPORTED_LOCALES_EXCLUDING_EN(): string[] {
  return Object.keys(resources)
    .filter((l) => l !== "en")
    .sort();
}
