// apps/desktop/src/components/editor/shell/__tests__/EditorShell.boot.test.tsx
//
// DoD wiring test (AGENTS.md "method defined but no call site proven" gap):
// editorShell.tsx's boot path MUST reach ensureFacadeReady() when photrez.facade=1
// (the producer call-site that arms the bridge), and MUST NOT when the flag is
// OFF. This proves the arming call-site is reachable from the real EditorShell
// mount — not merely that the symbol exists.
//
// ensureFacadeReady is mocked to decouple the mount from real wasm loading; the
// OTHER bridge exports (including isFacadeEnabled, which reads localStorage)
// stay real so the flag branch is the production code path.

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render } from "solid-js/web";
import * as bridge from "@/lib/protocol/bridge";
import { EditorShell } from "../EditorShell";

vi.mock("@/lib/protocol/bridge", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/protocol/bridge")>();
  return {
    ...actual,
    // EditorShell only awaits ensureFacadeReady() (fire-and-forget), so the
    // resolved value is irrelevant; a dummy module decouples the mount from the
    // real (headless-unfriendly) wasm loader.
    ensureFacadeReady: vi.fn(
      async () => ({}) as unknown as Awaited<ReturnType<typeof actual.ensureFacadeReady>>,
    ),
  };
});

const ensureFacadeReadySpy = vi.mocked(bridge.ensureFacadeReady);

describe("EditorShell boot facade-readiness wiring", () => {
  let container: HTMLDivElement;
  let dispose: () => void;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    ensureFacadeReadySpy.mockClear();
  });

  afterEach(() => {
    dispose?.();
    document.body.replaceChildren();
    localStorage.removeItem("photrez.facade");
  });

  function mountShell() {
    dispose = render(() => <EditorShell />, container);
  }

  it("photrez.facade=1 -> EditorShell boot calls ensureFacadeReady() (arms the bridge)", () => {
    localStorage.setItem("photrez.facade", "1");
    mountShell();
    expect(ensureFacadeReadySpy).toHaveBeenCalledTimes(1);
  });

  it("photrez.facade=0 (default) -> EditorShell boot does NOT call ensureFacadeReady()", () => {
    localStorage.removeItem("photrez.facade");
    mountShell();
    expect(ensureFacadeReadySpy).not.toHaveBeenCalled();
  });
});
