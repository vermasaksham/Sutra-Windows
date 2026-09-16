/**
 * The §7 body format, before editor integration. A fenced Markdown block keeps
 * the identity beside the prose it names. Reading never mints or repairs IDs.
 * See docs/design/v0.5-interpretation-format.md for the compatibility boundary.
 */
export type InterpretationMeta = {
  iid: string;
  evidence: string[];
  [key: string]: unknown;
};

export type InterpretationBlock = {
  meta: InterpretationMeta;
  /** Markdown inside the fence, including its original line endings. */
  body: string;
  /** Original framing is retained so an untouched block is byte-identical. */
  opening: string;
  closing: string;
  raw: string;
};

// ULIDs use Crockford's alphabet and fit 128 bits (the leading digit is 0–7).
const ULID = /^[0-7][0-9A-HJKMNP-TV-Z]{25}$/;
const INFO = "sutra-interpretation-v1";

function metadata(value: unknown): value is InterpretationMeta {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const fields = value as Record<string, unknown>;
  return (
    typeof fields.iid === "string" &&
    ULID.test(fields.iid) &&
    Array.isArray(fields.evidence) &&
    fields.evidence.every((id) => typeof id === "string" && ULID.test(id))
  );
}

/**
 * Read a block at the start of a Markdown token. This is deliberately not a
 * whole-document scanner: the Markdown lexer must decide whether a candidate
 * occurs in prose, a quoted example or another code fence.
 *
 * Unsupported versions, invalid metadata and unclosed fences return undefined.
 * The caller must retain them as ordinary fenced code, never strip the framing.
 */
export function readInterpretationBlock(
  source: string,
): InterpretationBlock | undefined {
  const opening =
    /^(~{3,}|`{3,})sutra-interpretation-v1[ \t]+([^\r\n]+)\r?\n/.exec(source);
  if (!opening) return;
  const fence = opening[1]!;
  const header = opening[2]!;
  // CommonMark disallows backticks in the info string of a backtick fence.
  if (fence[0] === "`" && header.includes("`")) return;
  let meta: unknown;
  try {
    meta = JSON.parse(header);
  } catch {
    return;
  }
  if (!metadata(meta)) return;

  const rest = source.slice(opening[0].length);
  const close = new RegExp(
    `^ {0,3}${fence[0]}{${fence.length},}[ \\t]*(?:\\r?\\n|$)`,
    "m",
  ).exec(rest);
  if (!close) return;
  const raw = opening[0] + rest.slice(0, close.index) + close[0];
  return {
    meta,
    body: rest.slice(0, close.index),
    opening: opening[0],
    closing: close[0],
    raw,
  };
}

/** Reading and serializing an unedited block must not normalize its prose. */
export function writeInterpretationBlock(block: InterpretationBlock): string {
  return block.opening + block.body + block.closing;
}

/**
 * Explicit creation only. The caller supplies an already minted identity and
 * the evidence the researcher chose; this function neither infers nor resolves
 * them. A longer tilde fence keeps embedded code examples inside the prose.
 */
export function createInterpretationBlock(
  meta: InterpretationMeta,
  body: string,
): string {
  if (!metadata(meta)) {
    throw new Error(
      "An interpretation needs a ULID and a list of evidence ULIDs",
    );
  }
  let length = 3;
  for (const line of body.split(/\r?\n/)) {
    const fence = /^ {0,3}(~{3,})[ \t]*$/.exec(line);
    if (fence) length = Math.max(length, fence[1]!.length + 1);
  }
  const fence = "~".repeat(length);
  const prose = body && !body.endsWith("\n") ? `${body}\n` : body;
  return `${fence}${INFO} ${JSON.stringify(meta)}\n${prose}${fence}\n`;
}
