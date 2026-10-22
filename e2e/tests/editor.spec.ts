import { expect, test } from "@playwright/test";

import {
  expectTopBlockText,
  getJson,
  getSelection,
  pasteText,
  plainText,
  selectText,
  setCaret,
  waitForEditor,
} from "./helpers";

const SVG_IMAGE =
  "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='320' height='160'%3E%3Crect width='100%25' height='100%25' fill='%23b4532a'/%3E%3C/svg%3E";

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await waitForEditor(page);
  await expect(page.locator(".ls-editor")).toBeVisible();
});

test("renders the seeded document and custom blocks", async ({ page }) => {
  await expect(page.locator(".ls-callout")).toHaveCount(1);
  await expect(page.locator(".ls-todo")).toHaveCount(2);

  const json = await getJson(page);
  expect(json).toHaveLength(12);
  expect(json[0].type).toBe("h1");
  expect(json[3].type).toBe("callout");
  expect(json[4].type).toBe("ul");
  expect(json[5].type).toBe("todo");
});

test("typing inserts text and keeps the model selection in sync", async ({
  page,
}) => {
  await setCaret(page, "2.0", 0);
  await page.keyboard.type("Hello ");

  await expectTopBlockText(page, 2, "Hello Select");
  const selection = await getSelection(page);
  expect(selection.selection).toEqual({
    anchor: { path: [2, 0], offset: 6 },
    focus: { path: [2, 0], offset: 6 },
  });
});

test("Enter splits a block and Backspace merges it back", async ({ page }) => {
  await setCaret(page, "2.0", 6);
  await page.keyboard.press("Enter");

  await expect.poll(async () => (await getJson(page)).length).toBe(13);
  await expectTopBlockText(page, 2, "Select");
  await expectTopBlockText(page, 3, "some text and try the toolbar");

  await page.keyboard.press("Backspace");

  await expect.poll(async () => (await getJson(page)).length).toBe(12);
  await expectTopBlockText(page, 2, "Select some text and try the toolbar");
});

test("the toolbar applies marks to a text selection", async ({ page }) => {
  await selectText(page, "2.0", 0, 6);
  await page.getByTitle("Bold (⌘B)").click();

  await expect
    .poll(async () => {
      const json = await getJson(page);
      const paragraph = json[2];
      const first = paragraph.children?.[0];
      return first?.text;
    })
    .toBe("Select");

  const json = await getJson(page);
  expect(json[2].children?.[0].bold).toBe(true);
  await expect(page.getByTitle("Bold (⌘B)")).toHaveClass(/active/);
});

test("contiguous typing is one undo step", async ({ page }) => {
  await setCaret(page, "2.0", 0);
  await page.keyboard.type("abc");
  await expectTopBlockText(page, 2, "abcSelect");

  const undo = page.getByTitle("Undo (⌘Z)");
  await expect(undo).toBeEnabled();
  await undo.click();

  await expect
    .poll(async () => plainText((await getJson(page))[2]))
    .not.toContain("abc");
  await expectTopBlockText(page, 2, "Select some text");
});

test("multi-line paste creates new blocks", async ({ page }) => {
  await setCaret(page, "2.0", 0);
  await pasteText(page, "pasted line one\npasted line two");

  await expect.poll(async () => (await getJson(page)).length).toBe(13);
  await expectTopBlockText(page, 2, "pasted line one");
  await expectTopBlockText(page, 3, "pasted line twoSelect");
});

test("custom todo state and toolbar-inserted callouts are serialized", async ({
  page,
}) => {
  await page.locator(".ls-todo-check").first().click();
  await expect
    .poll(async () => {
      const todos = (await getJson(page)).filter((node) => node.type === "todo");
      return todos[0]?.checked;
    })
    .toBe(true);

  await page.getByRole("button", { name: "Callout", exact: true }).click();
  await expect(page.locator(".ls-callout")).toHaveCount(2);
  await expect
    .poll(async () => {
      const json = await getJson(page);
      return json.filter((node) => node.type === "callout").length;
    })
    .toBe(2);
});

test("links can be applied to a selection and removed", async ({ page }) => {
  await selectText(page, "2.0", 0, 6);
  await page.getByRole("button", { name: "Link", exact: true }).click();
  await page.locator(".link-bar .media-input").fill("leptos.dev");
  await page.getByRole("button", { name: "Apply", exact: true }).click();

  await expect
    .poll(async () => {
      const json = await getJson(page);
      const paragraph = json[2];
      return paragraph.children?.some(
        (node) =>
          node.type === "a" &&
          node.href === "https://leptos.dev" &&
          plainText(node) === "Select",
      );
    })
    .toBe(true);

  await page.getByRole("button", { name: "Link", exact: true }).click();
  await page.getByRole("button", { name: "Remove", exact: true }).click();

  await expect
    .poll(async () => {
      const json = await getJson(page);
      return json[2].children?.some((node) => node.type === "a");
    })
    .toBe(false);
});

test("mentions insert inline voids and delete as atoms", async ({ page }) => {
  await setCaret(page, "2.0", 0);
  await page.getByRole("button", { name: "Mention", exact: true }).click();
  await page.locator(".mention-bar .media-input").fill("lep");
  await page.getByRole("button", { name: "@Leptos", exact: true }).click();

  await expect(page.locator(".ls-mention")).toHaveCount(1);
  await expect
    .poll(async () => {
      const json = await getJson(page);
      const mention = json[2].children?.find((node) => node.type === "mention");
      return mention?.name;
    })
    .toBe("Leptos");

  await page.locator(".ls-mention").click();
  await page.keyboard.press("Backspace");
  await expect(page.locator(".ls-mention")).toHaveCount(0);
  await expect
    .poll(async () => {
      const json = await getJson(page);
      return json[2].children?.some((node) => node.type === "mention");
    })
    .toBe(false);
});

test("images can be inserted, node-selected, deleted, and restored", async ({
  page,
}) => {
  await setCaret(page, "2.0", 0);
  await page.getByRole("button", { name: "Image", exact: true }).click();
  await page.locator(".image-bar .media-input").fill(SVG_IMAGE);
  await page.getByRole("button", { name: "Insert", exact: true }).click();

  await expect(page.locator(".ls-media")).toHaveCount(1);
  await page.locator(".ls-media").click();
  await expect(page.locator(".ls-media")).toHaveClass(/ls-selected/);
  const selection = await getSelection(page);
  expect(selection.nodeSelection).not.toBeNull();

  await page.keyboard.press("Backspace");
  await expect(page.locator(".ls-media")).toHaveCount(0);

  await page.getByTitle("Undo (⌘Z)").click();
  await expect(page.locator(".ls-media")).toHaveCount(1);
});

test("image toolbar controls align and delete the block", async ({ page }) => {
  await setCaret(page, "2.0", 0);
  await page.getByRole("button", { name: "Image", exact: true }).click();
  await page.locator(".image-bar .media-input").fill(SVG_IMAGE);
  await page.getByRole("button", { name: "Insert", exact: true }).click();
  await expect(page.locator(".ls-media")).toHaveCount(1);

  await page.locator(".ls-media-tools button").first().click();
  await expect(page.locator(".ls-media")).toHaveClass(/ls-media-right/);

  await page.locator(".ls-media-tools button").nth(1).click();
  await expect(page.locator(".ls-media")).toHaveCount(0);

  await page.getByTitle("Undo (⌘Z)").click();
  await expect(page.locator(".ls-media")).toHaveCount(1);
});

test("built-in void blocks can be selected and deleted", async ({ page }) => {
  const rule = page.locator("hr.ls-void");
  await expect(rule).toHaveCount(1);
  await rule.click();
  await expect(rule).toHaveClass(/ls-selected/);

  await page.keyboard.press("Backspace");
  await expect(page.locator("hr.ls-void")).toHaveCount(0);
});

test("IME composition reconciles the DOM text into the model", async ({
  page,
}) => {
  await setCaret(page, "2.0", 0);
  await page.evaluate(() => {
    const editor = document.querySelector(".ls-editor") as HTMLElement;
    const span = document.querySelector(
      '[data-text-path="2.0"]',
    ) as HTMLElement;
    const node = span.firstChild as Text;

    editor.dispatchEvent(
      new CompositionEvent("compositionstart", { bubbles: true, data: "" }),
    );

    node.data = "漢" + node.data;
    const range = document.createRange();
    range.setStart(node, 1);
    range.collapse(true);
    const selection = window.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);

    editor.dispatchEvent(
      new CompositionEvent("compositionend", { bubbles: true, data: "漢" }),
    );
  });

  await expectTopBlockText(page, 2, "漢Select");
});
