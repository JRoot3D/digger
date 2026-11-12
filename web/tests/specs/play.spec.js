// Playing on the page (web-build spec: frame pacing from the animation
// callback, canvas presentation and scaling, audio starts only after a user
// gesture, hidden tab pauses the game).

import { test, expect, open, attribute, frames, press, setHidden } from "./fixtures.js";

/// Console lines in which a browser reports audio held back for want of a
/// user gesture.
const AUTOPLAY = /autoplay|AudioContext.*(not allowed|gesture)|user gesture/i;

test("a key press starts a game that runs at 12.5 frames per second", async ({ page }) => {
  await open(page, "?quiet");
  await press(page, "Enter");
  await expect(page.locator("html")).toHaveAttribute("data-phase", "playing");
  const before = await frames(page);
  await page.waitForTimeout(10_000);
  const after = await frames(page);
  // 10 s at 80 ms a frame is 125 frames.
  expect(after - before).toBeGreaterThanOrEqual(119);
  expect(after - before).toBeLessThanOrEqual(131);
});

for (const [width, height, canvasWidth, canvasHeight, scale] of [
  [1920, 1080, 1280, 800, "2"],
  [1000, 700, 640, 400, "1"],
]) {
  test(`a ${width}x${height} viewport gives a ${canvasWidth}x${canvasHeight} canvas`, async ({ page }) => {
    await page.setViewportSize({ width, height });
    await open(page, "?quiet");
    const backing = () =>
      page.evaluate(() => {
        const canvas = document.getElementById("game");
        return [canvas.width, canvas.height];
      });
    await expect.poll(backing).toEqual([canvasWidth, canvasHeight]);
    await expect(page.locator("html")).toHaveAttribute("data-scale", scale);
  });
}

test("the viewport shrinking during a game changes the scale", async ({ page }) => {
  await page.setViewportSize({ width: 1920, height: 1080 });
  await open(page, "?quiet");
  await press(page, "Enter");
  await expect(page.locator("html")).toHaveAttribute("data-phase", "playing");
  await page.setViewportSize({ width: 1000, height: 700 });
  await expect(page.locator("html")).toHaveAttribute("data-scale", "1");
  await expect(page.locator("html")).toHaveAttribute("data-phase", "playing");
});

test("sound waits for a key press, which still starts the game", async ({ page, watch }) => {
  await open(page);
  expect(await attribute(page, "audio")).toBe("locked");
  await expect(page.locator("#sound-prompt")).toBeVisible();
  // Left alone, the page stays silent and the prompt stays.
  await page.waitForTimeout(2_000);
  expect(await attribute(page, "audio")).toBe("locked");
  await expect(page.locator("#sound-prompt")).toBeVisible();
  await press(page, "Enter");
  await expect(page.locator("html")).toHaveAttribute("data-audio", "running");
  await expect(page.locator("#sound-prompt")).toBeHidden();
  await expect(page.locator("html")).toHaveAttribute("data-phase", "playing");
  expect(watch.console.filter((line) => AUTOPLAY.test(line))).toEqual([]);
});

test("a quiet start shows no sound prompt", async ({ page }) => {
  await open(page, "?quiet");
  expect(await attribute(page, "audio")).toBe("off");
  await expect(page.locator("#sound-prompt")).toBeHidden();
  await press(page, "Enter");
  await expect(page.locator("html")).toHaveAttribute("data-phase", "playing");
  expect(await attribute(page, "audio")).toBe("off");
});

test("a hidden page stops the game, which shows its pause when visible again", async ({ page }) => {
  await open(page, "?quiet");
  await press(page, "Enter");
  await expect(page.locator("html")).toHaveAttribute("data-phase", "playing");
  // Past the start of the level, into the main phase where monsters move.
  await page.waitForTimeout(3_000);
  await setHidden(page, true);
  await expect(page.locator("html")).toHaveAttribute("data-phase", "paused");
  const hidden = await frames(page);
  await page.waitForTimeout(2_000);
  expect(await frames(page)).toBe(hidden);
  await setHidden(page, false);
  await page.waitForTimeout(1_000);
  await expect(page.locator("html")).toHaveAttribute("data-phase", "paused");
  // Hidden time is not caught up: in the 1 s since the page came back
  // about 12 (pause) frames ran, not the 37 that 3 s would give.
  expect(await frames(page)).toBeLessThanOrEqual(hidden + 20);
  // The player resumes as on the desktop.
  await press(page, "Space");
  await expect(page.locator("html")).toHaveAttribute("data-phase", "playing");
});
