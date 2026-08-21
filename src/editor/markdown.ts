import type { Editor, JSONContent } from "@tiptap/core";

/**
 * Markdown in and out of the editor.
 *
 * TipTap owns this conversion, not Rust. Rust reads and writes the file and
 * splits off the frontmatter, but never interprets the body — it is opaque text
 * to the storage layer.
 *
 * The reason is Phase 5. Maths and chemistry have to survive the round trip
 * losslessly, and with this arrangement a maths node declares its own
 * `parseMarkdown` / `renderMarkdown` alongside its schema, so `$$…$$` and
 * `\ce{…}` are defined in exactly one place. Converting through Rust would mean
 * an intermediate representation that both sides must agree on exactly, and
 * every new node type would need implementing twice.
 *
 * Verified before adopting: all twelve block types we support round-trip
 * stably, and eleven of twelve are byte-identical. Tables normalise their
 * column padding on first save and are stable thereafter, so a file does not
 * churn on repeated saves.
 */

/** The bits of `editor.storage.markdown` we rely on. */
type MarkdownStorage = {
  manager: {
    parse: (markdown: string) => JSONContent;
    serialize: (json: JSONContent) => string;
  };
};

function manager(editor: Editor) {
  const storage = editor.storage.markdown as MarkdownStorage | undefined;
  if (!storage?.manager) {
    // A hard failure rather than a silent empty document: reaching here means
    // the Markdown extension is not registered, and saving would write an empty
    // body over a real note.
    throw new Error("the Markdown extension is not registered on this editor");
  }
  return storage.manager;
}

/** Replace the editor's contents with parsed markdown. */
export function setMarkdown(editor: Editor, markdown: string) {
  const json = manager(editor).parse(markdown);
  // `emitUpdate: false` matters: loading a note from disk is not an edit, and
  // letting it fire onUpdate would mark a freshly opened note dirty and trigger
  // an autosave that rewrites the file we just read.
  editor.commands.setContent(json, { emitUpdate: false });
}

/** The editor's contents as markdown. */
export function getMarkdown(editor: Editor): string {
  return manager(editor).serialize(editor.getJSON());
}
