// Recordings on the page (web-build spec: loading a recording through a
// file picker, saving a recording as a download, audio starts only after a
// user gesture: the prompt is not part of the picture).

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { test, expect, open, press, notices } from "./fixtures.js";

const corpus = new URL("../../../tests/recordings/", import.meta.url);
const recording = (name) => fileURLToPath(new URL(`data/${name}`, corpus));
const expected = (name) =>
  readFileSync(new URL(`results/${name.replace(/\.e?drf$/, "")}.out`, corpus), "utf8").trim();

/// Picks the file `path` (or the file described by `payload`) in the
/// page's recording file input.
const pick = (page, file) => page.locator("#recording-file").setInputFiles(file);

for (const name of ["2test01.drf", "2riptest01.edrf"]) {
  for (const prompt of [false, true]) {
    const query = prompt ? "?speed=0" : "?speed=0&quiet";
    test(`${name} plays back to its recorded result (${query})`, async ({ page }) => {
      await open(page, query);
      await pick(page, recording(name));
      await expect(page.locator("html")).toHaveAttribute("data-phase", "playback");
      if (prompt) {
        // The prompt is visible throughout and changes nothing.
        await expect(page.locator("#sound-prompt")).toBeVisible();
        await expect(page.locator("html")).toHaveAttribute("data-audio", "locked");
      }
      await expect(page.locator("html")).toHaveAttribute("data-phase", "title", { timeout: 45_000 });
      await expect(page.locator("html")).toHaveAttribute("data-result", expected(name));
      if (prompt) await expect(page.locator("#sound-prompt")).toBeVisible();
      expect(await notices(page)).toEqual([]);
    });
  }
}

test("a file that is not a recording is rejected with a message", async ({ page }) => {
  await open(page, "?quiet");
  await pick(page, {
    name: "notes.txt",
    mimeType: "text/plain",
    buffer: Buffer.from("These are not the recordings you are looking for.\n"),
  });
  await expect(page.locator("#notices p")).toHaveCount(1);
  expect((await notices(page))[0]).toContain("notes.txt");
  await expect(page.locator("html")).toHaveAttribute("data-phase", "title");
  // A file over 16 MiB is refused for its size.
  await pick(page, {
    name: "holiday.edrf",
    mimeType: "video/mp4",
    buffer: Buffer.alloc(20 * 1024 * 1024, 7),
  });
  await expect(page.locator("#notices p")).toHaveCount(2);
  expect((await notices(page))[1]).toMatch(/holiday\.edrf.*(16 MiB|too large|larger)/);
  await expect(page.locator("html")).toHaveAttribute("data-phase", "title");
  expect(await page.locator("html").getAttribute("data-result")).toBeNull();
});

test("a played game downloads as a recording that plays back", async ({ page }) => {
  await open(page, "?quiet");
  const save = page.locator("#save-recording");
  await expect(save).toBeDisabled();
  // Nothing to save yet: the key gives no download.
  let downloads = 0;
  page.on("download", () => {
    downloads += 1;
  });
  await press(page, "F8");
  await page.waitForTimeout(500);
  expect(downloads).toBe(0);
  // A short scripted game, ended with the return-to-title key.
  await press(page, "Enter");
  await expect(page.locator("html")).toHaveAttribute("data-phase", "playing");
  await page.waitForTimeout(3_000);
  await press(page, "ArrowLeft", 1_000);
  await press(page, "ArrowDown", 800);
  await press(page, "F1");
  await press(page, "F10");
  await expect(page.locator("html")).toHaveAttribute("data-phase", "title");
  await expect(save).toBeEnabled();
  const [download] = await Promise.all([page.waitForEvent("download"), save.click()]);
  // The name the desktop build would give it.
  expect(download.suggestedFilename()).toMatch(/^[A-Z_]{3}\d+\.drf$/);
  const file = await download.path();
  const bytes = readFileSync(file);
  expect(bytes.subarray(0, 4).toString()).toBe("DRF\n");
  // The download is a recording: the page plays it back to the end
  // without a divergence.
  await open(page, "?speed=0&quiet");
  await pick(page, file);
  // A game this short plays back within a few animation callbacks.
  await expect(page.locator("html")).toHaveAttribute("data-result", /^score=\d+ level=1 ticks=[1-9]\d*$/);
  expect(await notices(page)).toEqual([]);
});
