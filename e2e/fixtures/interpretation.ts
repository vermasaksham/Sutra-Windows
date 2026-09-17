import { Editor } from "@tiptap/core";
import { extensions } from "../../src/editor/extensions";
import { Interpretation } from "../../src/editor/interpretation/Interpretation";
import {
  InterpretationLiteral,
  InterpretationMarkdown,
} from "../../src/editor/interpretation/InterpretationMarkdown";

// Isolated browser harness: production registration remains disabled.
const vocabulary = [...extensions, Interpretation, InterpretationLiteral];
const manager = new InterpretationMarkdown({ extensions: vocabulary });
let editor: Editor | undefined;
export function open(body: string) {
  editor?.destroy();
  document.body.innerHTML = '<div id="editor"></div>';
  editor = new Editor({
    element: document.querySelector("#editor") as HTMLElement,
    extensions: vocabulary,
    content: manager.parse(body),
  });
}
export function save() {
  return manager.serialize(editor!.getJSON());
}
export function undo() {
  return editor!.commands.undo();
}
