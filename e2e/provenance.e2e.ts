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
          sources: [{ id: ZHOU, page: "S12" }, { id: KO, page: "4" }],
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
