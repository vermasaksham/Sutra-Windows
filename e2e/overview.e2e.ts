import { expect, test } from "@playwright/test";
import { useVault } from "./vault";

/**
 * The research overview counts what is written; it never judges it. These
 * tests are mostly about that line: an unanswered question is one with nothing
 * under it, and a source is unused when nothing cites it — both facts, neither
 * an opinion.
 */
/** Reach the overview the way a person does: the command palette. */
async function openOverview(page: import("@playwright/test").Page) {
  await page.locator(".sutra-main").waitFor();
  await page.keyboard.press("Control+k");
  const palette = page.getByRole("dialog", { name: "Command palette" });
  await palette.getByLabel("Command or note").fill("where the work");
  await palette.getByRole("button", { name: /Where the work is/ }).click();
}

const NOTES = [
  {
    id: "01HQ3M8K2P00000000000000A1",
    title: "Thermal conductivity",
    body: "## My question\n\n## Source says\n\n> kappa is low\n\n## My interpretation\n\nThe ribbons decouple phonons.",
  },
  {
    id: "01HQ3M8K2P00000000000000A2",
    title: "Growth rate",
    body: "## Research questions\n\nWhy does iodine help? Three lines of thought so far.",
  },
  {
    id: "01SOURCEABCD12340000000000",
    type: "source",
    title: "Ko 2024",
    body: "",
  },
  {
    id: "01SOURCEEFGH56780000000000",
    type: "source",
    title: "Binnewies 2012",
    body: "",
  },
];

test.describe("the research overview", () => {
  test("separates questions asked from questions started", async ({ page }) => {
    await useVault(page, {
      notes: NOTES,
      citations: { "01SOURCEABCD12340000000000": 2 },
      withPage: 3,
      withQuote: 2,
    });
    await page.goto("/");

    await openOverview(page);

    const dialog = page.getByRole("dialog", { name: "Research overview" });
    await expect(dialog).toBeVisible();

    // "My question" has nothing under it; "Research questions" has prose.
    await expect(
      dialog.getByRole("button", { name: /My question/ }),
    ).toBeVisible();
    await expect(
      dialog.getByRole("button", { name: /Research questions/ }),
    ).toBeVisible();
  });

  test("names the sources nothing cites", async ({ page }) => {
    await useVault(page, {
      notes: NOTES,
      citations: { "01SOURCEABCD12340000000000": 2 },
    });
    await page.goto("/");

    await openOverview(page);

    const dialog = page.getByRole("dialog", { name: "Research overview" });
    // Ko 2024 is cited twice, Binnewies by nothing.
    await expect(dialog.getByText("Binnewies 2012")).toBeVisible();
    await expect(dialog.getByText("Ko 2024")).toHaveCount(0);
  });

  test("opens the note a question was asked in", async ({ page }) => {
    await useVault(page, { notes: NOTES });
    await page.goto("/");

    await openOverview(page);

    const dialog = page.getByRole("dialog", { name: "Research overview" });
    await dialog.getByRole("button", { name: /My question/ }).click();
    await expect(dialog).toHaveCount(0);
    await expect(page.locator(".sutra-prose")).toContainText("phonons");
  });
});
