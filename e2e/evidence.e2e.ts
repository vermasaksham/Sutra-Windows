import { expect, test } from "@playwright/test";
import { useVault } from "./vault";

const NOTE = "01HQ3M8K2P00000000000000A1";
const OTHER = "01HQ3M8K2P00000000000000B2";
const PAPER = "01HQ3M8K2P00000000000000C3";
const E1 = "01HQ3M8K2P0000000000000EE1";
const E2 = "01HQ3M8K2P0000000000000EE2";

/**
 * Browsing evidence apart from the notes that use it.
 *
 * The question this surface exists for is the one no note can answer about
 * itself: which *other* notes rest on the same sentence.
 */
function vault() {
  return {
    notes: [
      {
        id: NOTE,
        title: "Growth of ribbons",
        body: "## My interpretation\n\nThe alignment is the story.",
        sources: [
          { eid: E1, id: PAPER, at: "source", comment: "only two samples" },
          {
            eid: E2,
            id: PAPER,
            page: "S12",
            quote: "conductivity falls above 400 K",
            kind: "result",
          },
        ],
      },
      {
        id: OTHER,
        title: "Chapter 3 draft",
        body: "Nothing yet.",
        sources: [
          {
            eid: E1,
            id: PAPER,
            at: "source",
            comment: "supports the growth argument",
          },
        ],
      },
      {
        id: PAPER,
        type: "source",
        title: "α-Sb<sub>2</sub>O<sub>3</sub> polymorphs",
        body: "",
        source: { authors: "Zhou, Y.", year: "2019" },
        evidence: [
          {
            eid: E1,
            page: "431",
            quote: "ribbons align along c",
            kind: "measurement",
          },
        ],
      },
    ],
  };
}

async function openEvidence(page: import("@playwright/test").Page) {
  await page.getByRole("button", { name: "Evidence", exact: true }).click();
  await expect(page.getByLabel("Evidence", { exact: true })).toBeVisible();
}

test.describe("the evidence browser", () => {
  test("shows one item per quotation, and every note resting on it", async ({
    page,
  }) => {
    await useVault(page, vault());
    await page.goto("/");
    await openEvidence(page);

    const pane = page.getByLabel("Evidence", { exact: true });
    // One record, quoted by two notes — not two copies of one sentence.
    await expect(pane.getByText("ribbons align along c")).toHaveCount(1);
    // This note cites both items, so it appears on more than one of them.
    await expect(
      pane.getByRole("button", { name: /Growth of ribbons/ }).first(),
    ).toBeVisible();
    await expect(
      pane.getByRole("button", { name: /Chapter 3 draft/ }),
    ).toBeVisible();

    // Each reader's own remark stays attached to that reader, never to the
    // paper's words.
    await expect(pane.getByText("only two samples")).toBeVisible();
    await expect(pane.getByText("supports the growth argument")).toBeVisible();
  });

  test("names the paper as chemistry, not as markup", async ({ page }) => {
    await useVault(page, vault());
    await page.goto("/");
    await openEvidence(page);

    // The paper's own button on the item, not the filter's `<option>` — an
    // option is not visible to a reader, and what a reader sees is the point.
    await expect(
      page
        .getByLabel("Evidence", { exact: true })
        .getByRole("button", { name: "α-Sb₂O₃ polymorphs" })
        .first(),
    ).toBeVisible();
    await expect(page.getByText("<sub>")).toHaveCount(0);
  });

  test("searches the source's exact words", async ({ page }) => {
    await useVault(page, vault());
    await page.goto("/");
    await openEvidence(page);

    const pane = page.getByLabel("Evidence", { exact: true });
    await expect(pane.getByText("2 of 2")).toBeVisible();

    await pane.getByLabel("Search evidence text").fill("conductivity");
    await expect(pane.getByText("1 of 2")).toBeVisible();
    await expect(
      pane.getByText("conductivity falls above 400 K"),
    ).toBeVisible();
    await expect(pane.getByText("ribbons align along c")).toHaveCount(0);

    // The search is over the paper's words, not the reader's notes on them.
    await pane.getByLabel("Search evidence text").fill("only two samples");
    await expect(pane.getByText("0 of 2")).toBeVisible();
  });

  test("filters by kind of evidence", async ({ page }) => {
    await useVault(page, vault());
    await page.goto("/");
    await openEvidence(page);

    const pane = page.getByLabel("Evidence", { exact: true });
    await pane
      .getByLabel("Filter by kind of evidence")
      .selectOption("measurement");
    await expect(pane.getByText("1 of 2")).toBeVisible();
    await expect(pane.getByText("ribbons align along c")).toBeVisible();
  });

  test("opens the note resting on a quotation", async ({ page }) => {
    await useVault(page, vault());
    await page.goto("/");
    await openEvidence(page);

    await page
      .getByLabel("Evidence", { exact: true })
      .getByRole("button", { name: /Chapter 3 draft/ })
      .click();

    await expect(page.getByLabel("Note title")).toHaveValue("Chapter 3 draft");
    // And the browser stays open: comparing what rests on one sentence is the
    // whole point, and that takes more than one visit.
    await expect(page.getByLabel("Evidence", { exact: true })).toBeVisible();
  });

  test("says so when a reference points at a record that is gone", async ({
    page,
  }) => {
    await useVault(page, {
      notes: [
        {
          id: NOTE,
          title: "Growth of ribbons",
          body: "",
          sources: [
            { eid: E1, id: PAPER, at: "source", comment: "the key number" },
          ],
        },
        // The paper is here; its `evidence:` list is not.
        { id: PAPER, type: "source", title: "Zhou 2019", body: "" },
      ],
    });
    await page.goto("/");
    await openEvidence(page);

    const pane = page.getByLabel("Evidence", { exact: true });
    await expect(
      pane.getByText("The record for this evidence is missing"),
    ).toBeVisible();
    // What the reader wrote survives — it is all that is left of it.
    await expect(pane.getByText("the key number")).toBeVisible();
  });

  test("an empty vault says what would put something here", async ({
    page,
  }) => {
    await useVault(page, {
      notes: [{ id: NOTE, title: "Growth of ribbons", body: "" }],
    });
    await page.goto("/");
    await openEvidence(page);

    await expect(page.getByText(/Nothing recorded yet/)).toBeVisible();
  });
});
