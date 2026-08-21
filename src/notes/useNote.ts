import { useCallback, useEffect, useRef, useState } from "react";
import { notesApi, onVaultChanged, type NoteDoc } from "../vault/api";

const AUTOSAVE_DELAY = 600;

export type SaveState = "saved" | "dirty" | "saving" | "error";

/**
 * One open note: its buffer, its autosave, and what to do when the file
 * changes underneath it.
 */
export function useNote(id: string | null, onSaved: () => void) {
  const [doc, setDoc] = useState<NoteDoc | null>(null);
  const [saveState, setSaveState] = useState<SaveState>("saved");
  /** An external version waiting on the user, per the prompt-if-dirty rule. */
  const [conflict, setConflict] = useState<NoteDoc | null>(null);
  /** Bumped to force the editor to remount around externally-loaded content. */
  const [revision, setRevision] = useState(0);

  // Refs, not state: these are read inside timers and event listeners that must
  // see the latest value. State would close over whatever it was when the
  // listener was registered.
  const buffer = useRef({ title: "", body: "" });
  /** The last thing we successfully wrote. Used to recognise our own writes
   *  coming back from the file watcher. */
  const written = useRef({ title: "", body: "" });
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const currentId = useRef<string | null>(null);
  /**
   * Mirrors `conflict` for the benefit of the autosave timer.
   *
   * This is load-bearing. Without it a queued autosave fires while the prompt
   * is still on screen and writes the buffer over the external version — so the
   * file is already overwritten by the time the user picks, and the dialog is
   * decoration. Autosave has to stop until the conflict is resolved.
   */
  const pendingConflict = useRef<NoteDoc | null>(null);

  const adopt = useCallback((next: NoteDoc) => {
    pendingConflict.current = null;
    setDoc(next);
    buffer.current = { title: next.title, body: next.body };
    written.current = { title: next.title, body: next.body };
    setSaveState("saved");
    setRevision((r) => r + 1);
  }, []);

  // Load whenever the selected note changes.
  useEffect(() => {
    currentId.current = id;
    if (!id) {
      setDoc(null);
      return;
    }
    let cancelled = false;
    notesApi
      .read(id)
      .then((next) => {
        // The user may have clicked away while this was in flight.
        if (!cancelled) adopt(next);
      })
      .catch(() => {
        if (!cancelled) setSaveState("error");
      });
    return () => {
      cancelled = true;
    };
  }, [id, adopt]);

  const save = useCallback(async () => {
    const noteId = currentId.current;
    if (!noteId) return;
    // Never write while the user is being asked which version to keep.
    if (pendingConflict.current) return;
    const { title, body } = buffer.current;
    setSaveState("saving");
    try {
      await notesApi.save(noteId, title, body);
      written.current = { title, body };
      // Only claim "saved" if nothing was typed while the write was in flight.
      setSaveState(
        buffer.current.title === title && buffer.current.body === body
          ? "saved"
          : "dirty",
      );
      onSaved();
    } catch {
      setSaveState("error");
    }
  }, [onSaved]);

  const schedule = useCallback(() => {
    setSaveState("dirty");
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => void save(), AUTOSAVE_DELAY);
  }, [save]);

  const setBody = useCallback(
    (body: string) => {
      buffer.current.body = body;
      schedule();
    },
    [schedule],
  );

  const setTitle = useCallback(
    (title: string) => {
      buffer.current.title = title;
      setDoc((d) => (d ? { ...d, title } : d));
      schedule();
    },
    [schedule],
  );

  /** Flush a pending autosave immediately — on note switch or window close. */
  const flush = useCallback(async () => {
    if (timer.current) {
      clearTimeout(timer.current);
      timer.current = null;
      await save();
    }
  }, [save]);

  // Don't lose the last few hundred milliseconds of typing when switching away.
  useEffect(() => () => void flush(), [id, flush]);

  useEffect(() => {
    const onUnload = () => void flush();
    window.addEventListener("beforeunload", onUnload);
    return () => window.removeEventListener("beforeunload", onUnload);
  }, [flush]);

  // React to edits made outside the app.
  useEffect(() => {
    const unlisten = onVaultChanged(async (changed) => {
      const noteId = currentId.current;
      if (!noteId || !changed.includes(noteId)) return;

      let disk: NoteDoc;
      try {
        disk = await notesApi.read(noteId);
      } catch {
        return; // Deleted or unreadable; the list refresh will catch up.
      }

      // Our own save, echoed back by the watcher. Compare contents rather than
      // relying on a timing window: a slow disk would defeat a timer, and this
      // cannot produce a false positive.
      if (
        disk.body === written.current.body &&
        disk.title === written.current.title
      ) {
        return;
      }

      const dirty =
        buffer.current.body !== written.current.body ||
        buffer.current.title !== written.current.title;

      // Clean buffer: nothing to lose, take the file. Dirty: ask, because
      // silently reloading would throw away unsaved work.
      if (dirty) {
        // Cancel the queued autosave as well as blocking future ones; it may
        // already be counting down toward overwriting the file.
        if (timer.current) {
          clearTimeout(timer.current);
          timer.current = null;
        }
        pendingConflict.current = disk;
        setConflict(disk);
      } else {
        adopt(disk);
      }
    });
    return () => void unlisten.then((off) => off());
  }, [adopt]);

  /** Resolve a conflict: keep the buffer and overwrite the file, or discard the
   *  buffer and take the file. */
  const resolveConflict = useCallback(
    async (choice: "mine" | "theirs") => {
      const disk = conflict;
      // Clear the ref synchronously — `save()` reads it, and a state update
      // would not have landed yet by the time we call it below.
      pendingConflict.current = null;
      setConflict(null);
      if (!disk) return;
      if (choice === "theirs") adopt(disk);
      else await save();
    },
    [conflict, adopt, save],
  );

  return {
    doc,
    revision,
    saveState,
    conflict,
    setBody,
    setTitle,
    flush,
    resolveConflict,
  };
}
