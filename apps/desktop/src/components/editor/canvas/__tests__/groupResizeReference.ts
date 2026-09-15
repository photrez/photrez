// Reference geometry for the multi-selection group-transform gesture tests.
//
// The hook under test resizes the group BOX and then maps every member through that
// box. This module restates that mapping from the shared geometry function, with no
// routing code in it, so the legacy per-frame authority and the routed preview-and-batch
// authority have an outside judge instead of only being compared with each other.
//
// Held apart from the test file by the 1000-line file-size guard; the tests themselves
// need the mock graph that lives with the fixture.

import type { Transform2D } from "@/engine/types";
import { applyResizeHandle } from "@/viewport/transformGeometry";

/**
 * The two gesture members start unscaled and 200x200: one at (100,100), one at
 * (400,100). Their box is therefore 500 wide and 200 tall at (100,100), which puts the
 * SE corner at (600,300) and the east edge midpoint at (600,200).
 */
export const CORNER = { x: 600, y: 300 };
export const EDGE = { x: 600, y: 200 };

/** SE drag to 2x the group box: the first member scales in place, the second slides to 700. */
export const TARGET = { x: CORNER.x + 500, y: CORNER.y + 200 };

/**
 * The same SE grip dragged with no Shift held, so the aspect lock is in charge: the
 * pointer only travels in x and the box still grows in y. The drag is 290 px right,
 * which the corner rule turns into a factor of exactly 1.5 on BOTH axes
 * (1 + 290 * 500 / (500^2 + 200^2)), so the asserted numbers stay readable.
 */
export const PROPORTIONAL_TARGET = { x: CORNER.x + 290, y: CORNER.y };

export interface Gesture {
  /** Name shown in the test title. */
  label: string;
  handle: string;
  /** Where the pointer started, i.e. which grip was grabbed. */
  from: { x: number; y: number };
  /** Where the pointer ended. */
  to: { x: number; y: number };
  shift: boolean;
  /**
   * Where the member parked at (400,100) has to end up, written out. Everything else in
   * these cases is computed from the gesture; this one is remembered, so a change that
   * moved BOTH authorities together still has to answer for it instead of passing as
   * parity.
   */
  pin: { x: number; y: number; scaleX: number; scaleY: number };
}

/**
 * The proportional drags, one per handle family, each scaled to land on the same 1.5
 * group factor:
 * - the corner grip projects the drag onto the box diagonal and anchors the top-left, so
 *   a pointer that only moves in x still grows y and no member moves vertically;
 * - the east-edge grip takes its factor from x alone, keeps the west edge fixed, and
 *   grows y outward from the vertical centre, so the same 1.5 puts every member at y 50.
 * That anchor difference is the part of the proportional branch that changes per handle.
 * The same pointer travel with Shift held scales only the axis travelled, so none of
 * these numbers can come from the free-axis branch by accident.
 */
export const RESIZE_GESTURES: Gesture[] = [
  {
    label: "proportional corner (se, no Shift)",
    handle: "se",
    from: CORNER,
    to: PROPORTIONAL_TARGET,
    shift: false,
    pin: { x: 550, y: 100, scaleX: 1.5, scaleY: 1.5 },
  },
  {
    label: "proportional edge (e, no Shift)",
    handle: "e",
    from: EDGE,
    to: { x: EDGE.x + 250, y: EDGE.y },
    shift: false,
    pin: { x: 550, y: 50, scaleX: 1.5, scaleY: 1.5 },
  },
];

/** The SE grip with Shift held: every axis tracks the pointer, so the box doubles. */
export const FREE_AXIS_CORNER: Gesture = {
  label: "free-axis corner (se, Shift held)",
  handle: "se",
  from: CORNER,
  to: TARGET,
  shift: true,
  pin: { x: 700, y: 100, scaleX: 2, scaleY: 2 },
};

/** One member as the reference needs it: where it started, and what the model locks. */
export interface MemberStart {
  transform: Transform2D;
  lockPosition?: boolean;
  lockRotation?: boolean;
}

/**
 * Where a group resize has to leave each member, derived from the gesture instead of
 * pasted from a run: the box is resized by the shared geometry function, then each member
 * is mapped by the rule the hook documents - position scales away from the box's own
 * corner, size scales by the box's factor. Locks are NOT applied here, so the unlocked
 * result stays visible next to the locked one and a lock that quietly stopped working
 * cannot pass as "this member was going there anyway".
 */
export function expectedMemberTransforms(
  gesture: Gesture,
  group: { x: number; y: number; width: number; height: number },
  starts: Transform2D[],
): Transform2D[] {
  const box = applyResizeHandle(
    { x: group.x, y: group.y, rotation: 0, scaleX: 1, scaleY: 1, flipH: false, flipV: false },
    group.width,
    group.height,
    gesture.handle,
    gesture.to.x - gesture.from.x,
    gesture.to.y - gesture.from.y,
    gesture.shift,
    false,
  );
  const fx = Math.abs(box.scaleX);
  const fy = Math.abs(box.scaleY);
  return starts.map((t) => ({
    ...t,
    x: Math.round(box.x + (t.x - group.x) * fx),
    y: Math.round(box.y + (t.y - group.y) * fy),
    scaleX: t.scaleX * fx,
    scaleY: t.scaleY * fy,
  }));
}

/**
 * The model mutator's lock rule, which is also what the routed path pre-applies to the
 * preview and to the commit patch: a position-locked member keeps x/y and still takes the
 * scale. A rotation lock is not restated here because a resize writes no rotation for
 * either authority, so there is nothing for it to filter.
 */
export function withPositionLock(start: MemberStart, frame: Transform2D): Transform2D {
  return start.lockPosition
    ? { ...frame, x: start.transform.x, y: start.transform.y }
    : frame;
}
