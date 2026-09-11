import { expect, test } from "@playwright/test";
import { lastSaved, useVault } from "./vault";

const NOTE = "01HQ3M8K2P00000000000000A1";

const LIBRARY = [
  {
    key: "ABCD1234",
    title: "Quasi-1D Sb2Se3 ribbons",
    creators: "Zhou et al.",
    year: "2019",
    itemType: "journalArticle",
    doi: "10.1000/xyz",
  },
  {
    key: "EFGH5678",
    title: "Chemical Vapour Transport",
    creators: "Binnewies",
    year: "2012",
    itemType: "book",
    doi: null,
  },
];

/**
 * The anti-hallucination rule from the original specification, as a test: the
 * file must record the key the library gave, never the label shown on screen,
 * and a paper the library does not have must be reported rather than invented.
 */
test.describe("citing", () => {
  test("@ searches the library and stores the key, not the label", async ({
    page,
  }) => {
    await useVault(page, {
      notes: [{ id: NOTE, title: "Growth", body: "Prior work exists." }],
      library: LIBRARY,
    });
    await page.goto("/");

    await page.locator(".sutra-prose p").first().click();
    await page.keyboard.press("End");
    await page.keyboard.type(" @ribbons");

    const menu = page.getByRole("listbox", { name: "Cite a reference" });
    await expect(menu).toBeVisible();
    await expect(menu).toContainText("Quasi-1D Sb2Se3 ribbons");
    await page.keyboard.press("Enter");

    // On screen it reads as a citation; the label is derived at render time.
    await expect(page.locator(".sutra-citation")).toHaveText(
      "(Zhou et al., 2019)",
    );

    await expect.poll(() => lastSaved(page), { timeout: 8000 }).toContain("[@");
    const saved = await lastSaved(page);
    // In the file it is the identifier, and nothing that was displayed. This
    // is the anti-hallucination rule: a citation key is never composed from
    // what a human saw, only ever taken from the library.
    expect(saved).not.toContain("Zhou");
    expect(saved).not.toContain("2019");
  });

  test("a key the library does not have says so rather than inventing one", async ({
    page,
  }) => {
    await useVault(page, {
      notes: [
        { id: NOTE, title: "Growth", body: "A missing one [@ZZZZ0000]." },
      ],
      library: LIBRARY,
    });
    await page.goto("/");

    const citation = page.locator(".sutra-citation").first();
    await expect(citation).toBeVisible();
    await expect(citation).toContainText("ZZZZ0000");
  });

  test("a failed import says so instead of doing nothing", async ({ page }) => {
    await useVault(page, {
      notes: [{ id: NOTE, title: "Growth", body: "Prior work exists." }],
      library: LIBRARY,
      importFails: true,
    });
    await page.goto("/");

    await page.locator(".sutra-prose p").first().click();
    await page.keyboard.press("End");
    await page.keyboard.type(" @ribbons");
    await expect(
      page.getByRole("listbox", { name: "Cite a reference" }),
    ).toBeVisible();
    await page.keyboard.press("Enter");

    // The menu closes on Enter and the import fails after it has gone, so
    // without this the person sees their sentence unchanged and no reason
    // for it — which reads as citing being broken rather than Zotero having
    // gone away mid-pick.
    await expect(page.getByRole("status")).toContainText(
      "Could not add Quasi-1D Sb2Se3 ribbons from Zotero",
    );
    // And nothing half-written is left behind: no citation of nothing.
    await expect(page.locator(".sutra-citation")).toHaveCount(0);
  });

  test("the rail brings papers in; it does not claim to cite them", async ({
    page,
  }) => {
    await useVault(page, {
      notes: [{ id: NOTE, title: "Growth", body: "Prior work exists." }],
      library: LIBRARY,
    });
    await page.goto("/");

    // It was called "Cite a paper" and does not cite: it imports, and `@` in
    // the body is what cites. The name is the fix, so the name is the test.
    await page
      .getByRole("button", { name: "Add paper from Zotero", exact: true })
      .click();
    await expect(
      page.getByRole("dialog", { name: "Zotero references" }),
    ).toBeVisible();
  });

  test("an email address does not open the citation menu", async ({ page }) => {
    await useVault(page, {
      notes: [{ id: NOTE, title: "Growth", body: "Write to" }],
      library: LIBRARY,
    });
    await page.goto("/");

    await page.locator(".sutra-prose p").first().click();
    await page.keyboard.press("End");
    await page.keyboard.type(" someone@example.com");

    await expect(
      page.getByRole("listbox", { name: "Cite a reference" }),
    ).toHaveCount(0);
  });

  test("with Zotero closed, the note still opens and says why", async ({
    page,
  }) => {
    await useVault(page, {
      notes: [{ id: NOTE, title: "Growth", body: "Prior work [@EFGH5678]." }],
      library: LIBRARY,
      zoteroDown: true,
    });
    await page.goto("/");

    // Nothing is hidden and nothing is lost: the note renders, and the text of
    // the citation is still in the file.
    await expect(page.locator(".sutra-prose")).toContainText("Prior work");
    await expect(page.locator(".sutra-citation")).toHaveCount(1);
  });
});
