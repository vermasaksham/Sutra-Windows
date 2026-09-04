import { expect, test } from "@playwright/test";
import { useVault, type Note } from "./vault";

/**
 * A vault the size a PhD reaches, in the browser.
 *
 * The Rust side is measured separately and is fine — a full scan of 5,000
 * notes is about 80ms. The risk this covers is the other half: a note list
 * that builds five thousand rows of DOM, and a search box that filters them on
 * every keystroke.
 */
const many = (count: number): Note[] =>
  Array.from({ length: count }, (_, i) => ({
    id: `01HQ3M8K2P${String(i).padStart(16, "0")}`,
    title: `Note ${i} on Sb2Se3 growth`,
    folder: `Strand ${i % 7}`,
    tags: [`method/${["xrd", "dsc", "sem"][i % 3]}`],
    body: `Run ${i}. Antimony selenide ribbons grow along the c axis.`,
  }));

test.describe("a large vault", () => {
  test("opens, lists and searches without stalling", async ({ page }) => {
    test.setTimeout(90_000);
    await useVault(page, { notes: many(5_000) });

    const started = Date.now();
    await page.goto("/");
    await expect(page.locator(".sutra-prose")).toBeVisible();
    const opened = Date.now() - started;
    // Generous, because CI machines vary and this is a guard against an
    // accidental O(n²), not a stopwatch. A person waits through five seconds.
    expect(opened, `opening a 5,000-note vault took ${opened}ms`).toBeLessThan(
      15_000,
    );

    // Typing must stay responsive with every note in memory.
    const search = page.getByPlaceholder(/search/i);
    const typing = Date.now();
    await search.fill("Note 4242");
    await page.waitForTimeout(400);
    const filtered = Date.now() - typing;
    expect(filtered, `filtering took ${filtered}ms`).toBeLessThan(8_000);

    await expect(
      page.getByText("Note 4242 on Sb2Se3 growth").first(),
    ).toBeVisible();
  });

  test("does not build a DOM node for every note", async ({ page }) => {
    // The list is the one place where "render everything" stops being free.
    // If this ever fails, the note list needs windowing.
    await useVault(page, { notes: many(5_000) });
    await page.goto("/");
    await expect(page.locator(".sutra-prose")).toBeVisible();

    const nodes = await page.evaluate(
      () => document.querySelectorAll("*").length,
    );
    expect(
      nodes,
      `the page built ${nodes} DOM nodes for 5,000 notes`,
    ).toBeLessThan(20_000);
  });
});
