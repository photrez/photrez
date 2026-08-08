import type { LucideIcon, LucideProps } from "lucide-solid";
import { Dynamic } from "solid-js/web";
import {
  AlignCenter,
  AlignCenterHorizontal,
  AlignCenterVertical,
  AlignLeft,
  AlignRight,
  AlignStartHorizontal,
  AlignEndHorizontal,
  AlignStartVertical,
  AlignEndVertical,
  ArrowLeftRight,
  ArrowUpRight,
  Aperture,
  Box,
  Brush,
  Camera,
  ChartSpline,
  Check,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  Circle,
  Columns2,
  Contrast,
  Copy,
  Crop,
  Eraser,
  Eye,
  FlipHorizontal,
  FlipVertical,
  FolderPlus,
  Grid2X2,
  Grid3X3,
  History,
  Image as ImageIcon,
  ImagePlus,
  Layers,
  LineChart,
  Link,
  Lock,
  Unlink,
  Maximize2,
  Minus,
  MoreHorizontal,
  MousePointer2,
  Move,
  PaintBucket,
  Palette,
  PanelRightClose,
  PanelRightOpen,
  Pen,
  Pipette,
  Plus,
  RectangleHorizontal,
  Redo2,
  RotateCcw,
  RotateCw,
  Slice,
  Sliders,
  Sparkles,
  SplitSquareHorizontal,
  Square,
  SquareDashed,
  SquarePen,
  Stamp,
  StretchHorizontal,
  Sun,
  SwatchBook,
  Trash2,
  Type,
  Undo2,
  Unlock,
  X,
} from "lucide-solid";

export type IconName =
  | "undo"
  | "redo"
  | "minus"
  | "square"
  | "x"
  | "check"
  | "chevron-down"
  | "chevron-up"
  | "chevron-right"
  | "plus"
  | "link"
  | "unlink"
  | "align-v"
  | "align-h"
  | "align-left"
  | "align-right"
  | "align-text-left"
  | "align-text-center"
  | "align-text-right"
  | "align-top"
  | "align-bottom"
  | "stretch-h"
  | "flip-h"
  | "flip-v"
  | "camera"
  | "history"
  | "box"
  | "cursor"
  | "crop"
  | "rectangle"
  | "line"
  | "slice"
  | "brush"
  | "stamp"
  | "pen"
  | "eraser"
  | "square-pen"
  | "type"
  | "grid-2"
  | "columns"
  | "split-h"
  | "layers"
  | "contrast"
  | "spline"
  | "grid-3"
  | "circle"
  | "more"
  | "move"
  | "pipette"
  | "sun"
  | "palette"
  | "swatch"
  | "sparkles"
  | "aperture"
  | "arrowUpRight"
  | "eye"
  | "lock"
  | "unlock"
  | "folder-plus"
  | "copy"
  | "square-dashed"
  | "trash"
  | "maximize"
  | "paint-bucket"
  | "rotate"
  | "rotate-ccw"
  | "rotate-cw"
  | "swap"
  | "panel-right-open"
  | "panel-right-close"
  | "image"
  | "image-plus"
  | "sliders";

type IconProps = Omit<LucideProps, "children"> & {
  name: IconName;
  fillCurrent?: boolean;
};

const ICONS: Record<IconName, LucideIcon> = {
  undo: Undo2,
  redo: Redo2,
  minus: Minus,
  square: Square,
  x: X,
  check: Check,
  "chevron-down": ChevronDown,
  "chevron-up": ChevronUp,
  "chevron-right": ChevronRight,
  plus: Plus,
  link: Link,
  unlink: Unlink,
  "align-v": AlignCenterVertical,
  "align-h": AlignCenterHorizontal,
  "align-left": AlignStartVertical,
  "align-right": AlignEndVertical,
  "align-text-left": AlignLeft,
  "align-text-center": AlignCenter,
  "align-text-right": AlignRight,
  "align-top": AlignStartHorizontal,
  "align-bottom": AlignEndHorizontal,
  "stretch-h": StretchHorizontal,
  "flip-h": FlipHorizontal,
  "flip-v": FlipVertical,
  camera: Camera,
  history: History,
  box: Box,
  cursor: MousePointer2,
  crop: Crop,
  rectangle: RectangleHorizontal,
  slice: Slice,
  brush: Brush,
  stamp: Stamp,
  pen: Pen,
  eraser: Eraser,
  "square-pen": SquarePen,
  type: Type,
  "grid-2": Grid2X2,
  columns: Columns2,
  "split-h": SplitSquareHorizontal,
  layers: Layers,
  line: LineChart,
  contrast: Contrast,
  spline: ChartSpline,
  "grid-3": Grid3X3,
  circle: Circle,
  more: MoreHorizontal,
  move: Move,
  pipette: Pipette,
  sun: Sun,
  palette: Palette,
  swatch: SwatchBook,
  sparkles: Sparkles,
  aperture: Aperture,
  eye: Eye,
  lock: Lock,
  unlock: Unlock,
  "folder-plus": FolderPlus,
  copy: Copy,
  "square-dashed": SquareDashed,
  trash: Trash2,
  maximize: Maximize2,
  "paint-bucket": PaintBucket,
  rotate: RotateCw,
  "rotate-ccw": RotateCcw,
  "rotate-cw": RotateCw,
  swap: ArrowLeftRight,
  "arrowUpRight": ArrowUpRight,
  "panel-right-open": PanelRightOpen,
  "panel-right-close": PanelRightClose,
  image: ImageIcon,
  "image-plus": ImagePlus,
  sliders: Sliders,
};

export function Icon(props: IconProps) {
  const { name: _name, fillCurrent, ...svgProps } = props;

  return (
    <Dynamic
      component={ICONS[props.name]}
      aria-hidden="true"
      fill={fillCurrent ? "currentColor" : "none"}
      stroke-width={props.strokeWidth ?? 2}
      {...svgProps}
    />
  );
}
