import { useSyncExternalStore } from "react";

/**
 * Where the editing toolbar sits.
 *
 * Four edges rather than free positioning. A toolbar that can be dropped
 * anywhere ends up half off-screen, over the text, or somewhere a different
 * window size will hide it — and then it has to be rescued. Snapping to an
 * edge means every possible position is one a person can still reach.
 */

export type Dock = "top" | "bottom" | "left" | "right";

export const DOCKS: readonly Dock[] = ["top", "right", "bottom", "left"];

const KEY = "sutra.toolbar";

/** Vertical edges stack their buttons; horizontal ones run along. */
export function isVertical(dock: Dock): boolean {
  return dock === "left" || dock === "right";
}

/**
 * Which edge a drop belongs to.
 *
 * Distance is measured as a *fraction* of each axis, not in pixels, so the
 * decision does not change with the window's shape. On a wide short window the
 * top edge is physically closer to almost everything, and picking by raw
 * pixels would make the left and right edges nearly impossible to hit.
 *
 * Ties go to the horizontal edges, which are the ones with room for labels.
 */
export function nearestDock(
  x: number,
  y: number,
  width: number,
  height: number,
): Dock {
  // A zero-sized window is not a real question; answer without dividing by it.
  if (width <= 0 || height <= 0) return "top";

  const fromLeft = x / width;
  const fromRight = 1 - fromLeft;
  const fromTop = y / height;
  const fromBottom = 1 - fromTop;

  const nearest = Math.min(fromLeft, fromRight, fromTop, fromBottom);
  if (nearest === fromTop) return "top";
  if (nearest === fromBottom) return "bottom";
  if (nearest === fromLeft) return "left";
  return "right";
}

/** Narrow whatever was stored to an edge that exists. */
export function readDock(stored: string | null): Dock {
  return DOCKS.includes(stored as Dock) ? (stored as Dock) : "top";
}

let dock: Dock = "top";
const listeners = new Set<() => void>();

try {
  dock = readDock(localStorage.getItem(KEY));
} catch {
  // Storage disabled. The default stands.
}

export function setDock(next: Dock) {
  if (next === dock) return;
  dock = next;
  try {
    localStorage.setItem(KEY, next);
  } catch {
    // Not fatal: the choice still applies for this session.
  }
  for (const listener of listeners) listener();
}

export function currentDock(): Dock {
  return dock;
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function useDock(): Dock {
  return useSyncExternalStore(subscribe, currentDock, () => "top" as Dock);
}

/** A rectangle, as `getBoundingClientRect` gives one. */
export type Box = {
  left: number;
  right: number;
  top: number;
  bottom: number;
  width: number;
  height: number;
};

/**
 * How the toolbar is positioned for a given edge.
 *
 * `sticky` is only ever the top dock, and it is the reason this is a union
 * rather than four sets of coordinates. The top toolbar should *start* in the
 * flow — between the tags and the first line, where it reads as part of the
 * note — and only pin itself once the note scrolls under it. That is exactly
 * `position: sticky`, and nothing computed can imitate it: a fixed toolbar is
 * pinned from the first frame and never sits in the document at all.
 *
 * The other three are `fixed`, measured against the note column rather than
 * the window. Fixed is what makes them survive scrolling, and measuring
 * against the column is what stops a left dock from sitting on top of the
 * sidebar.
 */
export type Placement =
  | { position: "sticky" }
  | {
      position: "fixed";
      left?: number;
      right?: number;
      top?: number;
      bottom?: number;
      transform?: string;
    };

/** How far the floating docks sit from the column's edge, in px. */
const INSET = 12;

/**
 * The strip a side dock needs, in px: the inset plus the width of a column of
 * buttons plus its border, with a few px of air.
 *
 * The scroll container reserves this as padding while a side dock is active,
 * which is what keeps the toolbar off the text. Floating it over the margin
 * only works while there *is* a margin: the note column is centred but capped,
 * so on a narrow window it fills the container edge to edge and a toolbar
 * placed over it covers the first character of every line.
 */
export const SIDE_RESERVE = 56;

/**
 * The strip the bottom dock needs, in px.
 *
 * Same reasoning as `SIDE_RESERVE`, but it only bites at the very end of a
 * note: everywhere else the reader can scroll the covered line out from under
 * the bar, and on the last line there is nowhere left to scroll to. Generous
 * enough for the two rows the bar wraps into on a narrow window.
 */
export const BOTTOM_RESERVE = 96;

/**
 * Where to put the toolbar, given the edge and the note column's box.
 *
 * `viewport` is needed because `right` and `bottom` in fixed positioning are
 * measured from the window's edges, while the box we are aligning to is
 * measured from its top-left. The subtraction has to happen somewhere, and
 * here it can be tested.
 */
export function placement(
  dock: Dock,
  column: Box | null,
  viewport: { width: number; height: number },
): Placement {
  // The top dock belongs in the flow, so it needs no measurement at all —
  // which also makes it the right thing to fall back to when there is no
  // column to measure yet. A toolbar that renders in the flow is always
  // visible; a fixed one positioned from a null box would be at 0,0.
  if (dock === "top" || !column) return { position: "sticky" };

  if (dock === "bottom") {
    return {
      position: "fixed",
      // Centred on the column, not the window: the note is not centred in the
      // window once a context panel is open.
      left: column.left + column.width / 2,
      bottom: viewport.height - column.bottom + INSET,
      transform: "translateX(-50%)",
    };
  }

  const middle = {
    top: column.top + column.height / 2,
    transform: "translateY(-50%)",
  };

  return dock === "left"
    ? { position: "fixed", left: column.left + INSET, ...middle }
    : {
        position: "fixed",
        right: viewport.width - column.right + INSET,
        ...middle,
      };
}
