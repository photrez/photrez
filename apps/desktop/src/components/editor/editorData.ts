import type {
  DocumentTab,
  InspectorTab,
  LayerItem,
  MenuItem,
  StatusItem,
  ToolItem,
} from "./types";

export const APP_NAME = "photrez";

export const MENU_ITEMS: readonly MenuItem[] = [
  "File",
  "Edit",
  "Image",
  "Layer",
  "View",
  "Window",
  "Help",
] as const;

export const DOCUMENT_TABS: readonly DocumentTab[] = [
  { id: "portrait-retouch", label: "Portrait Retouch" },
  { id: "brand-poster", label: "Brand Poster" },
  { id: "fjord-edit", label: "Norway Fjord Edit", active: true },
  { id: "landing-page", label: "Landing Page Mockup" },
] as const;

export const TOOL_ITEMS: readonly ToolItem[] = [
  { id: "move", icon: "cursor", label: "Move Tool", active: true },
  { id: "selection", icon: "rectangle", label: "Rectangle Select",
    variants: [
      { id: "rect", icon: "rectangle", label: "Rectangle Select" },
      { id: "ellipse", icon: "circle", label: "Elliptical Select" },
    ] },
  { id: "crop", icon: "crop", label: "Crop Tool" },
  { id: "eyedropper", icon: "pipette", label: "Eyedropper Tool" },
  { id: "brush", icon: "brush", label: "Brush Tool" },
  { id: "eraser", icon: "eraser", label: "Eraser Tool" },
] as const;

export const INSPECTOR_TABS: readonly InspectorTab[] = [
  { id: "library", label: "Library" },
  { id: "adjust", label: "Adjust", active: true },
  { id: "presets", label: "Presets" },
] as const;

export const LAYERS: readonly LayerItem[] = [
  {
    id: "color-adjust-1",
    name: "Color Adjust 1",
    active: true,
    adjustment: true,
    thumbnailPosition: "50% 40%",
  },
  {
    id: "mountain",
    name: "Mountain",
    mask: true,
    thumbnailPosition: "50% 30%",
  },
  { id: "village", name: "Village", thumbnailPosition: "70% 80%" },
  {
    id: "water-reflection",
    name: "Water Reflection",
    thumbnailPosition: "50% 95%",
  },
  { id: "sky", name: "Sky", thumbnailPosition: "30% 8%" },
  {
    id: "background",
    name: "Background",
    locked: true,
    thumbnailPosition: "50% 50%",
  },
] as const;

export const STATUS_LEFT_ITEMS: readonly StatusItem[] = [
  { id: "size", label: "1920 × 1280 px" },
  { id: "zoom", label: "41%" },
  { id: "mode", label: "RGB/8" },
  { id: "profile", label: "sRGB IEC61966-2.1", hideBelow: "sm" },
] as const;

export const STATUS_CENTER_ITEMS: readonly StatusItem[] = [
  { id: "tool", label: "Move Tool" },
  { id: "layer", label: "Image Layer" },
] as const;

export const STATUS_RIGHT_ITEMS: readonly StatusItem[] = [
  { id: "snapshots", label: "Snapshots", icon: "camera" },
  { id: "history", label: "History", icon: "history" },
  { id: "assets", label: "Assets", icon: "box" },
] as const;
