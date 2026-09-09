import { expect, test } from "@playwright/test";

async function createBlankCanvas(page: import("@playwright/test").Page) {
  // Click "New Document" on the welcome screen → opens custom new-document dialog
  await page.getByRole("button", { name: "New Document" }).click();
  await page.locator('[role="dialog"]').waitFor({ state: "visible", timeout: 5000 });
  // Click Create (accepts default 1080×1080)
  await page.locator('[data-dialog-confirm]').click();
  await page.waitForTimeout(300);
}

test.describe("Precision Workbench dialogs", () => {
  test("About dialog matches the about-kind contract and restores menu focus", async ({ page }, testInfo) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Help" }).click();
    await page.getByRole("menuitem", { name: "About Photrez" }).click();

    const dialog = page.getByRole("dialog", { name: "About Photrez" });
    await expect(dialog).toBeVisible();
    // Product contract is kind="about" since d901082 (dedicated AboutDialog):
    // an About dialog is informational, so it renders as a plain role="dialog",
    // not an alertdialog. The data-dialog-kind attribute reflects that kind.
    await expect(dialog).toHaveAttribute("data-dialog-kind", "about");
    // The primary/confirm action receives focus on open (consistent with the
    // Resize/Export dialogs). Verified via document.activeElement because the
    // headless focus-resolution check on the scoped locator is flaky even when
    // the button is the active element.
    await expect
      .poll(() =>
        page.evaluate(
          () => (document.activeElement as HTMLElement | null)?.getAttribute("data-dialog-confirm") === "true",
        ),
      )
      .toBeTruthy();

    const geometry = await dialog.evaluate((element) => {
      const rect = element.getBoundingClientRect();
      const titlebar = element.querySelector<HTMLElement>("[data-dialog-titlebar]")!;
      const style = getComputedStyle(element);
      return {
        width: rect.width,
        titlebarHeight: titlebar.getBoundingClientRect().height,
        radius: style.borderRadius,
        shadow: style.boxShadow,
      };
    });
    expect(geometry.width).toBeGreaterThanOrEqual(320);
    expect(geometry.width).toBeLessThanOrEqual(420);
    expect(geometry.titlebarHeight).toBe(36);
    expect(geometry.radius).toBe("8px");
    expect(geometry.shadow).not.toBe("none");
    await dialog.screenshot({ path: testInfo.outputPath("about-dialog.png") });

    await page.keyboard.press("Escape");
    await expect(dialog).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Help" })).toBeFocused();
  });

  test("delete layer removes immediately without confirmation dialog", async ({ page }) => {
    await page.goto("/");
    await createBlankCanvas(page);
    // 1 layer from blank canvas
    await expect(page.locator("[data-layer-idx]")).toHaveCount(1);

    // Add a second layer so Delete is not disabled
    await page.getByRole("button", { name: "New Layer" }).click();
    await expect(page.locator("[data-layer-idx]")).toHaveCount(2);

    // Delete second layer — immediate, no confirmation dialog
    await page.getByRole("button", { name: "Delete Layer" }).click();
    await expect(page.locator("[data-layer-idx]")).toHaveCount(1);
  });

  test("Resize and Export share the compact frame and complete keyboard workflows", async ({ page }, testInfo) => {
    await page.goto("/");
    await createBlankCanvas(page);

    await page.getByRole("button", { name: "Image" }).click();
    await page.getByRole("menuitem", { name: "Resize Canvas" }).click();
    const resize = page.getByRole("dialog", { name: "Resize Canvas" });
    const width = resize.getByLabel("Width");
    await expect(width).toBeFocused();
    await expect(width).toHaveValue("1080");
    await page.keyboard.press("Shift+Tab");
    await expect(resize.getByRole("button", { name: "Resize", exact: true })).toBeFocused();
    await page.keyboard.press("Tab");
    await expect(width).toBeFocused();
    await width.fill("640");
    // commitW runs on blur/Enter (not on input); move focus to link Height.
    await width.press("Tab");
    await expect(resize.getByLabel("Height")).toHaveValue("640");
    await resize.screenshot({ path: testInfo.outputPath("resize-canvas-dialog.png") });
    await resize.getByRole("button", { name: "Resize", exact: true }).click();
    await expect(resize).toHaveCount(0);

    await page.getByRole("button", { name: "File" }).click();
    await page.getByRole("menuitem", { name: "Export" }).click();
    const exportDialog = page.getByRole("dialog", { name: "Export Image" });
    // Initial focus lands on the format dropdown (the dialog's primary action).
    // Verified via document.activeElement because headless toBeFocused on a
    // programmatically-focused button is flaky even when the element is active.
    await expect
      .poll(() =>
        page.evaluate(
          () => (document.activeElement as HTMLElement | null)?.getAttribute("aria-haspopup") === "listbox",
        ),
      )
      .toBeTruthy();
    // Open the format dropdown, then select JPEG
    await exportDialog.locator('button[aria-haspopup="listbox"]').click();
    await page.getByRole("button", { name: "JPEG" }).click();
    await expect(exportDialog.getByLabel("Quality")).toHaveValue("90");
    await expect(exportDialog.locator("[data-dialog-titlebar]")).toHaveCSS("height", "36px");
    await exportDialog.screenshot({ path: testInfo.outputPath("export-image-dialog.png") });
    // Refocus inside the dialog so Escape is captured
    await exportDialog.locator('button[aria-haspopup="listbox"]').focus();
    await page.keyboard.press("Escape");
    await expect(exportDialog).toHaveCount(0);
    await expect(page.getByRole("button", { name: "File" })).toBeFocused();
  });
});
