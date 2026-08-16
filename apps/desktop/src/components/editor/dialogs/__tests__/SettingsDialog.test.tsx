import { render, screen, fireEvent, cleanup } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { I18nProvider } from "@/i18n/I18nProvider";
import { DialogProvider, useDialog } from "../DialogProvider";
import i18n from "@/i18n";

function Opener() {
  const dialog = useDialog();
  return <button data-testid="open" onClick={() => void dialog.settings()}>open</button>;
}

beforeEach(() => {
  localStorage.clear();
  void i18n.changeLanguage("en");
});

afterEach(() => {
  cleanup();
  void i18n.changeLanguage("en");
});

describe("SettingsDialog", () => {
  it("opens via dialog.settings() and exposes the language control", async () => {
    render(() => (
      <I18nProvider>
        <DialogProvider>
          <Opener />
        </DialogProvider>
      </I18nProvider>
    ));
    fireEvent.click(screen.getByTestId("open"));
    await new Promise<void>((resolve) => queueMicrotask(() => resolve()));

    expect(document.querySelector('[data-dialog-kind="settings"]')).toBeTruthy();
    expect(screen.getByText("Interface")).toBeTruthy();
    expect(screen.getByRole("button", { name: /Language/i })).toBeTruthy();
  });

  it("switches locale from within the dialog and re-renders the title", async () => {
    render(() => (
      <I18nProvider>
        <DialogProvider>
          <Opener />
        </DialogProvider>
      </I18nProvider>
    ));
    fireEvent.click(screen.getByTestId("open"));
    await new Promise<void>((resolve) => queueMicrotask(() => resolve()));

    fireEvent.click(screen.getByRole("button", { name: /Language/i }));
    fireEvent.click(await screen.findByRole("button", { name: "Indonesia" }));

    // Title (and section header) re-render to the active locale.
    expect(await screen.findByText("Pengaturan")).toBeTruthy();
  });
});
