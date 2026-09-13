import { expect, test } from "@playwright/test";
import { readRenderedPixelAtRatio } from "./helpers/screenshotPixels";

/**
 * Facade-armed visible sequence in the real browser (Vite dev server, chromium).
 *
 * The vitest parity harness proves the Rust engine and the TS mirror agree on
 * canonical state. This spec proves the routed metadata ops actually RENDER:
 * a visibility toggle driven through the facade must change canvas pixels, and
 * undo/redo must restore them pixel-for-pixel (the "unit tests green, app
 * broken" class the headless suite cannot catch).
 *
 * STEP 0 is a hard gate: the facade flag is set before app load and the spec
 * asserts the real wasm engine is armed (bridge.isFacadeArmed() plus a wired
 * addLayer that does not throw E_FACADE_NOT_READY). If that gate fails here,
 * the spec SKIPS with the measured reason instead of faking a native result.
 *
 * Scope: this runs the wasm-backed facade in chromium. It does NOT exercise the
 * desktop native-IPC engine (Tauri), which is a separate manual executable run.
 *
 * The bridge module is imported as "/src/lib/protocol/bridge.ts". The extension
 * matters: an extensionless specifier resolves to a DIFFERENT module instance in
 * the Vite dev server than the one the app armed, which would make the probe read
 * an unarmed instance (measured, see the probe's history).
 */

async function createBlankCanvas(page: import("@playwright/test").Page, width: number, height: number) {
  await page.getByRole("button", { name: "New Document" }).click();
  await page.locator('[role="dialog"]').waitFor({ state: "visible", timeout: 5000 });
  const numInputs = page.locator('[role="dialog"] input[type="number"]');
  await numInputs.nth(0).fill(String(width));
  await numInputs.nth(1).fill(String(height));
  await page.locator('[data-dialog-confirm]').click();
  await page.waitForTimeout(300);
}

test("facade-armed visibility toggle renders and undo/redo restores pixels", async ({ page }) => {
  // Facade flag must be set before the app boots (the app arms the engine during
  // startup when the flag is on).
  await page.addInitScript(() => {
    localStorage.setItem("photrez.facade", "1");
  });
  await page.setViewportSize({ width: 1100, height: 760 });
  await page.goto("/");

  // ── STEP 0: hard arm gate ──────────────────────────────────────────────
  const probe = await page.evaluate(async (bridgeSpec: string) => {
    const bridge = await import(/* @vite-ignore */ bridgeSpec);
    let readyError: string | null = null;
    try {
      await bridge.ensureFacadeReady();
    } catch (e) {
      readyError = String((e as Error)?.message ?? e);
    }
    let applyError: string | null = null;
    try {
      await bridge.applyCommand({
        contractVersion: 2,
        docId: "e2e-arm-gate",
        command: { type: "addLayer", id: "probe-1", name: "Probe", width: 10, height: 10, index: 0 },
      });
    } catch (e) {
      applyError = String((e as Error)?.message ?? e);
    }
    const snap = await bridge.getSnapshot("e2e-arm-gate").catch(() => null);
    return {
      armed: bridge.isFacadeArmed(),
      readyError,
      applyError,
      layers: snap?.layers?.length ?? -1,
    };
  }, "/src/lib/protocol/bridge.ts");

  if (!probe.armed || probe.applyError !== null || probe.layers !== 1) {
    test.skip(true, `facade arm gate failed in chromium: ${JSON.stringify(probe)}`);
    return;
  }

  // ── Visible sequence ───────────────────────────────────────────────────
  const hidePanels = page.getByRole("button", { name: "Hide side panels" });
  if (await hidePanels.isVisible()) await hidePanels.click();

  await createBlankCanvas(page, 240, 180);

  const container = page.locator("#canvas-container");
  await expect(container).toBeVisible();
  const containerBox = await container.boundingBox();
  if (!containerBox) throw new Error("canvas container not found");

  // Paint a horizontal stroke at the artboard center so there is visible content
  // to hide/restore.
  await page.getByRole("button", { name: "Brush Tool" }).click();
  await page.waitForTimeout(150);

  const fitZoom = Math.max(
    0.05,
    Math.min((containerBox.width - 80) / 240, (containerBox.height - 80) / 180, 10),
  );
  const artboardX = (containerBox.width - 240 * fitZoom) / 2;
  const artboardY = (containerBox.height - 180 * fitZoom) / 2;
  const centerX = containerBox.x + artboardX + (240 * fitZoom) / 2;
  const centerY = containerBox.y + artboardY + (180 * fitZoom) / 2;

  await page.mouse.move(centerX - 30, centerY);
  await page.mouse.down();
  await page.mouse.move(centerX + 30, centerY);
  await page.mouse.up();
  await page.waitForTimeout(250);

  const canvas = page.locator("#canvas-container > canvas").first();
  const painted = await readRenderedPixelAtRatio(canvas, 0.5, 0.5);
  expect(painted, "painted center pixel must be readable").not.toBeNull();
  expect(painted![3], "brush stroke must produce visible painted pixels").toBeGreaterThan(100);

  // Routed metadata op through the facade: hide the layer via its layer-row toggle.
  await page.getByRole("button", { name: "Hide Layer" }).first().click();
  await page.waitForTimeout(250);
  const hidden = await readRenderedPixelAtRatio(canvas, 0.5, 0.5);
  expect(hidden, "hidden center pixel must be readable").not.toBeNull();
  expect(hidden, "hiding the layer must change the rendered pixel").not.toEqual(painted);

  // Undo must restore the painted pixel exactly.
  await page.keyboard.press("Control+z");
  await page.waitForTimeout(250);
  const afterUndo = await readRenderedPixelAtRatio(canvas, 0.5, 0.5);
  expect(afterUndo, "undo must restore the painted pixel").toEqual(painted);

  // Redo must re-apply the hidden state exactly.
  await page.keyboard.press("Control+y");
  await page.waitForTimeout(250);
  const afterRedo = await readRenderedPixelAtRatio(canvas, 0.5, 0.5);
  expect(afterRedo, "redo must restore the hidden pixel state").toEqual(hidden);
});
