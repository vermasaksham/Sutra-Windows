/**
 * A stored title, as readable text, with any leftover Zotero markup resolved.
 *
 * Zotero keeps a title's formatting as HTML, so a materials-chemistry paper
 * arrives as `Sb<sub>2</sub>Se<sub>3</sub> Nanosheet Film-Based Devices`.
 * Imports resolve that on the way in, but notes already in a vault were written
 * before they did, and their titles are still raw. This is the display half of
 * the same conversion: it runs where a title is *shown*, so an existing note
 * reads correctly without its file being rewritten underneath the researcher.
 *
 * `<sub>` and `<sup>` become Unicode — `Sb₂Se₃`, `Sb³⁺` — because in this field
 * a subscript is not decoration: `Sb₂Se₃` names a compound and `Sb2Se3` names a
 * string of characters that looks like one. Every other tag is dropped, and a
 * character Unicode has no raised or lowered form for stays on the line rather
 * than being replaced by something near it.
 *
 * Deliberately not an HTML renderer. Nothing here produces markup; the return
 * value is text, and React escapes it like any other string. A title that
 * contains no tags — which is every title imported since — is returned as it
 * came in.
 */
export function displayTitle(title: string): string {
  // The overwhelmingly common case, and worth not walking the string for.
  if (!title.includes("<") && !title.includes("&")) return title;

  let out = "";
  // Tag names, innermost last. Only sub/sup are tracked; the rest is structure
  // with no meaning left once this is a title.
  const script: Script[] = [];
  let i = 0;

  while (i < title.length) {
    const c = title[i]!;
    if (c === "<") {
      const close = title.indexOf(">", i);
      // An unclosed `<` is a less-than sign, not a tag.
      if (close === -1) {
        out += shift(c, script[script.length - 1]);
        i += 1;
        continue;
      }
      const raw = title.slice(i + 1, close);
      const closing = raw.startsWith("/");
      const name = raw
        .replace(/^\//, "")
        .trim()
        .split(/[\s/]/)[0]!
        .toLowerCase();
      if (!closing && name === "sub") script.push("sub");
      else if (!closing && name === "sup") script.push("sup");
      else if (closing && (name === "sub" || name === "sup")) script.pop();
      i = close + 1;
      continue;
    }

    if (c === "&") {
      const end = title.indexOf(";", i);
      // Bounded, so a bare ampersand does not swallow the rest of the title.
      if (end !== -1 && end - i <= 9) {
        const decoded = decodeEntity(title.slice(i + 1, end));
        if (decoded !== null) {
          for (const d of decoded) out += shift(d, script[script.length - 1]);
          i = end + 1;
          continue;
        }
      }
    }

    out += shift(c, script[script.length - 1]);
    i += 1;
  }

  // Removing a tag can leave a title double-spaced where the markup used to be.
  return out.split(/\s+/).filter(Boolean).join(" ");
}

type Script = "sub" | "sup";

const SUB: Record<string, string> = {
  "0": "₀",
  "1": "₁",
  "2": "₂",
  "3": "₃",
  "4": "₄",
  "5": "₅",
  "6": "₆",
  "7": "₇",
  "8": "₈",
  "9": "₉",
  "+": "₊",
  "-": "₋",
  "−": "₋",
  "=": "₌",
  "(": "₍",
  ")": "₎",
  a: "ₐ",
  e: "ₑ",
  h: "ₕ",
  i: "ᵢ",
  j: "ⱼ",
  k: "ₖ",
  l: "ₗ",
  m: "ₘ",
  n: "ₙ",
  o: "ₒ",
  p: "ₚ",
  r: "ᵣ",
  s: "ₛ",
  t: "ₜ",
  u: "ᵤ",
  v: "ᵥ",
  x: "ₓ",
};

const SUP: Record<string, string> = {
  "0": "⁰",
  "1": "¹",
  "2": "²",
  "3": "³",
  "4": "⁴",
  "5": "⁵",
  "6": "⁶",
  "7": "⁷",
  "8": "⁸",
  "9": "⁹",
  "+": "⁺",
  "-": "⁻",
  "−": "⁻",
  "=": "⁼",
  "(": "⁽",
  ")": "⁾",
  n: "ⁿ",
  i: "ⁱ",
};

/** One character, raised or lowered where Unicode has a form for it. */
function shift(c: string, script: Script | undefined): string {
  if (!script) return c;
  return (script === "sub" ? SUB[c] : SUP[c]) ?? c;
}

const NAMED: Record<string, string> = {
  amp: "&",
  lt: "<",
  gt: ">",
  quot: '"',
  apos: "'",
  nbsp: " ",
};

/** `null` when this is not an entity, so the `&` is kept as written. */
function decodeEntity(body: string): string | null {
  if (body.startsWith("#")) {
    const digits = body.slice(1);
    const hex = digits.startsWith("x") || digits.startsWith("X");
    const code = Number.parseInt(hex ? digits.slice(1) : digits, hex ? 16 : 10);
    if (!Number.isFinite(code) || code <= 0) return null;
    return String.fromCodePoint(code);
  }
  return NAMED[body.toLowerCase()] ?? null;
}
