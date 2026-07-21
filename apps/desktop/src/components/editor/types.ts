import type { IconName } from "./icons";
import type { ToolId } from "./tools/toolTypes";

export type MenuItem = "File" | "Edit" | "Image" | "Layer" | "View" | "Window" | "Help";

export type DocumentTab = {
  id: string;
  label: string;
  active?: boolean;
};

export type ToolItem = {
  id: ToolId;
  icon: IconName;
  label: string;
  active?: boolean;
  /** Sub-variants shown on right-click (tool-group fly-out). When present the
   *  visible icon/label follows the active variant. */
  variants?: Array<{ id: string; icon: IconName; label: string }>;
};

export type LayerItem = {
  id: string;
  name: string;
  active?: boolean;
  adjustment?: boolean;
  mask?: boolean;
  locked?: boolean;
  thumbnailPosition: string;
};

export type InspectorTab = {
  id: string;
  label: string;
  active?: boolean;
};

export type StatusItem = {
  id: string;
  label: string;
  icon?: IconName;
  hideBelow?: "sm" | "md";
};
