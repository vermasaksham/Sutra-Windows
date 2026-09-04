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
