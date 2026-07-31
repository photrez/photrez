import { describe, expect, it, vi } from "vitest";
import { runStartupOpenChain, type StartupOpenChainDeps } from "../startupOpenChain";

function makeDeps(overrides: Partial<StartupOpenChainDeps> = {}): StartupOpenChainDeps & {
  calls: string[];
} {
  const calls: string[] = [];
  const defaults: StartupOpenChainDeps = {
    getPendingOpenPath: vi.fn(async () => {
      calls.push("cli");
      return { path: null };
    }),
    openSingleFile: vi.fn(async () => {
      calls.push("open");
    }),
    listAutosaves: vi.fn(async () => {
      calls.push("list");
      return [];
    }),
    askRecover: vi.fn(async () => {
      calls.push("ask");
      return false;
    }),
    recoverAutosave: vi.fn(async () => {
      calls.push("recover");
    }),
    clearAutosaves: vi.fn(async () => {
      calls.push("clear");
    }),
    onError: vi.fn(),
    onRecovered: vi.fn(),
    onRecoverFailed: vi.fn(),
  };
  return {
    calls,
    ...defaults,
    // Wrap override functions in spies so toHaveBeenCalled* assertions work.
    ...Object.fromEntries(
      Object.entries(overrides).map(([key, value]) => [
        key,
        typeof value === "function" ? vi.fn(value) : value,
      ]),
    ),
  } as StartupOpenChainDeps & { calls: string[] };
}

describe("runStartupOpenChain", () => {
  it("runs the CLI open before listing autosaves (serialized, not raced)", async () => {
    let releaseList: () => void = () => {};
    const deps = makeDeps({
      getPendingOpenPath: async () => {
        deps.calls.push("cli");
        await new Promise<void>((r) => (releaseList = r));
        return { path: "C:\\photo.png" };
      },
      openSingleFile: async () => {
        deps.calls.push("open");
      },
      listAutosaves: async () => {
        deps.calls.push("list");
        return [];
      },
    });

    const promise = runStartupOpenChain(deps);
    // listAutosaves must NOT have been called while the CLI open is still pending.
    expect(deps.calls).toEqual(["cli"]);
    releaseList();
    await promise;
    expect(deps.calls).toEqual(["cli", "open", "list"]);
  });

  it("opens the CLI file when a path is present", async () => {
    const deps = makeDeps({
      getPendingOpenPath: async () => ({ path: "C:\\photo.png" }),
      openSingleFile: vi.fn(async () => {}),
    });
    await runStartupOpenChain(deps);
    expect(deps.openSingleFile).toHaveBeenCalledWith("C:\\photo.png");
    expect(deps.onError).not.toHaveBeenCalled();
  });

  it("does not open a file when the CLI path is null", async () => {
    const deps = makeDeps();
    await runStartupOpenChain(deps);
    expect(deps.openSingleFile).not.toHaveBeenCalled();
  });

  it("reports a CLI open failure but still runs recovery", async () => {
    const deps = makeDeps({
      getPendingOpenPath: async () => ({ path: "C:\\broken.png" }),
      openSingleFile: async () => {
        throw new Error("boom");
      },
      listAutosaves: async () => [{ docId: "a", displayName: "A", path: "a.ptz" }],
      askRecover: async () => true,
      recoverAutosave: async () => {},
      clearAutosaves: async () => {},
    });
    await runStartupOpenChain(deps);
    expect(deps.onError).toHaveBeenCalledWith("Failed to open file from command line: boom");
    expect(deps.recoverAutosave).toHaveBeenCalledWith({
      docId: "a",
      displayName: "A",
      path: "a.ptz",
    });
  });

  it("clears autosaves even when the user declines recovery", async () => {
    const deps = makeDeps({
      listAutosaves: async () => [{ docId: "a", displayName: "A", path: "a.ptz" }],
      askRecover: async () => false,
      clearAutosaves: async () => {},
    });
    await runStartupOpenChain(deps);
    expect(deps.recoverAutosave).not.toHaveBeenCalled();
    expect(deps.clearAutosaves).toHaveBeenCalledOnce();
  });

  it("reports per-entry recovery failures and keeps clearing", async () => {
    const deps = makeDeps({
      listAutosaves: async () => [
        { docId: "a", displayName: "A", path: "a.ptz" },
        { docId: "b", displayName: "B", path: "b.ptz" },
      ],
      askRecover: async () => true,
      recoverAutosave: async (e) => {
        if (e.docId === "b") throw new Error("corrupt");
      },
      clearAutosaves: async () => {},
    });
    await runStartupOpenChain(deps);
    expect(deps.onRecovered).toHaveBeenCalledWith(2);
    expect(deps.onRecoverFailed).toHaveBeenCalledWith(
      { docId: "b", displayName: "B", path: "b.ptz" },
      "corrupt",
    );
    expect(deps.clearAutosaves).toHaveBeenCalledOnce();
  });
});
