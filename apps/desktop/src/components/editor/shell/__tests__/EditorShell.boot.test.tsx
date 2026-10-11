// apps/desktop/src/components/editor/shell/__tests__/EditorShell.boot.test.tsx
//
// DoD wiring test (AGENTS.md "method defined but no call site proven" gap):
// EditorShell's boot path MUST reach ensureFacadeReady() - the producer call-site
// that arms the bridge. This proves the arming call-site is reachable from the
// real EditorShell mount - not merely that the symbol exists.
//
// The retired `photrez.facade` opt-out used to gate this arm, so boot only armed
// the bridge when the key was absent or non-"0". With the key retired the arm is
// unconditional, which is what the shipped profile already did. The second case
// below writes the retired key's opt-out value and asserts the arm STILL fires:
// a re-gated boot path would fail it, so this pair is also the guard against the
// flag coming back through this file.
//
// ensureFacadeReady is mocked to decouple the mount from real wasm loading; the
// OTHER bridge exports (including the retired flag predicate, which reads
// localStorage) stay real.

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

  it("shipped profile -> EditorShell boot calls ensureFacadeReady() (arms the bridge)", () => {
    mountShell();
    expect(ensureFacadeReadySpy).toHaveBeenCalledTimes(1);
  });

  it("no key read: the retired photrez.facade opt-out cannot suppress the boot arm", () => {
    // Writes the old opt-out value so that a boot path which reads the key again
    // (the pre-retirement `if (isFacadeEnabled())` shape) would skip the arm and
    // fail here. The retired-key write is the point of this case, not a setting.
    localStorage.setItem("photrez.facade", "0");
    mountShell();
    expect(ensureFacadeReadySpy).toHaveBeenCalledTimes(1);
  });
});
