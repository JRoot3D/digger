// Playing on the page (web-build spec: frame pacing from the animation
// callback, canvas presentation and scaling, audio starts only after a user
// gesture, hidden tab pauses the game).

import {
  test,
  expect,
  open,
  attribute,
  frames,
  press,
  setHidden,
  setDeviceViewport,
  hasAudioOutput,
} from "./fixtures.js";

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

/// The canvas's backing store, in device pixels, and its size on the page,
/// in CSS pixels.
const canvasSize = (page) =>
  page.evaluate(() => {
    const canvas = document.getElementById("game");
    const box = canvas.getBoundingClientRect();
    return { backing: [canvas.width, canvas.height], css: [box.width, box.height] };
  });

// The viewport sizes are device pixels, as in the spec's scenarios; on a
// project with 2 device pixels per CSS pixel (WebKit's `Desktop Safari`)
// the page is half as many CSS pixels wide and high.
for (const [width, height, canvasWidth, canvasHeight, scale] of [
  [1920, 1080, 1280, 800, "2"],
  [1000, 700, 640, 400, "1"],
]) {
  test(`a ${width}x${height} viewport gives a ${canvasWidth}x${canvasHeight} canvas`, async ({ page }) => {
    const ratio = await setDeviceViewport(page, width, height);
    await open(page, "?quiet");
    await expect
      .poll(async () => (await canvasSize(page)).backing)
      .toEqual([canvasWidth, canvasHeight]);
    await expect(page.locator("html")).toHaveAttribute("data-scale", scale);
    // Each backing pixel is one device pixel.
    expect((await canvasSize(page)).css).toEqual([canvasWidth / ratio, canvasHeight / ratio]);
  });
}

test.describe("on a display with 2 device pixels per CSS pixel", () => {
  test.use({ deviceScaleFactor: 2 });

  test("a 1280x720 viewport gives a 1920x1200 canvas at scale 3", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 720 });
    await open(page, "?quiet");
    await expect.poll(async () => (await canvasSize(page)).backing).toEqual([1920, 1200]);
    await expect(page.locator("html")).toHaveAttribute("data-scale", "3");
    expect((await canvasSize(page)).css).toEqual([960, 600]);
  });
});

test("the viewport shrinking during a game changes the scale", async ({ page }) => {
  await setDeviceViewport(page, 1920, 1080);
  await open(page, "?quiet");
  await expect(page.locator("html")).toHaveAttribute("data-scale", "2");
  await press(page, "Enter");
  await expect(page.locator("html")).toHaveAttribute("data-phase", "playing");
  await setDeviceViewport(page, 1000, 700);
  await expect(page.locator("html")).toHaveAttribute("data-scale", "1");
  await expect(page.locator("html")).toHaveAttribute("data-phase", "playing");
});

test("sound waits for a key press, which still starts the game", async ({ page, context, watch }) => {
  const output = await hasAudioOutput(context);
  await open(page);
  expect(await attribute(page, "audio")).toBe("locked");
  await expect(page.locator("#sound-prompt")).toBeVisible();
  // Left alone, the page stays silent and the prompt stays.
  await page.waitForTimeout(2_000);
  expect(await attribute(page, "audio")).toBe("locked");
  await expect(page.locator("#sound-prompt")).toBeVisible();
  await press(page, "Enter");
  if (output) {
    await expect(page.locator("html")).toHaveAttribute("data-audio", "running");
  } else {
    // This machine has no audio output for the browser to open (headless
    // Firefox and WebKit on a Linux runner without a sound server: see
    // docs/compat-quirks.md, web build). The key press still unlocks the
    // audio, which fails as the spec's "No audio device" scenario says:
    // the game goes on in silence and the page says sound is unavailable.
    await expect(page.locator("html")).toHaveAttribute("data-audio", "failed");
    await expect(page.locator("#notices p", { hasText: "Sound is unavailable" })).toHaveCount(1);
  }
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
