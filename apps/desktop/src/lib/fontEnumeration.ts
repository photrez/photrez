// Font enumeration for the text tool. Pure module by design (extraction-first):
// no engine/UI/Tauri imports, so it can be lifted into a standalone library.
//
// Strategy (plan §8): Tier 1 `window.queryLocalFonts()` (Local Font Access),
// Tier 2 hardcoded WEB_SAFE_FONTS list. The Rust `list_system_fonts` command is
// deliberately NOT used until proven needed (cuts a new Tauri command; the
// web-safe list covers Windows/macOS/Linux defaults). Arial is the final
// rasterizer fallback in textRasterizer regardless of this list.

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

// Cached lazily for the lifetime of the app session (plan §8.4: enumerate on
// first text-tool activation, never re-enumerate).
let cachedFonts: FontFamily[] | null = null;
let cachePromise: Promise<FontFamily[]> | null = null;

/**
 * Returns the app's font list. Resolves instantly (from cache) after the first
 * call. When queryLocalFonts is unavailable or the permission is denied, falls
 * back to the WEB_SAFE list. Never rejects — the text tool must never fail to
 * open because of font enumeration.
 */
export async function getAvailableFonts(): Promise<FontFamily[]> {
  if (cachedFonts) return cachedFonts;
  if (cachePromise) return cachePromise;

  cachePromise = (async () => {
    const api = isLocalFontsAvailable();
    if (api) {
      try {
        const fonts = await api.queryLocalFonts!();
        if (Array.isArray(fonts) && fonts.length > 0) {
          cachedFonts = deduplicateByFamily(fonts);
          return cachedFonts;
        }
      } catch {
        // Permission denied / API broken — fall through to WEB_SAFE.
      }
    }
    cachedFonts = WEB_SAFE_FONTS.map((family) => ({ family, styles: ["Regular"] }));
    return cachedFonts;
  })();

  try {
    return await cachePromise;
  } finally {
    cachePromise = null;
  }
}

/** Test support: reset the module-level cache (fonts re-enumerated next call). */
export function resetFontCache(): void {
  cachedFonts = null;
  cachePromise = null;
}
