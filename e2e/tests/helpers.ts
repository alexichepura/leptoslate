import { expect, type Page } from "@playwright/test";

export type JsonNode = {
  type?: string;
  text?: string;
  children?: JsonNode[];
  [key: string]: unknown;
};

export type SelectionJson = {
  selection: {
    anchor: { path: number[]; offset: number };
    focus: { path: number[]; offset: number };
  } | null;
  nodeSelection: number[] | null;
};

export async function waitForEditor(page: Page) {
  await page.waitForFunction(() => Boolean((window as any).__leptoslate));
}

export async function getJson(page: Page): Promise<JsonNode[]> {
  return page.evaluate(() => JSON.parse((window as any).__leptoslate.json()));
}

export async function getSelection(page: Page): Promise<SelectionJson> {
  return page.evaluate(() => JSON.parse((window as any).__leptoslate.selection()));
}

export function plainText(node: JsonNode): string {
  if (typeof node.text === "string") {
    return node.text;
  }
  return (node.children ?? []).map(plainText).join("");
}

export async function setCaret(page: Page, path: string, offset: number) {
  await page.evaluate(
    ({ path, offset }) => {
      const editor = document.querySelector(".ls-editor") as HTMLElement;
      editor.focus();
      const span = document.querySelector(
        `[data-text-path="${path}"]`,
      ) as HTMLElement;
      const node = span.firstChild as Text;
      const range = document.createRange();
      range.setStart(node, offset);
      range.collapse(true);
      const selection = window.getSelection()!;
      selection.removeAllRanges();
      selection.addRange(range);
      editor.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
    },
    { path, offset },
  );
  await page.waitForTimeout(30);
}

export async function selectText(
  page: Page,
  path: string,
  start: number,
  end: number,
) {
  await page.evaluate(
    ({ path, start, end }) => {
      const editor = document.querySelector(".ls-editor") as HTMLElement;
      editor.focus();
      const span = document.querySelector(
        `[data-text-path="${path}"]`,
      ) as HTMLElement;
      const node = span.firstChild as Text;
      const range = document.createRange();
      range.setStart(node, start);
      range.setEnd(node, end);
      const selection = window.getSelection()!;
      selection.removeAllRanges();
      selection.addRange(range);
      editor.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
    },
    { path, start, end },
  );
  await page.waitForTimeout(30);
}

export async function expectTopBlockText(
  page: Page,
  index: number,
  text: string,
) {
  await expect
    .poll(async () => {
      const json = await getJson(page);
      return json[index] ? plainText(json[index]) : "";
    })
    .toContain(text);
}

export async function pasteText(page: Page, text: string) {
  await page.evaluate((text) => {
    const data = new DataTransfer();
    data.setData("text/plain", text);
    document.querySelector(".ls-editor")!.dispatchEvent(
      new ClipboardEvent("paste", {
        clipboardData: data,
        bubbles: true,
        cancelable: true,
      }),
    );
  }, text);
}
