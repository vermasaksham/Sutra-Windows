import { expect, test } from "@playwright/test";
import { build } from "vite";
import { fileURLToPath } from "node:url";

declare global {
  interface Window {
    interpretationFixture: {
      open: (body: string) => void;
      save: () => string;
      undo: () => boolean;
    };
  }
}

let script: string;
test.beforeAll(async () => {
  const bundle = await build({
    configFile: false,
    logLevel: "error",
    define: { "process.env.NODE_ENV": JSON.stringify("production") },
    build: {
      write: false,
      lib: {
        entry: fileURLToPath(
          new URL("./fixtures/interpretation.ts", import.meta.url),
        ),
        name: "interpretationFixture",
        formats: ["iife"],
      },
    },
  });
  const output = Array.isArray(bundle) ? bundle[0] : bundle;
  if (!output || !("output" in output))
    throw new Error("Missing fixture bundle");
  const chunk = output.output.find((item) => item.type === "chunk");
  if (!chunk || chunk.type !== "chunk")
    throw new Error("Missing fixture script");
  script = chunk.code;
});

const header =
  '~~~sutra-interpretation-v1 {"iid":"01HQ3M8K2P00000000000000A1","evidence":["01HQ3M8K2P00000000000000E7"],"future":"keep"}';

test("interpretation edits save and reopen with identity; undo restores original bytes", async ({
  page,
}) => {
  await page.setContent("<body></body>");
  await page.addScriptTag({ content: script });
  const raw =
    header + "\r\n## My interpretation\r\n\r\nOnly two samples.\r\n~~~\r\n";
  await page.evaluate((body) => window.interpretationFixture.open(body), raw);
  expect(
    await page.evaluate(() => window.interpretationFixture.save()),
  ).toContain(raw);
  const prose = page.locator("section[data-interpretation] p");
  await prose.click();
  await page.keyboard.press("End");
  await page.keyboard.type(" More data needed.");
  const saved = await page.evaluate(() => window.interpretationFixture.save());
  expect(saved).toContain(header);
  expect(saved).toContain("Only two samples. More data needed.");
  await page.evaluate(() => window.interpretationFixture.undo());
  expect(
    await page.evaluate(() => window.interpretationFixture.save()),
  ).toContain(raw);
  await page.evaluate((body) => window.interpretationFixture.open(body), saved);
  await expect(page.locator("section[data-interpretation] p")).toContainText(
    "More data needed.",
  );
  expect(await page.evaluate(() => window.interpretationFixture.save())).toBe(
    saved,
  );
});

test("unsupported versions remain visible and unchanged after an unrelated edit", async ({
  page,
}) => {
  await page.setContent("<body></body>");
  await page.addScriptTag({ content: script });
  const raw =
    "~~~~sutra-interpretation-v2 {future}\r\n```js\r\nexample()\r\n```\r\n~~~~\r\n";
  await page.evaluate(
    (body) => window.interpretationFixture.open(body),
    raw + "\nOutside.",
  );
  await expect(page.locator("pre")).toContainText("example()");
  await page.locator(".tiptap > p").first().click();
  await page.keyboard.press("End");
  await page.keyboard.type(" Changed.");
  expect(
    await page.evaluate(() => window.interpretationFixture.save()),
  ).toContain(raw);
  expect(
    await page.evaluate(() => window.interpretationFixture.save()),
  ).toContain("Outside. Changed.");
});
