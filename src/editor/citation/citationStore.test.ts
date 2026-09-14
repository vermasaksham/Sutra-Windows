import { beforeEach, describe, expect, it } from "vitest";
import {
  rememberSource,
  resolved,
  setSources,
  setVaultNotes,
  vaultCandidates,
} from "./citationStore";
import type { NoteSummary, NoteType } from "../../vault/api";

function note(id: string, title: string, type: NoteType): NoteSummary {
  return {
    id,
    type,
    title,
    folder: "Library",
    position: 0,
    tags: [],
    icon: null,
    cover: null,
    source: type === "source" ? { zotero: "J938YE6Z" } : undefined,
    excerpt: "",
    updated: "2026-09-14T00:00:00Z",
  };
}

const SOURCE = "01HQ3M8K2P00000000000000A1";
const LITERATURE = "01HQ3M8K2P00000000000000B2";
const GONE = "01HQ3M8K2P00000000000000C3";

const paper = note(
  SOURCE,
  "Sb<sub>2</sub>Se<sub>3</sub> Nanosheet Films",
  "source",
);
const reading = note(LITERATURE, "Reading Singh 2023", "literature");

describe("what a citation resolves against", () => {
  beforeEach(() => {
    setVaultNotes([]);
    setSources([]);
  });

  /**
   * The bug behind "Source not in this vault": resolution read the source list,
   * which is filtered by note type, so a citation whose note existed but was
   * typed anything else reported the note as gone.
   */
  it("resolves a citation to a note that is not typed source", () => {
    setVaultNotes([paper, reading]);
    setSources([paper]);

    const [cited] = resolved([LITERATURE]);
    expect(cited).toBeDefined();
    expect(cited!.title).toBe("Reading Singh 2023");
    // Found, and reported as the wrong type rather than as missing.
    expect(cited!.sourceNote).toBe(false);
  });

  it("still reports a genuinely absent note as unresolved", () => {
    setVaultNotes([paper, reading]);
    setSources([paper]);
    expect(resolved([GONE])).toEqual([]);
  });

  it("keeps the @ menu source-only even though resolution is not", () => {
    setVaultNotes([paper, reading]);
    setSources([paper]);

    const offered = vaultCandidates("");
    expect(offered.map((c) => c.id)).toEqual([SOURCE]);
    // And it offers the paper by its readable name, not its Zotero markup.
    expect(offered[0]!.title).toBe("Sb₂Se₃ Nanosheet Films");
  });

  /**
   * Picking a Zotero item from the `@` menu imports it and inserts a citation
   * naming the new note in the same breath. Nothing else knows that note until
   * the app lists the vault again, so without this the sentence showed a raw
   * ULID in the meantime.
   */
  it("resolves a source note imported a moment ago", () => {
    setVaultNotes([reading]);
    setSources([]);
    expect(resolved([SOURCE])).toEqual([]);

    rememberSource(paper);

    const [cited] = resolved([SOURCE]);
    expect(cited!.sourceNote).toBe(true);
    expect(cited!.title).toBe("Sb₂Se₃ Nanosheet Films");
    // And it is offerable, so the same paper is not imported twice.
    expect(vaultCandidates("").map((c) => c.id)).toEqual([SOURCE]);
  });

  it("does not lose a just-imported source when the vault is listed again", () => {
    rememberSource(paper);
    // A listing that raced the import and does not contain it yet.
    setVaultNotes([reading]);
    expect(resolved([SOURCE])[0]?.title).toBe("Sb₂Se₃ Nanosheet Films");
  });
});
