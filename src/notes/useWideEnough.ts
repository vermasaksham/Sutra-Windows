import { useEffect, useState } from "react";

/**
 * Rail plus list plus context panel, and enough left for the note.
 *
 * 13rem + 19rem + 20rem of chrome is 832px before the page gets anything. Below
 * this the note column is narrower than a paragraph wants to be, and the title
 * — an `<input>`, so it clips rather than wraps — starts losing its end.
 */
const ENOUGH = 1280;

/**
 * Whether the window can afford a fourth column.
 *
 * The context panel is hidden below this and comes back when the window grows,
 * without touching the remembered preference: someone who wants it open has
 * said so, and a narrow window is a fact about right now rather than a change
 * of mind.
 */
export function useWideEnough(): boolean {
  const [wide, setWide] = useState(
    () => typeof window === "undefined" || window.innerWidth >= ENOUGH,
  );
  useEffect(() => {
    const query = window.matchMedia(`(min-width: ${ENOUGH}px)`);
    const update = () => setWide(query.matches);
    update();
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  }, []);
  return wide;
}
