import { expect, test } from "@playwright/test";
import { useVault, type Note } from "./vault";

const A = "01SOURCEAAAA00000000000000";
const B = "01SOURCEBBBB00000000000000";

/** Two source notes the library has rendered in a numbering style. */
const sources = (style: string, citation: string): Note[] => [
  {
    id: A,
    type: "source",
    title: "Quasi-1D Sb2Se3 ribbons",
    body: "",
    source: {
      authors: "Zhou, Y.; Wang, L.",
      year: "2019",
      styled: {
        [style]: { citation, bib: "Zhou, Y. et al. *Nature Energy* 2019." },
      },
    },
  },
  {
    id: B,
    type: "source",
    title: "Chemical Vapour Transport",
    body: "",
    source: {
      authors: "Binnewies, M.",
      year: "2012",
      styled: { [style]: { citation, bib: "Binnewies, M. *CVT*, 2012." } },
    },
  },
];

/**
 * In a numbering style — ACS, the default here — an in-text citation is the
 * paper's position in the reference list, not its author and year.
 *
 * The number cannot come from Zotero: it renders one item at a time and has no
 * idea what else the note cites, so it returns the same number for every one.
 * These tests exist because trusting it would print "(1)" against every paper.
 */
test.describe("numeric citations", () => {
  test("numbers by order of first appearance", async ({ page }) => {
    await useVault(page, {
      style: "acs",
      // Zotero renders each item alone, so both come back as "(1)".
      notes: [
        {
          id: "01HQ3M8K2P00000000000000N1",
          title: "Growth",
          body: `Transport is understood [@${B}]. The ribbons are quasi-1D [@${A}]. And again [@${B}].`,
          sources: [{ id: A }, { id: B }],
        },
        ...sources("acs", "(1)"),
      ],
    });
    await page.goto("/");

    const marks = page.locator(".sutra-citation");
    await expect(marks).toHaveCount(3);
    // First appearance wins: B is cited first, so B is 1 and A is 2 — and the
    // second mention of B is 1 again, not 3.
    await expect(marks.nth(0)).toHaveText("(1)");
    await expect(marks.nth(1)).toHaveText("(2)");
    await expect(marks.nth(2)).toHaveText("(1)");
  });

  test("the bibliography is in the same order as the numbers", async ({
    page,
  }) => {
    await useVault(page, {
      style: "acs",
      notes: [
        {
          id: "01HQ3M8K2P00000000000000N1",
          title: "Growth",
          body: `Transport is understood [@${B}]. The ribbons are quasi-1D [@${A}].`,
          // Recorded in the opposite order to the prose, deliberately: the
          // prose decides, or "[1]" and the first entry are different papers.
          sources: [{ id: A }, { id: B }],
        },
        ...sources("acs", "(1)"),
      ],
    });
    await page.goto("/");

    const entries = page.locator("section:has-text('Bibliography') li");
    await expect(entries.first()).toContainText("CVT");
    await expect(entries.nth(1)).toContainText("Nature Energy");
  });

  test("renumbers as a citation is inserted before another", async ({
    page,
  }) => {
    await useVault(page, {
      style: "acs",
      notes: [
        {
          id: "01HQ3M8K2P00000000000000N1",
          title: "Growth",
          body: `The ribbons are quasi-1D [@${A}].`,
          sources: [{ id: A }],
        },
        ...sources("acs", "(1)"),
      ],
      library: [
        {
          key: "EFGH5678",
          title: "Chemical Vapour Transport",
          creators: "Binnewies",
          year: "2012",
          itemType: "book",
        },
      ],
    });
    await page.goto("/");

    await expect(page.locator(".sutra-citation")).toHaveText(["(1)"]);

    // Put a new citation in front of it. The existing one must become 2.
    await page.locator(".sutra-prose p").first().click();
    await page.keyboard.press("Home");
    await page.keyboard.type("@Vapour");
    await expect(
      page.getByRole("listbox", { name: "Cite a reference" }),
    ).toBeVisible();
    await page.keyboard.press("Enter");

    await expect(page.locator(".sutra-citation")).toHaveCount(2);
    await expect(page.locator(".sutra-citation").nth(1)).toHaveText("(2)");
  });

  test("an author-date style still reads as author and date", async ({
    page,
  }) => {
    await useVault(page, {
      style: "apa",
      notes: [
        {
          id: "01HQ3M8K2P00000000000000N1",
          title: "Growth",
          body: `The ribbons are quasi-1D [@${A}].`,
          sources: [{ id: A }],
        },
        ...sources("apa", "(Zhou et al., 2019)"),
      ],
    });
    await page.goto("/");

    // Exactly as the library rendered it — not wrapped in a second pair of
    // parentheses, which is what happened before.
    await expect(page.locator(".sutra-citation")).toHaveText(
      "(Zhou et al., 2019)",
    );
  });
});
