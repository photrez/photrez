// SPDX-License-Identifier: AGPL-3.0-or-later
//
// The properties panel's two color controls (text color, text stroke color) drive
// the same interaction: a non-modal picker that emits onChange on every HSV tick
// and once at mount, then resolves with the chosen color - or null when the user
// cancels.
//
// On a layer the native arm owns, one command per tick would dispatch a burst of
// commands from the same tick, and every one of them is built from the version the
// facade held when the burst started, so the arm rejects all but the first (see the
// per-document chain note in facadeRegistry). The ticks therefore stay transient -
// the model and its raster move, the engine does not - and ONE command fires at the
// interaction boundary. A cancel re-runs the tick with the starting color so the
// model is not left ahead of the engine either. This is the boundary rule the
// opacity slider already follows. The legacy path (flag OFF, or a layer the arm
// does not own) keeps its per-tick commit untouched.
import type { DocumentEngine } from "@/engine/document";
import { isFacadeOwnedLayer } from "@/engine/document";
import type { TextData, TextStroke } from "@/engine/textTypes";
import { isFacadeEnabled } from "@/lib/protocol/facadeRegistry";
import type { ColorPickerDialogOptions } from "../dialogs/DialogProvider";
import {
  commitTextParamsEdit,
  type TextDataEditDeps,
  type TextParamsRouterDeps,
} from "./paramsRouting";

export interface TextColorPickerHost {
  /** The dialog opener (DialogProvider's `colorPicker`). */
  openPicker: (options: ColorPickerDialogOptions) => Promise<string | null>;
  workspace: {
    getActiveEngine(): DocumentEngine | null;
    getActiveHistory(): { commit(snapshot: unknown, label: string): void } | null;
    notifyVisualChange(): void;
  };
  renderer?: TextParamsRouterDeps["renderer"];
  scheduler?: TextParamsRouterDeps["scheduler"];
  selectedLayerId: () => string | null;
  sessionLayerId: () => string | null;
  setColorPickerOpen: (open: boolean) => void;
  setColorPickerTarget: (target: "foreground") => void;
}

export interface TextColorPicker {
  pickTextColor: (currentColor: string) => Promise<void>;
  pickTextStrokeColor: (currentColor: string, stroke: TextStroke) => Promise<void>;
}

export function createTextColorPicker(host: TextColorPickerHost): TextColorPicker {
  const editDeps = (transient: boolean): TextDataEditDeps => ({
    history: host.workspace.getActiveHistory(),
    renderer: host.renderer,
    scheduler: host.scheduler,
    notifyVisualChange: () => host.workspace.notifyVisualChange(),
    sessionLayerId: host.sessionLayerId(),
    transient,
  });

  const pick = async (opts: {
    title: string;
    initialColor: string;
    patch: (color: string) => Partial<TextData>;
    label: string;
  }): Promise<void> => {
    const engine = host.workspace.getActiveEngine();
    const layerId = host.selectedLayerId();
    if (!engine || !layerId) return;
    const routed = isFacadeEnabled() && isFacadeOwnedLayer(layerId);

    host.setColorPickerOpen(true);
    host.setColorPickerTarget("foreground");
    const chosen = await host.openPicker({
      title: opts.title,
      initialColor: opts.initialColor,
      target: "foreground",
      onChange: (color) =>
        commitTextParamsEdit(engine, layerId, opts.patch(color), opts.label, editDeps(routed)),
    });

    if (chosen) {
      commitTextParamsEdit(engine, layerId, opts.patch(chosen), opts.label, editDeps(false));
    } else if (routed) {
      commitTextParamsEdit(engine, layerId, opts.patch(opts.initialColor), opts.label, editDeps(true));
    }
    host.setColorPickerOpen(false);
  };

  return {
    pickTextColor: (currentColor: string) =>
      pick({
        title: "Text Color",
        initialColor: currentColor,
        patch: (color) => ({ color }),
        label: "Change Text Color",
      }),
    pickTextStrokeColor: (currentColor: string, stroke: TextStroke) =>
      pick({
        title: "Text Stroke Color",
        initialColor: currentColor,
        patch: (color) => ({ stroke: { ...stroke, color } }),
        label: "Change Stroke Color",
      }),
  };
}
