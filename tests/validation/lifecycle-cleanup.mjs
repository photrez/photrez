#!/usr/bin/env node
/**
 * C5.4 Lifecycle Cleanup Runtime Verification
 * Tests: layer removal cleanup, multiple removals, existing layer safety, doc isolation.
 * Uses rust_pixels_get_epoch to observe Rust store presence/absence.
 */
import { chromium } from '@playwright/test';
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
let pass = 0, fail = 0;
function check(label, ok, detail) {
  if (ok) { pass++; console.log(`  ✅ ${label}`); }
  else { fail++; console.error(`  ❌ ${label}${detail ? " — " + detail : ""}`); }
}

(async () => {
  const browser = await chromium.connectOverCDP("http://127.0.0.1:9222");
  const ctx = browser.contexts()[0];
  const page = ctx.pages()[0];

  const invoke = async (cmd, args) => {
    return await page.evaluate(async ({cmd, args}) => {
      return await window.__TAURI_INTERNALS__.invoke(cmd, args || {});
    }, {cmd, args});
  };

  const rustStoreExists = async (docId, layerId) => {
    try {
      await invoke("rust_pixels_get_epoch", { docId, layerId });
      return true;
    } catch { return false; }
  };

  // Close all open documents first
  await page.evaluate(() => {
    const ws = window.__photrezEditor.workspace;
    // Get all session IDs and remove them
    while (true) {
      const engine = ws.getActiveEngine();
      if (!engine) break;
      try { ws.removeDocument(engine.getId()); } catch { break; }
    }
  });
  await sleep(500);

  const createDoc = async (name) => {
    return await page.evaluate((name) => {
      const ed = window.__photrezEditor;
      const ws = ed.workspace;
      const id = name + "-" + Date.now();
      const session = ws.constructor.createBlankDocument(id, name, 100, 100, { backgroundColor: "white" });
      ws.addDocument(session);
      const engine = ws.getActiveEngine();
      const layer = engine.getLayers().find(l => l.type === "raster" && !l.isBackground) || engine.addLayer("L", 100, 100);
      engine.setActiveLayer(layer.id);
      return { docId: id, layerId: layer.id };
    }, name);
  };

  const rustPaint = async (docId, layerId) => {
    await invoke("rust_pixels_open_document", { docId });
    const layerData = await page.evaluate(({layerId}) => {
      const engine = window.__photrezEditor.workspace.getActiveEngine();
      const layer = engine.getLayer(layerId);
      if (!layer) return null;
      const c = new OffscreenCanvas(layer.width, layer.height);
      const ctx = c.getContext("2d");
      ctx.fillStyle = "#FFFFFF";
      ctx.fillRect(0, 0, layer.width, layer.height);
      const id = ctx.getImageData(0, 0, layer.width, layer.height);
      return { w: layer.width, h: layer.height, bytes: Array.from(id.data) };
    }, {layerId});
    if (layerData) {
      await invoke("rust_pixels_init", { docId, layerId, width: layerData.w, height: layerData.h, bytes: layerData.bytes });
    }
    await invoke("rust_pixels_write_region", {
      docId, layerId, x: 0, y: 0, w: 100, h: 100,
      rgba: new Array(100 * 100 * 4).fill(128)
    });
  };

  // ═══════════════════════════════════════════════
  // SCENARIO 1: Layer creation cleanup
  // ═══════════════════════════════════════════════
  console.log("\n═══ Scenario 1: Layer creation cleanup ═══");
  const s1doc = await createDoc("s1");
  const s1newLayer = await page.evaluate(() => {
    const engine = window.__photrezEditor.workspace.getActiveEngine();
    const history = window.__photrezEditor.workspace.getActiveHistory();
    history.commit(engine.snapshot(), "pre-add"); // production pattern
    const layer = engine.addLayer("Temp", 100, 100);
    return layer.id;
  });

  await rustPaint(s1doc.docId, s1newLayer);
  const s1before = await rustStoreExists(s1doc.docId, s1newLayer);
  check("S1: Rust store exists before undo", s1before);

  await page.keyboard.down("Control");
  await page.keyboard.press("z");
  await page.keyboard.up("Control");
  await sleep(1000);

  const s1gone = await page.evaluate(({id}) => !window.__photrezEditor.workspace.getActiveEngine().getLayer(id), {id: s1newLayer});
  check("S1: Layer gone from TS", s1gone);

  const s1after = await rustStoreExists(s1doc.docId, s1newLayer);
  check("S1: Rust store REMOVED", !s1after, s1after ? "still exists!" : "");

  // ═══════════════════════════════════════════════
  // SCENARIO 2: Multiple removed layers
  // ═══════════════════════════════════════════════
  console.log("\n═══ Scenario 2: Multiple removed layers ═══");
  const s2doc = await createDoc("s2");
  const s2layers = await page.evaluate(() => {
    const engine = window.__photrezEditor.workspace.getActiveEngine();
    const history = window.__photrezEditor.workspace.getActiveHistory();
    history.commit(engine.snapshot(), "pre-add-A");
    const a = engine.addLayer("A", 100, 100);
    history.commit(engine.snapshot(), "pre-add-B");
    const b = engine.addLayer("B", 100, 100);
    return { a: a.id, b: b.id };
  });

  await rustPaint(s2doc.docId, s2layers.a);
  await rustPaint(s2doc.docId, s2layers.b);

  const s2a_before = await rustStoreExists(s2doc.docId, s2layers.a);
  const s2b_before = await rustStoreExists(s2doc.docId, s2layers.b);
  check("S2: Both stores exist before undo", s2a_before && s2b_before);

  // Undo twice
  await page.keyboard.down("Control");
  await page.keyboard.press("z");
  await page.keyboard.up("Control");
  await sleep(500);
  await page.keyboard.down("Control");
  await page.keyboard.press("z");
  await page.keyboard.up("Control");
  await sleep(1000);

  const s2a_gone = await page.evaluate(({id}) => !window.__photrezEditor.workspace.getActiveEngine().getLayer(id), {id: s2layers.a});
  const s2b_gone = await page.evaluate(({id}) => !window.__photrezEditor.workspace.getActiveEngine().getLayer(id), {id: s2layers.b});
  check("S2: Both layers gone from TS", s2a_gone && s2b_gone);

  const s2a_after = await rustStoreExists(s2doc.docId, s2layers.a);
  const s2b_after = await rustStoreExists(s2doc.docId, s2layers.b);
  check("S2: Layer A store REMOVED", !s2a_after, s2a_after ? "still exists" : "");
  check("S2: Layer B store REMOVED", !s2b_after, s2b_after ? "still exists" : "");

  // ═══════════════════════════════════════════════
  // SCENARIO 3: Existing layer NOT removed
  // ═══════════════════════════════════════════════
  console.log("\n═══ Scenario 3: Existing layer NOT removed ═══");
  const s3doc = await createDoc("s3");

  await rustPaint(s3doc.docId, s3doc.layerId);
  const s3epoch_before = await invoke("rust_pixels_get_epoch", { docId: s3doc.docId, layerId: s3doc.layerId });
  check("S3: Rust store exists (epoch=" + s3epoch_before + ")", true);

  await invoke("rust_pixels_write_region", {
    docId: s3doc.docId, layerId: s3doc.layerId, x: 0, y: 0, w: 100, h: 100,
    rgba: new Array(100 * 100 * 4).fill(200)
  });

  await page.keyboard.down("Control");
  await page.keyboard.press("z");
  await page.keyboard.up("Control");
  await sleep(1000);

  const s3still = await page.evaluate(({id}) => !!window.__photrezEditor.workspace.getActiveEngine().getLayer(id), {id: s3doc.layerId});
  check("S3: Existing layer still in TS", s3still);

  const s3store = await rustStoreExists(s3doc.docId, s3doc.layerId);
  check("S3: Rust store STILL EXISTS (not removed)", s3store, "store was incorrectly removed!");

  const s3epoch_after = await invoke("rust_pixels_get_epoch", { docId: s3doc.docId, layerId: s3doc.layerId });
  check("S3: Epoch coherent after undo (epoch=" + s3epoch_after + ")", s3epoch_after >= 0);

  // ═══════════════════════════════════════════════
  // SCENARIO 4: Document isolation
  // ═══════════════════════════════════════════════
  console.log("\n═══ Scenario 4: Document isolation ═══");
  const s4docA = await createDoc("s4A");
  // Paint doc A's layer via Rust BEFORE switching
  await rustPaint(s4docA.docId, s4docA.layerId);

  const s4docB = await createDoc("s4B");
  // Paint doc B's layer via Rust (doc B is now active)
  await rustPaint(s4docB.docId, s4docB.layerId);

  const s4a = await rustStoreExists(s4docA.docId, s4docA.layerId);
  const s4b = await rustStoreExists(s4docB.docId, s4docB.layerId);
  check("S4: Both stores exist", s4a && s4b);

  // Switch to doc A, add temp layer there, paint via Rust
  await page.evaluate(({docId}) => {
    window.__photrezEditor.workspace.switchDocument(docId);
  }, {docId: s4docA.docId});
  await sleep(300);
  const s4temp = await page.evaluate(() => {
    const engine = window.__photrezEditor.workspace.getActiveEngine();
    const history = window.__photrezEditor.workspace.getActiveHistory();
    history.commit(engine.snapshot(), "pre-add-temp");
    return engine.addLayer("TempA", 100, 100).id;
  });
  await rustPaint(s4docA.docId, s4temp);

  await page.keyboard.down("Control");
  await page.keyboard.press("z");
  await page.keyboard.up("Control");
  await sleep(1000);

  const s4orig = await rustStoreExists(s4docA.docId, s4docA.layerId);
  check("S4: Doc A original store untouched", s4orig);

  const s4tempGone = await rustStoreExists(s4docA.docId, s4temp);
  check("S4: Doc A temp store REMOVED", !s4tempGone, s4tempGone ? "still exists" : "");

  const s4bStill = await rustStoreExists(s4docB.docId, s4docB.layerId);
  check("S4: Doc B store untouched", s4bStill);

  // ═══════════════════════════════════════════════
  console.log(`\n══════════════════════════════════════════════`);
  console.log(`  RESULTS: ${pass} passed, ${fail} failed`);
  console.log(`══════════════════════════════════════════════`);

  await browser.close();
  process.exit(fail > 0 ? 1 : 0);
})();
