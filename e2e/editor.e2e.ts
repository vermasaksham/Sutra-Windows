import { expect, test } from "@playwright/test";
import { lastSaved, useVault } from "./vault";

const NOTE = "01HQ3M8K2P00000000000000A1";

test.describe("writing", () => {
  test("types into the note and saves markdown, not HTML", async ({ page }) => {
    await useVault(page, {
      notes: [
        {
          id: NOTE,
          title: "Growth",
          body: "The seed layer decides the texture.",
        },
      ],
    });
    await page.goto("/");

    const body = page.locator(".sutra-prose");
    await expect(body).toContainText("The seed layer decides the texture.");

    await body.locator("p").first().click();
    await page.keyboard.press("End");
    await page.keyboard.type(" Iodine carries it.");

    // The file is the product. Assert on what was written, not on the DOM.
    await expect
      .poll(() => lastSaved(page), { timeout: 8000 })
      .toContain("The seed layer decides the texture. Iodine carries it.");
    expect(await lastSaved(page)).not.toContain("<p>");
  });

  test("the slash menu inserts a heading", async ({ page }) => {
    await useVault(page, {
      notes: [{ id: NOTE, title: "Growth", body: "First line." }],
    });
    await page.goto("/");

    await page.locator(".sutra-prose p").first().click();
    await page.keyboard.press("End");
    await page.keyboard.press("Enter");
    await page.keyboard.type("/h2");

    const menu = page.getByRole("listbox");
    await expect(menu).toBeVisible();
    await page.keyboard.press("Enter");
    await page.keyboard.type("Method");

    await expect(page.locator(".sutra-prose h2")).toHaveText("Method");
    await expect
      .poll(() => lastSaved(page), { timeout: 8000 })
      .toContain("## Method");
  });

  test("a heading typed as markdown becomes a heading", async ({ page }) => {
    await useVault(page, {
      notes: [{ id: NOTE, title: "Growth", body: "First line." }],
    });
    await page.goto("/");

    await page.locator(".sutra-prose p").first().click();
    await page.keyboard.press("End");
    await page.keyboard.press("Enter");
    await page.keyboard.type("# Results");

    await expect(page.locator(".sutra-prose h1")).toHaveText("Results");
  });
});
