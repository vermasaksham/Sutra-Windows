import { expect, test } from "@playwright/test";
import {
  lastExported,
  textOf,
  useVault,
  type ExportedBlock,
  type ExportedDocument,
} from "./vault";

const NOTE = "01HQ3M8K2P00000000000000A1";
const OTHER = "01HQ3M8K2P00000000000000B2";
const SOURCE = "01HQ3M8K2P00000000000000C3";

/**
 * What reaches the exporter.
 *
 * Rust turns one of these documents into a .docx and has its own tests for
 * that half. Every v0.2 export defect lived on this side: content that never
 * made it into the document at all, so no amount of correct OOXML could have
 * saved it.
 *
 * Each test here corresponds to a defect the v0.2 audit found.
 */

async function exportOpenNote(page: import("@playwright/test").Page) {
  await page.getByRole("button", { name: "Export" }).click();
  await page.getByRole("menuitem", { name: /Word/ }).click();
  await expect.poll(() => lastExported(page), { timeout: 15000 }).toBeTruthy();
  return (await lastExported(page)) as ExportedDocument;
}

const paragraphs = (doc: ExportedDocument): ExportedBlock[] =>
  doc.blocks.filter((b) => b.kind === "paragraph");

test.describe("Word export", () => {
  test("a wikilink exports as the note's title, not its id", async ({
    page,
  }) => {
    await useVault(page, {
      notes: [
        {
          id: NOTE,
          title: "Growth",
          body: `Compare with [[${OTHER}]] before concluding.`,
        },
        { id: OTHER, title: "Phonon transport", body: "" },
      ],
    });
    await page.goto("/");

    const doc = await exportOpenNote(page);
    const text = paragraphs(doc).map(textOf).join(" ");

    expect(text).toContain("Phonon transport");
    expect(text).not.toContain(OTHER);
  });

  test("a link to a note that no longer exists keeps its id visible", async ({
    page,
  }) => {
    // Never silently dropped: the reference really is in the file, and the
    // brackets say it is a reference to something missing.
    await useVault(page, {
      notes: [{ id: NOTE, title: "Growth", body: `Compare with [[${OTHER}]].` }],
    });
    await page.goto("/");

    const doc = await exportOpenNote(page);
    expect(paragraphs(doc).map(textOf).join(" ")).toContain(`[[${OTHER}]]`);
  });

  test("an inline formula stays inside its sentence", async ({ page }) => {
    // The defect: the formula was lifted out and appended as its own block,
    // so the sentence exported as "The bandgap is  eV".
    await useVault(page, {
      notes: [
        {
          id: NOTE,
          title: "Growth",
          body: "The bandgap is $E_g = 1.18$ eV at room temperature.",
        },
      ],
    });
    await page.goto("/");

    const doc = await exportOpenNote(page);
    const sentence = paragraphs(doc).find((b) =>
      textOf(b).includes("The bandgap is"),
    );
    expect(sentence, "no paragraph carried the sentence").toBeTruthy();

    const runs = sentence!.runs ?? [];
    expect(
      runs.some((r) => r.image),
      "the formula did not stay in the paragraph",
    ).toBe(true);
    expect(textOf(sentence!)).toContain(" eV at room temperature.");
  });

  test("a formula inside a quotation survives", async ({ page }) => {
    // Evidence lives in blockquotes. The quote branch handled no inline maths
    // at all, so a formula in a quoted passage vanished without trace.
    await useVault(page, {
      notes: [
        {
          id: NOTE,
          title: "Reading",
          body: "> They report $T_c = 620$ K for the same film.",
        },
      ],
    });
    await page.goto("/");

    const doc = await exportOpenNote(page);
    const quote = doc.blocks.find((b) => b.kind === "quote");
    expect(quote, "the quotation did not reach the document").toBeTruthy();
    expect(textOf(quote!)).toContain("They report");
    expect(
      (quote!.runs ?? []).some((r) => r.image),
      "the quoted formula was lost",
    ).toBe(true);
  });

  test("a table keeps its formatting, its formulas and its citations", async ({
    page,
  }) => {
    await useVault(page, {
      style: "acs",
      // The note under test comes first: the app opens the first note in the
      // list, and exporting is always "the open note".
      notes: [
        {
          id: NOTE,
          title: "Growth",
          body: [
            "| Parameter | Value |",
            "| --- | --- |",
            "| Source | 560 **°C** |",
            `| Bandgap | $E_g$ [@${SOURCE}] |`,
          ].join("\n"),
        },
        {
          id: SOURCE,
          type: "source",
          title: "Zhou 2019",
          body: "",
          source: {
            authors: "Zhou, Y.",
            year: "2019",
            styled: { acs: { citation: "1", bib: "Zhou, Y. 2019." } },
          },
        },
      ],
    });
    await page.goto("/");

    const doc = await exportOpenNote(page);
    const table = doc.blocks.find((b) => b.kind === "table") as
      | Extract<ExportedBlock, { kind: "table" }>
      | undefined;
    expect(table, "the table did not reach the document").toBeTruthy();

    const cells = table!.rows.flat();
    const flat = cells.map((cell) => cell.map((r) => r.text).join(""));

    expect(flat.join(" ")).toContain("560");
    expect(
      cells.some((cell) => cell.some((r) => r.bold)),
      "a bold value in a cell lost its formatting",
    ).toBe(true);
    expect(
      cells.some((cell) => cell.some((r) => r.image)),
      "a formula in a cell was lost",
    ).toBe(true);
    expect(
      flat.join(" "),
      "a citation in a cell was lost",
    ).toMatch(/\(?1\)?/);
  });

  test("a realistic research note loses nothing", async ({ page }) => {
    // Headings, prose, a wikilink, a citation, an inline formula, a block
    // formula, a quotation with a formula and a formatted table, all at once.
    await useVault(page, {
      style: "acs",
      notes: [
        {
          id: NOTE,
          title: "Sb2Se3 growth",
          body: [
            "## Summary",
            "",
            `Ribbons align along [001], as [@${SOURCE}] reports, and the gap is $E_g = 1.18$ eV.`,
            "",
            `See also [[${OTHER}]].`,
            "",
            "> The measured $T_c$ was 620 K.",
            "",
            // The block fence needs `$$` on its own lines — the single-line
            // form is an inline formula, not a display equation.
            "$$",
            "\\ce{Sb2Se3 + 3H2 -> 2Sb + 3H2Se}",
            "$$",
            "",
            "| Parameter | Value |",
            "| --- | --- |",
            "| Source | 560 **°C** |",
          ].join("\n"),
        },
        {
          id: SOURCE,
          type: "source",
          title: "Zhou 2019",
          body: "",
          source: {
            authors: "Zhou, Y.",
            year: "2019",
            styled: { acs: { citation: "1", bib: "Zhou, Y. 2019." } },
          },
        },
        { id: OTHER, title: "Phonon transport", body: "" },
      ],
    });
    await page.goto("/");

    const doc = await exportOpenNote(page);
    const everything = doc.blocks.map((b) => textOf(b)).join(" ");

    // Nothing a person wrote went missing.
    expect(everything).toContain("Summary");
    expect(everything).toContain("Ribbons align along [001]");
    expect(everything).toContain(" eV.");
    expect(everything).toContain("Phonon transport");
    expect(everything).not.toContain(OTHER);
    expect(everything).toContain("The measured");

    // Both kinds of formula are present as pictures.
    const inlineImages = doc.blocks.flatMap((b) =>
      (b.runs ?? []).filter((r) => r.image),
    );
    expect(
      inlineImages.length,
      "the inline formula and the quoted one must both be runs",
    ).toBeGreaterThanOrEqual(2);
    expect(
      doc.blocks.some((b) => b.kind === "image"),
      "the display equation must be its own block",
    ).toBe(true);
    expect(doc.blocks.some((b) => b.kind === "table")).toBe(true);

    // And the bibliography came with it.
    expect(doc.references.length).toBeGreaterThan(0);
  });
});
