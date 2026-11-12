// Shared helpers of the browser smoke tests.

import { test as base, expect } from "@playwright/test";

export { expect };

/// A test whose page is watched: console errors and uncaught exceptions
/// are collected in `problems`, and every request that leaves the served
/// prefix in `strays`. Both must be empty when the test ends, unless the
/// test clears them on purpose.
export const test = base.extend({
  watch: [
    async ({ page, baseURL }, use) => {
      const watch = { problems: [], strays: [], console: [] };
      page.on("console", (message) => {
        watch.console.push(`${message.type()}: ${message.text()}`);
        if (message.type() === "error") watch.problems.push(`console error: ${message.text()}`);
      });
      page.on("pageerror", (error) => watch.problems.push(`page error: ${error.message}`));
      page.on("request", (request) => {
        const url = request.url();
        if (!url.startsWith(baseURL) && !url.startsWith("data:") && !url.startsWith("blob:")) {
          watch.strays.push(url);
        }
      });
      await use(watch);
      expect(watch.problems, "console errors and page errors").toEqual([]);
      expect(watch.strays, "requests outside the served prefix").toEqual([]);
    },
    { auto: true },
  ],
});

/// Opens the page with the query string `query` (such as `?quiet`) and
/// waits until the title screen runs.
export async function open(page, query = "") {
  await page.goto(`./${query}`);
  await titleShown(page);
}

/// Waits until the loading indicator is gone and the title screen runs.
export async function titleShown(page) {
  await expect(page.locator("#loading")).toHaveCount(0, { timeout: 30_000 });
  await expect(page.locator("html")).toHaveAttribute("data-phase", "title", { timeout: 30_000 });
}

/// The page root's `data-` attribute `name`.
export function attribute(page, name) {
  return page.locator("html").getAttribute(`data-${name}`);
}

/// Game frames run so far (`data-frames`).
export async function frames(page) {
  return Number(await attribute(page, "frames"));
}

/// Gives the canvas the keyboard without pressing a key on it.
export async function focusGame(page) {
  await page.locator("#game").focus();
}

/// Presses `key` with the canvas focused, held for `ms` milliseconds so
/// that at least one game frame (80 ms) sees it.
export async function press(page, key, ms = 200) {
  await focusGame(page);
  await page.keyboard.down(key);
  await page.waitForTimeout(ms);
  await page.keyboard.up(key);
}

/// Makes the page report itself hidden (`hidden = true`) or visible again,
/// as a tab switch does, and sends the `visibilitychange` event.
export async function setHidden(page, hidden) {
  await page.evaluate((hidden) => {
    Object.defineProperty(document, "hidden", { configurable: true, get: () => hidden });
    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      get: () => (hidden ? "hidden" : "visible"),
    });
    document.dispatchEvent(new Event("visibilitychange"));
  }, hidden);
}

/// Text of every notice on the page.
export async function notices(page) {
  return page.locator("#notices p").allTextContents();
}
