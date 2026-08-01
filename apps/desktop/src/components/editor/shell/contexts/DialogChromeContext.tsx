import { createContext, useContext, type JSX } from "solid-js";
import type { Accessor, Setter } from "solid-js";

/**
 * UI chrome state: dialog toggles, loading/renaming transient state,
 * color-picker "pick from canvas" mode, fullscreen chrome visibility.
 * Domain context of EditorProvider (see EditorContext.tsx for composition).
 */
export interface DialogChromeValue {
  colorPickerOpen: Accessor<boolean>;
  setColorPickerOpen: Setter<boolean>;
  colorPickerTarget: Accessor<"foreground" | "background">;
  setColorPickerTarget: Setter<"foreground" | "background">;

  showResizeDialog: Accessor<boolean>;
  setShowResizeDialog: Setter<boolean>;
  showExportDialog: Accessor<boolean>;
  setShowExportDialog: Setter<boolean>;
  showPrintDialog: Accessor<boolean>;
  setShowPrintDialog: Setter<boolean>;

  loadingMessage: Accessor<string | null>;
  setLoadingMessage: Setter<string | null>;
  renamingLayerId: Accessor<string | null>;
  setRenamingLayerId: Setter<string | null>;
  renameLayerName: Accessor<string>;
  setRenameLayerName: Setter<string>;
  chromeVisible: Accessor<boolean>;
  setChromeVisible: Setter<boolean>;
}

export const DialogChromeContext = createContext<DialogChromeValue>();

export function DialogChromeProvider(props: { value: DialogChromeValue; children: JSX.Element }) {
  return (
    <DialogChromeContext.Provider value={props.value}>
      {props.children}
    </DialogChromeContext.Provider>
  );
}

export function useDialogChrome(): DialogChromeValue {
  const ctx = useContext(DialogChromeContext);
  if (!ctx) {
    throw new Error("useDialogChrome must be used within an EditorProvider");
  }
  return ctx;
}
