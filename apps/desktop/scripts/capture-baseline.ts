/**
 * Capture the golden-master baseline set: PNGs of the app's UI chrome, plus a
 * manifest that records what each shot shows and where it came from.
 *
 * The 74 shot definitions (ids, categories, selectors, needsDoc, notes) live in
 * ./baseline-shots.json beside this script; the interaction recipes are the
 * PLANS table below.
 *
 * Conventions this capture enforces:
 *  - the app locale is pinned to "en" before boot (addInitScript)
 *  - one fresh page load per shot (state never leaks between shots)
 *  - tooltips suppressed by default (`[role="tooltip"]{display:none}`),
 *    unless the shot is one of the three tooltip shots (allowTooltip)
 *  - failures of native-bridge `invoke` calls are classified as expected noise
 *    (`tauri-invoke-unavailable`), not problems - in a browser there is no
 *    native host and every such call rejects on every shot
 *  - a failing shot is recorded and the run continues; exit code 1 if any failed
 *  - a blank document is opened first when the shot needs one
 *
 * Output: <out>/shots/<id>.png + <out>/shots/manifest.json with provenance
 * (sourceCommit, locale, host, toolVersions, scripts) + a summary on stdout.
 *
 * Usage (from apps/desktop; the dev server must be up):
 *   bun run scripts/capture-baseline.ts --out <dir>              # all shots
 *   bun run scripts/capture-baseline.ts --out <dir> --only shell-titlebar
 *   bun run scripts/capture-baseline.ts --out <dir> --only dialog,toast
 *   bun run scripts/capture-baseline.ts --out <dir> --compare-boxes <manifest.json>
 *     # optional: record box drift of each shot against an earlier manifest
 */
import { chromium, type Browser, type Locator, type Page } from "@playwright/test";
import { createHash } from "node:crypto";
import { execSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import * as os from "node:os";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..", "..");
const SHOTS_PATH = path.join(SCRIPT_DIR, "baseline-shots.json");
const VIEWPORT = { width: 1366, height: 820 };

// ---------------------------------------------------------------- args

function argValue(flag: string): string | undefined {
  const i = process.argv.indexOf(flag);
  // PowerShell can pass a quoted token through verbatim; strip wrapping quotes.
  return i >= 0 ? process.argv[i + 1]?.replace(/^"+|"+$/g, "") : undefined;
}

const OUT_DIR = argValue("--out");
if (!OUT_DIR) {
  console.error("--out <dir> is required (the capture output directory)");
  process.exit(2);
}
const BASE_URL = argValue("--url") ?? "http://127.0.0.1:1420";
const ONLY = argValue("--only")
  ?.split(/[\s,]+/) // PowerShell joins a comma list with spaces
  .map((s) => s.trim())
  .filter(Boolean);

// ---------------------------------------------------------------- shots + compare boxes

interface BaselineShot {
  id: string;
  category: string;
  needsDoc: boolean;
  selector: string | null;
  note: string;
}

interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

const SHOTS: BaselineShot[] = JSON.parse(readFileSync(SHOTS_PATH, "utf8").replace(/^\uFEFF/, ""));

/**
 * Optional: an earlier manifest to report per-shot box drift against (records
 * dx/dy/dw/dh when a delta exceeds 4 px). The comparison target is passed in,
 * never hard-coded, so this script carries no reference to local-only files.
 */
const COMPARE_BOXES_PATH = argValue("--compare-boxes");
const compareBoxes = new Map<string, Box>();
if (COMPARE_BOXES_PATH) {
  const earlier = JSON.parse(readFileSync(COMPARE_BOXES_PATH, "utf8").replace(/^\uFEFF/, "")) as {
    shots?: { id: string; box?: Box }[];
  };
  for (const s of earlier.shots ?? []) if (s.box) compareBoxes.set(s.id, s.box);
}

// ---------------------------------------------------------------- browser-side helpers

const RAIL = "aside:not([class*='border-l'])";
const RAIL_SWATCH = "div[class*='size-[38px]']";
const OPTION_BAR = "div[class*='@container']";

/** Call `__photrezEditor.<method>(...args)` on the page. */
async function facade(page: Page, method: string, ...args: unknown[]): Promise<void> {
  await page.evaluate(
    ([m, a]) => {
      const editor = (window as unknown as { __photrezEditor?: Record<string, (...xs: unknown[]) => unknown> })
        .__photrezEditor;
      if (!editor) throw new Error("__photrezEditor debug handle is not exposed (is this a dev build?)");
      const fn = editor[m as string];
      if (typeof fn !== "function") throw new Error(`__photrezEditor.${m} is not a function`);
      fn(...(a as unknown[]));
    },
    [method, args] as const,
  );
}

async function editorCommand(page: Page, command: string): Promise<void> {
  await page.evaluate((c) => {
    window.dispatchEvent(new CustomEvent("photrez://editor-command", { detail: c }));
  }, command);
}

async function railDisabled(page: Page): Promise<boolean> {
  return page.locator(RAIL).first().evaluate((el) => {
    const cls = el.className;
    return cls.includes("opacity-50") || cls.includes("grayscale");
  });
}

async function createBlankCanvas(page: Page): Promise<void> {
  await page.getByRole("button", { name: "New Document", exact: true }).click();
  const dialog = page.locator("[data-dialog-kind='new-document']");
  await dialog.waitFor({ state: "visible", timeout: 10_000 });
  await dialog.locator("input[type='text'], input:not([type])").first().fill("Baseline");
  const numInputs = dialog.locator("input[type='number']");
  await numInputs.nth(0).fill("800");
  await numInputs.nth(1).fill("600");
  await page.locator("[data-dialog-confirm]").click();
  await page.locator("[data-document-tab]").first().waitFor({ state: "visible", timeout: 20_000 });
  await page.waitForTimeout(300);
}

async function openMenu(page: Page, name: string): Promise<void> {
  const id = `app-menu-${name}`;
  const trigger = page.locator(`[aria-controls='${id}']`).first();
  await trigger.click();
  const popup = page.locator(`#${id}`);
  try {
    await popup.waitFor({ state: "visible", timeout: 800 });
  } catch {
    // Some menubars open on hover; retry with a hover before giving up.
    await trigger.hover();
    await popup.waitFor({ state: "visible", timeout: 2_000 });
  }
  await page.waitForTimeout(120);
}

async function hoverFirstMenuItem(page: Page, menu: string): Promise<void> {
  const item = page.locator(`#app-menu-${menu} [role='menuitem']:not(:disabled)`).first();
  await item.hover();
  await page.waitForTimeout(120);
}

async function clickCanvas(page: Page, button: "right"): Promise<void> {
  const canvas = page.locator("#canvas-container").first();
  const box = await canvas.boundingBox();
  if (!box) throw new Error("canvas container has no bounding box");
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2, { button });
  await page.waitForTimeout(200);
}

async function pressHold(page: Page, locator: Locator): Promise<void> {
  const box = await locator.boundingBox();
  if (!box) throw new Error("press target has no bounding box");
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.waitForTimeout(150);
}

async function toast(page: Page, severity: "info" | "warn" | "error", count = 1): Promise<void> {
  const messages: Record<string, string> = {
    info: "Baseline capture: info toast",
    warn: "Baseline capture: warning toast",
    error: "Baseline capture: error toast",
  };
  const calls = Array.from({ length: count }, () => `m.showToast(${JSON.stringify(messages[severity]!)}, ${JSON.stringify(severity)})`).join("; ");
  // The bare specifier /src/components/editor/Toast resolves to a SECOND module
  // instance in dev (different module key than the app's Toast.tsx), so the
  // app's ToastHost never sees the signal. Reuse the exact URL the app loaded.
  await page.evaluate(`(async () => {
    const url = performance.getEntriesByType("resource").map((e) => e.name).find((n) => n.includes("/src/components/editor/Toast"))
      ?? "/src/components/editor/Toast.tsx";
    const m = await import(url);
    ${calls}
  })()`);
  await page.waitForTimeout(250);
}

// ---------------------------------------------------------------- per-shot recipes

interface ShotPlan {
  /** Runs after the document precondition (when needsDoc). */
  action?: (page: Page) => Promise<void>;
  /** Runs after the screenshot (mouse release et al). */
  after?: (page: Page) => Promise<void>;
  allowTooltip?: boolean;
}

const TOOL_BY_SHOT: Record<string, string> = {
  "option-move": "move",
  "option-selection": "selection",
  "option-crop": "crop",
  "option-eyedropper": "eyedropper",
  "option-brush": "brush",
  "option-eraser": "eraser",
  "option-paintBucket": "paintBucket",
  "option-gradient": "gradient",
  "option-shape": "shape",
  "option-text": "text",
};

const MENU_BY_SHOT: Record<string, string> = {
  "menu-file": "file",
  "menu-edit": "edit",
  "menu-image": "image",
  "menu-layer": "layer",
  "menu-view": "view",
  "menu-window": "window",
  "menu-help": "help",
  "menu-settings": "settings",
};

const railTool = (page: Page, name: string): Locator => page.getByRole("button", { name }).first();

const PLANS: Record<string, ShotPlan> = {};

// shell -----------------------------------------------------------------
PLANS["chrome-hidden"] = {
  action: async (page) => {
    await facade(page, "setChromeVisible", false);
    await page.waitForTimeout(250);
  },
};
// rail-disabled-no-doc: no doc on purpose; nothing extra.
// menu ------------------------------------------------------------------
for (const [shotId, menu] of Object.entries(MENU_BY_SHOT)) {
  PLANS[shotId] = { action: (page) => openMenu(page, menu) };
}
PLANS["menu-file-hover-item"] = {
  action: async (page) => {
    await openMenu(page, "file");
    await hoverFirstMenuItem(page, "file");
  },
};
PLANS["menu-settings-hover-item"] = {
  action: async (page) => {
    await openMenu(page, "settings");
    await hoverFirstMenuItem(page, "settings");
  },
};
// optionbar -------------------------------------------------------------
for (const [shotId, tool] of Object.entries(TOOL_BY_SHOT)) {
  PLANS[shotId] = {
    action: async (page) => {
      await facade(page, "setActiveTool", tool);
      await page.waitForTimeout(300);
    },
  };
}
// dock ------------------------------------------------------------------
PLANS["dock-properties"] = {
  action: async (page) => {
    await facade(page, "setRightDockOpen", true);
    await facade(page, "setInspectorTab", "adjust");
    await facade(page, "setAdjustSubTab", "properties");
    await page.waitForTimeout(250);
  },
};
PLANS["dock-adjustments"] = {
  action: async (page) => {
    await facade(page, "setRightDockOpen", true);
    await facade(page, "setInspectorTab", "adjust");
    await facade(page, "setAdjustSubTab", "adjustments");
    await page.waitForTimeout(250);
  },
};
PLANS["dock-presets"] = {
  action: async (page) => {
    await facade(page, "setRightDockOpen", true);
    await facade(page, "setInspectorTab", "presets");
    await page.waitForTimeout(250);
  },
};
PLANS["dock-history"] = {
  action: async (page) => {
    await facade(page, "setRightDockOpen", true);
    await facade(page, "setRightDockPanel", "history");
    await page.waitForTimeout(250);
  },
};
PLANS["dock-closed"] = {
  action: async (page) => {
    await facade(page, "setRightDockOpen", false);
    await page.waitForTimeout(250);
  },
};
// dialog ----------------------------------------------------------------
PLANS["dialog-resize"] = {
  action: async (page) => {
    await editorCommand(page, "image.resize");
    await page.locator("[data-dialog-kind='resize-canvas']").waitFor({ state: "visible", timeout: 5_000 });
    await page.waitForTimeout(200);
  },
};
PLANS["dialog-resize-full"] = {
  action: async (page) => {
    await editorCommand(page, "image.resize");
    await page.locator("[data-dialog-kind='resize-canvas']").waitFor({ state: "visible", timeout: 5_000 });
    await page.waitForTimeout(200);
  },
};
PLANS["dialog-export"] = {
  action: async (page) => {
    await editorCommand(page, "file.export");
    await page.locator("[data-dialog-kind='export']").waitFor({ state: "visible", timeout: 5_000 });
    await page.waitForTimeout(200);
  },
};
PLANS["dialog-print"] = {
  action: async (page) => {
    await editorCommand(page, "file.print");
    await page.locator("[data-dialog-kind='print']").waitFor({ state: "visible", timeout: 5_000 });
    await page.waitForTimeout(200);
  },
};
PLANS["dialog-new-document"] = {
  action: async (page) => {
    await editorCommand(page, "file.new");
    await page.locator("[data-dialog-kind='new-document']").waitFor({ state: "visible", timeout: 5_000 });
    await page.waitForTimeout(200);
  },
};
PLANS["dialog-settings"] = {
  action: async (page) => {
    await editorCommand(page, "app.settings");
    await page.locator("[data-dialog-kind='settings']").waitFor({ state: "visible", timeout: 5_000 });
    await page.waitForTimeout(200);
  },
};
PLANS["dialog-about"] = {
  action: async (page) => {
    await editorCommand(page, "help.about");
    await page.locator("[data-dialog-kind='about']").waitFor({ state: "visible", timeout: 5_000 });
    await page.waitForTimeout(200);
  },
};
PLANS["dialog-color-picker"] = {
  action: async (page) => {
    // The facade signals (setColorPickerTarget/setColorPickerOpen) still flip,
    // but nothing renders from them any more; the live path is the swatch
    // click, which is also the real user gesture. First 38px swatch = fg.
    await page.locator(`${RAIL} ${RAIL_SWATCH}`).first().click();
    await page.locator("[data-dialog-kind='color-picker']").waitFor({ state: "visible", timeout: 5_000 });
    await page.waitForTimeout(200);
  },
};
// toast -----------------------------------------------------------------
PLANS["toast-info"] = { action: (page) => toast(page, "info") };
PLANS["toast-warn"] = { action: (page) => toast(page, "warn") };
PLANS["toast-error"] = { action: (page) => toast(page, "error") };
PLANS["toast-stacked"] = {
  action: async (page) => {
    await toast(page, "info", 1);
    await toast(page, "warn", 1);
    await toast(page, "error", 1);
    await page.waitForTimeout(200);
  },
};
// loading ---------------------------------------------------------------
PLANS["loading-overlay"] = {
  action: async (page) => {
    await facade(page, "setLoadingMessage", "Baseline capture: loading overlay");
    await page.waitForTimeout(250);
  },
  after: (page) => facade(page, "setLoadingMessage", null),
};
PLANS["loading-status"] = {
  action: async (page) => {
    await facade(page, "setStatusLoadingMessage", "3/7");
    await page.waitForTimeout(250);
  },
  after: (page) => facade(page, "setStatusLoadingMessage", null),
};
// empty -----------------------------------------------------------------
PLANS["empty-new-document"] = {
  action: async (page) => {
    await page.getByRole("button", { name: "New Document", exact: true }).click();
    await page.locator("[data-dialog-kind='new-document']").waitFor({ state: "visible", timeout: 10_000 });
    await page.waitForTimeout(250);
  },
};
// context ---------------------------------------------------------------
PLANS["ctx-canvas"] = {
  action: async (page) => {
    await clickCanvas(page, "right");
    await page.locator("div[data-testid='canvas-context-menu']").waitFor({ state: "visible", timeout: 5_000 });
    await page.waitForTimeout(150);
  },
};
PLANS["ctx-brush"] = {
  action: async (page) => {
    await facade(page, "setActiveTool", "brush");
    await page.waitForTimeout(200);
    await clickCanvas(page, "right");
    await page.waitForTimeout(300);
  },
};
PLANS["ctx-layer"] = {
  action: async (page) => {
    await page.locator("div[data-layer-idx]").first().click({ button: "right" });
    await page.locator("div[data-testid='layer-context-menu']").waitFor({ state: "visible", timeout: 5_000 });
    await page.waitForTimeout(150);
  },
};
PLANS["ctx-export"] = {
  action: async (page) => {
    const trigger = page.getByRole("button", { name: /export/i }).first();
    await trigger.click();
    await page.locator("div[role='menu']:not([id^='app-menu-'])").first().waitFor({ state: "visible", timeout: 5_000 });
    await page.waitForTimeout(150);
  },
};
// tooltip ---------------------------------------------------------------
PLANS["tooltip-rail-tool"] = {
  allowTooltip: true,
  action: async (page) => {
    await railTool(page, "Move Tool").hover();
    await page.waitForTimeout(700);
  },
};
PLANS["tooltip-rail-variant-tool"] = {
  allowTooltip: true,
  action: async (page) => {
    await railTool(page, "Rectangle Select").hover();
    await page.waitForTimeout(700);
  },
};
PLANS["tooltip-rail-swatch"] = {
  allowTooltip: true,
  action: async (page) => {
    await page.locator(`${RAIL} ${RAIL_SWATCH}`).first().hover();
    await page.waitForTimeout(700);
  },
};
// rail ------------------------------------------------------------------
PLANS["rail-hover-inactive"] = {
  action: async (page) => {
    await railTool(page, "Brush Tool").hover();
    await page.waitForTimeout(200);
  },
};
PLANS["rail-hover-active"] = {
  action: async (page) => {
    await railTool(page, "Move Tool").hover();
    await page.waitForTimeout(200);
  },
};
PLANS["rail-press-tool"] = {
  action: (page) => pressHold(page, railTool(page, "Brush Tool")),
  after: (page) => page.mouse.up(),
};
PLANS["rail-press-swatch"] = {
  action: (page) => pressHold(page, page.locator(`${RAIL} ${RAIL_SWATCH}`).first()),
  after: (page) => page.mouse.up(),
};
PLANS["rail-active-brush"] = {
  action: async (page) => {
    await facade(page, "setActiveTool", "brush");
    await page.waitForTimeout(250);
  },
};
PLANS["rail-hover-swatch"] = {
  action: async (page) => {
    await page.locator(`${RAIL} ${RAIL_SWATCH}`).first().hover();
    await page.waitForTimeout(200);
  },
};
PLANS["rail-flyout"] = {
  action: async (page) => {
    await railTool(page, "Rectangle Select").click({ button: "right" });
    await page.locator("div[class*='left-full']").first().waitFor({ state: "visible", timeout: 5_000 });
    await page.waitForTimeout(200);
  },
};
// hover -----------------------------------------------------------------
PLANS["hover-doc-tab"] = {
  action: async (page) => {
    await page.locator("div[data-document-tab]").first().hover();
    await page.waitForTimeout(200);
  },
};
PLANS["hover-layer-item"] = {
  action: async (page) => {
    await page.locator("div[data-layer-idx]").first().hover();
    await page.waitForTimeout(250);
  },
};
PLANS["hover-dock-tab"] = {
  action: async (page) => {
    await page.locator("nav[aria-label='Layer dock tabs'] [role='tab']").first().hover();
    await page.waitForTimeout(200);
  },
};
PLANS["hover-option-button"] = {
  action: async (page) => {
    // "More Options" is the single visible button in the option bar; the
    // align/flip quick-action buttons are rendered hidden (0x0) and a plain
    // .first() would wait on them forever.
    await page.getByRole("button", { name: "More Options" }).first().hover();
    await page.waitForTimeout(200);
  },
};
PLANS["press-option-button"] = {
  action: (page) => pressHold(page, page.getByRole("button", { name: "More Options" }).first()),
  after: (page) => page.mouse.up(),
};

// ---------------------------------------------------------------- noise classification

interface NoiseEntry {
  category: string;
  count: number;
  samples: string[];
}

function classifyNoise(messages: string[]): { noise: NoiseEntry[]; problems: string[] } {
  const noise: NoiseEntry[] = [];
  const problems: string[] = [];
  const invoke: string[] = [];
  for (const m of messages) {
    if (/invoke|__TAURI|tauri/i.test(m)) invoke.push(m);
    else problems.push(m);
  }
  if (invoke.length > 0) {
    noise.push({
      category: "tauri-invoke-unavailable",
      count: invoke.length,
      samples: invoke.slice(0, 2).map((m) => m.split("\n")[0]!.slice(0, 200)),
    });
  }
  return { noise, problems };
}

// ---------------------------------------------------------------- provenance

function sha256File(p: string): string | null {
  try {
    return createHash("sha256").update(readFileSync(p)).digest("hex");
  } catch {
    return null;
  }
}

function sh(cmd: string): string | null {
  try {
    return execSync(cmd, { cwd: ROOT, encoding: "utf8" }).trim();
  } catch {
    return null;
  }
}

const require = createRequire(import.meta.url);

function playwrightVersion(): string | null {
  try {
    const resolved = require.resolve("@playwright/test/package.json");
    return JSON.parse(readFileSync(resolved, "utf8")).version as string;
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------- main

async function main(): Promise<void> {
  let shots = SHOTS;
  if (ONLY && ONLY.length > 0) {
    shots = shots.filter((s) => ONLY.some((f) => s.id === f || s.category === f || s.id.startsWith(f)));
  }
  if (shots.length === 0) {
    console.error("no shots matched the --only filter");
    process.exit(2);
  }

  const outShots = path.join(OUT_DIR, "shots");
  mkdirSync(outShots, { recursive: true });

  const sourceCommit = sh("git rev-parse HEAD");
  const dirty = sh("git status --porcelain");
  const scriptPath = path.join(SCRIPT_DIR, "capture-baseline.ts");
  const wasmPath = path.join(SCRIPT_DIR, "..", "src", "wasm", "pkg", "photrez_core_bg.wasm");

  const scriptSha = sha256File(scriptPath);
  const scriptBlob = sh(`git hash-object "${scriptPath}"`);

  const browser: Browser = await chromium.launch({
    args: ["--use-gl=angle", "--use-angle=swiftshader-webgl"],
  });
  const browserVersion = browser.version();

  let renderer = "unknown";
  {
    const probe = await browser.newPage();
    renderer = await probe.evaluate(() => {
      const c = document.createElement("canvas");
      const gl = c.getContext("webgl2");
      if (!gl) return "no webgl2";
      const dbg = gl.getExtension("WEBGL_debug_renderer_info");
      return String(dbg ? gl.getParameter(dbg.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER));
    });
    await probe.close();
  }

  const results: Record<string, unknown>[] = [];
  let okCount = 0;
  let failCount = 0;

  for (const shot of shots) {
    const plan: ShotPlan = PLANS[shot.id] ?? {};
    const file = path.join(outShots, `${shot.id}.png`);
    const problems: string[] = [];
    const consoleErrors: string[] = [];

    const context = await browser.newContext({ viewport: VIEWPORT, deviceScaleFactor: 1 });
    try {
      await context.addInitScript(() => {
        try {
          // Pins. locale = stable strings. bgFlagRoute=0 is the app's own
          // documented rollback switch for the guarded background-flag commit:
          // with it armed (the default) the document-open path waits for a
          // native seed that cannot exist in a browser (no Tauri invoke) and
          // refuses the open. Found live during the rebuild - without this pin
          // no needsDoc shot can be captured at all.
          localStorage.setItem("photrez.locale", "en");
          localStorage.setItem("photrez.bgFlagRoute", "0");
        } catch {
          /* about:blank has no storage; the init script reruns per document */
        }
      });
      const page = await context.newPage();
      page.on("console", (msg) => {
        const type = msg.type();
        if (type === "error") consoleErrors.push(`error: ${msg.text()}`);
        // Warnings are only interesting when they are the Tauri-missing family
        // (e.g. "[canonical-seed] shadow seed failed ... invoke"); the rest is
        // GL driver chatter.
        if (type === "warning" && /invoke|__TAURI|tauri/i.test(msg.text())) {
          consoleErrors.push(`warning: ${msg.text()}`);
        }
      });
      page.on("pageerror", (err) => consoleErrors.push(String(err)));
      page.on("requestfailed", (req) => {
        consoleErrors.push(`requestfailed: ${req.url()} :: ${req.failure()?.errorText ?? "?"}`);
      });

      await page.goto(BASE_URL, { waitUntil: "domcontentloaded", timeout: 30_000 });
      await page.getByRole("banner").first().waitFor({ state: "visible", timeout: 30_000 });

      if (!plan.allowTooltip) {
        await page.addStyleTag({ content: '[role="tooltip"]{display:none !important}' });
      }

      if (shot.needsDoc) {
        await createBlankCanvas(page);
        if (await railDisabled(page)) {
          problems.push("rail shows the disabled state while a document is open");
        }
      }

      if (plan.action) await plan.action(page);

      // Screenshot.
      const target = shot.selector ? page.locator(shot.selector).first() : null;
      let box: Box | null = null;
      if (target) {
        box = (await target.boundingBox()) as Box | null;
        if (!box) problems.push(`selector matched no visible element: ${shot.selector}`);
        else await target.screenshot({ path: file });
      } else {
        box = { x: 0, y: 0, ...VIEWPORT };
        await page.screenshot({ path: file });
      }

      if (plan.after) await plan.after(page);

      const { noise, problems: realProblems } = classifyNoise(consoleErrors);
      problems.push(...realProblems);

      let boxDrift: Record<string, number> | null = null;
      const earlierBox = compareBoxes.get(shot.id);
      if (earlierBox && box) {
        const drift = {
          dx: Math.round(box.x - earlierBox.x),
          dy: Math.round(box.y - earlierBox.y),
          dw: Math.round(box.width - earlierBox.width),
          dh: Math.round(box.height - earlierBox.height),
        };
        if (Object.values(drift).some((v) => Math.abs(v) > 4)) boxDrift = drift;
      }

      const ok = problems.length === 0;
      ok ? okCount++ : failCount++;
      results.push({
        id: shot.id,
        category: shot.category,
        file: `${shot.id}.png`,
        ok,
        problems,
        needsDoc: shot.needsDoc,
        selector: shot.selector,
        note: shot.note,
        box,
        boxDrift,
        reason: null,
        expectedNoise: noise,
      });
      console.log(`${ok ? "ok  " : "FAIL"} ${shot.id}${boxDrift ? ` (box drift ${JSON.stringify(boxDrift)})` : ""}`);
      if (!ok) for (const p of problems) console.log(`     - ${p.slice(0, 300)}`);
    } catch (err) {
      failCount++;
      const diagnostics = consoleErrors.slice(0, 3).map((m) => m.split("\n")[0]!.slice(0, 300));
      results.push({
        id: shot.id,
        category: shot.category,
        file: `${shot.id}.png`,
        ok: false,
        problems: [
          `capture threw: ${err instanceof Error ? err.message.split("\n")[0] : String(err)}`,
          ...diagnostics.map((d) => `console: ${d}`),
        ],
        needsDoc: shot.needsDoc,
        selector: shot.selector,
        note: shot.note,
        box: null,
        boxDrift: null,
        reason: null,
        expectedNoise: [],
      });
      console.log(`FAIL ${shot.id} (threw)`);
      console.log(`     - ${err instanceof Error ? err.message.split("\n")[0] : String(err)}`);
      for (const d of diagnostics) console.log(`     - console: ${d}`);
    } finally {
      await context.close();
    }
  }

  await browser.close();

  const manifest = {
    capturedAt: new Date().toISOString(),
    url: BASE_URL,
    viewport: VIEWPORT,
    deviceScaleFactor: 1,
    sourceCommit,
    sourceDirty: dirty !== "",
    locale: "en",
    pins: {
      "photrez.locale": "en",
      "photrez.bgFlagRoute": "0",
    },
    host: {
      os: `${os.type()} ${os.release()}`,
      platform: process.platform,
      arch: process.arch,
      browser: `Chromium ${browserVersion}`,
      renderer,
    },
    toolVersions: {
      bun: (process.versions as Record<string, string | undefined>).bun ?? null,
      playwright: playwrightVersion(),
      wasm: { path: "apps/desktop/src/wasm/pkg/photrez_core_bg.wasm", sha256: sha256File(wasmPath) },
    },
    scripts: {
      path: "apps/desktop/scripts/capture-baseline.ts",
      sha256: scriptSha,
      gitBlob: scriptBlob,
      shotsTable: {
        path: "apps/desktop/scripts/baseline-shots.json",
        sha256: sha256File(SHOTS_PATH),
        gitBlob: sh(`git hash-object "${SHOTS_PATH}"`),
      },
    },
    note:
      "Golden-master baselines for the native UI port's parity checks. Captured from the " +
      "web-rendered app via its Vite dev server (headless Chromium, ANGLE/SwiftShader, " +
      "deviceScaleFactor 1). Pins: photrez.locale=en (stable strings) and " +
      "photrez.bgFlagRoute=0 - the app's own documented rollback switch; without it the " +
      "document-open path waits for a native seed that a browser cannot provide and refuses " +
      "the open. Document recipe: blank canvas named \"Baseline\", 800x600. Capture " +
      "conventions and the tolerance policy for comparisons: see README.md beside this manifest.",
    total: results.length,
    ok: okCount,
    failed: failCount,
    shots: results,
  };

  writeFileSync(path.join(outShots, "manifest.json"), JSON.stringify(manifest, null, 2) + "\n");
  console.log(`\n${okCount} ok, ${failCount} failed -> ${outShots}`);
  if (failCount > 0) process.exit(1);
}

await main();
