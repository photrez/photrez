export interface SelectionState {
  x: number;
  y: number;
  width: number;
  height: number;
  angle: number;
  /** Marquee shape. "rect" = existing behavior; "ellipse" = ellipse marquee. Default "rect". */
  shape?: "rect" | "ellipse";
  /** When true, the selected pixels are everything outside these bounds. */
  inverted?: boolean;
}

export interface SelectionRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface SelectionValidationResult {
  valid: boolean;
  errors: string[];
}
