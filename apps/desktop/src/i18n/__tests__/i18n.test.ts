import { afterEach, beforeEach, describe, expect, it } from "vitest";
import i18n from "../index";

describe("i18n core (en/id)", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    void i18n.changeLanguage("en");
  });

  it("translates known keys per locale", async () => {
    await i18n.changeLanguage("en");
    expect(i18n.t("common.cancel")).toBe("Cancel");
    await i18n.changeLanguage("id");
    expect(i18n.t("common.cancel")).toBe("Batal");
  });

  it("falls back to English for a key missing in the active locale", () => {
    // _fallbackTest only exists in English; with Indonesian active, i18next
    // must fall back to the English resource.
    i18n.addResource("en", "translation", "common._fallbackTest", "Fallback EN");
    void i18n.changeLanguage("id");
    expect(i18n.t("common._fallbackTest")).toBe("Fallback EN");
  });

  it("interpolates count into simple strings", async () => {
    await i18n.changeLanguage("en");
    expect(i18n.t("status.layers", { count: 1 })).toBe("1 layer");
    expect(i18n.t("status.layers", { count: 3 })).toBe("3 layers");
  });

  it("uses the same plural form in Indonesian (no singular/plural split)", async () => {
    await i18n.changeLanguage("id");
    expect(i18n.t("status.layers", { count: 1 })).toBe("1 lapisan");
    expect(i18n.t("status.layers", { count: 5 })).toBe("5 lapisan");
  });
});
