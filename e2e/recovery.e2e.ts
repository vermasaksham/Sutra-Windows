import { expect, test } from "@playwright/test";
import { useVault } from "./vault";

const NOTE = "01HQ3M8K2P00000000000000A1";

/**
 * What the app does when the vault on disk is not in the state it expects.
 *
 * The rule throughout is preserve, warn, let the researcher reconcile — never
 * guess and overwrite. These cover the half of that a browser can see.
 */
test.describe("recovery", () => {
  test("says so when two files claim to be the same note", async ({ page }) => {
    await useVault(page, {
      notes: [{ id: NOTE, title: "Growth", body: "Ribbons align." }],
      idClashes: [
        {
          id: NOTE,
          opened: "Research/Growth.md",
          shadowed: "Research/Growth (conflicted copy).md",
        },
      ],
    });
    await page.goto("/");

    await expect(
      page.getByText("Two files claim to be the same note"),
    ).toBeVisible();
    // Both named, so the researcher can go and compare them.
    await expect(page.getByText("Research/Growth.md")).toBeVisible();
    await expect(
      page.getByText("Research/Growth (conflicted copy).md"),
    ).toBeVisible();
    // And it is explicit that nothing was changed.
    await expect(page.getByText(/nothing has been changed/i)).toBeVisible();
  });

  test("says nothing when the vault is consistent", async ({ page }) => {
    await useVault(page, {
      notes: [{ id: NOTE, title: "Growth", body: "Ribbons align." }],
    });
    await page.goto("/");
    await page.locator(".sutra-prose").waitFor();

    await expect(page.getByText(/claim to be the same note/i)).toHaveCount(0);
  });
});
