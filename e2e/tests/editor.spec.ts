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

test("structured tables render with semantic markup", async ({ page }) => {
  await page.evaluate(
    (json) => (window as any).__leptoslate.load(json),
    JSON.stringify([
      {
        type: "table",
        children: [
          {
            type: "thead",
            children: [
              {
                type: "tr",
                children: [
                  { type: "th", children: [{ text: "Name" }] },
                  { type: "th", children: [{ text: "Value" }] },
                ],
              },
            ],
          },
          {
            type: "tbody",
            children: [
              {
                type: "tr",
                children: [
                  { type: "td", children: [{ text: "Range" }] },
                  { type: "td", children: [{ text: "500 km" }] },
                ],
              },
            ],
          },
        ],
      },
    ]),
  );

  await expect(page.locator(".ls-table-wrap table")).toHaveCount(1);
  await expect(page.locator(".ls-table-wrap thead th")).toHaveText([
    "Name",
    "Value",
  ]);
  await expect(page.locator(".ls-table-wrap tbody td")).toHaveText([
    "Range",
    "500 km",
  ]);
});

test("rich preset owns list and table markup and styles", async ({ page }) => {
  await page.evaluate(
    (json) => (window as any).__leptoslate.load(json),
    JSON.stringify([
      {
        type: "ul",
        children: [{ type: "li", children: [{ text: "one" }] }],
      },
      {
        type: "table",
        children: [
          {
            type: "tbody",
            children: [
              {
                type: "tr",
                children: [{ type: "td", children: [{ text: "cell" }] }],
              },
            ],
          },
        ],
      },
    ]),
  );

  const list = page.locator("ul.ls-list").first();
  await expect(list).toHaveCount(1);
  await expect(list.locator("li.ls-list-item")).toHaveText(["one"]);
  expect(
    await list.evaluate((element) => getComputedStyle(element).paddingLeft),
  ).not.toBe("0px");
  expect(
    await list
      .locator("li.ls-list-item")
      .first()
      .evaluate((element) => getComputedStyle(element).display),
  ).toBe("list-item");

  const table = page.locator(".ls-table-wrap table.ls-table").first();
  await expect(table).toHaveCount(1);
  await expect(table.locator("td")).toHaveText("cell");
  expect(
    await table.evaluate((element) => getComputedStyle(element).borderCollapse),
  ).toBe("collapse");

  const richStyle = await page.evaluate(() =>
    Array.from(document.querySelectorAll("style"))
      .map((element) => element.textContent ?? "")
      .some((text) => text.includes(".ls-table")),
  );
  expect(richStyle).toBe(true);
});

test("toolbar inserts a table and table controls edit it", async ({ page }) => {
  await page.getByRole("button", { name: "Table", exact: true }).click();
  await expect(page.locator(".table-bar")).not.toHaveClass(/hidden/);
  const table = page.locator(".ls-table-wrap table").last();
  await expect(table.locator("thead tr")).toHaveCount(1);
  await expect(table.locator("thead th")).toHaveCount(3);
  await expect(table.locator("tbody tr")).toHaveCount(2);

  await table.locator("tbody td").first().click();
  await page.getByRole("button", { name: "Add row below" }).click();
  await expect(table.locator("tbody tr")).toHaveCount(3);

  await page.getByRole("button", { name: "Add column after" }).click();
  await expect(table.locator("thead th")).toHaveCount(4);

  await page.getByRole("button", { name: "Delete row" }).click();
  await expect(table.locator("tbody tr")).toHaveCount(2);

  await page.getByRole("button", { name: "Delete column" }).click();
  await expect(table.locator("thead th")).toHaveCount(3);
});

test("table cells navigate and resist structural deletion", async ({ page }) => {
  await page.evaluate(
    (json) => (window as any).__leptoslate.load(json),
    JSON.stringify([
      {
        type: "table",
        children: [
          {
            type: "tbody",
            children: [
              {
                type: "tr",
                children: [
                  { type: "td", children: [{ text: "A" }] },
                  { type: "td", children: [{ text: "B" }] },
                ],
              },
            ],
          },
        ],
      },
    ]),
  );

  await setCaret(page, "0.0.0.0.0", 0);
  await page.keyboard.press("Tab");
  await expect
    .poll(async () => (await getSelection(page)).selection?.anchor.path)
    .toEqual([0, 0, 0, 1, 0]);

  await page.keyboard.press("Enter");
  await page.keyboard.press("Backspace");
  await expect.poll(async () => (await getJson(page)).length).toBe(1);
  const json = await getJson(page);
  expect(json[0].type).toBe("table");
  expect(json[0].children?.[0]?.children).toHaveLength(1);
  expect(json[0].children?.[0]?.children?.[0]?.children).toHaveLength(2);
  expect(plainText(json[0])).toBe("AB");

  await page.keyboard.type("X");
  await expect.poll(async () => plainText((await getJson(page))[0])).toBe(
    "AXB",
  );
});

test("copy includes HTML marks and pastes them back", async ({ page }) => {
  await page
    .context()
    .grantPermissions(["clipboard-read", "clipboard-write"]);
  await page.evaluate(
    (json) => (window as any).__leptoslate.load(json),
    JSON.stringify([
      { type: "p", children: [{ text: "Bold", bold: true }] },
    ]),
  );
  await selectText(page, "0.0", 0, 4);
  const modifier = process.platform === "darwin" ? "Meta" : "Control";
  await page.keyboard.press(`${modifier}+C`);
  const copied = await page.evaluate(async () => {
    const items = await navigator.clipboard.read();
    const out: Record<string, string> = {};
    for (const item of items) {
      for (const type of item.types) {
        out[type] = await (await item.getType(type)).text();
      }
    }
    return out;
  });
  expect(copied["text/plain"]).toBe("Bold");
  expect(copied["text/html"]).toContain("font-weight");

  await page.evaluate(
    (json) => (window as any).__leptoslate.load(json),
    JSON.stringify([{ type: "p", children: [{ text: "" }] }]),
  );
  await setCaret(page, "0.0", 0);
  await page.evaluate((html) => {
    const data = new DataTransfer();
    data.setData("text/html", html);
    data.setData("text/plain", "Bold");
    document.querySelector(".ls-editor")!.dispatchEvent(
      new ClipboardEvent("paste", {
        clipboardData: data,
        bubbles: true,
        cancelable: true,
      }),
    );
  }, copied["text/html"]);
  await expect
    .poll(async () => (await getJson(page))[0]?.children?.[0]?.bold)
    .toBe(true);
});

test("cut writes rich HTML and removes the model text", async ({ page }) => {
  await page
    .context()
    .grantPermissions(["clipboard-read", "clipboard-write"]);
  await page.evaluate(
    (json) => (window as any).__leptoslate.load(json),
    JSON.stringify([
      { type: "p", children: [{ text: "Cut", bold: true }] },
    ]),
  );
  await selectText(page, "0.0", 0, 3);
  const modifier = process.platform === "darwin" ? "Meta" : "Control";
  await page.keyboard.press(`${modifier}+X`);
  const copied = await page.evaluate(async () => {
    const items = await navigator.clipboard.read();
    const out: Record<string, string> = {};
    for (const item of items) {
      for (const type of item.types) {
        out[type] = await (await item.getType(type)).text();
      }
    }
    return out;
  });
  expect(copied["text/plain"]).toBe("Cut");
  expect(copied["text/html"]).toContain("font-weight");
  await expect.poll(async () => plainText((await getJson(page))[0])).toBe("");
});

test("custom block copy preserves model properties", async ({ page }) => {
  await page.evaluate(
    (json) => (window as any).__leptoslate.load(json),
    JSON.stringify([
      {
        type: "callout",
        tone: "warn",
        children: [{ text: "Keep my props" }],
      },
    ]),
  );
  await page
    .context()
    .grantPermissions(["clipboard-read", "clipboard-write"]);
  await page.locator('[data-node-path="0"]').hover();
  await page.getByRole("button", { name: "Select block" }).click();
  const modifier = process.platform === "darwin" ? "Meta" : "Control";
  await page.keyboard.press(`${modifier}+C`);
  const copied = await page.evaluate(async () => {
    const items = await navigator.clipboard.read();
    for (const item of items) {
      if (item.types.includes("text/html")) {
        return await (await item.getType("text/html")).text();
      }
    }
    return "";
  });
  expect(copied).toContain("data-leptoslate-model");

  await page.evaluate(
    (json) => (window as any).__leptoslate.load(json),
    JSON.stringify([{ type: "p", children: [{ text: "" }] }]),
  );
  await setCaret(page, "0.0", 0);
  await page.evaluate((copied) => {
    const data = new DataTransfer();
    data.setData("text/html", copied);
    document.querySelector(".ls-editor")!.dispatchEvent(
      new ClipboardEvent("paste", {
        clipboardData: data,
        bubbles: true,
        cancelable: true,
      }),
    );
  }, copied);
  await expect.poll(async () => (await getJson(page))[0]?.type).toBe("callout");
  const json = await getJson(page);
  expect(json[0]?.tone).toBe("warn");
  expect(plainText(json[0]!)).toBe("Keep my props");
});

test("copying a selected table preserves the table structure", async ({
  page,
}) => {
  await page.evaluate(
    (json) => (window as any).__leptoslate.load(json),
    JSON.stringify([
      {
        type: "table",
        children: [
          {
            type: "tbody",
            children: [
              {
                type: "tr",
                children: [
                  { type: "td", children: [{ text: "A" }] },
                  { type: "td", children: [{ text: "B" }] },
                ],
              },
            ],
          },
        ],
      },
    ]),
  );
  await page.locator('[data-node-path="0"]').hover();
  await page.getByRole("button", { name: "Select block" }).click();
  const html = await page.evaluate(() => {
    const data = new DataTransfer();
    document.querySelector(".ls-editor")!.dispatchEvent(
      new ClipboardEvent("copy", {
        clipboardData: data,
        bubbles: true,
        cancelable: true,
      }),
    );
    return data.getData("text/html");
  });
  expect(html).toContain("<table");
  expect(html).toContain("<td");

  await page.evaluate(
    (json) => (window as any).__leptoslate.load(json),
    JSON.stringify([{ type: "p", children: [{ text: "" }] }]),
  );
  await setCaret(page, "0.0", 0);
  await page.evaluate((copied) => {
    const data = new DataTransfer();
    data.setData("text/html", copied);
    document.querySelector(".ls-editor")!.dispatchEvent(
      new ClipboardEvent("paste", {
        clipboardData: data,
        bubbles: true,
        cancelable: true,
      }),
    );
  }, html);
  await expect(page.locator(".ls-table-wrap table")).toHaveCount(1);
  await expect.poll(async () => (await getJson(page))[0]?.type).toBe("table");
});

test("editable plugins can intercept keyboard input", async ({ page }) => {
  await setCaret(page, "2.0", 0);
  await page.keyboard.press("F2");
  await expect
    .poll(async () => {
      const json = await getJson(page);
      return json[2]?.children?.[0]?.text;
    })
    .toBe("★Select some text and try the toolbar. The value is plain ");
  const selection = await getSelection(page);
  expect(selection.selection?.anchor.offset).toBe(1);
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

test("on_change reports content edits and ignores selection changes", async ({
  page,
}) => {
  const changes = () =>
    page.evaluate(() =>
      JSON.parse((window as any).__leptoslate.changes()),
    );

  expect((await changes()).count).toBe(0);

  await setCaret(page, "2.0", 0);
  await page.keyboard.type("New ");
  await expect
    .poll(async () => (await changes()).count)
    .toBeGreaterThan(0);

  const afterTyping = await changes();
  expect(afterTyping.last).toContain("New Select");
  const typingCount = afterTyping.count;

  // Changing only the selection must not notify.
  await selectText(page, "2.0", 0, 3);
  await page.waitForTimeout(150);
  expect((await changes()).count).toBe(typingCount);

  // Formatting changes the document and must notify.
  await page.getByTitle("Bold (⌘B)").click();
  await expect
    .poll(async () => (await changes()).count)
    .toBeGreaterThan(typingCount);

  const afterFormatting = await changes();
  expect(afterFormatting.count).toBe(typingCount + 1);
  expect(afterFormatting.last).toContain('"bold":true');
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

test("selection bubble applies marks near the selection", async ({ page }) => {
  await selectText(page, "2.0", 0, 6);
  const bubble = page.locator(".ls-selection-bubble");
  await expect(bubble).toHaveClass(/visible/);

  const box = await bubble.boundingBox();
  expect(box?.width).toBeGreaterThan(0);
  expect(box?.height).toBeGreaterThan(0);

  await bubble.locator('button[title="Bold"]').click();
  await expect
    .poll(async () => {
      const json = await getJson(page);
      return json[2].children?.[0]?.bold;
    })
    .toBe(true);
  await expect(bubble).toHaveClass(/visible/);

  await setCaret(page, "2.0", 0);
  await expect(bubble).not.toHaveClass(/visible/);
});

test("document selectionchange syncs the model selection", async ({ page }) => {
  await page.evaluate(() => {
    const editor = document.querySelector(".ls-editor") as HTMLElement;
    editor.focus();
    const span = document.querySelector(
      '[data-text-path="2.0"]',
    ) as HTMLElement;
    const range = document.createRange();
    range.setStart(span.firstChild as Text, 0);
    range.setEnd(span.firstChild as Text, 6);
    const selection = window.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
  });

  await expect
    .poll(async () => {
      const selection = await getSelection(page);
      return [
        selection.selection?.anchor.offset,
        selection.selection?.focus.offset,
      ];
    })
    .toEqual([0, 6]);
  await expect(page.locator(".ls-selection-bubble")).toHaveClass(/visible/);
});

test("scroll hides the hover bar and selection bubble", async ({ page }) => {
  await page.locator('[data-node-path="2"]').hover();
  await expect(page.locator(".ls-hover-bar")).toHaveClass(/visible/);
  await page.evaluate(() => window.scrollTo(0, 200));
  await expect(page.locator(".ls-hover-bar")).not.toHaveClass(/visible/);

  await selectText(page, "2.0", 0, 6);
  await expect(page.locator(".ls-selection-bubble")).toHaveClass(/visible/);
  await page.evaluate(() => window.scrollTo(0, 400));
  await expect(page.locator(".ls-selection-bubble")).not.toHaveClass(
    /visible/,
  );
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

test("nested Markdown lists are inserted", async ({ page }) => {
  await page.evaluate(
    (json) => (window as any).__leptoslate.load(json),
    JSON.stringify([{ type: "p", children: [{ text: "" }] }]),
  );
  await setCaret(page, "0.0", 0);
  await page.evaluate(() => {
    (window as any).__leptoslate.markdown(
      "- parent\n  - child one\n  - child two\n- sibling",
    );
  });

  await expect.poll(async () => (await getJson(page))[0]?.type).toBe("ul");
  const list = (await getJson(page))[0];
  expect(list.children).toHaveLength(2);
  expect(list.children?.[0]?.children?.[1]?.type).toBe("ul");
  expect(plainText(list.children![0]!)).toBe("parentchild onechild two");
  expect(plainText(list.children![1]!)).toBe("sibling");
});

test("unsafe pasted links are flattened", async ({ page }) => {
  await page.evaluate(
    (json) => (window as any).__leptoslate.load(json),
    JSON.stringify([{ type: "p", children: [{ text: "" }] }]),
  );
  await setCaret(page, "0.0", 0);
  await page.evaluate(() => {
    const data = new DataTransfer();
    data.setData(
      "text/html",
      '<p><a href="javascript:alert(1)">bad</a> <a href="/safe">good</a></p>',
    );
    document.querySelector(".ls-editor")!.dispatchEvent(
      new ClipboardEvent("paste", {
        clipboardData: data,
        bubbles: true,
        cancelable: true,
      }),
    );
  });
  await expect.poll(async () => plainText((await getJson(page))[0])).toBe(
    "bad good",
  );
  const children = (await getJson(page))[0].children ?? [];
  const links = children.filter((child) => child.type === "a");
  expect(links).toHaveLength(1);
  expect(links[0].href).toBe("/safe");
});

test("HTML paste preserves nested lists, tables and inline images", async ({
  page,
}) => {
  await setCaret(page, "2.0", 0);
  await page.evaluate(() => {
    const data = new DataTransfer();
    data.setData(
      "text/html",
      '<ul><li>Parent<ul><li>Child</li></ul></li></ul><table><tr><th>Name</th><th>Value</th></tr><tr><td>A</td><td>1</td></tr></table><p>Before <img src="data:image/png;base64,abc" alt="inline"> after</p>',
    );
    document
      .querySelector(".ls-editor")!
      .dispatchEvent(
        new ClipboardEvent("paste", {
          clipboardData: data,
          bubbles: true,
          cancelable: true,
        }),
      );
  });

  const json = await getJson(page);
  const nested = json.find(
    (node) =>
      node.type === "ul" &&
      node.children?.[0]?.children?.some(
        (child) => child.type === "ul" && plainText(child) === "Child",
      ),
  );
  expect(nested).toBeTruthy();
  expect(plainText(nested!)).toBe("ParentChild");

  const table = json.find((node) => node.type === "table");
  expect(table?.children?.map((node) => node.type)).toEqual([
    "thead",
    "tbody",
  ]);
  const headerRow = table?.children?.[0]?.children?.[0];
  expect(headerRow?.children?.map((cell) => cell.type)).toEqual(["th", "th"]);
  expect(headerRow?.children?.[0]?.children?.[0]?.text).toBe("Name");
  const bodyRow = table?.children?.[1]?.children?.[0];
  expect(bodyRow?.children?.map((cell) => cell.type)).toEqual(["td", "td"]);
  expect(bodyRow?.children?.[0]?.children?.[0]?.text).toBe("A");

  const inlineImage = json.find((node) =>
    node.children?.some((child) => child.type === "inline-image"),
  );
  expect(inlineImage).toBeTruthy();
  expect(
    inlineImage?.children?.find((child) => child.type === "inline-image")
      ?.src,
  ).toBe("data:image/png;base64,abc");
});

test("HTML can be dropped at a point in the document", async ({ page }) => {
  const box = await page.locator('[data-text-path="2.0"]').boundingBox();
  expect(box).not.toBeNull();
  await page.evaluate(
    ({ x, y }) => {
      const data = new DataTransfer();
      data.setData("text/html", "<p>Dropped <strong>HTML</strong></p>");
      document.querySelector(".ls-editor")!.dispatchEvent(
        new DragEvent("drop", {
          dataTransfer: data,
          clientX: x,
          clientY: y,
          bubbles: true,
          cancelable: true,
        }),
      );
    },
    { x: box!.x + 5, y: box!.y + 5 },
  );

  await expect
    .poll(async () => {
      const json = await getJson(page);
      return json.some((node) => plainText(node).includes("Dropped HTML"));
    })
    .toBe(true);
  const json = await getJson(page);
  const dropped = json.find((node) => plainText(node).includes("Dropped"));
  expect(
    dropped?.children?.some(
      (child) => child.text === "HTML" && child.bold === true,
    ),
  ).toBe(true);
});

test("toolbar and floating controls expose accessible state", async ({
  page,
}) => {
  await expect(
    page.getByRole("toolbar", { name: "Formatting", exact: true }),
  ).toBeVisible();
  const bold = page.getByRole("button", { name: "Bold (⌘B)" });
  const italic = page.getByRole("button", { name: "Italic (⌘I)" });
  await expect(bold).toHaveAttribute("aria-pressed", "false");

  await bold.focus();
  await page.keyboard.press("ArrowRight");
  await expect(italic).toBeFocused();

  await page.locator('[data-node-path="2"]').hover();
  const blockActions = page.getByRole("toolbar", { name: "Block actions" });
  await expect(blockActions).toHaveClass(/visible/);
  await page.getByRole("button", { name: "Select block" }).click();
  await expect(page.locator('[aria-live="polite"]')).toHaveText(
    "Block selected",
  );

  await page.locator('[data-text-path="2.0"]').click();
  await selectText(page, "2.0", 0, 6);
  const formatting = page.getByRole("toolbar", { name: "Text formatting" });
  await expect(formatting).toHaveClass(/visible/);
  await formatting.getByRole("button", { name: "Bold" }).click();
  await expect(bold).toHaveAttribute("aria-pressed", "true");
  await expect(
    formatting.getByRole("button", { name: "Bold" }),
  ).toHaveAttribute("aria-pressed", "true");
});

test("custom mark styles are applied to text", async ({ page }) => {
  const code = page.locator('[data-text-path="2.1"]');
  await expect(code).toHaveCSS("background-color", "rgb(253, 238, 229)");
  await expect(code).toHaveCSS("color", "rgb(138, 59, 22)");
});

test("multi-line paste creates new blocks", async ({ page }) => {
  await setCaret(page, "2.0", 0);
  await pasteText(page, "pasted line one\npasted line two");

  await expect.poll(async () => (await getJson(page)).length).toBe(13);
  await expectTopBlockText(page, 2, "pasted line one");
  await expectTopBlockText(page, 3, "pasted line twoSelect");
});

test("HTML paste preserves blocks, marks, and links", async ({ page }) => {
  await setCaret(page, "2.0", 0);
  await page.evaluate(() => {
    const data = new DataTransfer();
    data.setData(
      "text/html",
      '<h2>Pasted</h2><p>Hello <strong>bold</strong> and <a href="https://leptos.dev">link</a></p><ul><li>One</li><li>Two</li></ul>',
    );
    data.setData("text/plain", "Pasted\n\nHello bold and link\n\nOne\nTwo");
    document
      .querySelector(".ls-editor")!
      .dispatchEvent(
        new ClipboardEvent("paste", {
          clipboardData: data,
          bubbles: true,
          cancelable: true,
        }),
      );
  });

  await expect.poll(async () => (await getJson(page)).length).toBe(15);
  const json = await getJson(page);
  expect(json[2].type).toBe("h2");
  expect(plainText(json[2])).toBe("Pasted");
  expect(
    json[3].children?.some(
      (node) => node.text === "bold" && node.bold === true,
    ),
  ).toBe(true);
  expect(
    json[3].children?.some(
      (node) =>
        node.type === "a" && node.href === "https://leptos.dev",
    ),
  ).toBe(true);
  expect(json[4].type).toBe("ul");
  expect(json[4].children).toHaveLength(2);
});

test("hover bar adds, duplicates, and deletes blocks", async ({ page }) => {
  const block = page.locator('[data-node-path="2"]');
  await block.hover();
  await expect(page.locator(".ls-hover-bar")).toHaveClass(/visible/);
  await page.getByTitle("Add block below").click();
  await expect.poll(async () => (await getJson(page)).length).toBe(13);
  await expect(page.locator('[data-node-path="3"]')).toHaveText("");

  await block.hover();
  await page.getByTitle("Duplicate block").click();
  await expect.poll(async () => (await getJson(page)).length).toBe(14);
  await expect
    .poll(async () => {
      const json = await getJson(page);
      return plainText(json[3]);
    })
    .toContain("Select some text");

  await page.locator('[data-node-path="3"]').hover();
  await page.getByTitle("Delete block").click();
  await expect.poll(async () => (await getJson(page)).length).toBe(13);
});

test("multi-block selection deletes a range and undo restores it", async ({
  page,
}) => {
  const first = page.locator('[data-node-path="1"]');
  const second = page.locator('[data-node-path="2"]');
  await first.hover();
  await page.getByTitle("Select block").click();
  await expect(first).toHaveClass(/ls-block-selected/);

  await page.keyboard.press("Shift+ArrowDown");
  await expect(first).toHaveClass(/ls-block-selected/);
  await expect(second).toHaveClass(/ls-block-selected/);
  await expect(page.locator('[data-node-path="3"]')).not.toHaveClass(
    /ls-block-selected/,
  );
  const selection = await getSelection(page);
  expect(selection.nodeSelection).toEqual([1]);
  expect(selection.nodeSelectionEnd).toEqual([2]);

  await page.keyboard.press("Delete");
  await expect.poll(async () => (await getJson(page)).length).toBe(10);

  await page.getByTitle("Undo (⌘Z)").click();
  await expect.poll(async () => (await getJson(page)).length).toBe(12);
  await expect(first).toHaveClass(/ls-block-selected/);
  await expect(second).toHaveClass(/ls-block-selected/);

  await page.keyboard.press("ArrowUp");
  await expect(first).toHaveClass(/ls-block-selected/);
  await expect(second).not.toHaveClass(/ls-block-selected/);
});

test("drag handle reorders blocks and undo restores order", async ({ page }) => {
  const block = page.locator('[data-node-path="2"]');
  await block.hover();
  await expect(page.locator(".ls-hover-bar")).toHaveClass(/visible/);

  const handle = await page.getByTitle("Drag to move (↑/↓)").boundingBox();
  const target = await page.locator('[data-node-path="1"]').boundingBox();
  expect(handle).not.toBeNull();
  expect(target).not.toBeNull();
  await page.mouse.move(handle!.x + handle!.width / 2, handle!.y + handle!.height / 2);
  await page.mouse.down();
  await page.mouse.move(target!.x + 100, target!.y + 5, { steps: 10 });
  await page.mouse.up();

  await expect
    .poll(async () => {
      const json = await getJson(page);
      return [plainText(json[1]), plainText(json[2])];
    })
    .toEqual([
      "Select some text and try the toolbar. The value is plain serde-compatible JSON, so it can be persisted, diffed or sent to a server unchanged.",
      "A Rust rewrite of Slate for Leptos. The document model, operations, normalization engine and undo history are pure Rust; this page is the Leptos view layer.",
    ]);

  await page.getByTitle("Undo (⌘Z)").click();
  await expect
    .poll(async () => {
      const json = await getJson(page);
      return plainText(json[1]);
    })
    .toContain("A Rust rewrite of Slate");

  await block.hover();
  await page.getByTitle("Drag to move (↑/↓)").focus();
  await page.keyboard.press("ArrowUp");
  await expect
    .poll(async () => {
      const json = await getJson(page);
      return plainText(json[1]);
    })
    .toContain("Select some text");
});

test("list type buttons switch between bulleted and numbered lists", async ({
  page,
}) => {
  await page.locator('[data-text-path="4.1.0"]').click();
  await page.getByRole("button", { name: "Numbered list" }).click();
  await expect.poll(async () => (await getJson(page))[4]?.type).toBe("ol");

  await page.getByRole("button", { name: "Bulleted list" }).click();
  await expect.poll(async () => (await getJson(page))[4]?.type).toBe("ul");
});

test("table blocks can be reordered with the drag handle", async ({ page }) => {
  const block = page.locator('[data-node-path="7"]');
  await block.scrollIntoViewIfNeeded();
  await block.hover();
  const handle = await page.getByTitle("Drag to move (↑/↓)").boundingBox();
  const target = await page.locator('[data-node-path="6"]').boundingBox();
  expect(handle).not.toBeNull();
  expect(target).not.toBeNull();

  await page.mouse.move(handle!.x + handle!.width / 2, handle!.y + handle!.height / 2);
  await page.mouse.down();
  await page.mouse.move(target!.x + 100, target!.y + 5, { steps: 12 });
  await page.mouse.up();

  await expect.poll(async () => (await getJson(page))[6]?.type).toBe("table");
});

test("Tab and Shift+Tab indent and outdent list items", async ({ page }) => {
  await setCaret(page, "4.1.0", 1);
  await page.keyboard.press("Tab");

  await expect
    .poll(async () => {
      const json = await getJson(page);
      const list = json[4];
      return list.children?.length;
    })
    .toBe(3);
  await expect
    .poll(async () => {
      const json = await getJson(page);
      const firstItem = json[4].children?.[0];
      const nested = firstItem?.children?.find((node) => node.type === "ul");
      return nested?.children?.length;
    })
    .toBe(1);

  await page.keyboard.press("Shift+Tab");
  await expect
    .poll(async () => {
      const json = await getJson(page);
      return json[4].children?.length;
    })
    .toBe(4);
  await expect
    .poll(async () => {
      const json = await getJson(page);
      return plainText(json[4].children?.[1] ?? { children: [] });
    })
    .toContain("Use ⌘B");
});

test("Backspace at the first list item lifts it before the list", async ({
  page,
}) => {
  await setCaret(page, "4.0.0", 0);
  await page.keyboard.press("Backspace");

  await expect.poll(async () => (await getJson(page)).length).toBe(13);
  const json = await getJson(page);
  expect(json[4].type).toBe("p");
  expect(plainText(json[4])).toContain("Type, select and format text");
  expect(json[5].type).toBe("ul");
  expect(json[5].children).toHaveLength(3);
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

test("typing @ opens the picker and inserts the chosen mention", async ({
  page,
}) => {
  await setCaret(page, "2.0", 0);
  await page.keyboard.type("@lep");

  await expect(page.locator(".mention-bar")).not.toHaveClass(/hidden/);
  await expect(page.locator(".mention-option")).toHaveCount(1);
  await expect(page.locator(".mention-option")).toHaveText("@Leptos");

  await page.locator(".mention-option").click();
  await expect(page.locator(".ls-mention")).toHaveCount(1);
  await expect
    .poll(async () => {
      const json = await getJson(page);
      return json[2].children?.find((node) => node.type === "mention")?.name;
    })
    .toBe("Leptos");
  await expect
    .poll(async () => {
      const json = await getJson(page);
      return plainText(json[2]).includes("@lep");
    })
    .toBe(false);
});

test("Escape dismisses the mention picker without reopening while typing", async ({
  page,
}) => {
  await setCaret(page, "2.0", 0);
  await page.keyboard.type("@sla");
  await expect(page.locator(".mention-bar")).not.toHaveClass(/hidden/);

  await page.keyboard.press("Escape");
  await expect(page.locator(".mention-bar")).toHaveClass(/hidden/);

  await page.keyboard.type("x");
  await expect(page.locator(".mention-bar")).toHaveClass(/hidden/);
});

test("clicking away closes the mention picker", async ({ page }) => {
  await setCaret(page, "2.0", 0);
  await page.keyboard.type("@");
  await expect(page.locator(".mention-bar")).not.toHaveClass(/hidden/);

  await page.mouse.click(5, 5);
  await expect(page.locator(".mention-bar")).toHaveClass(/hidden/);
});

test("mentions insert inline voids and delete as atoms", async ({ page }) => {
  await setCaret(page, "2.0", 0);
  await page.getByRole("button", { name: "Mention", exact: true }).click();
  await page.locator(".mention-bar .media-input").fill("lep");
  await page.getByRole("option", { name: "@Leptos", exact: true }).click();

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
  await expect(page.locator(".ls-media")).toHaveClass(/ls-selected/);
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
