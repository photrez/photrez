import { render, screen, fireEvent, cleanup } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { I18nProvider, useI18n } from "../I18nProvider";
import { LanguageSwitcher } from "../LanguageSwitcher";
import i18n from "../index";

function Probe() {
  const { t } = useI18n();
  return <span data-testid="probe">{t("common.cancel")}</span>;
}

function Harness() {
  return (
    <I18nProvider>
      <LanguageSwitcher />
      <Probe />
    </I18nProvider>
  );
}

beforeEach(() => {
  localStorage.clear();
  void i18n.changeLanguage("en");
});

afterEach(() => {
  cleanup();
  void i18n.changeLanguage("en");
});

describe("LanguageSwitcher wiring", () => {
  it("switches locale and updates consumers reactively", async () => {
    render(() => <Harness />);

    // initial: English
    expect(screen.getByText(/Language/)).toBeTruthy();
    expect((await screen.findByTestId("probe")).textContent).toBe("Cancel");

    // open the SelectDropdown trigger and pick Indonesia
    fireEvent.click(screen.getByRole("button", { name: /Language/i }));
    fireEvent.click(await screen.findByRole("button", { name: "Indonesia" }));

    // reactive change: prefix + probe re-render
    expect(await screen.findByText(/Bahasa/)).toBeTruthy();
    expect((await screen.findByTestId("probe")).textContent).toBe("Batal");
  });

  it("renders the option popover in document.body, escaping the component subtree (no overflow clip)", async () => {
    const host = document.createElement("div");
    host.setAttribute("data-overflow-host", "");
    document.body.appendChild(host);
    render(
      () => (
        <I18nProvider>
          <div data-overflow-host>
            <LanguageSwitcher />
          </div>
        </I18nProvider>
      ),
      { container: host },
    );

    fireEvent.click(screen.getByRole("button", { name: /Language/i }));
    const option = await screen.findByRole("button", { name: "Indonesia" });
    // Popover must live on document.body, NOT inside the overflow-hidden host,
    // otherwise the title-bar `overflow-hidden` would clip it.
    expect(option.closest("[data-overflow-host]")).toBeNull();
    host.remove();
  });
});
