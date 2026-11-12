// Browser smoke tests of the web build (add-web-build-and-releases, design
// decision 12). The page is built and served by `cargo xtask web --serve`
// under a sub-directory, so the tests also show that every reference is
// relative. The tests observe the page from outside: its root `data-`
// attributes, the canvas pixels, local storage, downloads and the console.

import { defineConfig, devices } from "@playwright/test";

const port = Number(process.env.DIGGER_WEB_PORT ?? 8123);
// With DIGGER_WEB_SITE set to a built site (the `site/` of an unpacked
// release archive), that site is served as it is instead of a fresh build.
const site = process.env.DIGGER_WEB_SITE;
const siteOption = site ? ` --site ${JSON.stringify(site)}` : "";
export const prefix = "/games/digger/";
export const origin = `http://127.0.0.1:${port}`;

export default defineConfig({
  testDir: "specs",
  // The tests measure frame rates; several pages at once on a small
  // machine would make the numbers unreliable.
  workers: 1,
  fullyParallel: false,
  forbidOnly: Boolean(process.env.CI),
  // One retry, so real failures stay visible (design, Risks).
  retries: 1,
  timeout: 60_000,
  reporter: process.env.CI ? [["list"], ["github"]] : "list",
  use: {
    baseURL: `${origin}${prefix}`,
    trace: "retain-on-failure",
  },
  webServer: {
    command: `cargo xtask web --serve --port ${port} --prefix ${prefix}${siteOption}`,
    cwd: "../..",
    url: `${origin}${prefix}`,
    reuseExistingServer: !process.env.CI,
    // A first build of the WebAssembly program takes a few minutes.
    timeout: 900_000,
    stdout: "ignore",
    stderr: "pipe",
  },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
    { name: "firefox", use: { ...devices["Desktop Firefox"] } },
    { name: "webkit", use: { ...devices["Desktop Safari"] } },
  ],
});
