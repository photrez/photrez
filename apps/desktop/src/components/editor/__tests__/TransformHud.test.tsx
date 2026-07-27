import { describe, expect, it } from "vitest";
import { render } from "solid-js/web";
import { TransformHud } from "../TransformHud";

describe("TransformHud", () => {
  it("renders 2-line HUD card floating at top-right of cursor pointer", () => {
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <TransformHud
          mode="resize"
          clientX={100}
          clientY={120}
          zoom={0.5}
          width={320}
          height={240}
          scalePercent={125}
          snapActive={true}
        />
      ),
      container,
    );

    const card = container.firstElementChild as HTMLElement;
    expect(card).not.toBeNull();
    expect(card.style.left).toBe("116px"); // 100 + 16 offset
    expect(card.style.top).toBe("106px");  // 120 - 14 offset (with -translate-y-full)

    expect(container.textContent).toContain("320");
    expect(container.textContent).toContain("240");
    expect(container.textContent).toContain("125%");
    expect(container.textContent).toContain("SNAP");

    dispose();
    container.parentNode?.removeChild(container);
  });
});
