import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@/lib/desktop/tauriWindow", () => ({
  isTauriRuntime: vi.fn(),
}));

vi.mock("@/tauri/native", () => ({
  listSystemFonts: vi.fn(),
}));

import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { listSystemFonts } from "@/tauri/native";
import {
  getAvailableFonts,
  getInstantFonts,
  prewarmFonts,
  resetFontCache,
  WEB_SAFE_FONTS,
} from "../fontEnumeration";

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("fontEnumeration tiering", () => {
  beforeEach(() => {
    resetFontCache();
    vi.mocked(isTauriRuntime).mockReturnValue(false);
    vi.mocked(listSystemFonts).mockReset();
    delete (window as unknown as { queryLocalFonts?: unknown }).queryLocalFonts;
  });

  afterEach(() => {
    delete (window as unknown as { queryLocalFonts?: unknown }).queryLocalFonts;
    vi.unstubAllGlobals();
  });

  it("uses the Tauri native command in the desktop runtime and never calls queryLocalFonts", async () => {
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    vi.mocked(listSystemFonts).mockResolvedValue([
      { family: "Arial", styles: ["Regular"] },
      { family: "Zapf Dingbats", styles: ["Bold"] },
    ]);
    const querySpy = vi.fn().mockResolvedValue([]);
    (window as unknown as { queryLocalFonts: unknown }).queryLocalFonts = querySpy;

    const fonts = await getAvailableFonts();

    expect(listSystemFonts).toHaveBeenCalledTimes(1);
    expect(querySpy).not.toHaveBeenCalled();
    expect(fonts).toEqual([
      { family: "Arial", styles: ["Regular"] },
      { family: "Zapf Dingbats", styles: ["Bold"] },
    ]);
  });

  it("falls back to the web tier (queryLocalFonts) when the native command fails", async () => {
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    vi.mocked(listSystemFonts).mockRejectedValue(new Error("E_FONT_ENUM"));
    (window as unknown as { queryLocalFonts: unknown }).queryLocalFonts = vi
      .fn()
      .mockResolvedValue([{ family: "Segoe UI", style: "Regular" }]);

    const fonts = await getAvailableFonts();

    expect(fonts).toEqual([{ family: "Segoe UI", styles: ["Regular"] }]);
  });

  it("falls back to WEB_SAFE when both tiers fail", async () => {
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    vi.mocked(listSystemFonts).mockRejectedValue(new Error("down"));
    // No queryLocalFonts on window either.

    const fonts = await getAvailableFonts();

    expect(fonts).toHaveLength(WEB_SAFE_FONTS.length);
    expect(fonts[0].family).toBe("Arial");
  });

  it("caches after the first call (native enumerated once)", async () => {
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    vi.mocked(listSystemFonts).mockResolvedValue([{ family: "Arial", styles: ["Regular"] }]);

    const first = await getAvailableFonts();
    const second = await getAvailableFonts();

    expect(listSystemFonts).toHaveBeenCalledTimes(1);
    expect(second).toBe(first);
  });

  it("getInstantFonts returns the WEB_SAFE list synchronously (never blocks the UI)", () => {
    const instant = getInstantFonts();
    expect(instant).toHaveLength(WEB_SAFE_FONTS.length);
    expect(instant[0]).toEqual({ family: "Arial", styles: ["Regular"] });
  });

  it("prewarmFonts is a no-op in the web runtime (no prompt outside a gesture)", async () => {
    vi.mocked(isTauriRuntime).mockReturnValue(false);
    const querySpy = vi.fn().mockResolvedValue([{ family: "Arial", style: "Regular" }]);
    (window as unknown as { queryLocalFonts: unknown }).queryLocalFonts = querySpy;

    prewarmFonts();
    await tick();

    expect(querySpy).not.toHaveBeenCalled();
    expect(listSystemFonts).not.toHaveBeenCalled();
  });

  it("prewarmFonts warms the native cache in the desktop runtime", async () => {
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    vi.mocked(listSystemFonts).mockResolvedValue([{ family: "Arial", styles: ["Regular"] }]);

    prewarmFonts();
    await tick();

    expect(listSystemFonts).toHaveBeenCalledTimes(1);
    // A later call resolves instantly from the warm cache.
    const fonts = await getAvailableFonts();
    expect(fonts).toEqual([{ family: "Arial", styles: ["Regular"] }]);
    expect(listSystemFonts).toHaveBeenCalledTimes(1);
  });

  it("web tier dedupes families and sorts styles (queryLocalFonts path)", async () => {
    (window as unknown as { queryLocalFonts: unknown }).queryLocalFonts = vi.fn().mockResolvedValue([
      { family: "B Font", style: "Bold" },
      { family: "A Font", style: "Regular" },
      { family: "A Font", style: "Bold" },
    ]);

    const fonts = await getAvailableFonts();

    expect(fonts).toEqual([
      { family: "A Font", styles: ["Bold", "Regular"] },
      { family: "B Font", styles: ["Bold"] },
    ]);
  });
});
