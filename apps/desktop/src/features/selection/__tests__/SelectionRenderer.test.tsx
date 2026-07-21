import { describe, it, expect, vi } from "vitest";
import { render } from "solid-js/web";
import { SelectionRenderer } from "../SelectionRenderer";
import { SelectionState } from "../SelectionTypes";

function renderComponent(props: {
  selection: SelectionState | null;
  zoom?: number;
  pan?: { x: number; y: number };
  onHandlePointerDown?: (handleId: string) => void;
  onRotatePointerDown?: () => void;
  editMode?: boolean;
  canvasWidth?: number;
  canvasHeight?: number;
}) {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  container.appendChild(svg);
  const dispose = render(
    () => (
      <SelectionRenderer
        selection={props.selection}
        zoom={props.zoom ?? 1}
        pan={props.pan ?? { x: 0, y: 0 }}
        onHandlePointerDown={props.onHandlePointerDown ?? vi.fn()}
        onRotatePointerDown={props.onRotatePointerDown ?? vi.fn()}
        editMode={props.editMode ?? false}
        canvasWidth={props.canvasWidth}
        canvasHeight={props.canvasHeight}
      />
    ),
    svg,
  );
  return { container, dispose };
}

describe("SelectionRenderer — base state (no edit mode)", () => {
  it("renders nothing when selection is null", () => {
    const { container, dispose } = renderComponent({ selection: null });
    expect(container.querySelector("[data-selection-group]")).toBeNull();
    dispose();
  });

  it("renders marquee rect with screen-space coordinates in base state", () => {
    const sel: SelectionState = { x: 100, y: 50, width: 200, height: 150, angle: 0 };
    const { container, dispose } = renderComponent({ selection: sel, zoom: 2, pan: { x: 10, y: 20 } });
    const rect = container.querySelector("[data-selection-marquee] rect");
    expect(rect).not.toBeNull();
    expect(rect!.getAttribute("x")).toBe("210");
    expect(rect!.getAttribute("y")).toBe("120");
    expect(rect!.getAttribute("width")).toBe("400");
    expect(rect!.getAttribute("height")).toBe("300");
    dispose();
  });

  it("marquee rect uses animate-dash class in base state", () => {
    const sel: SelectionState = { x: 0, y: 0, width: 100, height: 100, angle: 0 };
    const { container, dispose } = renderComponent({ selection: sel });
    expect(container.querySelector("[data-selection-marquee] rect[data-march]")).not.toBeNull();
    dispose();
  });

  it("marquee rect uses non-scaling stroke in base state", () => {
    const sel: SelectionState = { x: 0, y: 0, width: 100, height: 100, angle: 0 };
    const { container, dispose } = renderComponent({ selection: sel, zoom: 4 });
    const rect = container.querySelector("[data-selection-marquee] rect");
    expect(rect!.getAttribute("vector-effect")).toBe("non-scaling-stroke");
    dispose();
  });

  it("renders with rotation group when angle !== 0 (base state)", () => {
    const sel: SelectionState = { x: 0, y: 0, width: 100, height: 100, angle: 45 };
    const { container, dispose } = renderComponent({ selection: sel });
    const g = container.querySelector("[data-selection-group]");
    expect(g).not.toBeNull();
    expect(g!.getAttribute("transform")).toContain("rotate(45");
    dispose();
  });

  it("selection group has data-selection-active=true in base state", () => {
    const sel: SelectionState = { x: 0, y: 0, width: 100, height: 100, angle: 0 };
    const { container, dispose } = renderComponent({ selection: sel });
    const group = container.querySelector("[data-selection-group]");
    expect(group).not.toBeNull();
    expect(group!.getAttribute("data-selection-active")).toBe("true");
    dispose();
  });
});

describe("SelectionRenderer — inverted selection", () => {
  it("renders the canvas boundary plus the excluded inner marquee", () => {
    const selection: SelectionState = {
      x: 10, y: 20, width: 30, height: 40, angle: 0, inverted: true,
    };
    const { container, dispose } = renderComponent({
      selection,
      zoom: 2,
      pan: { x: 5, y: 7 },
      canvasWidth: 100,
      canvasHeight: 80,
    });

    const boundary = container.querySelector("[data-selection-inverted-boundary] rect");
    expect(boundary).not.toBeNull();
    expect(boundary!.getAttribute("x")).toBe("5");
    expect(boundary!.getAttribute("y")).toBe("7");
    expect(boundary!.getAttribute("width")).toBe("200");
    expect(boundary!.getAttribute("height")).toBe("160");
    expect(container.querySelector("[data-selection-marquee]")).not.toBeNull();
    expect(container.querySelector("[data-selection-group]")!.getAttribute("data-selection-inverted")).toBe("true");
    dispose();
  });

  it("does not expose transform handles for a non-rectangular complement", () => {
    const selection: SelectionState = {
      x: 10, y: 20, width: 30, height: 40, angle: 0, inverted: true,
    };
    const { container, dispose } = renderComponent({ selection, editMode: true });
    expect(container.querySelectorAll("[data-handle-id]")).toHaveLength(0);
    expect(container.querySelector("[data-rotation-handle]")).toBeNull();
    dispose();
  });
});

describe("SelectionRenderer — base state: NO transform handles", () => {
  it("base state does NOT render any resize handles", () => {
    const sel: SelectionState = { x: 0, y: 0, width: 100, height: 100, angle: 0 };
    const { container, dispose } = renderComponent({ selection: sel });
    const handles = container.querySelectorAll("[data-handle-id]");
    expect(handles.length).toBe(0);
    dispose();
  });

  it("base state does NOT render rotation handle", () => {
    const sel: SelectionState = { x: 0, y: 0, width: 100, height: 100, angle: 0 };
    const { container, dispose } = renderComponent({ selection: sel });
    const rotHandle = container.querySelector("[data-rotation-handle]");
    expect(rotHandle).toBeNull();
    dispose();
  });

  it("base state does NOT render rotation connector line", () => {
    const sel: SelectionState = { x: 0, y: 0, width: 100, height: 100, angle: 0 };
    const { container, dispose } = renderComponent({ selection: sel });
    const connector = container.querySelector("[data-rotation-connector]");
    expect(connector).toBeNull();
    dispose();
  });

  it("base state has data-mode attribute set to 'base'", () => {
    const sel: SelectionState = { x: 0, y: 0, width: 100, height: 100, angle: 0 };
    const { container, dispose } = renderComponent({ selection: sel });
    const group = container.querySelector("[data-selection-group]");
    expect(group!.getAttribute("data-mode")).toBe("base");
    dispose();
  });
});

describe("SelectionRenderer — edit mode (transform handles visible)", () => {
  it("edit mode renders 8 resize handles as squares", () => {
    const sel: SelectionState = { x: 0, y: 0, width: 100, height: 100, angle: 0 };
    const { container, dispose } = renderComponent({ selection: sel, editMode: true });
    const handles = container.querySelectorAll("[data-handle-id]");
    expect(handles.length).toBe(8);
    handles.forEach((h) => {
      expect(h.tagName.toLowerCase()).toBe("rect");
    });
    dispose();
  });

  it("edit mode marks corner handles with data-handle-type=corner", () => {
    const sel: SelectionState = { x: 0, y: 0, width: 100, height: 100, angle: 0 };
    const { container, dispose } = renderComponent({ selection: sel, editMode: true });
    const nw = container.querySelector('[data-handle-id="nw"]');
    expect(nw!.getAttribute("data-handle-type")).toBe("corner");
    dispose();
  });

  it("edit mode marks edge handles with data-handle-type=edge", () => {
    const sel: SelectionState = { x: 0, y: 0, width: 100, height: 100, angle: 0 };
    const { container, dispose } = renderComponent({ selection: sel, editMode: true });
    const n = container.querySelector('[data-handle-id="n"]');
    expect(n!.getAttribute("data-handle-type")).toBe("edge");
    dispose();
  });

  it("edit mode renders rotation handle as circle", () => {
    const sel: SelectionState = { x: 0, y: 0, width: 100, height: 100, angle: 0 };
    const { container, dispose } = renderComponent({ selection: sel, editMode: true });
    const rotHandle = container.querySelector("[data-rotation-handle]");
    expect(rotHandle).not.toBeNull();
    expect(rotHandle!.tagName.toLowerCase()).toBe("circle");
    dispose();
  });

  it("edit mode renders rotation connector line", () => {
    const sel: SelectionState = { x: 50, y: 100, width: 100, height: 80, angle: 0 };
    const { container, dispose } = renderComponent({ selection: sel, editMode: true });
    const line = container.querySelector("[data-rotation-connector]");
    expect(line).not.toBeNull();
    expect(line!.tagName.toLowerCase()).toBe("line");
    dispose();
  });

  it("edit mode positions nw handle at top-left corner", () => {
    const sel: SelectionState = { x: 50, y: 30, width: 100, height: 80, angle: 0 };
    const { container, dispose } = renderComponent({ selection: sel, editMode: true });
    const nw = container.querySelector('[data-handle-id="nw"]') as SVGRectElement;
    expect(nw.getAttribute("x")).toBe("46");
    expect(nw.getAttribute("y")).toBe("26");
    expect(nw.getAttribute("width")).toBe("8");
    expect(nw.getAttribute("height")).toBe("8");
    dispose();
  });

  it("edit mode positions se handle at bottom-right corner", () => {
    const sel: SelectionState = { x: 50, y: 30, width: 100, height: 80, angle: 0 };
    const { container, dispose } = renderComponent({ selection: sel, editMode: true });
    const se = container.querySelector('[data-handle-id="se"]') as SVGRectElement;
    expect(se.getAttribute("x")).toBe("146");
    expect(se.getAttribute("y")).toBe("106");
    dispose();
  });

  it("edit mode positions n edge handle at top-center", () => {
    const sel: SelectionState = { x: 50, y: 30, width: 100, height: 80, angle: 0 };
    const { container, dispose } = renderComponent({ selection: sel, editMode: true });
    const n = container.querySelector('[data-handle-id="n"]') as SVGRectElement;
    expect(n.getAttribute("x")).toBe("97");
    expect(n.getAttribute("y")).toBe("27");
    dispose();
  });

  it("edit mode rotation handle is positioned above top edge", () => {
    const sel: SelectionState = { x: 50, y: 100, width: 100, height: 80, angle: 0 };
    const { container, dispose } = renderComponent({ selection: sel, editMode: true });
    const rotHandle = container.querySelector("[data-rotation-handle]") as SVGCircleElement;
    expect(rotHandle.getAttribute("cx")).toBe("100");
    expect(rotHandle.getAttribute("cy")).toBe("76");
    dispose();
  });

  it("edit mode renders connector line from top-center to rotation handle (stops at handle edge)", () => {
    const sel: SelectionState = { x: 50, y: 100, width: 100, height: 80, angle: 0 };
    const { container, dispose } = renderComponent({ selection: sel, editMode: true });
    const line = container.querySelector("[data-rotation-connector]");
    expect(line).not.toBeNull();
    expect(line!.tagName.toLowerCase()).toBe("line");
    expect(line!.getAttribute("x1")).toBe("100");
    expect(line!.getAttribute("y1")).toBe("96");
    expect(line!.getAttribute("x2")).toBe("100");
    expect(line!.getAttribute("y2")).toBe("81");
    dispose();
  });

  it("edit mode calls onHandlePointerDown with correct handle id", () => {
    const onHandle = vi.fn();
    const sel: SelectionState = { x: 0, y: 0, width: 100, height: 100, angle: 0 };
    const { container, dispose } = renderComponent({
      selection: sel,
      onHandlePointerDown: onHandle,
      editMode: true,
    });
    const seHandle = container.querySelector('[data-handle-id="se"]');
    expect(seHandle).not.toBeNull();
    seHandle!.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    expect(onHandle).toHaveBeenCalledWith("se");
    dispose();
  });

  it("edit mode calls onRotatePointerDown on rotation handle pointer down", () => {
    const onRotate = vi.fn();
    const sel: SelectionState = { x: 0, y: 0, width: 100, height: 100, angle: 0 };
    const { container, dispose } = renderComponent({
      selection: sel,
      onRotatePointerDown: onRotate,
      editMode: true,
    });
    const rotHandle = container.querySelector("[data-rotation-handle]");
    expect(rotHandle).not.toBeNull();
    rotHandle!.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    expect(onRotate).toHaveBeenCalled();
    dispose();
  });

  it("edit mode has data-mode attribute set to 'edit'", () => {
    const sel: SelectionState = { x: 0, y: 0, width: 100, height: 100, angle: 0 };
    const { container, dispose } = renderComponent({ selection: sel, editMode: true });
    const group = container.querySelector("[data-selection-group]");
    expect(group!.getAttribute("data-mode")).toBe("edit");
    dispose();
  });

  it("edit mode has stronger stroke-width (1.5+) for handles", () => {
    const sel: SelectionState = { x: 0, y: 0, width: 100, height: 100, angle: 0 };
    const { container, dispose } = renderComponent({ selection: sel, editMode: true });
    const nw = container.querySelector('[data-handle-id="nw"]')!;
    const strokeWidth = parseFloat(nw.getAttribute("stroke-width") || "0");
    expect(strokeWidth).toBeGreaterThanOrEqual(1.5);
    dispose();
  });
});

describe("SelectionRenderer — ellipse shape", () => {
  it("renders <ellipse> instead of <rect> inside marquee when shape='ellipse'", () => {
    const sel: SelectionState = { x: 100, y: 50, width: 200, height: 150, angle: 0, shape: "ellipse" };
    const { container, dispose } = renderComponent({ selection: sel });

    // Should have NO <rect> inside marquee
    const rects = container.querySelectorAll("[data-selection-marquee] rect");
    expect(rects.length).toBe(0);

    // Should have <ellipse> elements
    const ellipses = container.querySelectorAll("[data-selection-marquee] ellipse");
    expect(ellipses.length).toBe(2); // double-stroke

    // Verify ellipse cx/cy/rx/ry match the selection geometry
    const first = ellipses[0];
    expect(first.getAttribute("cx")).toBe("200");  // x + w/2 = 100 + 100
    expect(first.getAttribute("cy")).toBe("125");  // y + h/2 = 50 + 75
    expect(first.getAttribute("rx")).toBe("100");  // w/2 = 200/2
    expect(first.getAttribute("ry")).toBe("75");   // h/2 = 150/2

    dispose();
  });

  it("renders <ellipse> marquee with zoom and pan applied", () => {
    const sel: SelectionState = { x: 100, y: 50, width: 200, height: 150, angle: 0, shape: "ellipse" };
    const { container, dispose } = renderComponent({ selection: sel, zoom: 2, pan: { x: 10, y: 20 } });

    const ellipses = container.querySelectorAll("[data-selection-marquee] ellipse");
    expect(ellipses.length).toBe(2);
    const first = ellipses[0];
    // screenX = 100*2 + 10 = 210, screenW = 200*2 = 400 → cx = 210 + 200 = 410
    expect(first.getAttribute("cx")).toBe("410");
    // screenY = 50*2 + 20 = 120, screenH = 150*2 = 300 → cy = 120 + 150 = 270
    expect(first.getAttribute("cy")).toBe("270");
    expect(first.getAttribute("rx")).toBe("200");  // 400/2
    expect(first.getAttribute("ry")).toBe("150");  // 300/2

    dispose();
  });
});
