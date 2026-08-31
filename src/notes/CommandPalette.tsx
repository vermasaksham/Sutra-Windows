import { useEffect, useMemo, useRef, useState } from "react";
import { MOD, SHIFT, shortcut } from "../platform";
import { NOTE_TYPES, type NoteSummary, type NoteType } from "../vault/api";

/**
 * Everything the app can do, behind one keystroke.
 *
 * The point is that power lives here rather than in a toolbar — section 21 asks
 * for Obsidian's reach without Notion's wall of controls, and a palette is how
 * you get one without the other.
 *
 * Notes are offered alongside actions rather than in a separate mode. Someone
 * typing "sb2se3" usually wants the note, and making them choose a mode first
 * is the friction this is supposed to remove.
 */

export type Command = {
  id: string;
  label: string;
  /** Groups the list. Also what someone can type to narrow to a group. */
  group: string;
  hint?: string;
  /**
   * Hidden until something is typed.
   *
   * Nine near-identical "Set type to …" rows are noise on an empty palette —
   * they push everything else below the fold to serve a case nobody is in yet.
   * Typing "type", or the name of a kind, brings them straight back.
   */
  whenTyping?: boolean;
  run: () => void;
};

type Props = {
  notes: NoteSummary[];
  onClose: () => void;
  onOpenNote: (id: string) => void;
  onNewNote: () => void;
  onCapture: () => void;
  onSearch: (query: string) => void;
  onSetType: (type: NoteType) => void;
  onExportDocx: () => void;
  onExportPdf: () => void;
  onReindex: () => void;
  /** Null when no note is open, which disables the note-specific commands. */
  openNoteId: string | null;
};

export default function CommandPalette({
  notes,
  onClose,
  onOpenNote,
  onNewNote,
  onCapture,
  onSearch,
  onSetType,
  onExportDocx,
  onExportPdf,
  onReindex,
  openNoteId,
}: Props) {
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);

  useEffect(() => inputRef.current?.focus(), []);

  const commands = useMemo<Command[]>(() => {
    const out: Command[] = [
      {
        id: "capture",
        label: "Capture to Inbox",
        group: "Create",
        hint: shortcut(MOD, "N"),
        run: onCapture,
      },
      {
        id: "new",
        label: "New note in the current folder",
        group: "Create",
        run: onNewNote,
      },
    ];

    if (openNoteId) {
      for (const type of NOTE_TYPES) {
        out.push({
          id: `type:${type.value}`,
          label: `Set type to ${type.label}`,
          group: "This note",
          whenTyping: true,
          run: () => onSetType(type.value),
        });
      }
      out.push(
        {
          id: "docx",
          label: "Export as Word (.docx)",
          group: "This note",
          run: onExportDocx,
        },
        {
          id: "pdf",
          label: "Print, or save as PDF",
          group: "This note",
          run: onExportPdf,
        },
      );
    }

    out.push({
      id: "reindex",
      label: "Rebuild the search index",
      group: "Vault",
      hint: "safe — rebuilt from the markdown",
      run: onReindex,
    });

    return out;
  }, [
    onCapture,
    onNewNote,
    onSetType,
    onExportDocx,
    onExportPdf,
    onReindex,
    openNoteId,
  ]);

  const trimmed = query.trim();
  const results = useMemo<Command[]>(() => {
    const needle = trimmed.toLowerCase();
    const matching = needle
      ? commands.filter(
          (c) =>
            c.label.toLowerCase().includes(needle) ||
            c.group.toLowerCase().includes(needle) ||
            // So "type" finds all nine at once.
            (c.whenTyping && "type".includes(needle)),
        )
      : commands.filter((c) => !c.whenTyping);

    const matchingNotes = needle
      ? notes
          .filter((n) => (n.title || "Untitled").toLowerCase().includes(needle))
          .slice(0, 8)
          .map<Command>((n) => ({
            id: `note:${n.id}`,
            label: n.title || "Untitled",
            group: "Notes",
            hint: n.folder || "top level",
            run: () => onOpenNote(n.id),
          }))
      : [];

    // Searching the vault is always last and always offered, so the old Ctrl+K
    // reflex still lands somewhere useful rather than on nothing.
    const searchFallback: Command[] = needle
      ? [
          {
            id: "search",
            label: `Search the vault for “${trimmed}”`,
            group: "Find",
            hint: shortcut(MOD, SHIFT, "F"),
            run: () => onSearch(trimmed),
          },
        ]
      : [
          {
            id: "search",
            label: "Search the vault",
            group: "Find",
            hint: shortcut(MOD, SHIFT, "F"),
            run: () => onSearch(""),
          },
        ];

    return [...matchingNotes, ...matching, ...searchFallback];
  }, [commands, notes, trimmed, onOpenNote, onSearch]);

  // A narrowing query can leave the cursor past the end of the list.
  useEffect(() => setSelected(0), [trimmed]);

  useEffect(() => {
    listRef.current
      ?.querySelector('[data-selected="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [selected]);

  function onKeyDown(event: React.KeyboardEvent) {
    if (event.key === "Escape") return onClose();
    if (results.length === 0) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setSelected((i) => (i + 1) % results.length);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setSelected((i) => (i - 1 + results.length) % results.length);
    } else if (event.key === "Enter") {
      event.preventDefault();
      const chosen = results[selected];
      if (chosen) {
        onClose();
        chosen.run();
      }
    }
  }

  let lastGroup = "";

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Command palette"
      className="sutra-no-print fixed inset-0 z-50 flex justify-center bg-canvas/70 px-6 pt-24 backdrop-blur-sm"
      onClick={onClose}
    >
      <div
        className="flex max-h-[60vh] w-full max-w-xl flex-col overflow-hidden rounded-xl border border-border bg-surface shadow-pane"
        onClick={(event) => event.stopPropagation()}
      >
        <input
          ref={inputRef}
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={onKeyDown}
          placeholder="Type a command, or part of a note's title"
          aria-label="Command or note"
          className="border-b border-border bg-transparent px-4 py-3 text-ink outline-none placeholder:text-ink-muted"
        />

        {results.length === 0 ? (
          <p className="px-4 py-3 text-sm text-ink-muted">Nothing matches.</p>
        ) : (
          <ul ref={listRef} className="overflow-y-auto p-1">
            {results.map((command, index) => {
              const heading =
                command.group !== lastGroup ? command.group : null;
              lastGroup = command.group;
              return (
                <li key={command.id}>
                  {heading && (
                    <p className="px-3 pt-2 pb-1 text-[0.6875rem] font-semibold tracking-wide text-ink-muted uppercase">
                      {heading}
                    </p>
                  )}
                  <button
                    type="button"
                    data-selected={index === selected}
                    onMouseEnter={() => setSelected(index)}
                    onClick={() => {
                      onClose();
                      command.run();
                    }}
                    className={[
                      "flex w-full items-baseline gap-3 rounded-lg px-3 py-1.5 text-left transition-colors duration-150 ease-out",
                      index === selected ? "bg-row-active" : "",
                    ].join(" ")}
                  >
                    <span
                      className={[
                        "min-w-0 flex-1 truncate text-sm",
                        index === selected ? "text-accent" : "text-ink",
                      ].join(" ")}
                    >
                      {command.label}
                    </span>
                    {command.hint && (
                      <span className="shrink-0 font-mono text-[0.6875rem] text-ink-muted">
                        {command.hint}
                      </span>
                    )}
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </div>
  );
}
