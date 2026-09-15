import { expect, test, type Page } from "@playwright/test";
import { notesNow, useVault, type VaultOptions } from "./vault";

/**
 * Reading a paper and taking evidence out of it.
 *
 * The workflow from docs/design/v0.4-reading-workflow.md:
 * Source → PDF → page-aware reading → select → Evidence → (Interpretation).
 *
 * Two properties are asserted everywhere rather than in one test, because both
 * are the kind that break quietly:
 *
 *  - **The note list gives way, the context panel does not.** That is the whole
 *    layout decision, and a regression would look like a cosmetic change.
 *  - **A quotation is the source's words and a comment is the reader's**, and
 *    they never arrive as one string.
 */

const WRITING = "01HQ3M8K2P00000000000000A1";
const SOURCE = "01HQ3M8K2P00000000000000B1";

function vault(extra: Partial<VaultOptions> = {}): VaultOptions {
  return {
    notes: [
      {
        id: WRITING,
        type: "literature",
        title: "Growth of Sb2Se3",
        body: "What I make of it.",
        sources: [],
      },
      {
        id: SOURCE,
        type: "source",
        title: "Zhou 2019",
        body: "",
        source: {
          authors: "Zhou et al.",
          year: "2019",
          zotero: "ZHOU2019",
          pdf: "Zhou 2019.pdf",
        },
      },
    ],
    ...extra,
  };
}

/** Open the note being written, then the paper, then press Read paper text. */
async function startReading(page: Page) {
  await page.getByRole("button", { name: "Growth of Sb2Se3" }).first().click();
  await expect(page.locator(".sutra-prose")).toContainText("What I make of it");
  await page.getByRole("button", { name: "Zhou 2019" }).first().click();
  await page.getByRole("button", { name: "Read paper text" }).click();
}

test.describe("reading a paper", () => {
  test("opens in place of the note list and closes back to it", async ({
    page,
  }) => {
    await useVault(page, vault());
    await page.goto("/");

    // The list is there to begin with.
    await expect(page.getByPlaceholder("Search")).toBeVisible();

    await startReading(page);

    const pane = page.getByLabel("Reading Zhou 2019");
    await expect(pane).toBeVisible();
    // Said before anything else, because the pane sits where the note list
    // was: what is below is the paper's own words, not a note.
    await expect(pane.getByText("Extracted PDF text")).toBeVisible();
    await expect(
      pane.getByText("for figures and layout, read it in Zotero"),
    ).toBeVisible();
    // The list has given way — same slot, so it cannot be on screen too.
    await expect(page.getByPlaceholder("Search")).toHaveCount(0);
    // …and the context panel has not. Reading returns the editor to the note
    // being written, so what the panel shows is that note's evidence — which
    // is the point of keeping the panel rather than the list.
    await expect(page.locator(".sutra-prose")).toContainText(
      "What I make of it",
    );
    await expect(
      page.getByText("Sources", { exact: false }).first(),
    ).toBeVisible();

    await pane.getByRole("button", { name: "Close reading" }).click();
    await expect(pane).toHaveCount(0);
    await expect(page.getByPlaceholder("Search")).toBeVisible();
  });

  test("shows the text under the page it came from", async ({ page }) => {
    await useVault(page, vault());
    await page.goto("/");
    await startReading(page);

    const pane = page.getByLabel("Reading Zhou 2019");
    await expect(pane).toContainText("p. 1");
    await expect(pane).toContainText("p. 2");
    await expect(pane).toContainText("ribbons grow along the [001] direction");
    // It says what it is, so nobody wonders where the figures went.
    await expect(pane).toContainText(
      "for figures and layout, read it in Zotero",
    );
  });

  test("a selection becomes evidence carrying its page and exact words", async ({
    page,
  }) => {
    await useVault(page, vault());
    await page.goto("/");
    await startReading(page);

    // Select inside page 2, so the page recorded has to be derived from where
    // the selection is rather than from whichever page happens to be first.
    await page.evaluate(() => {
      const second = document.querySelector('[data-page="2"]');
      if (!second) throw new Error("page 2 is not rendered");
      const range = document.createRange();
      range.selectNodeContents(second);
      const selection = window.getSelection();
      selection?.removeAllRanges();
      selection?.addRange(range);
      second.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
    });

    const capture = page.getByRole("button", { name: /Capture as evidence/ });
    await expect(capture).toBeVisible();
    // The page is on the control itself, so what is about to be recorded is
    // visible before it is recorded — and it says *PDF* page, because that is
    // the only page fact a selection has.
    await expect(capture).toContainText("PDF p. 2");
    await capture.click();

    // It landed on the note being written, with the source's words verbatim.
    await expect(
      page.getByText("Carrier lifetime was 1.2 ns.", { exact: false }).first(),
    ).toBeVisible();

    // And the provenance it recorded is the position in the file, left
    // unlabelled. ADR 0004: the number printed on the paper is what a citation
    // carries, and absent beats invented — until v0.5 this wrote the position
    // into the label, so a paper offprinted from 431 recorded "p. 1".
    const [recorded] = (await notesNow(page))
      .flatMap((note) => note.sources ?? [])
      .filter((entry) => entry.quote?.includes("Carrier lifetime"));
    expect(recorded).toBeDefined();
    expect(recorded!.page_index).toBe(2);
    expect(recorded!.page ?? null).toBeNull();
    expect(recorded!.origin).toBe("selection");
    expect(recorded!.zotero).toBe("ZHOU2019");
  });

  test("evidence goes to the note being written, never the source", async ({
    page,
  }) => {
    await useVault(page, vault());
    await page.goto("/");
    await startReading(page);

    await page.evaluate(() => {
      const first = document.querySelector('[data-page="1"]');
      const range = document.createRange();
      range.selectNodeContents(first!);
      const selection = window.getSelection();
      selection?.removeAllRanges();
      selection?.addRange(range);
      first!.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
    });
    await page.getByRole("button", { name: /Capture as evidence/ }).click();

    const written = await notesNow(page);
    const writing = written.find((n) => n.id === WRITING);
    const source = written.find((n) => n.id === SOURCE);
    expect(writing?.sources?.length ?? 0).toBe(1);
    // The paper records nothing about itself.
    expect(source?.sources ?? []).toEqual([]);
    // And what it recorded carries the source's words and no interpretation:
    // capture does not collect an opinion.
    expect(writing?.sources?.[0]?.quote).toContain("ribbons grow along");
    expect(writing?.sources?.[0]?.comment ?? null).toBeFalsy();
  });
});

test.describe("Zotero annotations", () => {
  const annotated = () =>
    vault({
      annotations: [
        {
          key: "AN1",
          text: "ribbons are held together by van der Waals forces",
          comment: "is this consistent with fig. 3?",
          colour: "#ffd400",
          page: "431",
        },
        { key: "AN2", text: "carrier lifetime of 1.2 ns", page: "433" },
      ],
    });

  test("shows the source's words and the reader's apart", async ({ page }) => {
    await useVault(page, annotated());
    await page.goto("/");
    await startReading(page);
    await page.getByRole("button", { name: "Annotations" }).click();

    const pane = page.getByLabel("Reading Zhou 2019");
    const quote = pane.locator(".sutra-voice-source").first();
    const mine = pane.locator(".sutra-voice-interpretation").first();

    await expect(quote).toContainText("van der Waals forces");
    await expect(mine).toContainText("is this consistent with fig. 3?");
    // Not one string: the reader's words are labelled and in their own element.
    await expect(mine).toContainText("your note:");
    await expect(quote).not.toContainText("is this consistent");
  });

  test("imports one at a time and then marks it as taken", async ({ page }) => {
    await useVault(page, annotated());
    await page.goto("/");
    await startReading(page);
    await page.getByRole("button", { name: "Annotations" }).click();

    const pane = page.getByLabel("Reading Zhou 2019");
    // Two on offer, and no control that takes both: bulk capture is not built.
    await expect(pane.getByRole("button", { name: "Capture" })).toHaveCount(2);
    await expect(pane.getByRole("button", { name: /Capture all/ })).toHaveCount(
      0,
    );

    await pane.getByRole("button", { name: "Capture" }).first().click();

    await expect(pane.getByText("✓ already captured")).toHaveCount(1);
    // The one taken no longer offers to be taken again; the other still does.
    await expect(pane.getByRole("button", { name: "Capture" })).toHaveCount(1);

    const imported = (await notesNow(page))
      .find((note) => note.id === WRITING)
      ?.sources?.find((citation) => citation.annotation === "AN1");
    expect(imported?.origin).toBe("annotation");
    expect(imported?.zotero).toBe("ZHOU2019");
  });

  test("an annotation already captured is marked before anything is pressed", async ({
    page,
  }) => {
    const options = annotated();
    options.notes[0]!.sources = [
      {
        eid: "01EVIDENCEEXISTING",
        id: SOURCE,
        page: "431",
        quote: "ribbons are held together by van der Waals forces",
        annotation: "AN1",
      },
    ];
    await useVault(page, options);
    await page.goto("/");
    await startReading(page);
    await page.getByRole("button", { name: "Annotations" }).click();

    const pane = page.getByLabel("Reading Zhou 2019");
    // Recognised by Zotero's key, before any import in this session.
    await expect(pane.getByText("✓ already captured")).toHaveCount(1);
    await expect(pane.getByRole("button", { name: "Capture" })).toHaveCount(1);
  });

  test("Zotero being unavailable does not break the pane", async ({ page }) => {
    await useVault(page, vault({ zoteroDown: true }));
    await page.goto("/");
    await startReading(page);

    const pane = page.getByLabel("Reading Zhou 2019");
    // The text half is unaffected — it never needed Zotero.
    await expect(pane).toContainText("ribbons grow along");

    await page.getByRole("button", { name: "Annotations" }).click();
    await expect(pane).toContainText("Zotero is not answering");
  });
});

test.describe("when the PDF cannot be read", () => {
  const cases: Array<[string, VaultOptions["pdf"], string]> = [
    ["no PDF attached", { state: "notAttached" }, "has no PDF"],
    [
      "the Zotero path is unresolved",
      {
        state: "unresolved",
        why: "the attachment shape has not been verified",
      },
      "cannot open Zotero’s copy of this PDF yet",
    ],
    [
      "the file has moved",
      { state: "missing", detail: "not where it should be" },
      "is not where it should be",
    ],
    ["it is password-protected", { state: "locked" }, "password-protected"],
    ["it is a scan", { state: "noTextLayer" }, "is a scan"],
    [
      "the parser gave up",
      { state: "failed", detail: "malformed cross-reference table" },
      "could not read this PDF",
    ],
  ];

  for (const [name, pdf, said] of cases) {
    test(`says so when ${name}`, async ({ page }) => {
      await useVault(page, vault({ pdf }));
      await page.goto("/");
      await startReading(page);

      const pane = page.getByLabel("Reading Zhou 2019");
      await expect(pane).toContainText(said);
      // Whatever happened, it is stated rather than shown as a broken screen,
      // and the pane is still usable.
      await expect(
        pane.getByRole("button", { name: "Close reading" }),
      ).toBeVisible();
    });
  }

  /**
   * The pending-verification case specifically. It must not read as a fault in
   * the library, the paper or the note — the rest of the workflow is untouched,
   * and saying "no PDF" here would send someone hunting for a problem that does
   * not exist.
   */
  test("an unresolved Zotero PDF is not presented as a missing one", async ({
    page,
  }) => {
    await useVault(
      page,
      vault({
        pdf: {
          state: "unresolved",
          why: "not verified against a real library",
        },
        annotations: [{ key: "AN1", text: "still importable", page: "431" }],
      }),
    );
    await page.goto("/");
    await startReading(page);

    const pane = page.getByLabel("Reading Zhou 2019");
    await expect(pane).toContainText("Nothing is wrong with the paper");
    await expect(pane).not.toContainText("has no PDF");
    await expect(pane).not.toContainText("could not read this PDF");

    // And the half that does not need a path still works.
    await page.getByRole("button", { name: "Annotations" }).click();
    await expect(pane).toContainText("still importable");
    await expect(pane.getByRole("button", { name: "Capture" })).toHaveCount(1);
  });
});

test.describe("a narrow window", () => {
  test("hides the context panel first, and still reads", async ({ page }) => {
    await useVault(page, vault());
    // Below the 1280px threshold the context panel is already hidden; reading
    // takes the list's place either way, so the rule is unchanged by this work.
    await page.setViewportSize({ width: 1100, height: 900 });
    await page.goto("/");
    await startReading(page);

    const pane = page.getByLabel("Reading Zhou 2019");
    await expect(pane).toBeVisible();
    await expect(pane).toContainText("ribbons grow along");
    // The context panel is the one that went, as it does today without reading.
    await expect(page.getByText("LINKED FROM")).toHaveCount(0);
  });
});
