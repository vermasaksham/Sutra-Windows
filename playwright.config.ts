import { defineConfig, devices } from "@playwright/test";

/**
 * End-to-end tests, against the built app in a real browser.
 *
 * These exist because until now nothing proved the app *worked*. CI checked
 * types, formatting, unit tests and that Rust compiles — all of which pass
 * happily while the editor is broken. Every regression this session was found
 * by driving a browser by hand; this is that, kept.
 *
 * They run against `vite preview`, not the dev server: the production bundle
 * is what ships, and a minifier or a build-time transform breaking something
 * is exactly the class of bug a dev-server test would miss.
 */
export default defineConfig({
  testDir: "./e2e",
  // Not `*.spec.ts`: Vitest's default glob claims that name, and one runner
  // trying to execute the other's tests is a confusing way to find out.
  testMatch: "**/*.e2e.ts",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  // A flaky end-to-end test is worse than none: it teaches people to re-run
  // rather than to look. If one is flaky, fix it or delete it.
  workers: process.env.CI ? 2 : undefined,
  reporter: process.env.CI ? [["github"], ["list"]] : [["list"]],
  timeout: 30_000,
  expect: { timeout: 8_000 },

  use: {
    baseURL: "http://127.0.0.1:4173",
    trace: "retain-on-failure",
    // A window wide enough for the context panel, since several tests depend
    // on the three-column layout being there at all.
    viewport: { width: 1400, height: 900 },
  },

  projects: [
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
        launchOptions: {
          // Set PLAYWRIGHT_CHROMIUM_PATH when the browser is provisioned
          // outside Playwright's own download (some sandboxes and CI images
          // do this). Unset, Playwright uses the browser it installed.
          executablePath: process.env.PLAYWRIGHT_CHROMIUM_PATH || undefined,
        },
      },
    },
  ],

  webServer: {
    // `--host 127.0.0.1` is not decoration. Vite's preview server binds to
    // "localhost", which on Windows resolves to ::1 first — so the server came
    // up fine and Playwright, polling 127.0.0.1, waited three minutes and gave
    // up. Naming the interface makes both ends agree on which localhost.
    command:
      "npm run build && npm run preview -- --host 127.0.0.1 --port 4173 --strictPort",
    url: "http://127.0.0.1:4173/",
    reuseExistingServer: !process.env.CI,
    // A cold Rust-free build is seconds; this is slack for a cold CI runner.
    timeout: 240_000,
  },
});
