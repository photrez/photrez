import { createContext, useContext, type JSX } from "solid-js";
import type { Accessor, Setter } from "solid-js";
import type { LayerNode, DocumentTabSummary, SelectionState } from "@/engine/types";

/**
 * Document-derived canvas state: tabs, layers, selection, hover, dimensions,
 * zoom/pan. Kept in sync by workspaceSync (see EditorContext.tsx).
 * Domain context of EditorProvider (see EditorContext.tsx for composition).
 */
export interface DocumentStateValue {
  zoom: Accessor<number>;
  setZoom: Setter<number>;
  pan: Accessor<{ x: number; y: number }>;
  setPan: Setter<{ x: number; y: number }>;

  documents: Accessor<DocumentTabSummary[]>;
  activeDocumentId: Accessor<string | null>;
  layers: Accessor<LayerNode[]>;
  activeLayerId: Accessor<string | null>;
  selectedLayerId: Accessor<string | null>;
  setSelectedLayerId: Setter<string | null>;
  selection: Accessor<SelectionState | null>;
  setSelection: Setter<SelectionState | null>;
  selectionEditMode: Accessor<boolean>;
  setSelectionEditMode: Setter<boolean>;
  selectionConstraintMode: Accessor<"normal" | "ratio" | "size">;
  setSelectionConstraintMode: Setter<"normal" | "ratio" | "size">;
  selectionRatioW: Accessor<number>;
  setSelectionRatioW: Setter<number>;
  selectionRatioH: Accessor<number>;
  setSelectionRatioH: Setter<number>;
  selectionSizeW: Accessor<number>;
  setSelectionSizeW: Setter<number>;
  selectionSizeH: Accessor<number>;
  setSelectionSizeH: Setter<number>;
  selectionShape: Accessor<"rect" | "ellipse">;
  setSelectionShape: Setter<"rect" | "ellipse">;
  hoveredLayerId: Accessor<string | null>;
  setHoveredLayerId: Setter<string | null>;
  hoverHandle: Accessor<string | null>;
  setHoverHandle: Setter<string | null>;
  // Rotate cursor hover position (screen-space)
  hoverPos: Accessor<{ x: number; y: number } | null>;
  setHoverPos: Setter<{ x: number; y: number } | null>;
  docWidth: Accessor<number>;
  docHeight: Accessor<number>;
  viewportWidth: Accessor<number>;
  setViewportWidth: Setter<number>;
  viewportHeight: Accessor<number>;
  setViewportHeight: Setter<number>;
}

export const DocumentStateContext = createContext<DocumentStateValue>();

export function DocumentStateProvider(props: { value: DocumentStateValue; children: JSX.Element }) {
  return (
    <DocumentStateContext.Provider value={props.value}>
      {props.children}
    </DocumentStateContext.Provider>
  );
}

export function useDocumentState(): DocumentStateValue {
  const ctx = useContext(DocumentStateContext);
  if (!ctx) {
    throw new Error("useDocumentState must be used within an EditorProvider");
  }
  return ctx;
}
