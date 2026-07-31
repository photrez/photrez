import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render } from "solid-js/web";
import { ErrorBoundary } from "../ErrorBoundary";

function mount(children: () => any, fallback = (err: Error, reset: () => void) => <button onClick={reset}>reset</button>) {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ErrorBoundary fallback={(err, reset) => <div>{fallback(err, reset)}</div>}>
        {children()}
      </ErrorBoundary>
    ),
    container,
  );
  return { container, dispose };
}

/** Throws on render until `ok` flips true. */
function Bomb(props: { ok: () => boolean }) {
  if (!props.ok()) throw new Error("boom");
  return <div>safe content</div>;
}

describe("ErrorBoundary", () => {
  beforeEach(() => {
    vi.spyOn(window, "addEventListener");
    vi.spyOn(window, "removeEventListener");
  });
  afterEach(() => {
    vi.restoreAllMocks();
    document.body.innerHTML = "";
  });

  it("shows fallback when child render throws", () => {
    let ok = false;
    const { container } = mount(() => <Bomb ok={() => ok} />);
    expect(container.textContent).toContain("reset");
    expect(container.textContent).not.toContain("safe content");
  });

  it("reset re-renders children after render error", () => {
    let ok = false;
    const { container } = mount(() => <Bomb ok={() => ok} />);

    // Fix the failure, then reset via fallback callback.
    ok = true;
    (container.querySelector("button") as HTMLButtonElement).click();
    expect(container.textContent).toContain("safe content");
    expect(container.textContent).not.toContain("reset");
  });

  it("catches uncaught window errors (async) via the same fallback", () => {
    const { container } = mount(() => <div>happy path</div>);
    expect(container.textContent).toContain("happy path");

    window.dispatchEvent(new ErrorEvent("error", { message: "async broke", error: new Error("async broke") }));
    expect(container.textContent).toContain("reset");
    expect(container.textContent).not.toContain("happy path");
  });

  it("reset clears async errors and restores children", () => {
    const { container } = mount(() => <div>happy path</div>);
    window.dispatchEvent(new ErrorEvent("error", { message: "async broke" }));

    (container.querySelector("button") as HTMLButtonElement).click();
    expect(container.textContent).toContain("happy path");
    expect(container.textContent).not.toContain("reset");
  });

  it("registers and cleans up the window error listener", () => {
    const { container, dispose } = mount(() => <div>happy path</div>);
    expect(window.addEventListener).toHaveBeenCalledWith("error", expect.any(Function));

    dispose();
    expect(window.removeEventListener).toHaveBeenCalledWith("error", expect.any(Function));
    expect(container.textContent).toBe("");
  });
});
