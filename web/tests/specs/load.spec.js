// The page loads and the title screen runs (web-build spec: static page
// delivery, loading and failure states, supported browsers).

import { test, expect, open, attribute } from "./fixtures.js";

test("the title screen appears with no errors and no request outside the prefix", async ({ page, watch }) => {
  await open(page);
  await expect(page.locator("#failure")).toBeHidden();
  // The title picture is drawn: the canvas holds more than one colour.
  await expect
    .poll(
      () =>
        page.evaluate(() => {
          const canvas = document.getElementById("game");
          const context = canvas.getContext("2d");
          const { data } = context.getImageData(0, 0, canvas.width, canvas.height);
          let lit = 0;
          for (let i = 0; i < data.length; i += 4) {
            if (data[i] | data[i + 1] | data[i + 2]) lit += 1;
          }
          return lit;
        }),
      { timeout: 10_000 },
    )
    .toBeGreaterThan(1000);
  expect(await attribute(page, "phase")).toBe("title");
  expect(watch.strays).toEqual([]);
});
