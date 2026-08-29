import { useEffect } from "react";

export type Shortcuts = {
  search: () => void;
  newNote: () => void;
  save: () => void;
};

/**
 * Application-level keyboard shortcuts.
 *
 * Only the ones that are not the editor's own — TipTap already owns bold,
 * italic, lists, undo and the rest, and duplicating them here would fight it.
 *
 * Deliberately ignored while focus is in a text field, apart from the ones
 * that use a modifier: Ctrl+S should work mid-sentence, but a bare key must
 * never steal a keystroke from someone typing.
 */
export function useShortcuts({ search, newNote, save }: Shortcuts) {
  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      const mod = event.ctrlKey || event.metaKey;
      if (!mod) return;

      const key = event.key.toLowerCase();
      if (key === "k") {
        event.preventDefault();
        search();
      } else if (key === "n") {
        event.preventDefault();
        newNote();
      } else if (key === "s") {
        // The app autosaves, so this exists for the muscle memory rather than
        // the need — and to stop the browser's own save dialog appearing.
        event.preventDefault();
        save();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [search, newNote, save]);
}
