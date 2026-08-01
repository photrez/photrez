import { createContext, useContext, type JSX } from "solid-js";
import type { Accessor, Setter } from "solid-js";
import type { HistoryItem } from "@/engine/history";

/**
 * History panel + right dock / inspector layout state.
 * Domain context of EditorProvider (see EditorContext.tsx for composition).
 */
export interface HistoryDockValue {
  historyItems: Accessor<HistoryItem[]>;
  activeHistoryIndex: Accessor<number>;
  navigateHistory: (index: number) => void;
  rightDockPanel: Accessor<"layers" | "history">;
  setRightDockPanel: Setter<"layers" | "history">;

  rightDockOpen: Accessor<boolean>;
  setRightDockOpen: (open: boolean) => void;
  rightDockLayout: Accessor<"side-by-side" | "stacked">;
  setRightDockLayout: (layout: "side-by-side" | "stacked") => void;
  inspectorTab: Accessor<"library" | "adjust" | "presets">;
  setInspectorTab: Setter<"library" | "adjust" | "presets">;
  adjustSubTab: Accessor<"properties" | "adjustments">;
  setAdjustSubTab: Setter<"properties" | "adjustments">;
}

export const HistoryDockContext = createContext<HistoryDockValue>();

export function HistoryDockProvider(props: { value: HistoryDockValue; children: JSX.Element }) {
  return (
    <HistoryDockContext.Provider value={props.value}>
      {props.children}
    </HistoryDockContext.Provider>
  );
}

export function useHistoryDock(): HistoryDockValue {
  const ctx = useContext(HistoryDockContext);
  if (!ctx) {
    throw new Error("useHistoryDock must be used within an EditorProvider");
  }
  return ctx;
}
