import { useCallback, useEffect, useRef, useState } from "react";
import {
  pdfApi,
  sourcesApi,
  zoteroApi,
  type Annotation,
  type Citation,
  type NoteSummary,
  type PdfOutcome,
} from "../vault/api";
import { displayTitle } from "./titleText";

/**
 * Reading a paper, and taking evidence out of it.
 *
 * Sits where the note list sits, so reading and writing are visible at once —
 * see docs/design/v0.4-reading-workflow.md for why the list gives way rather
 * than the context panel: reading one paper is exactly when every other note is
 * least interesting, and exactly when watching **Sources** fill up matters most.
 *
 * **This is a capture surface, not a viewer.** Sutra extracts text; it does not
 * render a page as the publisher drew it, and this does not pretend otherwise.
 * Figures, layout and equations are Zotero's reader's job, and the header says
 * so rather than leaving someone to wonder why the figures are missing.
 */

type Mode = "text" | "annotations";

export default function ReadingPane({
  source,
  itemKey,
  vaultPdf,
  target,
  captured,
  onCaptured,
  onClose,
  onReport,
}: {
  /** The paper being read. */
  source: NoteSummary;
  /** Zotero's *item* key — what the source note records. The backend finds
   *  the attachment from it, because a paper is an item but its file and
   *  its annotations both hang off an attachment. */
  itemKey: string | null;
  /** The vault-relative path, when the PDF is the vault's own. */
  vaultPdf: string | null;
  /** The note evidence will be written to — normally the literature note being
   *  written. Null when nothing suitable is open, which disables capture and
   *  says why rather than failing at the press. */
  target: NoteSummary | null;
  /** Evidence already on the target note, so an annotation already taken can be
   *  marked instead of offered again. */
  captured: Citation[];
  onCaptured: () => void;
  onClose: () => void;
  onReport: (message: string, cause: unknown) => void;
}) {
  const [mode, setMode] = useState<Mode>("text");
  const [outcome, setOutcome] = useState<PdfOutcome | null>(null);
  const [annotations, setAnnotations] = useState<Annotation[] | null>(null);
  const [zoteroError, setZoteroError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [selection, setSelection] = useState<{
    text: string;
    page: string;
  } | null>(null);
  const pagesRef = useRef<HTMLDivElement>(null);

  // Which Zotero annotations are already evidence on this note. By key, which
  // is exact — no guessing from matching text, which would call two identical
  // highlights on different pages the same one.
  const already = new Set(
    captured.map((c) => c.annotation).filter((k): k is string => Boolean(k)),
  );

  useEffect(() => {
    let live = true;
    setOutcome(null);
    const request = vaultPdf
      ? pdfApi.ofVaultFile(vaultPdf)
      : itemKey
        ? pdfApi.ofZoteroItem(itemKey)
        : Promise.resolve<PdfOutcome>({ state: "notAttached" });

    request
      .then((result) => live && setOutcome(result))
      .catch((cause: unknown) => {
        if (!live) return;
        // A real fault rather than one of the named states — no vault, a
        // command that does not exist. Shown as a failure because that is what
        // it is, rather than pretending it is a state of the paper.
        setOutcome({
          state: "failed",
          detail: cause instanceof Error ? cause.message : String(cause),
        });
      });
    return () => {
      live = false;
    };
  }, [vaultPdf, itemKey]);

  useEffect(() => {
    if (!itemKey) return;
    let live = true;
    setZoteroError(null);
    zoteroApi
      .annotations(itemKey)
      .then((found) => live && setAnnotations(found))
      .catch((cause: unknown) => {
        if (!live) return;
        // Zotero being closed is not a failure of this pane: a vault-attached
        // PDF still reads, and the text half above is unaffected.
        setAnnotations([]);
        setZoteroError(cause instanceof Error ? cause.message : String(cause));
      });
    return () => {
      live = false;
    };
  }, [itemKey]);

  /** What is selected, and which page heading it sits under. */
  const readSelection = useCallback(() => {
    const active = window.getSelection();
    const text = active?.toString().trim() ?? "";
    if (!text || !active || active.rangeCount === 0) {
      setSelection(null);
      return;
    }
    let node: Node | null = active.getRangeAt(0).startContainer;
    while (node && !(node instanceof HTMLElement && node.dataset.page)) {
      node = node.parentNode;
    }
    const page = node instanceof HTMLElement ? (node.dataset.page ?? "") : "";
    // Page provenance is structural: text only ever renders inside a page, so a
    // selection that found no page means the selection is not in the document.
    if (!page) {
      setSelection(null);
      return;
    }
    setSelection({ text, page });
  }, []);

  const capture = async () => {
    if (!selection || !target) return;
    setBusy("selection");
    try {
      const entry: Citation = {
        id: source.id,
        page: selection.page,
        // Exactly what was selected. Not trimmed further, not normalised, not
        // re-wrapped: it is the source's words and the whole point is that it
        // is unaltered.
        quote: selection.text,
        // Left empty on purpose. Interpretation is a second, deliberate act —
        // the capture control does not collect an opinion.
      };
      // The complete list, as `set_citations` expects. Existing entries are
      // passed through untouched — including fields this component does not
      // know about, which is why they are spread rather than rebuilt.
      await sourcesApi.setCitations(target.id, [
        ...captured.map((c) => ({ ...c })),
        entry,
      ]);
      window.getSelection()?.removeAllRanges();
      setSelection(null);
      onCaptured();
    } catch (cause) {
      onReport("Could not record that evidence", cause);
    } finally {
      setBusy(null);
    }
  };

  const importOne = async (annotation: Annotation) => {
    if (!target) return;
    setBusy(annotation.key);
    try {
      // One at a time, deliberately. There is no bulk import: see the design
      // note. Filling a note with evidence nobody read costs more than the
      // clicks save.
      await zoteroApi.captureAnnotations(target.id, source.id, [annotation]);
      onCaptured();
    } catch (cause) {
      onReport("Could not import that annotation", cause);
    } finally {
      setBusy(null);
    }
  };

  return (
    <div
      className="sutra-no-print flex h-full w-list shrink-0 flex-col border-r border-l border-border bg-canvas"
      aria-label={`Reading ${displayTitle(source.title)}`}
    >
      <div className="flex items-start justify-between gap-2 px-3 pt-3 pb-2">
        <div className="min-w-0">
          {/*
            Said before the title, because the pane looks like the note list it
            replaced. What is below is the paper's own words as the PDF stores
            them — not a note, and not something to edit.
          */}
          <p className="text-[0.6875rem] font-semibold tracking-wide text-ink-muted uppercase">
            Extracted PDF text
          </p>
          <p className="truncate text-sm font-semibold text-ink">
            {displayTitle(source.title)}
          </p>
          <p className="mt-0.5 text-[0.6875rem] text-ink-muted">
            Text only — for figures and layout, read it in Zotero.
          </p>
        </div>
        <button
          type="button"
          onClick={onClose}
          aria-label="Close reading"
          className="shrink-0 rounded px-1.5 text-ink-muted transition-colors hover:text-accent"
        >
          ×
        </button>
      </div>

      <div className="flex gap-1 border-b border-border px-3 pb-2">
        {(["text", "annotations"] as const).map((which) => (
          <button
            key={which}
            type="button"
            onClick={() => setMode(which)}
            aria-pressed={mode === which}
            className={`rounded px-2 py-1 text-xs transition-colors ${
              mode === which
                ? "bg-accent-bg text-accent"
                : "text-ink-muted hover:text-accent"
            }`}
          >
            {which === "text" ? "Text" : "Annotations"}
          </button>
        ))}
      </div>

      {!target && (
        <p className="border-b border-border px-3 py-2 text-xs text-ink-soft">
          Open the note you are writing to capture evidence into it. Reading
          works either way.
        </p>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto">
        {mode === "text" ? (
          <TextSide
            outcome={outcome}
            source={source}
            pagesRef={pagesRef}
            onSelect={readSelection}
          />
        ) : (
          <AnnotationSide
            annotations={annotations}
            already={already}
            zoteroError={zoteroError}
            itemKey={itemKey}
            busy={busy}
            canCapture={Boolean(target)}
            onImport={(a) => void importOne(a)}
          />
        )}
      </div>

      {mode === "text" && selection && target && (
        <div className="border-t border-border bg-surface px-3 py-2">
          <p className="mb-1.5 line-clamp-3 text-xs text-ink-soft italic">
            “{selection.text}”
          </p>
          <button
            type="button"
            onClick={() => void capture()}
            disabled={busy === "selection"}
            className="w-full rounded-lg bg-accent px-2 py-1.5 text-xs font-semibold text-surface transition-opacity hover:opacity-90 disabled:opacity-60"
          >
            {busy === "selection"
              ? "Recording…"
              : `Capture as evidence · p. ${selection.page}`}
          </button>
        </div>
      )}
    </div>
  );
}

/** The extracted text, or the named reason there is none. */
function TextSide({
  outcome,
  source,
  pagesRef,
  onSelect,
}: {
  outcome: PdfOutcome | null;
  source: NoteSummary;
  pagesRef: React.RefObject<HTMLDivElement | null>;
  onSelect: () => void;
}) {
  if (!outcome) {
    return <p className="px-3 py-4 text-xs text-ink-muted">Reading…</p>;
  }

  // Every arm is a state of the paper, named. None of them says "error",
  // because none of them is one — and every one of them leaves the note, the
  // citation and the bibliography working, because those depend on a ULID in
  // the markdown and never on a file.
  switch (outcome.state) {
    case "notAttached":
      return <Note>This source has no PDF.</Note>;
    case "unresolved":
      return (
        <Note tone="pending">
          <strong className="font-semibold">
            Sutra cannot open Zotero&rsquo;s copy of this PDF yet.
          </strong>
          <span className="mt-1 block">{outcome.why}</span>
          <span className="mt-1 block">
            Nothing is wrong with the paper or your library. Annotations still
            import, and the note, its citation and the bibliography are
            unaffected.
          </span>
        </Note>
      );
    case "missing":
      return (
        <Note>
          The PDF is recorded on this source but is not where it should be.
          <span className="mt-1 block text-ink-muted">{outcome.detail}</span>
        </Note>
      );
    case "locked":
      return (
        <Note>
          This PDF is password-protected, so its text cannot be read. Zotero can
          still open it.
        </Note>
      );
    case "noTextLayer":
      return (
        <Note>
          This PDF is a scan — there is no text in it to select. Its pages are
          pictures.
        </Note>
      );
    case "failed":
      return (
        <Note>
          Sutra could not read this PDF.
          <span className="mt-1 block text-ink-muted">{outcome.detail}</span>
        </Note>
      );
    case "text":
      return (
        <div ref={pagesRef} onMouseUp={onSelect} onKeyUp={onSelect}>
          {outcome.pages.map((page) => (
            <section key={page.number} className="px-3 py-2">
              {/*
                The page is the container its text sits in, so a selection can
                never be made without one — provenance is structural rather
                than something computed afterwards and hoped to be right.
              */}
              <p
                id={`sutra-page-${page.number}`}
                className="mb-1 text-[0.6875rem] font-semibold tracking-wide text-ink-muted uppercase"
              >
                p. {page.number}
              </p>
              {/*
                `selectable` is not decoration. The app turns selection off on
                `html` — a webview should not behave like a web page — and hands
                it back only where someone is reading or writing prose. Without
                it this text cannot be selected at all, and the entire
                select-and-capture workflow silently does nothing.
              */}
              <p
                data-page={String(page.number)}
                className="selectable sutra-voice sutra-voice-source text-sm leading-relaxed whitespace-pre-wrap"
              >
                {page.text}
              </p>
            </section>
          ))}
          <p className="px-3 py-3 text-[0.6875rem] text-ink-muted">
            {outcome.pages.length} page
            {outcome.pages.length === 1 ? "" : "s"} of{" "}
            {displayTitle(source.title)}
            {outcome.cached ? " · from cache" : ""}
          </p>
        </div>
      );
  }
}

/** Zotero's highlights, one deliberate import at a time. */
function AnnotationSide({
  annotations,
  already,
  zoteroError,
  itemKey,
  busy,
  canCapture,
  onImport,
}: {
  annotations: Annotation[] | null;
  already: Set<string>;
  zoteroError: string | null;
  itemKey: string | null;
  busy: string | null;
  canCapture: boolean;
  onImport: (annotation: Annotation) => void;
}) {
  if (!itemKey) {
    return <Note>This source did not come from Zotero.</Note>;
  }
  if (zoteroError) {
    return (
      <Note>
        Zotero is not answering, so its annotations cannot be read.
        <span className="mt-1 block text-ink-muted">{zoteroError}</span>
      </Note>
    );
  }
  if (!annotations) {
    return <p className="px-3 py-4 text-xs text-ink-muted">Asking Zotero…</p>;
  }
  if (annotations.length === 0) {
    return <Note>No highlights or notes on this paper yet.</Note>;
  }

  return (
    <ul className="flex flex-col">
      {annotations.map((annotation) => {
        const taken = already.has(annotation.key);
        return (
          <li
            key={annotation.key}
            className="border-b border-border px-3 py-2.5 last:border-b-0"
          >
            <div className="mb-1 flex items-center gap-1.5">
              {annotation.colour && (
                // Shown because the researcher chose it. Nothing is derived
                // from it — no kind, no filter, no order. A private colour
                // scheme read as data would be invented provenance.
                <span
                  aria-hidden="true"
                  className="size-2 shrink-0 rounded-full border border-border"
                  style={{ backgroundColor: annotation.colour }}
                />
              )}
              {annotation.page && (
                <span className="text-[0.6875rem] text-ink-muted tabular-nums">
                  p. {annotation.page}
                </span>
              )}
              {taken && (
                <span className="ml-auto text-[0.6875rem] font-semibold text-accent">
                  ✓ already captured
                </span>
              )}
            </div>

            {annotation.text && (
              // The source's words.
              <p className="selectable sutra-voice sutra-voice-source text-sm leading-relaxed">
                {annotation.text}
              </p>
            )}
            {annotation.comment && (
              // The reader's words. Indented, labelled, and never presented
              // beside the quotation as though they were the same kind of
              // thing — the one distinction this import exists to preserve.
              <p className="sutra-voice sutra-voice-interpretation mt-1.5 ml-3 text-xs text-ink-soft">
                <span className="font-semibold text-ink-muted">
                  your note:{" "}
                </span>
                {annotation.comment}
              </p>
            )}

            {!taken && (
              <button
                type="button"
                onClick={() => onImport(annotation)}
                disabled={busy === annotation.key || !canCapture}
                title={
                  canCapture
                    ? undefined
                    : "Open the note you are writing to capture into it"
                }
                className="mt-2 rounded-lg border border-border px-2 py-1 text-xs text-ink-soft transition-colors hover:border-accent hover:text-accent disabled:opacity-50"
              >
                {busy === annotation.key ? "Capturing…" : "Capture"}
              </button>
            )}
          </li>
        );
      })}
    </ul>
  );
}

function Note({
  children,
  tone,
}: {
  children: React.ReactNode;
  tone?: "pending";
}) {
  return (
    <p
      role="status"
      className={`m-3 rounded-lg border px-3 py-2 text-xs ${
        tone === "pending"
          ? "border-accent bg-accent-bg text-ink"
          : "border-border text-ink-soft"
      }`}
    >
      {children}
    </p>
  );
}
