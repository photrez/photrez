import { afterEach, describe, expect, it } from "vitest";
import { render } from "solid-js/web";
import { BottomStatusBar } from "../BottomStatusBar";
import { EditorProvider } from "../EditorContext";
import { I18nProvider } from "@/i18n/I18nProvider";
import i18n from "@/i18n";
import { WorkspaceManager } from "@/engine/workspace";
import { WebGL2Backend } from "@/renderer/webgl2";
import { RenderScheduler } from "@/renderer/scheduler";
import { ViewportCamera } from "@/viewport/viewportCamera";

const tick = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

describe("BottomStatusBar i18n", () => {
  let dispose: (() => void) | null = null;

  afterEach(async () => {
    dispose?.();
    dispose = null;
    document.body.replaceChildren();
    await i18n.changeLanguage("en");
  });

  it("renders localized status labels and updates when locale changes", async () => {
    const workspace = new WorkspaceManager();
    const camera = new ViewportCamera();
    const renderer = new WebGL2Backend();
    const scheduler = new RenderScheduler(() => {});
    workspace.addDocument(WorkspaceManager.createBlankDocument("i18n-status", "I18n Status", 800, 600));

    const container = document.createElement("div");
    document.body.appendChild(container);
    dispose = render(
      () => (
        <I18nProvider>
          <EditorProvider workspace={workspace} renderer={renderer} scheduler={scheduler} camera={camera}>
            <BottomStatusBar />
          </EditorProvider>
        </I18nProvider>
      ),
      container,
    );

    // Default locale (en)
    expect(container.textContent).toContain("Canvas:");
    expect(container.textContent).toContain("Zoom:");
    expect(container.textContent).toContain("History");

    // Switch to Indonesian — labels must localize
    await i18n.changeLanguage("id");
    await tick();

    expect(container.textContent).toContain("Kanvas:");
    expect(container.textContent).toContain("Zum:");
    expect(container.textContent).toContain("Riwayat");
  });
});
