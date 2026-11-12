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

/// Starts collecting every value the page root's `data-phase` and
/// `data-audio`, and the sound prompt's `hidden`, take from now on, each
/// in the order set (the current one first, repeats dropped).
const watchStatus = (page) =>
  page.evaluate(() => {
    const root = document.documentElement;
    const prompt = document.getElementById("sound-prompt");
    const seen = { phase: [], audio: [], promptHidden: [] };
    const note = (list, value) => {
      if (list[list.length - 1] !== value) list.push(value);
    };
    const now = () => {
      note(seen.phase, root.dataset.phase);
      note(seen.audio, root.dataset.audio);
      note(seen.promptHidden, prompt.hidden);
    };
    now();
    // A record per change, with the value it replaced: within one
    // animation callback the playback may set a phase and leave it again.
    const handle = (records) => {
      for (const record of records) {
        const list =
          record.target === prompt
            ? seen.promptHidden
            : record.attributeName === "data-phase"
              ? seen.phase
              : seen.audio;
        note(list, record.target === prompt ? record.oldValue !== null : record.oldValue);
      }
      now();
    };
    const observer = new MutationObserver(handle);
    observer.observe(root, { attributes: true, attributeOldValue: true, attributeFilter: ["data-phase", "data-audio"] });
    observer.observe(prompt, { attributes: true, attributeOldValue: true, attributeFilter: ["hidden"] });
    window.__statusSeen = () => {
      handle(observer.takeRecords());
      return seen;
    };
  });

/// What [`watchStatus`] collected.
const statusSeen = (page) => page.evaluate(() => window.__statusSeen());

for (const name of ["2test01.drf", "2riptest01.edrf"]) {
  for (const prompt of [false, true]) {
    const query = prompt ? "?speed=0" : "?speed=0&quiet";
    test(`${name} plays back to its recorded result (${query})`, async ({ page }) => {
      await open(page, query);
      await watchStatus(page);
      await pick(page, recording(name));
      // The result is shown when the playback ends, and the title screen
      // follows.
      await expect(page.locator("html")).toHaveAttribute("data-result", expected(name), { timeout: 45_000 });
      await expect(page.locator("html")).toHaveAttribute("data-phase", "title");
      const seen = await statusSeen(page);
      // The playback ran: at speed 0 a short recording can be over before
      // a poll of the attribute would see it, so every value the page set
      // is checked instead.
      expect(seen.phase).toContain("playback");
      if (prompt) {
        // The prompt is visible throughout and changes nothing.
        expect(seen.audio).toEqual(["locked"]);
        expect(seen.promptHidden).toEqual([false]);
        await expect(page.locator("#sound-prompt")).toBeVisible();
      }
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
