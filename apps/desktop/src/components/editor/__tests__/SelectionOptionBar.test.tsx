import { describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { createSignal } from "solid-js";
import { SelectionOptionBar } from "../SelectionOptionBar";
import * as EditorContextModule from "../shell/EditorContext";

function createMockEditor(overrides: Record<string, any> = {}) {
  const defaults: Record<string, any> = {
    activeTool: "selection",
    selection: null,
    selectionEditMode: false,
    selectionShape: "rect",
    selectionConstraintMode: "normal",
    selectionRatioW: 1,
    selectionRatioH: 1,
    selectionSizeW: 100,
    selectionSizeH: 100,
    workspace: {
      getActiveEngine: () => ({
        getSelection: () => null,
        getActiveLayerId: () => null,
        getLayer: () => null,
      }),
      getActiveHistory: () => ({
        commit: () => {},
      }),
    },
    renderer: {
      uploadImage: () => {},
    },
    scheduler: {
      requestRender: () => {},
    },
  };
  const merged = { ...defaults, ...overrides };
  const signals: Record<string, any> = {};
  for (const [key, val] of Object.entries(merged)) {
    if (typeof val === "function" || (val && typeof val === "object" && !("x" in val || "w" in val))) {
      signals[key] = val;
    } else {
      const [s, set] = createSignal(val);
      signals[key] = s;
      const setKey = "set" + key.charAt(0).toUpperCase() + key.slice(1);
      signals[setKey] = set;
    }
  }
  return signals;
}

describe("SelectionOptionBar", () => {
  it("renders with style selector and disabled fields when no selection", () => {
    const mock = createMockEditor({ selection: null });
    vi.spyOn(EditorContextModule, "useEditor").mockReturnValue(mock as any);

    const root = document.createElement("div");
    document.body.appendChild(root);
    const dispose = render(() => <SelectionOptionBar />, root);

    expect(root.textContent).toContain("Selection");
    expect(root.textContent).toContain("Style: Normal");

    // X, Y, W, H inputs should be disabled
    const inputs = root.querySelectorAll("input");
    // Since style is Normal, constraint W/H inputs are not shown.
    // The shown inputs are X, Y, W, H, R.
    expect(inputs.length).toBe(5);
    inputs.forEach((input) => {
      expect(input.disabled).toBe(true);
    });

    // Action buttons like Cut, Copy, Paste, etc. should be disabled/inactive.
    // Query by label (not index) since the marquee-shape toggle buttons
    // precede the action buttons now.
    const buttons = Array.from(root.querySelectorAll("button"));
    const cutBtn = buttons.find((b) => b.textContent?.includes("Cut"));
    expect(cutBtn).toBeDefined();
    expect(cutBtn!.disabled).toBe(true);

    dispose();
    root.remove();
    vi.restoreAllMocks();
  });

  it("enables fields when active selection exists", () => {
    const mock = createMockEditor({
      selection: { x: 10, y: 20, width: 300, height: 200, angle: 0 },
    });
    vi.spyOn(EditorContextModule, "useEditor").mockReturnValue(mock as any);

    const root = document.createElement("div");
    document.body.appendChild(root);
    const dispose = render(() => <SelectionOptionBar />, root);

    const inputs = root.querySelectorAll("input");
    expect(inputs.length).toBe(5);
    inputs.forEach((input) => {
      expect(input.disabled).toBe(false);
    });

    // Values should match the selection bounds
    expect(inputs.item(0).value).toBe("10"); // X
    expect(inputs.item(1).value).toBe("20"); // Y
    expect(inputs.item(2).value).toBe("300"); // W
    expect(inputs.item(3).value).toBe("200"); // H

    dispose();
    root.remove();
    vi.restoreAllMocks();
  });

  it("updates selection constraint mode and shows W/H inputs when style is Fixed Ratio", () => {
    const mock = createMockEditor({
      selectionConstraintMode: "ratio",
      selectionRatioW: 16,
      selectionRatioH: 9,
    });
    vi.spyOn(EditorContextModule, "useEditor").mockReturnValue(mock as any);

    const root = document.createElement("div");
    document.body.appendChild(root);
    const dispose = render(() => <SelectionOptionBar />, root);

    expect(root.textContent).toContain("Style: Fixed Ratio");

    // Because style is ratio, W/H constraint inputs are rendered first, then X, Y, W, H, R.
    // Total inputs = 2 (constraint W, H) + 5 (selection X, Y, W, H, R) = 7 inputs.
    const inputs = root.querySelectorAll("input");
    expect(inputs.length).toBe(7);

    // The first two inputs are constraint W and H, which are enabled
    expect(inputs.item(0).disabled).toBe(false);
    expect(inputs.item(1).disabled).toBe(false);
    expect(inputs.item(0).value).toBe("16");
    expect(inputs.item(1).value).toBe("9");

    // Updating constraint W triggers setSelectionRatioW
    inputs.item(0).dispatchEvent(new Event("focus", { bubbles: true }));
    inputs.item(0).value = "4";
    inputs.item(0).dispatchEvent(new Event("input", { bubbles: true }));
    inputs.item(0).dispatchEvent(new Event("blur", { bubbles: true }));
    expect(mock.selectionRatioW()).toBe(4);

    dispose();
    root.remove();
    vi.restoreAllMocks();
  });

  it("shows size constraint W/H inputs when style is Fixed Size", () => {
    const mock = createMockEditor({
      selectionConstraintMode: "size",
      selectionSizeW: 400,
      selectionSizeH: 300,
    });
    vi.spyOn(EditorContextModule, "useEditor").mockReturnValue(mock as any);

    const root = document.createElement("div");
    document.body.appendChild(root);
    const dispose = render(() => <SelectionOptionBar />, root);

    expect(root.textContent).toContain("Style: Fixed Size");

    const inputs = root.querySelectorAll("input");
    expect(inputs.length).toBe(7);

    expect(inputs.item(0).value).toBe("400");
    expect(inputs.item(1).value).toBe("300");

    inputs.item(1).dispatchEvent(new Event("focus", { bubbles: true }));
    inputs.item(1).value = "250";
    inputs.item(1).dispatchEvent(new Event("input", { bubbles: true }));
    inputs.item(1).dispatchEvent(new Event("blur", { bubbles: true }));
    expect(mock.selectionSizeH()).toBe(250);

    dispose();
    root.remove();
    vi.restoreAllMocks();
  });
});
