import { expect, test, type Page } from "@playwright/test";
import { lastExported, textOf, useVault, type ExportedDocument } from "./vault";

/**
 * Assembling a chapter out of notes.
 *
 * The half the Rust tests cannot see: that the ordered list on screen is the
 * order the export writes, that reordering it saves, and that a note deleted out
 * from under a chapter is visible rather than silently gone.
 */
test.describe("chapters", () => {
  const notes = [
    {
      id: "chapter-3",
      type: "chapter",
      title: "3. Growth of Sb2Se3",
      body: "The opening argument.\n",
      sequence: ["note-growth", "note-optics"],
    },
    { id: "note-growth", title: "Growth", body: "How the films were made.\n" },
    { id: "note-optics", title: "Optics", body: "What they absorbed.\n" },
  ];

  async function open(page: Page, options = { notes }) {
    await useVault(page, options);
    await page.goto("/");
  }

  test("lists the notes it assembles, in order", async ({ page }) => {
    await open(page);

    const items = page
      .getByRole("region", { name: "Chapter contents" })
      .getByRole("listitem");
    await expect(items).toHaveCount(2);
    await expect(items.nth(0)).toContainText("Growth");
    await expect(items.nth(1)).toContainText("Optics");
  });

  test("moving a note down saves the new order", async ({ page }) => {
    await open(page);

    const panel = page.getByRole("region", { name: "Chapter contents" });
    await panel.getByRole("button", { name: "Move Growth down" }).click();

    const items = panel.getByRole("listitem");
    await expect(items.nth(0)).toContainText("Optics");
    await expect(items.nth(1)).toContainText("Growth");
  });

  test("the first note cannot be moved up, nor the last down", async ({
    page,
  }) => {
    await open(page);

    const panel = page.getByRole("region", { name: "Chapter contents" });
    await expect(
      panel.getByRole("button", { name: "Move Growth up" }),
    ).toBeDisabled();
    await expect(
      panel.getByRole("button", { name: "Move Optics down" }),
    ).toBeDisabled();
  });

  test("removing a note takes it out of the chapter, not the vault", async ({
    page,
  }) => {
    await open(page);

    const panel = page.getByRole("region", { name: "Chapter contents" });
    await panel
      .getByRole("button", { name: "Remove Growth from the chapter" })
      .click();

    await expect(panel.getByRole("listitem")).toHaveCount(1);
    // Still in the vault. Asserted through the note list's own per-row control
    // rather than by title, because the chapter is itself called "3. Growth of
    // Sb2Se3" and a title match would find it instead of the note.
    await expect(
      page.getByRole("button", { name: "Move Growth to trash" }),
    ).toBeVisible();
  });

  test("a note the vault no longer has is named rather than dropped", async ({
    page,
  }) => {
    await open(page, {
      notes: [
        {
          id: "chapter-3",
          type: "chapter",
          title: "3. Growth of Sb2Se3",
          body: "",
          sequence: ["note-gone", "note-optics"],
        },
        { id: "note-optics", title: "Optics", body: "What they absorbed.\n" },
      ],
    });

    const items = page
      .getByRole("region", { name: "Chapter contents" })
      .getByRole("listitem");
    await expect(items).toHaveCount(2, "the position must still be there");
    await expect(items.nth(0)).toContainText("Note not in this vault");
    await expect(items.nth(1)).toContainText("Optics");
  });

  test("exports the chapter's own body then its notes, in order", async ({
    page,
  }) => {
    await open(page);

    await page.getByRole("button", { name: "Export" }).click();
    await page.getByRole("menuitem", { name: /Word/ }).click();
    await expect
      .poll(() => lastExported(page), { timeout: 15000 })
      .toBeTruthy();
    const document = (await lastExported(page)) as ExportedDocument;
    const text = document.blocks.map(textOf).join(" ");

    expect(document.title).toBe("3. Growth of Sb2Se3");
    // The chapter's prose first, then each note under a heading of its own.
    expect(text).toContain("The opening argument.");
    expect(text.indexOf("The opening argument.")).toBeLessThan(
      text.indexOf("How the films were made."),
    );
    expect(text.indexOf("How the films were made.")).toBeLessThan(
      text.indexOf("What they absorbed."),
    );
    // And the member titles are written as headings, unlike the chapter's own.
    const headings = document.blocks.filter((b) => b.kind === "heading");
    expect(headings.map(textOf)).toEqual(["Growth", "Optics"]);
  });

  test("a member note says which chapter it is in", async ({ page }) => {
    await open(page);

    await page
      .getByRole("button", { name: "Growth", exact: true })
      .first()
      .click();
    await expect(page.getByText("In a chapter")).toBeVisible();
    await expect(page.getByText("note 1 of 2")).toBeVisible();
  });

  test("an ordinary note says nothing about chapters", async ({ page }) => {
    await open(page, {
      notes: [{ id: "note-alone", title: "Alone", body: "Prose.\n" }],
    });

    await expect(page.getByText(/In a chapter|In \d+ chapters/)).toHaveCount(0);
  });
});
