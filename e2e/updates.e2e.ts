import { expect, test } from "@playwright/test";
import { useVault } from "./vault";

const NOTE = "01HQ3M8K2P00000000000000A1";
const openSettings = async (page: import("@playwright/test").Page) => {
  await page.getByRole("button", { name: /settings/i }).click();
};

/**
 * The update check is a button, never a timer. These tests are as much about
 * what does *not* happen — no request until someone asks — as about what does.
 */
test.describe("checking for updates", () => {
  test("shows the running version without asking the network", async ({
    page,
  }) => {
    let asked = false;
    await useVault(page, {
      notes: [{ id: NOTE, title: "Growth", body: "A note." }],
      update: { current: "0.1.0", latest: "0.2.0", newer: true },
    });
    await page.exposeFunction("__checked", () => {
      asked = true;
    });
    await page.goto("/");
    await openSettings(page);

    await expect(page.getByText(/Sutra 0\.1\.0/)).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Check for updates" }),
    ).toBeVisible();
    expect(asked).toBe(false);
  });

  test("offers the new version once you ask", async ({ page }) => {
    await useVault(page, {
      notes: [{ id: NOTE, title: "Growth", body: "A note." }],
      update: { current: "0.1.0", latest: "0.2.0", newer: true },
    });
    await page.goto("/");
    await openSettings(page);

    await page.getByRole("button", { name: "Check for updates" }).click();
    await expect(
      page.getByText("Sutra 0.2.0 is out. You have 0.1.0."),
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "Get 0.2.0" })).toBeVisible();
  });

  test("says so when you are already on the newest", async ({ page }) => {
    await useVault(page, {
      notes: [{ id: NOTE, title: "Growth", body: "A note." }],
      update: { current: "0.2.0", latest: "0.2.0", newer: false },
    });
    await page.goto("/");
    await openSettings(page);

    await page.getByRole("button", { name: "Check for updates" }).click();
    await expect(page.getByText("This is the newest release.")).toBeVisible();
  });

  test("a failed check says it failed rather than 'up to date'", async ({
    page,
  }) => {
    // Being told nothing is wrong when the check never happened is worse than
    // being told the check failed.
    await useVault(page, {
      notes: [{ id: NOTE, title: "Growth", body: "A note." }],
      update: "fail",
    });
    await page.goto("/");
    await openSettings(page);

    await page.getByRole("button", { name: "Check for updates" }).click();
    await expect(page.getByText(/could not reach GitHub/i)).toBeVisible();
    await expect(page.getByText("This is the newest release.")).toHaveCount(0);
  });
});
