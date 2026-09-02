import { useMemo, useState } from "react";
import { bibliography, emphasisRuns, useCitationStyle } from "./citationStyle";
import { CITATION_STYLES, type Citation, type NoteSummary } from "../vault/api";

/**
 * The reference list for this note, in the chosen style.
 *
 * Rendered by Zotero, not here — see citationStyle.ts. This component's whole
 * job is to put the strings in citation order, say which of them are the real
 * style and which are a fallback, and let them be copied into a manuscript.
 */
export default function Bibliography({
  citations,
  sources,
  onOpen,
}: {
  citations: Citation[];
  sources: NoteSummary[];
  onOpen: (id: string) => void;
}) {
  const style = useCitationStyle();
  const [copied, setCopied] = useState(false);

  const entries = useMemo(
    () =>
      bibliography(
        citations.map((c) => c.id),
        sources,
        style,
      ),
    [citations, sources, style],
  );

  if (entries.length === 0) return null;

  const label =
    CITATION_STYLES.find((s) => s.id === style)?.label ?? style ?? "unstyled";
  const unstyled = entries.filter((e) => !e.styled).length;

  async function copy() {
    try {
      await navigator.clipboard.writeText(
        entries.map((e) => e.text).join("\n"),
      );
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // Clipboard permission refused. The text is on screen and selectable,
      // so this is a lost convenience rather than a lost reference list.
      setCopied(false);
    }
  }

  return (
    <section>
      <div className="flex items-baseline justify-between gap-2">
        <h2 className="text-xs font-semibold tracking-wide text-ink-muted uppercase">
          Bibliography
        </h2>
        <button
          type="button"
          onClick={() => void copy()}
          className="sutra-no-print text-xs text-ink-muted transition-colors duration-150 ease-out hover:text-accent"
        >
          {copied ? "copied" : "copy"}
        </button>
      </div>

      <p className="mt-0.5 text-xs text-ink-muted">{label}</p>

      <ol className="selectable mt-2 flex list-decimal flex-col gap-1.5 pl-4">
        {entries.map((entry) => (
          <li key={entry.id} className="text-xs text-ink-soft">
            <button
              type="button"
              onClick={() => onOpen(entry.id)}
              className="text-left transition-colors duration-150 ease-out hover:text-accent"
            >
              {emphasisRuns(entry.text).map((run, i) =>
                run.emphasis ? (
                  <em key={i}>{run.text}</em>
                ) : (
                  <span key={i}>{run.text}</span>
                ),
              )}
            </button>
            {!entry.styled && (
              // Never quietly pass a fallback off as the chosen style: someone
              // pasting this into a manuscript needs to know which lines the
              // library has not actually rendered.
              <span className="ml-1 text-highlight">
                · not yet in this style
              </span>
            )}
          </li>
        ))}
      </ol>

      {unstyled > 0 && (
        <p className="mt-1.5 text-xs text-highlight">
          {unstyled === 1
            ? "One entry has not been rendered in this style. Settings → References → Restyle existing sources."
            : `${unstyled} entries have not been rendered in this style. Settings → References → Restyle existing sources.`}
        </p>
      )}
    </section>
  );
}
