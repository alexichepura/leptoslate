import { expect, test } from "@playwright/test";

import { getJson, setCaret, waitForEditor } from "./helpers";

test("large documents only update the typed block", async ({ page }) => {
  test.setTimeout(120_000);
  await page.goto("/");
  await waitForEditor(page);

  const blocks = Array.from({ length: 1000 }, (_, index) => ({
    type: "p",
    children: [{ text: `Block ${index} baseline text` }],
  }));

  await page.evaluate(
    (json) => (window as any).__leptoslate.load(json),
    JSON.stringify(blocks),
  );
  await expect.poll(async () => (await getJson(page)).length).toBe(1000);
  await expect(page.locator('[data-node-path="500"]')).toBeVisible();

  await setCaret(page, "500.0", 0);
  await page.evaluate(() => {
    const editor = document.querySelector(".ls-editor") as HTMLElement;
    (window as any).__mutations = 0;
    (window as any).__mutationObserver = new MutationObserver((records) => {
      (window as any).__mutations += records.length;
    });
    (window as any).__mutationObserver.observe(editor, {
      subtree: true,
      childList: true,
      characterData: true,
      attributes: true,
    });
    (window as any).__typingStart = performance.now();
  });

  await page.keyboard.type("benchmark typing");

  await expect
    .poll(async () => {
      const node = (await getJson(page))[500];
      return (node.children ?? []).map((child) => child.text ?? "").join("");
    })
    .toContain("benchmark typing");

  const metrics = await page.evaluate(() => {
    (window as any).__typingEnd = performance.now();
    (window as any).__mutationObserver.disconnect();
    return {
      mutations: (window as any).__mutations as number,
      elapsed: (window as any).__typingEnd - (window as any).__typingStart,
    };
  });

  console.log(
    `1000 blocks / 16 keystrokes: ${metrics.mutations} mutation records, ${metrics.elapsed.toFixed(0)} ms`,
  );
  expect(metrics.mutations).toBeLessThan(100);
  expect(metrics.elapsed).toBeLessThan(5000);
});
