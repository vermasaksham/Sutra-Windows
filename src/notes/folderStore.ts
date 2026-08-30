/**
 * Which folder the open note is in.
 *
 * The slash menu's "attach a file" item needs this so an attachment lands
 * beside the note that references it, but slash items are defined once at
 * module scope and never see React state. A one-value module store is how the
 * wikilink titles and citation lookups already cross that gap; this follows the
 * same pattern rather than inventing a second one.
 */
let folder: string | null = null;

export function setCurrentFolder(next: string | null) {
  folder = next;
}

export function currentFolder(): string | null {
  return folder;
}
