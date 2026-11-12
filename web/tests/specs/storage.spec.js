// Settings and high scores in local storage, and the start-up options of
// the address (web-build spec: settings and high scores persist in local
// storage, play without usable storage, start-up options from URL query
// parameters).

import { test, expect, open, press, notices } from "./fixtures.js";

const stored = (page, key) => page.evaluate((key) => localStorage.getItem(key), key);

test("a setting survives a reload", async ({ page }) => {
  await open(page, "?quiet");
  expect(await stored(page, "digger:settings")).toBeNull();
  // F7 switches music off; the browser keeps the settings as they change.
  await press(page, "F7");
  await expect.poll(() => stored(page, "digger:settings")).not.toBeNull();
  const off = await stored(page, "digger:settings");
  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-phase", "title");
  expect(await stored(page, "digger:settings")).toBe(off);
  // Music was read back as off: the same key now switches it on again,
  // which gives other stored settings than the music-off ones.
  await press(page, "F7");
  await expect.poll(() => stored(page, "digger:settings")).not.toBe(off);
  await press(page, "F7");
  await expect.poll(() => stored(page, "digger:settings")).toBe(off);
});

test("damaged stored settings give the defaults and are replaced by the next change", async ({ page }) => {
  await open(page, "?quiet");
  await page.evaluate(() => localStorage.setItem("digger:settings", "this is [not settings"));
  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-phase", "title");
  // The default bindings are in effect.
  const fire = page.locator("#controls-list tr", { hasText: "Player 1 fire" });
  await expect(fire).toContainText("F1");
  expect(await stored(page, "digger:settings")).toBe("this is [not settings");
  await press(page, "F7");
  await expect.poll(() => stored(page, "digger:settings")).not.toBe("this is [not settings");
  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-phase", "title");
  expect(await notices(page)).toEqual([]);
});

test("only keys starting with digger: are used", async ({ page }) => {
  await open(page, "?quiet");
  await press(page, "F7");
  await press(page, "Enter");
  await expect(page.locator("html")).toHaveAttribute("data-phase", "playing");
  await page.waitForTimeout(1_000);
  await press(page, "F10");
  await expect(page.locator("html")).toHaveAttribute("data-phase", "title");
  const keys = await page.evaluate(() => Object.keys(localStorage));
  expect(keys.length).toBeGreaterThan(0);
  for (const key of keys) expect(key).toMatch(/^digger:/);
});

test("without storage the page says so and the game stays playable", async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(window, "localStorage", {
      configurable: true,
      get() {
        throw new DOMException("site storage is denied", "SecurityError");
      },
    });
  });
  await open(page, "?quiet");
  await expect(page.locator("#notices p")).toHaveCount(1);
  const [notice] = await notices(page);
  expect(notice).toMatch(/settings/);
  expect(notice).toMatch(/high scores/);
  expect(notice).toMatch(/until the page is closed/);
  // Changing a setting and playing raise no further notice.
  await press(page, "F7");
  await press(page, "Enter");
  await expect(page.locator("html")).toHaveAttribute("data-phase", "playing");
  const before = Number(await page.locator("html").getAttribute("data-frames"));
  await page.waitForTimeout(1_000);
  expect(Number(await page.locator("html").getAttribute("data-frames"))).toBeGreaterThan(before + 5);
  await expect(page.locator("#notices p")).toHaveCount(1);
});

test("an invalid start-up option is named in a warning", async ({ page }) => {
  await open(page, "?speed=fast&mode=two-players&colour=red");
  const shown = await notices(page);
  expect(shown).toHaveLength(1);
  expect(shown[0]).toContain("speed");
  expect(shown[0]).toContain("fast");
  expect(shown[0]).not.toContain("colour");
  expect(await page.locator("#notices p").first().getAttribute("class")).toBe("warning");
});
