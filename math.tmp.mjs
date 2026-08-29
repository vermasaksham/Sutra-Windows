import { JSDOM } from "/tmp/claude-0/-home-user-Local-PDF-Translator---Windows/0e0644c7-0a87-5ac3-a547-eee6bb985523/scratchpad/node_modules/jsdom/lib/api.js";
const dom = new JSDOM("<!doctype html><html><body></body></html>");
globalThis.window = dom.window; globalThis.document = dom.window.document;
Object.defineProperty(globalThis, "navigator", { value: dom.window.navigator, configurable: true });
globalThis.DOMParser = dom.window.DOMParser; globalThis.Element = dom.window.Element;
globalThis.Node = dom.window.Node; globalThis.HTMLElement = dom.window.HTMLElement;

const { Editor, Node: TNode, mergeAttributes } = await import("@tiptap/core");
const { Markdown } = await import("@tiptap/markdown");
const StarterKit = (await import("@tiptap/starter-kit")).default;

// Same specs as src/editor/math/*.ts, minus the React node views.
const INLINE_MATH = /^\$(?![\s$])((?:[^$\n\\]|\\.)+?)(?<![\s\\])\$/;
const BLOCK_MATH = /^\$\$[ \t]*\r?\n([\s\S]*?)\r?\n?\$\$[ \t]*(?:\r?\n|$)/;

const MathInline = TNode.create({
  name: "mathInline", group: "inline", inline: true, atom: true,
  addAttributes: () => ({ latex: { default: "" } }),
  parseHTML: () => [{ tag: "span[data-math-inline]" }],
  renderHTML: ({ HTMLAttributes }) => ["span", mergeAttributes(HTMLAttributes, { "data-math-inline": "" })],
  markdownTokenizer: { name: "mathInline", level: "inline", start: (s) => s.indexOf("$"),
    tokenize: (s) => { const m = INLINE_MATH.exec(s); if (!m) return; return { type: "mathInline", raw: m[0], latex: m[1] }; } },
  parseMarkdown: (t) => ({ type: "mathInline", attrs: { latex: t.latex ?? "" } }),
  renderMarkdown: (n) => `$${n.attrs?.latex ?? ""}$`,
});

const MathBlock = TNode.create({
  name: "mathBlock", group: "block", atom: true,
  addAttributes: () => ({ latex: { default: "" } }),
  parseHTML: () => [{ tag: "div[data-math-block]" }],
  renderHTML: ({ HTMLAttributes }) => ["div", mergeAttributes(HTMLAttributes, { "data-math-block": "" })],
  markdownTokenizer: { name: "mathBlock", level: "block", start: (s) => s.indexOf("$$"),
    tokenize: (s) => { const m = BLOCK_MATH.exec(s); if (!m) return; return { type: "mathBlock", raw: m[0], latex: m[1] }; } },
  parseMarkdown: (t) => ({ type: "mathBlock", attrs: { latex: t.latex ?? "" } }),
  renderMarkdown: (n) => `$$\n${n.attrs?.latex ?? ""}\n$$`,
});

const editor = new Editor({ extensions: [StarterKit, MathBlock, MathInline, Markdown], content: "" });
const m = editor.storage.markdown.manager;

const cases = [
  ["inline maths",        "Einstein wrote $E = mc^2$ in 1905."],
  ["inline chemistry",    "The reaction $\\ce{H2O}$ is water."],
  ["block chemistry",     "$$\n\\ce{Sb2Se3 + 3I2 <=> 2SbI3 + 3Se}\n$$"],
  ["block with braces",   "$$\n\\frac{\\partial f}{\\partial x} = 2x_{1}\n$$"],
  ["underscores+stars",   "$$\na_1 * b_2 \\cdot c_{3}\n$$"],
  ["mhchem arrows",       "$$\n\\ce{A ->[\\text{cat}] B}\n$$"],
  ["maths amid prose",    "Before.\n\n$$\nx^2\n$$\n\nAfter."],
  ["currency is not maths", "It costs $5 and $7 in total."],
  ["lone dollar",         "A price of $ and nothing else."],
  ["two inline formulas", "Both $a_1$ and $b_2$ hold."],
];

let ok = 0, bad = 0;
for (const [label, src] of cases) {
  editor.commands.setContent(m.parse(src));
  const p1 = m.serialize(editor.getJSON());
  editor.commands.setContent(m.parse(p1));
  const p2 = m.serialize(editor.getJSON());
  const json = JSON.stringify(editor.getJSON());
  const nodes = (json.split('"mathInline"').length - 1) + (json.split('"mathBlock"').length - 1);
  const identical = p1.trim() === src.trim();
  const stable = p1.trim() === p2.trim();
  const good = identical && stable;
  good ? ok++ : bad++;
  console.log(`${good ? "ok  " : "FAIL"} ${label.padEnd(22)} nodes=${nodes}`);
  if (!good) { console.log(`       in : ${JSON.stringify(src)}`); console.log(`       out: ${JSON.stringify(p1.trim())}`); }
}
console.log(`\n${ok} lossless, ${bad} broken`);
