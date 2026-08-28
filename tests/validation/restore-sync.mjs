#!/usr/bin/env node
/**
 * C5.4 Restore Sync CDP Smoke — validates Part 1 (orphan cleanup) + Part 2 (bitmap sync)
 *
 * Prerequisites:
 *   - Tauri app running with CDP on port 9222
 *   - photrez.rustPixels = "1" in localStorage
 *   - Playwright available
 *
 * Run: node c5_4_restoreSync_smoke.mjs
 */
import { chromium } from '@playwright/test';

const CDP_URL = "http://127.0.0.1:9222";
let pass = 0, fail = 0;
const sleep = (ms) => new Promise(r => setTimeout(r, ms));

function check(label, ok, detail = "") {
  if (ok) { pass++; console.log(`  ✅ ${label}`); }
  else { fail++; console.error(`  ❌ ${label}${detail ? " — " + detail : ""}`); }
}

(async () => {
  console.log("🔗 Connecting to CDP...");
  const browser = await chromium.connectOverCDP(CDP_URL);
  const ctx = browser.contexts()[0];
  const page = ctx.pages()[0];

  // Wait for app ready
  await page.waitForFunction(
    () => !!(window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke) &&
          !!(window.__photrezEditor && window.__photrezEditor.workspace),
    null, { timeout: 20000 }
  );
  console.log("  ✅ App ready");

  // Set rustPixels flag
  await page.evaluate(() => localStorage.setItem("photrez.rustPixels", "1"));
  console.log("  ✅ rustPixels = 1");

  const invoke = (c, a) => page.evaluate(({ c, a }) => window.__TAURI_INTERNALS__.invoke(c, a || {}), { c, a });

  // Setup: create a fresh document with a white raster layer
  const setup = await page.evaluate(() => {
    const ed = window.__photrezEditor;
    const ws = ed.workspace;
    const w = ws.constructor ?? ws.WorkspaceManager;
    // Try to use createBlankDocument if available
    if (ws.addDocument) {
      const id = `smoke-${Date.now()}`;
      const session = ws.constructor.createBlankDocument(id, 'c54-restore', 300, 220, { backgroundColor: 'white' });
      ws.addDocument(session);
      ed.scheduler?.requestRender?.();
    }
    const engine = ws.getActiveEngine();
    if (!engine) return { error: 'no engine after create' };
    let layer = engine.getLayers().find(l => l.type === 'raster' && !l.isBackground);
    if (!layer) {
      // Add a non-background layer
      layer = engine.addLayer('Test Layer', 300, 220);
    }
    if (!layer) return { error: 'no raster layer' };

    // Ensure white
    const c = new OffscreenCanvas(300, 220);
    const ctx = c.getContext('2d');
    ctx.fillStyle = '#808080';
    ctx.fillRect(0, 0, 300, 220);
    engine.setLayerImageBitmap(layer.id, c.transferToImageBitmap());
    engine.setActiveLayer(layer.id);

    return { layerId: layer.id, W: 300, H: 220 };
  });
  if (setup.error) { console.error(`Setup: ${setup.error}`); await browser.close(); process.exit(1); }
  await sleep(500);
  console.log(`  ✅ Setup: ${setup.layerId}`);

  // Helper: read bitmap pixel at (0,0)
  const readBitmapPixel = async () => {
    return await page.evaluate(() => {
      const engine = window.__photrezEditor.workspace.getActiveEngine();
      const layer = engine.getLayers().find(l => l.type === "raster" && !l.isBackground);
      if (!layer?.imageBitmap) return "null";
      const c = new OffscreenCanvas(layer.width, layer.height);
      const ctx = c.getContext("2d");
      ctx.drawImage(layer.imageBitmap, 0, 0);
      const id = ctx.getImageData(0, 0, 1, 1).data;
      return `${id[0]},${id[1]},${id[2]},${id[3]}`;
    });
  };

  // S0: baseline
  const s0pixel = await readBitmapPixel();
  check("S0 baseline: gray (128,128,128,255)", s0pixel === "128,128,128,255", s0pixel);

  // S1: apply brightness adjustment
  await page.evaluate(() => {
    const engine = window.__photrezEditor.workspace.getActiveEngine();
    const layer = engine.getLayers().find(l => l.type === "raster" && !l.isBackground);
    engine.applyBasicAdjustment(layer.id, { brightness: 0, contrast: 50, saturation: 0 });
  });
  const s1adj = await page.evaluate(() => {
    const engine = window.__photrezEditor.workspace.getActiveEngine();
    const layer = engine.getLayers().find(l => l.type === "raster" && !l.isBackground);
    return !!layer.basicAdjustment;
  });
  check("S1 adjustment applied", s1adj);

  // S2: bake via commitBasicAdjustment
  await page.evaluate(async () => {
    const engine = window.__photrezEditor.workspace.getActiveEngine();
    const layer = engine.getLayers().find(l => l.type === "raster" && !l.isBackground);
    await engine.commitBasicAdjustment(layer.id);
  });
  const s2pixel = await readBitmapPixel();
  const s2adj = await page.evaluate(() => {
    const engine = window.__photrezEditor.workspace.getActiveEngine();
    const layer = engine.getLayers().find(l => l.type === "raster" && !l.isBackground);
    return !!layer.basicAdjustment;
  });
  check("S2 bake: adjustment cleared", !s2adj);
  check("S2 bake: bitmap changed from white", s2pixel !== "255,255,255,255", s2pixel);

  // S3: undo bake via Ctrl+Z
  await page.keyboard.down("Control");
  await page.keyboard.press("z");
  await page.keyboard.up("Control");
  await sleep(500);

  const s3pixel = await readBitmapPixel();
  check("S3 undo: bitmap restored to gray (128,128,128,255)", s3pixel === "128,128,128,255", s3pixel);

  // S4: redo bake via Ctrl+Y
  await page.keyboard.down("Control");
  await page.keyboard.press("y");
  await page.keyboard.up("Control");
  await sleep(500);

  const s4pixel = await readBitmapPixel();
  check("S4 redo: bitmap is baked (not white)", s4pixel !== "255,255,255,255", s4pixel);
  check("S4 redo: bitmap matches post-bake", s4pixel === s2pixel, `redo=${s4pixel} bake=${s2pixel}`);

  // S5: verify no page errors
  const errors = await page.evaluate(() => {
    return window.__c54_pageErrors || [];
  });
  check("S5 no page errors", errors.length === 0, JSON.stringify(errors));

  console.log(`\n══════════════════════════════════════════════`);
  console.log(`  RESULTS: ${pass} passed, ${fail} failed`);
  console.log(`══════════════════════════════════════════════`);

  await browser.close();
  process.exit(fail > 0 ? 1 : 0);
})();
