import { expect, test } from "@playwright/test";
import { lastSaved, useVault } from "./vault";

const NOTE = "01HQ3M8K2P00000000000000A1";

/**
 * Guards the maths behaviour, including three faults found by driving this by
 * hand: a chemical equation that opened rendered with no caret, a caret that
 * landed outside `\ce{}` instead of inside it, and a sentence typed after a
 * formula going nowhere because focus fell to the document body.
 */
test.describe("formulas", () => {
  test("`$…$` becomes a formula as it is typed, and stays LaTeX on disk", async ({
    page,
  }) => {
    await useVault(page, {
      notes: [{ id: NOTE, title: "Growth", body: "The gap is" }],
    });
    await page.goto("/");

    await page.locator(".sutra-prose p").first().click();
    await page.keyboard.press("End");
    await page.keyboard.type(" $E_g = 1.18$");

    await expect(page.locator(".sutra-math-inline")).toHaveCount(1);
    await expect
      .poll(() => lastSaved(page), { timeout: 8000 })
      .toContain("$E_g = 1.18$");
  });

  test("the toolbar starts an inline formula, ready to type", async ({
    page,
  }) => {
    // The point of the button: somebody who does not know that `$…$` is the
    // syntax should still be able to write a formula in a sentence.
    await useVault(page, {
      notes: [{ id: NOTE, title: "Growth", body: "The bandgap is" }],
    });
    await page.goto("/");

    await page.locator(".sutra-prose p").first().click();
    await page.keyboard.press("End");
    await page.keyboard.type(" ");
    await page.getByRole("button", { name: "Inline equation" }).click();

    // Straight into its input — not a node to hunt for and click.
    const input = page.locator(".sutra-math-input");
    await expect(input).toBeVisible();
    await expect(input).toBeFocused();

    await page.keyboard.type("E = mc^2");
    await page.keyboard.press("Escape");

    await expect(page.locator(".sutra-math-inline")).toHaveCount(1);
    await expect
      .poll(() => lastSaved(page), { timeout: 8000 })
      .toContain("$E = mc^2$");
  });

  test("a toolbar formula stays in the sentence it was started in", async ({
    page,
  }) => {
    // The whole difference between this and the Equation button. The formula
    // must sit between the words, and typing afterwards must continue the same
    // paragraph rather than a new one.
    await useVault(page, {
      notes: [{ id: NOTE, title: "Growth", body: "The bandgap is" }],
    });
    await page.goto("/");

    await page.locator(".sutra-prose p").first().click();
    await page.keyboard.press("End");
    await page.keyboard.type(" ");
    await page.getByRole("button", { name: "Inline equation" }).click();
    await page.keyboard.type("E_g");
    await page.keyboard.press("Escape");
    await page.keyboard.type(" eV at room temperature.");

    // One paragraph, holding the words either side of the formula.
    const paragraphs = page.locator(".sutra-prose p");
    await expect(paragraphs).toHaveCount(1);
    await expect(paragraphs.first().locator(".sutra-math-inline")).toHaveCount(
      1,
    );
    await expect(paragraphs.first()).toContainText("The bandgap is");
    await expect(paragraphs.first()).toContainText("eV at room temperature.");

    await expect
      .poll(() => lastSaved(page), { timeout: 8000 })
      .toContain("$E_g$ eV at room temperature.");
  });

  test("the slash menu starts one too", async ({ page }) => {
    await useVault(page, {
      notes: [{ id: NOTE, title: "Growth", body: "Mass and energy: " }],
    });
    await page.goto("/");

    await page.locator(".sutra-prose p").first().click();
    await page.keyboard.press("End");
    await page.keyboard.type("/inline");
    await page.getByText("Inline equation").click();

    await expect(page.locator(".sutra-math-input")).toBeFocused();
    await page.keyboard.type("m");
    await page.keyboard.press("Escape");

    await expect
      .poll(() => lastSaved(page), { timeout: 8000 })
      .toContain("$m$");
  });

  test("editing an inline formula does not box it or grow the line", async ({
    page,
  }) => {
    await useVault(page, {
      notes: [
        {
          id: NOTE,
          title: "Growth",
          body: "The gap is $E_g = 1.18$ eV at room temperature.",
        },
      ],
    });
    await page.goto("/");

    const paragraph = page.locator(".sutra-prose p").first();
    const before = (await paragraph.boundingBox())!.height;

    await page.locator(".sutra-math-inline").first().click();
    const input = page.locator(".sutra-math-input");
    await expect(input).toBeVisible();

    // The complaint this fixes: the source used to open in a bordered, filled
    // input that was taller than the line and shoved the sentence along.
    const style = await input.evaluate((el) => {
      const s = getComputedStyle(el);
      return {
        border: s.borderStyle,
        background: s.backgroundColor,
        padding: s.padding,
      };
    });
    expect(style.border).toBe("none");
    expect(style.background).toBe("rgba(0, 0, 0, 0)");
    expect(style.padding).toBe("0px");

    const after = (await paragraph.boundingBox())!.height;
    expect(after).toBe(before);
  });

  test("a chemical equation opens ready to type, with the caret inside \\ce{}", async ({
    page,
  }) => {
    await useVault(page, {
      notes: [{ id: NOTE, title: "Growth", body: "Transport step." }],
    });
    await page.goto("/");

    await page.locator(".sutra-prose p").first().click();
    await page.keyboard.press("End");
    await page.getByRole("button", { name: "Chemical equation" }).click();

    // It used to open *rendered* — an all-but-invisible strip with no caret,
    // because the test for "new" was `latex === ""` and this inserts `\ce{}`.
    const source = page.locator(".sutra-math-textarea");
    await expect(source).toBeVisible();
    await expect(source).toHaveValue("\\ce{}");

    await page.keyboard.type("Sb2Se3 + 3I2 <=> 2SbI3 + 3Se");
    await page.keyboard.press("Escape");

    // One wrapper, not two: the caret was inside the braces.
    await expect
      .poll(() => lastSaved(page), { timeout: 8000 })
      .toContain("$$\n\\ce{Sb2Se3 + 3I2 <=> 2SbI3 + 3Se}\n$$");
  });

  test("writing continues after a formula is finished", async ({ page }) => {
    await useVault(page, {
      notes: [{ id: NOTE, title: "Growth", body: "Transport step." }],
    });
    await page.goto("/");

    await page.locator(".sutra-prose p").first().click();
    await page.keyboard.press("End");
    await page.getByRole("button", { name: "Chemical equation" }).click();
    await expect(page.locator(".sutra-math-textarea")).toBeVisible();
    await page.keyboard.type("Sb2Se3 + 3I2 <=> 2SbI3 + 3Se");
    await page.keyboard.press("Escape");

    // Closing the source unmounted the only focused element and focus fell to
    // the body, so this whole sentence used to be typed into nothing.
    await page.keyboard.type("This is the transport step.");
    await expect
      .poll(() => lastSaved(page), { timeout: 8000 })
      .toContain("This is the transport step.");
  });

  test("a displayed equation sits on its own line, clear of the prose", async ({
    page,
  }) => {
    await useVault(page, {
      notes: [
        {
          id: NOTE,
          title: "Growth",
          body: "Before.\n\n$$\n\\ce{Sb2Se3 + 3I2 <=> 2SbI3 + 3Se}\n$$\n\nAfter.",
        },
      ],
    });
    await page.goto("/");

    const block = page.locator(".sutra-math-block");
    await expect(block).toHaveCount(1);
    const box = (await block.boundingBox())!;

    for (const p of await page.locator(".sutra-prose > p").all()) {
      const b = (await p.boundingBox())!;
      const overlaps =
        b.y < box.y + box.height - 2 && b.y + b.height > box.y + 2;
      expect(overlaps).toBe(false);
    }
  });
});
