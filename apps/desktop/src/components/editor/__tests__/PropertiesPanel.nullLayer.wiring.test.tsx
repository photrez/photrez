// SPDX-License-Identifier: AGPL-3.0-or-later

// Closing the last document must not kill the editor shell.  Real app, real crash:
//
//   [Unhandled error] TypeError: Cannot read properties of null (reading 'width')
//    > width  src/components/editor/PropertiesPanel.tsx:748:70
//    > value  src/components/editor/primitives.tsx:245:34
//
// which drops the UI into the full-shell `Kesalahan Editor / Coba Lagi` boundary.
//
// WHY THE <Show> GUARD IS NOT A NULL GUARD.  The panel already has the right empty
// state - `<Show when={safeLayer()} fallback={<CanvasProperties />}>`.  That guards the
// FIRST render only.  A reactive prop inside the still-mounted Transform branch is read
// again while that Show is being disposed (a field being committed reads `props.value` in
// EditableNumField.commit(), primitives.tsx:245), and by then `safeLayer()` is null.  So
// the VALUES must tolerate null, not the container.
//
// THE TRIGGER USED HERE, and why it is not the document-close teardown.  Driving the real
// close (clearing the selection AND the active document id) sends Solid into runaway
// re-evaluation and OOM-kills the vitest worker, which is not a usable RED.  Instead this
// reproduces the one thing the defect actually is - a null dereference of the values - by
// reading them while the layer is gone but the branch is still live: inside `batch()` the
// signal write is visible to an untracked memo read immediately, while the Show's disposal
// is still queued at the end of the batch.  The reported TypeError is then produced by the
// real production code path, not by a stand-in.
//
// The negative-dimension hazard (typing -5 into W) is a DIFFERENT defect and is not
// exercised here.

import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { batch } from "solid-js";
import { EditorProvider, useEditor } from "../shell/EditorContext";
import { PropertiesPanel } from "../PropertiesPanel";
import { WorkspaceManager } from "@/engine/workspace";

interface PanelField {
  label: string;
  value: string;
  disabled: boolean;
}

/** The number fields of the Transform/Appearance sections, in DOM order. */
function panelFields(container: HTMLElement): PanelField[] {
  return Array.from(container.querySelectorAll<HTMLInputElement>("input[type='text']")).map((input) => ({
    label: input.parentElement?.querySelector("span")?.textContent ?? "?",
    value: input.value,
    disabled: input.disabled,
  }));
}

function fieldInputs(container: HTMLElement): HTMLInputElement[] {
  return Array.from(container.querySelectorAll<HTMLInputElement>("input[type='text']"));
}

function byLabel(container: HTMLElement, label: string): HTMLInputElement {
  const input = fieldInputs(container).find((i) => i.parentElement?.querySelector("span")?.textContent === label);
  if (!input) throw new Error(`no field labelled ${label} in the panel`);
  return input;
}

function mountPanel(docId: string, opts: { selectLayer: boolean }) {
  const workspace = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument(docId, "Props", 120, 90);
  workspace.addDocument(session);
  const engine = session.engine;

  // A non-background layer, so `locked` is false.  On the background layer the live
  // `locked` value and the null-case default (`locked ?? true`) could not be told apart.
  const layer = engine.addLayer("Subject", 120, 90);
  engine.transformLayer(layer.id, { x: 17, y: 23, scaleX: 1.5, scaleY: 2, rotation: 33 });

  const renderer = {
    uploadImage: vi.fn(),
    destroyTexture: vi.fn(),
    resize: vi.fn(),
    resizeToViewport: vi.fn(),
  };
  const scheduler = { requestRender: vi.fn() };
  const container = document.createElement("div");
  document.body.appendChild(container);

  let clearSelection = () => {};
  const dispose = render(
    () => (
      <EditorProvider workspace={workspace} renderer={renderer as any} scheduler={scheduler as any}>
        <Harness
          layerId={opts.selectLayer ? layer.id : null}
          onReady={(clear) => {
            clearSelection = clear;
          }}
        />
      </EditorProvider>
    ),
    container,
  );

  return {
    container,
    dispose,
    engine,
    layer,
    clearSelection: () => clearSelection(),
  };
}

function Harness(props: { layerId: string | null; onReady: (clear: () => void) => void }) {
  const editor = useEditor();
  if (props.layerId !== null) editor.setSelectedLayerId(props.layerId);
  props.onReady(() => editor.setSelectedLayerId(null));
  return <PropertiesPanel />;
}

/**
 * Commit the W field the way a user does (focus, type, Enter) at the exact moment the
 * selection is cleared, and collect any error the runtime reports for it.  An exception
 * thrown inside a DOM event listener does not propagate out of `dispatchEvent`, so it has
 * to be caught as an unhandled error rather than with try/catch.
 */
function commitWFieldInTheTeardownWindow(
  container: HTMLElement,
  clearSelection: () => void,
): { messages: string[]; thrown: string[] } {
  const input = byLabel(container, "W");
  input.focus();
  input.value = "7";
  input.dispatchEvent(new Event("input", { bubbles: true }));

  const messages: string[] = [];
  const onError = (e: ErrorEvent) => messages.push(`${e.message}\n${e.error?.stack ?? ""}`);
  const onUncaught = (e: Error) => messages.push(`${e.message}\n${e.stack ?? ""}`);
  window.addEventListener("error", onError);
  process.on("uncaughtException", onUncaught);

  try {
    batch(() => {
      clearSelection();
      input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    });
  } finally {
    window.removeEventListener("error", onError);
    process.off("uncaughtException", onUncaught);
  }

  return { messages, thrown: messages.filter((m) => /Cannot read properties of null/.test(m)) };
}

describe("PropertiesPanel values tolerate a missing layer", () => {
  afterEach(() => {
    document.body.replaceChildren();
    vi.restoreAllMocks();
  });

  it("commits a Transform field while the layer is gone without dereferencing null", () => {
    const ctx = mountPanel("props-null-teardown", { selectLayer: true });
    try {
      // Premise, asserted so this cannot pass by never reaching the field at all.
      const before = panelFields(ctx.container);
      expect(before.map((f) => f.label), "the Transform section must be mounted first").toContain("W");
      expect(byLabel(ctx.container, "W").disabled, "an unlocked layer keeps its fields live").toBe(false);

      const { thrown } = commitWFieldInTheTeardownWindow(ctx.container, ctx.clearSelection);

      expect(
        thrown,
        "clearing the selection and committing a Transform field must not dereference the missing layer",
      ).toEqual([]);

      // And the guard is not a blanking: with no layer the Transform section is gone and the
      // canvas empty state has taken over.
      expect(panelFields(ctx.container).map((f) => f.label), "no Transform fields without a layer").not.toContain("W");
      expect(
        Array.from(ctx.container.querySelectorAll("button")).some((b) => /^\d+ .+ \d+ px$/.test(b.textContent ?? "")),
        "the canvas Size row is the empty state",
      ).toBe(true);
    } finally {
      ctx.dispose();
    }
  });

  it("hands over to the canvas empty state when the active layer goes away", () => {
    // Not a falsification case: clearing the selection outside a batch disposes the branch
    // before its effects run, so this passes on the broken tree too.  It is here so the
    // null-tolerant read cannot be mistaken for a guard that leaves the panel blank.
    const ctx = mountPanel("props-null-empty", { selectLayer: true });
    try {
      expect(panelFields(ctx.container).map((f) => f.label), "premise: a layer is selected").toContain("W");

      ctx.clearSelection();

      const labels = panelFields(ctx.container).map((f) => f.label);
      expect(labels, "no Transform fields without a layer").not.toContain("W");
      expect(labels, "no Position field without a layer").not.toContain("X");
      expect(
        Array.from(ctx.container.querySelectorAll("button")).some((b) => /^\d+ .+ \d+ px$/.test(b.textContent ?? "")),
        "the canvas Size row is the empty state",
      ).toBe(true);
    } finally {
      ctx.dispose();
    }
  });

  it("with a live layer every displayed value is byte-equal to the pre-change output", () => {
    // A null guard that blanks the panel whenever a layer IS present would sail past the
    // two cases above and be worse than the crash, so the live values are pinned both
    // against the engine and against the literals recorded from the pre-change tree.
    const ctx = mountPanel("props-null-live", { selectLayer: true });
    try {
      const t = ctx.engine.getLayer(ctx.layer.id)!.transform;
      const w = ctx.layer.width * t.scaleX;
      const h = ctx.layer.height * t.scaleY;

      // Recorded from the pre-change tree at fd47ca01.
      expect(byLabel(ctx.container, "X").value).toBe("17");
      expect(byLabel(ctx.container, "Y").value).toBe("23");
      expect(byLabel(ctx.container, "W").value).toBe("180");
      expect(byLabel(ctx.container, "H").value).toBe("180");
      expect(byLabel(ctx.container, "R").value).toBe("33");

      // And the same numbers derived from the engine, so the literals above cannot rot
      // into passing on their own.
      const shown = panelFields(ctx.container).map((f) => f.value);
      expect(shown).toContain(String(t.x));
      expect(shown).toContain(String(t.y));
      expect(shown).toContain(String(w));
      expect(shown).toContain(String(h));
      expect(shown).toContain(String(t.rotation));

      // An unlocked layer keeps every lock-derived field enabled.  If the null default
      // (`locked ?? true`) leaked into the live case, these would read `true`.
      for (const f of panelFields(ctx.container)) {
        expect(f.disabled, `${f.label} must stay enabled while the layer is unlocked`).toBe(false);
      }
    } finally {
      ctx.dispose();
    }
  });
});
