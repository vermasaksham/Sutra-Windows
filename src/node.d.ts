/**
 * The sliver of Node's standard library the tests use.
 *
 * Declared here rather than installed as @types/node on purpose. That package
 * would put Node's globals into the app's type space too — `setTimeout`
 * starts returning a `NodeJS.Timeout` instead of a number, and a browser
 * mistake stops being a type error. This app runs in a webview; only its
 * tests run in Node.
 *
 * The tests need this because Vitest gives a `.css` import no usable export,
 * so a test that reads a stylesheet as text has to open the file itself.
 */
declare module "node:fs" {
  export function readFileSync(path: string, encoding: "utf8"): string;
}

declare module "node:url" {
  export function fileURLToPath(url: string | URL): string;
}
