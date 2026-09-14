import { expect, test } from "@playwright/test";
import { useVault } from "./vault";

const NOTE = "01HQ3M8K2P00000000000000A1";
const ZHOU = "01HQ3M8K2P00000000000000C3";
const KO = "01HQ3M8K2P00000000000000D4";

const source = (id: string, title: string) => ({
  id,
  type: "source",
  title,
  body: "",
  source: { authors: title, year: "2019" },
});

/**
 * Where a note's two records of what it cites disagree.
 *
 * `[@ref]` in the body is the mark in the sentence; a `sources:` entry is the
 * provenance record with the page and the quote. Neither is derived from the
 * other, so they can drift — and until v0.2.1 only one direction of drift was
 * visible.
 *
 * Nothing here is ever fixed automatically. A half-written paragraph looks
 * exactly like a mistake, and deleting a transcribed quote to tidy up a list
 * is the silent loss this release exists to remove.
 */
test.describe("citation consistency", () => {
  test("says nothing when the prose and the record agree", async ({ page }) => {
    await useVault(page, {
      // The note under test comes first: the app opens the first note in the
      // list, and the Sources panel is hidden on a source note by design.
      notes: [
        {
          id: NOTE,
          title: "Growth",
          body: `Ribbons align, as [@${ZHOU}] reports.`,
          sources: [{ id: ZHOU, page: "S12" }],
        },
        source(ZHOU, "Zhou 2019"),
      ],
    });
    await page.goto("/");

    await expect(page.getByText("Citation consistency")).toHaveCount(0);
  });

  test("reports a source recorded but never cited", async ({ page }) => {
    // The direction that was invisible before v0.2.1: cut the sentence, keep
    // the bibliography entry, and nothing said so.
    await useVault(page, {
      notes: [
        {
          id: NOTE,
          title: "Growth",
          body: `Ribbons align, as [@${ZHOU}] reports.`,
          sources: [
            { id: ZHOU, page: "S12" },
            { id: KO, page: "4" },
          ],
        },
        source(ZHOU, "Zhou 2019"),
        source(KO, "Ko 2024"),
      ],
    });
    await page.goto("/");

    await expect(page.getByText("Citation consistency")).toBeVisible();
    await expect(
      page.getByText("1 recorded source is not cited in this note."),
    ).toBeVisible();
    // Named, so the researcher can see which one.
    await expect(
      page.getByText("Recorded here but not cited anywhere in the text"),
    ).toBeVisible();
  });

  test("reports a citation with no provenance record", async ({ page }) => {
    await useVault(page, {
      notes: [
        {
          id: NOTE,
          title: "Growth",
          body: `Ribbons align, as [@${ZHOU}] reports.`,
          sources: [],
        },
        source(ZHOU, "Zhou 2019"),
      ],
    });
    await page.goto("/");

    await expect(
      page.getByText("1 citation in the text has no provenance record."),
    ).toBeVisible();
  });

  test("reports both directions at once, and changes nothing", async ({
    page,
  }) => {
    await useVault(page, {
      notes: [
        {
          id: NOTE,
          title: "Growth",
          body: `Ribbons align, as [@${ZHOU}] reports.`,
          sources: [{ id: KO, page: "4" }],
        },
        source(ZHOU, "Zhou 2019"),
        source(KO, "Ko 2024"),
      ],
    });
    await page.goto("/");

    await expect(
      page.getByText(
        "1 recorded source is not cited in this note. 1 citation in the text has no provenance record.",
      ),
    ).toBeVisible();
    await expect(page.getByText("Nothing has been changed")).toBeVisible();
  });
});

/**
 * What the Sources panel says when a citation does not resolve to a source
 * note — and, more importantly, when it does resolve and the panel used to say
 * it did not.
 *
 * Resolution read the *source* list, which is filtered by note type, so a
 * citation whose note existed under any other type was reported as "Source not
 * in this vault" while the note sat in the vault being cited.
 */
test.describe("a citation that does not resolve to a source note", () => {
  test("names the note when it exists but is not a source", async ({
    page,
  }) => {
    await useVault(page, {
      notes: [
        {
          id: NOTE,
          title: "Growth",
          body: `Ribbons align, as [@${KO}] reports.`,
          sources: [{ id: KO, page: "4" }],
        },
        // A real note, cited, and typed something other than `source`.
        {
          id: KO,
          type: "literature",
          title: "Reading Ko 2024",
          body: "",
        },
      ],
    });
    await page.goto("/");

    // In the panel, in the sentence, and in the bibliography.
    await expect(page.getByText("Reading Ko 2024").first()).toBeVisible();
    await expect(page.getByText("(Reading Ko 2024)")).toBeVisible();
    await expect(page.getByText("Source note missing")).toHaveCount(0);
    await expect(
      page.getByText("A literature note, not a source."),
    ).toBeVisible();
  });

  test("offers a way back when nothing in the vault has the id", async ({
    page,
  }) => {
    await useVault(page, {
      notes: [
        {
          id: NOTE,
          title: "Growth",
          body: `Ribbons align, as [@${ZHOU}] reports.`,
          sources: [{ id: ZHOU, page: "S12" }],
        },
      ],
    });
    await page.goto("/");

    // In the panel, and in the sentence itself.
    await expect(
      page.getByText("Source note missing", { exact: true }),
    ).toBeVisible();
    await expect(page.getByText("(source note missing)")).toBeVisible();
    // The id is present as something to search for, not as the paper's name.
    await expect(page.getByText(".sutra/trash")).toBeVisible();
    await expect(page.getByText(`Reference ${ZHOU}`)).toHaveCount(0);
  });

  /**
   * Zotero keeps a title's formatting as HTML, and a materials library is full
   * of it. Shown raw, a citation reads `Sb<sub>2</sub>Se<sub>3</sub>`.
   */
  test("shows a chemistry title as chemistry, not as markup", async ({
    page,
  }) => {
    await useVault(page, {
      notes: [
        {
          id: NOTE,
          title: "Growth",
          body: `Ribbons align, as [@${ZHOU}] reports.`,
          sources: [{ id: ZHOU, page: "S12" }],
        },
        {
          id: ZHOU,
          type: "source",
          title: "α-Sb<sub>2</sub>O<sub>3</sub> polymorphs",
          body: "",
          // Authors kept plain: the title is what this test is about, and
          // Zotero's markup lives there.
          source: { authors: "Zhou, Y.", year: "2019" },
        },
      ],
    });
    await page.goto("/");

    await expect(page.getByText("α-Sb₂O₃ polymorphs").first()).toBeVisible();
    await expect(page.getByText("<sub>")).toHaveCount(0);
  });
});
