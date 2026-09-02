import { useEffect } from "react";

export type Shortcuts = {
  /** The command palette. */
  palette: () => void;
  /** Focus the search field in the note list. */
  search: () => void;
  /** Capture to the Inbox, with no decisions to make. */
  capture: () => void;
  /** Show or hide the context panel. */
  context: () => void;
  /** The Zotero reference picker. */
  references: () => void;
  save: () => void;
};

/**
 * Application-level keyboard shortcuts.
 *
 * Only the ones that are not the editor's own — TipTap already owns bold,
 * italic, lists, undo and the rest, and duplicating them here would fight it.
 *
 * The bindings follow section 21: Ctrl+K is the command palette, Ctrl+Shift+F
 * is search, Ctrl+N captures, Ctrl+\ shows or hides the context panel, and
 * Ctrl+Shift+Z opens the Zotero picker. Ctrl+K used to open search directly, so for a
 * while the wrong reflex will open the palette instead — which is why the
 * palette's first offer for any typed text is to search the vault for it.
 *
 * Every binding here uses a modifier, so none of them can steal a keystroke
 * from someone mid-sentence.
 */
export function useShortcuts({
  palette,
  search,
  capture,
  context,
  references,
  save,
}: Shortcuts) {
  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      // Ctrl on Windows and Linux, Cmd on macOS. Accepting either everywhere
      // costs nothing and spares anyone switching between the two.
      const mod = event.ctrlKey || event.metaKey;
      if (!mod || event.altKey) return;

      const key = event.key.toLowerCase();
      if (key === "k" && !event.shiftKey) {
        event.preventDefault();
        palette();
      } else if (key === "f" && event.shiftKey) {
        event.preventDefault();
        search();
      } else if (key === "n" && !event.shiftKey) {
        event.preventDefault();
        capture();
      } else if (key === "\\") {
        event.preventDefault();
        context();
      } else if (key === "z" && event.shiftKey) {
        // Shift matters: Ctrl+Z is undo, and stealing it would be a disaster
        // in a text editor.
        event.preventDefault();
        references();
      } else if (key === "s") {
        // The app autosaves, so this exists for the muscle memory rather than
        // the need — and to stop the browser's own save dialog appearing.
        event.preventDefault();
        save();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [palette, search, capture, context, references, save]);
}
