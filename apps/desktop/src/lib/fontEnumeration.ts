// Font enumeration for the text tool. The tier order:
//
//   Tier 1 — Tauri native `list_system_fonts` (Rust/fontdb). No permission
//            prompt and no webview gap; this is the desktop-app path.
//   Tier 2 — `window.queryLocalFonts()` (Local Font Access). Web runtime only:
//            the browser shows its own (expected) permission prompt there.
//   Tier 3 — hardcoded WEB_SAFE_FONTS (instant placeholder + last resort).
//
// The old code always tried queryLocalFonts first — inside the desktop app the
// webview can surface a browser-style "let this site see your fonts?" prompt,
// which reads as a website permission in a native app. Arial remains the final
// rasterizer fallback in textRasterizer regardless of this list.
//
// This module stays dependency-light: BOTH runtime checks are lazy imports, so
// importing fontEnumeration never pulls in Tauri bindings.

export interface FontFamily {
  /** Font family name for CSS (e.g. "Arial"). */
  family: string;
  /** Available styles (e.g. ["Regular", "Bold"]). */
  styles: string[];
}

/** Pre-installed fonts across Windows/macOS/Linux defaults. */
export const WEB_SAFE_FONTS: readonly string[] = [
  // Sans-serif
  "Arial",
  "Arial Black",
  "Calibri",
  "Helvetica",
  "Segoe UI",
  "Tahoma",
  "Trebuchet MS",
  "Verdana",
  // Serif
  "Cambria",
  "Georgia",
  "Times New Roman",
  // Monospace
  "Cascadia Code",
  "Consolas",
  "Courier New",
  "Lucida Console",
  // Display
  "Impact",
  // CJK (Windows)
  "Microsoft YaHei",
  "MS Gothic",
  "Malgun Gothic",
] as const;

interface LocalFontData {
  family: string;
  style: string;
}

interface LocalFontsAPI {
  queryLocalFonts?: () => Promise<LocalFontData[]>;
}

function isLocalFontsAvailable(): LocalFontsAPI | null {
  // Guard `window` for worker/node contexts (module is importable anywhere).
  if (typeof window === "undefined") return null;
  const w = window as unknown as LocalFontsAPI;
  return typeof w.queryLocalFonts === "function" ? w : null;
}

/** Deduplicate by family, collect styles, sort alphabetically. Never throws. */
export function deduplicateByFamily(fonts: LocalFontData[]): FontFamily[] {
  const map = new Map<string, Set<string>>();
  for (const f of fonts) {
    if (typeof f.family !== "string" || f.family === "") continue;
    if (!map.has(f.family)) map.set(f.family, new Set());
    map.get(f.family)!.add(typeof f.style === "string" ? f.style : "Regular");
  }
  return Array.from(map.entries())
    .map(([family, styles]) => ({ family, styles: Array.from(styles).sort() }))
    .sort((a, b) => a.family.localeCompare(b.family));
}

/** WEB_SAFE list as FontFamily[], usable synchronously (never blocks the UI). */
export function getInstantFonts(): FontFamily[] {
  return WEB_SAFE_FONTS.map((family) => ({ family, styles: ["Regular"] }));
}

// Cached lazily for the lifetime of the app session (enumerate once, never
// re-enumerate). `cachePromise` dedupes concurrent callers.
let cachedFonts: FontFamily[] | null = null;
let cachePromise: Promise<FontFamily[]> | null = null;

/**
 * Returns the app's font list. Resolves instantly (from cache) after the first
 * call. Never rejects — the text tool must never fail to open because of font
 * enumeration. In the web runtime, calling this OUTSIDE a user gesture can show
 * the Local Font Access permission prompt (see prewarmFonts).
 */
export async function getAvailableFonts(): Promise<FontFamily[]> {
  if (cachedFonts) return cachedFonts;
  if (cachePromise) return cachePromise;

  cachePromise = (async () => {
    const fonts = await enumerateTiered();
    cachedFonts = fonts.length > 0 ? fonts : getInstantFonts();
    return cachedFonts;
  })();

  try {
    return await cachePromise;
  } finally {
    cachePromise = null;
  }
}

async function isTauri(): Promise<boolean> {
  try {
    const { isTauriRuntime } = await import("@/lib/desktop/tauriWindow");
    return isTauriRuntime();
  } catch {
    return false;
  }
}

async function enumerateTiered(): Promise<FontFamily[]> {
  // Tier 1 — Tauri native (no permission prompt).
  if (await isTauri()) {
    try {
      const { listSystemFonts } = await import("@/tauri/native");
      const fonts = await listSystemFonts();
      if (Array.isArray(fonts) && fonts.length > 0) {
        return fonts.map((f) => ({
          family: f.family,
          styles: Array.isArray(f.styles) && f.styles.length > 0 ? f.styles : ["Regular"],
        }));
      }
    } catch {
      // Native command unavailable/failed — do NOT fall through to queryLocalFonts in desktop mode,
      // because queryLocalFonts triggers Chromium's web permission prompt inside the native app window.
      return [];
    }
    return [];
  }

  // Tier 2 — Local Font Access (web runtime; browser permission prompt).
  const api = isLocalFontsAvailable();
  if (api) {
    try {
      const fonts = await api.queryLocalFonts!();
      if (Array.isArray(fonts) && fonts.length > 0) {
        return deduplicateByFamily(fonts);
      }
    } catch {
      // Permission denied / API broken — fall through to WEB_SAFE.
    }
  }

  return [];
}

/**
 * Warm the font cache at app startup. Native-only: in the web runtime this is a
 * no-op so the Local Font Access permission prompt is never triggered outside a
 * user gesture on the font dropdown (which calls getAvailableFonts itself).
 */
export function prewarmFonts(): void {
  // Native-only warm-up: no-op on the web runtime so the Local Font Access
  // permission prompt is never triggered outside a dropdown user gesture.
  void (async () => {
    if (!(await isTauri())) return;
    await getAvailableFonts();
  })();
}

/** Test support: reset the module-level cache (fonts re-enumerated next call). */
export function resetFontCache(): void {
  cachedFonts = null;
  cachePromise = null;
}
