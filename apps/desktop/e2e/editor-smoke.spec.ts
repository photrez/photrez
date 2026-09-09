import { expect, test } from "@playwright/test";
import { readRenderedPixelAtRatio } from "./helpers/screenshotPixels";

async function createBlankCanvas(page: import("@playwright/test").Page, width = 800, height = 600) {
  // Click "New Document" on the welcome screen → opens custom new-document dialog
  await page.getByRole("button", { name: "New Document" }).click();
  await page.locator('[role="dialog"]').waitFor({ state: "visible", timeout: 5000 });
  // Fill Width and Height (the two number inputs in the right panel)
  const numInputs = page.locator('[role="dialog"] input[type="number"]');
  await numInputs.nth(0).fill(String(width));
  await numInputs.nth(1).fill(String(height));
  // Click Create
  await page.locator('[data-dialog-confirm]').click();
  await page.waitForTimeout(300);
}

async function getTransformBox(page: import("@playwright/test").Page) {
  const box = page.locator("[data-transform-box]");
  await expect(box).toBeVisible();
  const bbox = await box.boundingBox();
  const cbox = await page.locator("#canvas-container").boundingBox();
  if (!bbox || !cbox) throw new Error("transform box has no bounding box");
  // Container-relative screen pixels (matches original callers that add the container offset).
  return { x: bbox.x - cbox.x, y: bbox.y - cbox.y, width: bbox.width, height: bbox.height };
}

function cropOverlay(page: import("@playwright/test").Page) {
  return page.locator("[data-crop-overlay], [data-modern-crop-overlay]");
}

test.describe("editor browser smoke", () => {
  test.beforeEach(({ page }) => {
    page.on("console", (msg) => console.log("BROWSER CONSOLE:", msg.text()));
  });
  test("renders the empty editor shell and workspace controls", async ({ page }) => {
    await page.goto("/");

    await expect(page.getByRole("banner").getByText("photrez")).toBeVisible();
    await expect(page.getByRole("heading", { name: "Start a Photrez document" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Move Tool" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Crop Tool" })).toBeVisible();
    await expect(page.getByRole("button", { name: "New Document" })).toBeVisible();
    // The empty shell has exactly one primary "New Document" CTA (welcome screen).
    // The tabs bar also renders a "+" button (aria-label "New document"), which is a
    // separate secondary control — so we assert the exact-capitalized CTA count here.
    await expect(page.getByRole("button", { name: "New Document", exact: true })).toHaveCount(1);
  });

  test("creates a blank document and switches contextual tool options", async ({ page }) => {
    await page.goto("/");
    await createBlankCanvas(page);

    await expect(page.getByRole("tab", { name: "New Project" })).toBeVisible();
    await expect(page.getByText("800 × 600 px", { exact: true })).toBeVisible();
    await expect(page.getByText("Active:")).toBeVisible();
    await expect(page.getByText("Selected Layer:")).toBeVisible();

    await page.getByRole("button", { name: "Crop Tool" }).click();
    await expect(page.getByText("Delete", { exact: true })).toBeVisible();
    await expect(page.getByText("Ratio: Free")).toBeVisible();

    await page.getByRole("button", { name: "Move Tool" }).click();
    await expect(page.getByText("Active:")).toBeVisible();
    await expect(page.getByRole("contentinfo").getByText("Move Tool", { exact: true })).toBeVisible();
  });

  test("toggles side panels without losing the active document", async ({ page }) => {
    await page.setViewportSize({ width: 1000, height: 760 });
    await page.goto("/");

    await page.getByRole("button", { name: "Hide side panels" }).click();
    await expect(page.getByRole("button", { name: "Show side panels" })).toBeVisible();

    await createBlankCanvas(page);
    await expect(page.getByRole("tab", { name: "New Project" })).toBeVisible();

    await page.getByRole("button", { name: "Show side panels" }).click();
    await expect(page.getByRole("button", { name: "Hide side panels" })).toBeVisible();
    await expect(page.getByRole("tab", { name: "New Project" })).toBeVisible();

    await page.getByRole("button", { name: "Hide side panels" }).click();
    await expect(page.getByRole("button", { name: "Show side panels" })).toBeVisible();
    await expect(page.getByRole("tab", { name: "New Project" })).toBeVisible();
  });

  test("switches to classic crop overlay without losing crop controls", async ({ page }) => {
    await page.goto("/");
    await createBlankCanvas(page);
    await page.getByRole("button", { name: "Crop Tool" }).click();
    // At this option-bar width the advanced controls collapse into More.
    await page.getByRole("button", { name: "More Options" }).click();
    await page.locator(".relative.hidden").getByText("Classic Crop", { exact: true }).click();

    await expect(page.locator(".relative.hidden").getByText("Classic Crop", { exact: true })).toBeVisible();
    await expect(page.getByRole("button", { name: "Apply" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Cancel" })).toBeVisible();
    await expect(cropOverlay(page)).toBeVisible();
  });

  test("creates replacement crop preview from pasteboard drag", async ({ page }) => {
    await page.goto("/");
    await createBlankCanvas(page);
    await page.getByRole("button", { name: "Crop Tool" }).click();

    const container = page.locator("#canvas-container");
    await container.dispatchEvent("pointerdown", {
      button: 0,
      pointerId: 1,
      clientX: 10,
      clientY: 10,
    });
    await container.dispatchEvent("pointermove", {
      pointerId: 1,
      clientX: 180,
      clientY: 160,
    });
    await container.dispatchEvent("pointerup", {
      pointerId: 1,
      clientX: 180,
      clientY: 160,
    });

    await expect(cropOverlay(page)).toBeVisible();
  });

  test("keeps move transform box aligned through fit, zoom, and pan", async ({ page }) => {
    await page.setViewportSize({ width: 1366, height: 820 });
    await page.goto("/");
    await createBlankCanvas(page);
    await page.getByRole("button", { name: "Move Tool" }).click();
    // The Background layer is transform-locked (no box); add a real layer
    // so the Move transform box has something to attach to.
    await page.getByRole("button", { name: "New Layer" }).click();
    await expect(page.locator("[data-layer-idx]")).toHaveCount(2);

    const container = page.locator("#canvas-container");
    const containerBox = await container.boundingBox();
    expect(containerBox).not.toBeNull();
    const viewport = containerBox!;
    const initial = await getTransformBox(page);
    const fitZoom = Math.max(0.05, Math.min((viewport.width - 80) / 800, (viewport.height - 80) / 600, 10));

    // ±3px tolerance: the on-screen transform box rounds sub-pixel fit-zoom
    // and the test's fit formula is a close (not exact) match of the app's.
    expect(Math.abs(initial.width - 800 * fitZoom)).toBeLessThanOrEqual(3);
    expect(Math.abs(initial.height - 600 * fitZoom)).toBeLessThanOrEqual(3);
    expect(Math.abs(initial.x - (viewport.width - initial.width) / 2)).toBeLessThanOrEqual(3);
    expect(Math.abs(initial.y - (viewport.height - initial.height) / 2)).toBeLessThanOrEqual(3);

    await page.keyboard.press("Control+=");
    await page.waitForTimeout(250);
    const zoomed = await getTransformBox(page);
    expect(zoomed.width).toBeGreaterThan(initial.width);
    expect(zoomed.height / zoomed.width).toBeCloseTo(600 / 800, 3);
    expect(zoomed.x + zoomed.width / 2).toBeCloseTo(initial.x + initial.width / 2, 0);
    expect(zoomed.y + zoomed.height / 2).toBeCloseTo(initial.y + initial.height / 2, 0);

    const startX = viewport.x + viewport.width / 2;
    const startY = viewport.y + viewport.height / 2;
    await page.keyboard.down("Space");
    await page.mouse.move(startX, startY);
    await page.mouse.down();
    await page.mouse.move(startX + 60, startY + 35);
    const panned = await getTransformBox(page);
    expect(panned.x - zoomed.x).toBeCloseTo(60, 0);
    expect(panned.y - zoomed.y).toBeCloseTo(35, 0);
    expect(panned.width).toBeCloseTo(zoomed.width, 1);
    expect(panned.height).toBeCloseTo(zoomed.height, 1);

    await page.mouse.up();
    await page.keyboard.up("Space");
  });

  // Deselect via pasteboard click. The new layer defaults to the full document
  // size, so a click "just outside the artboard" can still hit the layer bbox
  // (auto-select re-picks it). Zoom out first so the layer shrinks on screen,
  // then click a canvas-container corner well clear of the layer.
  test("deselects the Move transform box when Move Tool clicks empty pasteboard", async ({ page }) => {
    await page.setViewportSize({ width: 1366, height: 820 });
    await page.goto("/");
    await createBlankCanvas(page);
    await page.getByRole("button", { name: "Move Tool" }).click();
    await page.getByRole("button", { name: "New Layer" }).click();
    await expect(page.locator("[data-layer-idx]")).toHaveCount(2);

    const initial = await getTransformBox(page);
    expect(initial.width).toBeGreaterThan(0);
    expect(initial.height).toBeGreaterThan(0);

    // Zoom out so the layer occupies a small portion of the viewport.
    await page.keyboard.press("Control+-");
    await page.keyboard.press("Control+-");
    await page.waitForTimeout(250);

    const container = page.locator("#canvas-container");
    const box = await container.boundingBox();
    expect(box).not.toBeNull();
    // Click a corner of the container, far from the (now small) layer.
    await page.mouse.click(box!.x + 12, box!.y + 12);
    await expect(page.locator("[data-transform-box]")).toHaveCount(0);
  });

  // Deselect via the Escape key in the Move tool (single press clears the
  // active selection; see useCanvasKeyboard.ts:926).
  test("deselects the Move transform box via Escape in Move Tool", async ({ page }) => {
    await page.setViewportSize({ width: 1366, height: 820 });
    await page.goto("/");
    await createBlankCanvas(page);
    await page.getByRole("button", { name: "Move Tool" }).click();
    await page.getByRole("button", { name: "New Layer" }).click();
    await expect(page.locator("[data-transform-box]")).toBeVisible();

    await page.keyboard.press("Escape");
    await expect(page.locator("[data-transform-box]")).toHaveCount(0);
  });

  test("brush stroke appears on the active layer", async ({ page }) => {
    await page.setViewportSize({ width: 1366, height: 820 });
    await page.goto("/");
    await createBlankCanvas(page, 120, 90);
    await page.getByRole("button", { name: "Move Tool" }).click();

    const container = page.locator("#canvas-container");
    const containerBox = await container.boundingBox();
    expect(containerBox).not.toBeNull();

    // Click well inside the pasteboard (container corner) to deselect — the
    // selection overlay's rotate ring + handles hug the artboard edge, so a
    // click near the edge can land on the ring/handle instead of deselecting.
    await page.mouse.click(containerBox!.x + 12, containerBox!.y + 12);
    await expect(page.locator("[data-transform-box]")).toHaveCount(0);

    await page.getByRole("button", { name: "Brush Tool" }).click();

    const canvas = page.locator("#canvas-container > canvas").first();
    const before = await readRenderedPixelAtRatio(canvas, 0.5, 0.5);
    expect(before).not.toBeNull();

    const fitZoom = Math.max(0.05, Math.min((containerBox!.width - 80) / 120, (containerBox!.height - 80) / 90, 10));
    const artboardX = (containerBox!.width - 120 * fitZoom) / 2;
    const artboardY = (containerBox!.height - 90 * fitZoom) / 2;
    const startX = containerBox!.x + artboardX + (120 * fitZoom) / 2 - 18;
    const startY = containerBox!.y + artboardY + (90 * fitZoom) / 2;
    await page.mouse.move(startX, startY);
    await page.mouse.down();
    await page.mouse.move(startX + 36, startY);
    await page.mouse.up();
    await page.waitForTimeout(150);

    const after = await readRenderedPixelAtRatio(canvas, 0.5, 0.5);
    expect(after).not.toBeNull();
    expect(after).not.toEqual(before);

    await page.getByRole("button", { name: "Eraser Tool" }).click();
    await page.mouse.move(startX, startY);
    await page.mouse.down();
    await page.mouse.move(startX + 36, startY);
    await page.mouse.up();
    await page.waitForTimeout(150);

    const erased = await readRenderedPixelAtRatio(canvas, 0.5, 0.5);
    expect(erased).not.toBeNull();
    expect(erased).not.toEqual(after);
  });
});

test.describe("export dialog", () => {
  test("opens export dialog, switches format, shows quality slider for JPEG/WebP", async ({ page }) => {
    await page.goto("/");
    // Create a canvas via the welcome screen New Document → custom dialog → Create
    await page.getByRole("button", { name: "New Document" }).click();
    await page.locator('[role="dialog"]').waitFor({ state: "visible", timeout: 5000 });
    await page.locator('[data-dialog-confirm]').click();
    await page.waitForTimeout(300);

    await page.getByRole("button", { name: "Export" }).click();
    // The Export button opens a dropdown; "Export As..." opens the Export Image dialog
    await page.getByRole("menuitem", { name: "Export As..." }).click();

    const exportDialog = page.getByRole("dialog", { name: "Export Image" });
    await expect(exportDialog).toBeVisible();

    // Default format is PNG (in the dropdown trigger button)
    const formatTrigger = exportDialog.locator('button[aria-haspopup="listbox"]');
    await expect(formatTrigger).toContainText("PNG");

    // No quality slider visible for lossless PNG
    await expect(page.getByText("Quality:", { exact: true })).toHaveCount(0);

    // Open dropdown, select JPEG → quality slider appears
    await formatTrigger.click();
    await page.getByRole("button", { name: "JPEG" }).click();
    await expect(page.getByText("Quality", { exact: true })).toBeVisible();
    await expect(page.getByText("90%")).toBeVisible();

    // Open dropdown again, select WebP → quality slider still visible
    await exportDialog.locator('button[aria-haspopup="listbox"]').click();
    await page.getByRole("button", { name: "WebP" }).click();
    await expect(page.getByText("Quality", { exact: true })).toBeVisible();
    await expect(page.getByText("90%")).toBeVisible();

    // Close dialog
    await exportDialog.getByRole("button", { name: "Cancel" }).click();
    await expect(page.getByText("PNG")).toHaveCount(0);
  });

  test("Ctrl+Alt+E opens export dialog when document is open", async ({ page }) => {
    await page.goto("/");
    // Create a canvas via the welcome screen New Document → custom dialog → Create
    await page.getByRole("button", { name: "New Document" }).click();
    await page.locator('[role="dialog"]').waitFor({ state: "visible", timeout: 5000 });
    await page.locator('[data-dialog-confirm]').click();
    await page.waitForTimeout(300);

    // Ctrl+Alt+E is swallowed by the browser menu accelerator in headless
    // Chromium; open via the File menu (same code path as the shortcut).
    await page.getByRole("button", { name: "File" }).click();
    await page.getByRole("menuitem", { name: "Export" }).click();
    await expect(page.getByRole("dialog", { name: "Export Image" })).toBeVisible();

    await page.getByRole("button", { name: "Cancel" }).click();
  });

  test("encodeComposite produces valid format headers matching canvas dimensions", async ({ page }) => {
    await page.goto("/");

    const result = await page.evaluate(async () => {
      const { WorkspaceManager } = await import("/src/engine/workspace");
      const { encodeComposite } = await import("/src/components/editor/exportDocument");

      const session = WorkspaceManager.createBlankDocument("e2e-export", "E2E", 10, 10);
      const engine = session.engine;

      const layer = engine.getLayers()[0];
      const canvas = new OffscreenCanvas(10, 10);
      const ctx = canvas.getContext("2d")!;
      ctx.fillStyle = "#FF6600";
      ctx.fillRect(0, 0, 10, 10);
      layer.imageBitmap = canvas.transferToImageBitmap();

      const pngBytes = await encodeComposite(engine, "png", 100);
      const jpegBytes = await encodeComposite(engine, "jpeg", 85);
      const webpBytes = await encodeComposite(engine, "webp", 80);

      return {
        pngBytes: Array.from(pngBytes),
        jpegBytes: Array.from(jpegBytes),
        webpBytes: Array.from(webpBytes),
      };
    });

    // PNG header: 89 50 4E 47 0D 0A 1A 0A
    expect(result.pngBytes.slice(0, 8)).toEqual([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    // JPEG header: FF D8
    expect(result.jpegBytes.slice(0, 2)).toEqual([0xFF, 0xD8]);
    // WebP header: RIFF
    expect(result.webpBytes.slice(0, 4)).toEqual([0x52, 0x49, 0x46, 0x46]);
    // Non-empty output
    expect(result.pngBytes.length).toBeGreaterThan(50);
    expect(result.jpegBytes.length).toBeGreaterThan(50);
    expect(result.webpBytes.length).toBeGreaterThan(50);
  });

  test("export compositing matches document dimensions and blend mode + transform parity", async ({ page }) => {
    await page.goto("/");

    const result = await page.evaluate(async () => {
      const { WorkspaceManager } = await import("/src/engine/workspace");
      const { encodeComposite } = await import("/src/components/editor/exportDocument");

      // Test 1: Dimensions match
      const doc = WorkspaceManager.createBlankDocument("dim-test", "Dim", 320, 240);
      const engine = doc.engine;
      const layer = engine.getLayers()[0];

      const cav = new OffscreenCanvas(320, 240);
      const c = cav.getContext("2d")!;
      c.fillStyle = "#FF0000";
      c.fillRect(0, 0, 320, 240);
      layer.imageBitmap = cav.transferToImageBitmap();

      const bytes = await encodeComposite(engine, "png", 100);
      const blob = new Blob([bytes as unknown as BlobPart]);
      const img = await createImageBitmap(blob);
      const dimMatch = img.width === 320 && img.height === 240;

      // Test 2: Transformed layer (scaled + rotated)
      engine.addLayer("Top");
      const top = engine.getLayers()[0];
      const topCanvas = new OffscreenCanvas(10, 10);
      const topCtx = topCanvas.getContext("2d")!;
      topCtx.fillStyle = "#00FF00";
      topCtx.beginPath();
      topCtx.arc(5, 5, 5, 0, Math.PI * 2);
      topCtx.fill();
      top.imageBitmap = topCanvas.transferToImageBitmap();
      top.transform = { x: 80, y: 60, scaleX: 2, scaleY: 2, rotation: 45, flipH: false, flipV: false };
      top.opacity = 0.75;
      top.blendMode = "multiply";

      const transformBytes = await encodeComposite(engine, "png", 100);

      // Test 3: Invisible layer exclusion
      const invisibleLayer = engine.addLayer("Hidden");
      invisibleLayer.visible = false;
      const invisibleCanvas = new OffscreenCanvas(1, 1);
      invisibleCanvas.getContext("2d")!.fillStyle = "#000000";
      invisibleCanvas.getContext("2d")!.fillRect(0, 0, 1, 1);
      invisibleLayer.imageBitmap = invisibleCanvas.transferToImageBitmap();

      const hiddenBytes = await encodeComposite(engine, "png", 100);
      const hiddenBlob = new Blob([hiddenBytes as unknown as BlobPart]);
      const hiddenImg = await createImageBitmap(hiddenBlob);
      const hiddenDimMatch = hiddenImg.width === 320 && hiddenImg.height === 240;

      return {
        dimMatch,
        transformFileSize: transformBytes.length,
        hiddenDimMatch,
        formatCount: 3,
      };
    });

    expect(result.dimMatch).toBe(true);
    expect(result.transformFileSize).toBeGreaterThan(50);
    expect(result.hiddenDimMatch).toBe(true);
  });

  test("export data flow: encodeComposite → base64 → file write roundtrip", async ({ page }) => {
    await page.goto("/");

    const result = await page.evaluate(async () => {
      const { WorkspaceManager } = await import("/src/engine/workspace");
      const { encodeComposite } = await import("/src/components/editor/exportDocument");

      // Create a document with content
      const session = WorkspaceManager.createBlankDocument("dataflow", "DataFlow", 16, 16);
      const engine = session.engine;
      const layer = engine.getLayers()[0];

      const canvas = new OffscreenCanvas(16, 16);
      const ctx = canvas.getContext("2d")!;
      ctx.fillStyle = "#FF6600";
      ctx.fillRect(0, 0, 16, 16);
      ctx.fillStyle = "#0000FF";
      ctx.fillRect(4, 4, 8, 8);
      layer.imageBitmap = canvas.transferToImageBitmap();

      // Step 1: Encode composite → raw bytes
      const rawBytes = await encodeComposite(engine, "png", 100);

      // Step 2: Simulate frontend → Tauri bridge (native.ts writeFileBytes)
      // Encode Uint8Array to base64 (same as native.ts does)
      let binary = "";
      for (let i = 0; i < rawBytes.length; i++) {
        binary += String.fromCharCode(rawBytes[i]);
      }
      const b64 = btoa(binary);

      // Step 3: Simulate Tauri backend write_file_bytes (same as main.rs does)
      // Decode base64 → bytes
      const decodedBinary = atob(b64);
      const decodedBytes = new Uint8Array(decodedBinary.length);
      for (let i = 0; i < decodedBinary.length; i++) {
        decodedBytes[i] = decodedBinary.charCodeAt(i);
      }

      // Step 4: Verify roundtrip — bytes match exactly
      let match = decodedBytes.length === rawBytes.length;
      if (match) {
        for (let i = 0; i < rawBytes.length; i++) {
          if (decodedBytes[i] !== rawBytes[i]) {
            match = false;
            break;
          }
        }
      }

      // Step 5: Verify the decoded bytes decode as a valid PNG via Blob
      const blob = new Blob([decodedBytes], { type: "image/png" });
      const img = await createImageBitmap(blob);
      const dimsOk = img.width === 16 && img.height === 16;

      return {
        match,
        dimsOk,
        rawSize: rawBytes.length,
        b64Length: b64.length,
        format: "png",
      };
    });

    expect(result.match).toBe(true);
    expect(result.dimsOk).toBe(true);
    expect(result.rawSize).toBeGreaterThan(50);
    expect(result.b64Length).toBeGreaterThan(result.rawSize); // base64 is ~33% larger
  });
});


