import { render } from "solid-js/web";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { vi, describe, it, expect, beforeEach, afterEach } from "vitest";
import { EyedropperOptionBar } from "../EyedropperOptionBar";
import { createSignal } from "solid-js";

function createMockEditor(initialColor = "#ff0000") {
  const [fgColor, setFgColor] = createSignal(initialColor);
  return {
    fgColor,
    setFgColor,
  };
}

describe("EyedropperOptionBar", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("renders Eyedropper tool pill and color code", () => {
    const mock = createMockEditor("#ff0000");
    mockUseEditor(mock);

    const root = document.createElement("div");
    document.body.appendChild(root);
    const dispose = render(() => <EyedropperOptionBar />, root);

    expect(root.textContent).toContain("Eyedropper");
    expect(root.textContent).toContain("#ff0000");

    dispose();
    root.remove();
  });

  it("renders Auto-Copy checkbox and toggles it", () => {
    const mock = createMockEditor("#00ff00");
    mockUseEditor(mock);

    const root = document.createElement("div");
    document.body.appendChild(root);
    const dispose = render(() => <EyedropperOptionBar />, root);

    const checkbox = root.querySelector("input[type='checkbox']") as HTMLInputElement;
    expect(checkbox).not.toBeNull();
    expect(checkbox.checked).toBe(false);

    // Simulate toggle
    checkbox.click();
    expect(checkbox.checked).toBe(true);
    expect(localStorage.getItem("photrez_eyedropper_autocopy")).toBe("true");

    dispose();
    root.remove();
  });
});
