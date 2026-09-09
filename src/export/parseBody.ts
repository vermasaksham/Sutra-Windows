import { MarkdownManager } from "@tiptap/markdown";
import type { JSONContent } from "@tiptap/core";
import { extensions } from "../editor/extensions";

/**
 * A note body, as markdown, turned into the document tree the exporter walks —
 * without an editor.
 *
 * The export path used to start from `editor.getJSON()`, which tied exporting to
 * the one note that happened to be on screen. That was fine while a document
 * *was* a note, and it is the wrong shape for the thing this is groundwork for:
 * a chapter is an ordered list of notes, and only one of them can be in the
 * editor at a time.
 *
 * Nothing here reimplements the conversion. `MarkdownManager` is the same class
 * the editor keeps in `editor.storage.markdown`, registered with the same
 * extension list, so a maths node's `parseMarkdown` is read from exactly one
 * place. If the two could drift, the round-trip guarantee in `editor/markdown.ts`
 * would be worth nothing.
 *
 * Built once and reused: registering the extensions walks and sorts the whole
 * list, and exporting a chapter would otherwise pay for it per note.
 */
let manager: MarkdownManager | null = null;

function markdown(): MarkdownManager {
  manager ??= new MarkdownManager({ extensions });
  return manager;
}

export function parseBody(body: string): JSONContent {
  return markdown().parse(body);
}
