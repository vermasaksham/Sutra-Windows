import { expect, test } from "@playwright/test";
import { useVault } from "./vault";

const NOTE = "01HQ3M8K2P00000000000000A1";
const LONG = Array.from(
  { length: 60 },
  (_, i) =>
    `Paragraph ${i + 1}. Antimony selenide ribbons grow along the c axis.`,
).join("\n\n");

const geometry = (page: import("@playwright/test").Page) =>
  page.evaluate(() => {
    const bar = document.querySelector(
      '[role="toolbar"][aria-label="Formatting"]',
    )!;
    const text = document.querySelector(".sutra-prose")!;
    const b = bar.getBoundingClientRect();
    const t = text.getBoundingClientRect();
    return {
      position: getComputedStyle(bar).position,
      bar: { left: b.left, right: b.right, top: b.top, bottom: b.bottom },
      text: { left: t.left, right: t.right },
      onScreen:
        b.top >= 0 &&
        b.bottom <= innerHeight &&
        b.left >= 0 &&
        b.right <= innerWidth,
    };
  });

const scroll = async (page: import("@playwright/test").Page, to: number) => {
  await page.evaluate((y) => {
    document.querySelector(".sutra-main")!.scrollTop = y;
  }, to);
  await page.waitForTimeout(250);
};

/**
 * Guards the toolbar staying with the reader. It used to be positioned inside
 * the scrolling note area and slid away with the text — a formatting toolbar
 * you have to scroll back to is not a toolbar.
 */
test.describe("the editing toolbar", () => {
  for (const dock of ["bottom", "left", "right"] as const) {
    test(`docked ${dock}, it does not move when the note scrolls`, async ({
      page,
    }) => {
      await useVault(page, {
        notes: [{ id: NOTE, title: "Growth", body: LONG }],
        dock,
      });
      await page.goto("/");
      await expect(
        page.getByRole("toolbar", { name: "Formatting" }),
      ).toBeVisible();

      const before = await geometry(page);
      await scroll(page, 2000);
      const after = await geometry(page);

      expect(after.position).toBe("fixed");
      expect(after.bar).toEqual(before.bar);
      expect(after.onScreen).toBe(true);
    });
  }

  test("docked top, it starts in the note and then pins", async ({ page }) => {
    await useVault(page, {
      notes: [{ id: NOTE, title: "Growth", body: LONG }],
      dock: "top",
    });
    await page.goto("/");
    await expect(
      page.getByRole("toolbar", { name: "Formatting" }),
    ).toBeVisible();

    const before = await geometry(page);
    expect(before.position).toBe("sticky");
    // In the flow to begin with: below the header, not stuck to the top.
    expect(before.bar.top).toBeGreaterThan(100);

    await scroll(page, 2000);
    const after = await geometry(page);
    expect(after.bar.top).toBeLessThan(before.bar.top);
    expect(after.onScreen).toBe(true);
  });

  for (const dock of ["left", "right"] as const) {
    test(`docked ${dock}, it never covers the text`, async ({ page }) => {
      // The bug this replaces: on a narrower window the note column fills the
      // pane edge to edge, and the toolbar sat on the first character of every
      // line.
      await page.setViewportSize({ width: 1180, height: 900 });
      await useVault(page, {
        notes: [{ id: NOTE, title: "Growth", body: LONG }],
        dock,
      });
      await page.goto("/");
      await expect(
        page.getByRole("toolbar", { name: "Formatting" }),
      ).toBeVisible();

      const g = await geometry(page);
      if (dock === "left") expect(g.bar.right).toBeLessThanOrEqual(g.text.left);
      else expect(g.bar.left).toBeGreaterThanOrEqual(g.text.right);
    });
  }
});
