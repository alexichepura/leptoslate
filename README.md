# leptoslate

A Rust rewrite of [Slate](https://docs.slatejs.org/), built for [Leptos](https://leptos.dev/).

The workspace contains a framework-agnostic document engine with the same concepts as Slate — nodes, paths, points, ranges, operations, normalization and undo history — plus a `contenteditable` view for Leptos that maps browser selections to the document model.

![leptoslate demo](docs/screenshot.png)

```
crates/
  leptoslate-core/   document model, operations, transforms, normalization, history
  leptoslate/        Leptos renderer, EditorHandle, DOM selection mapping, input handling
examples/
  demo/              Trunk-based demo application
```

## What is implemented

**Document model**

- Elements and text nodes with arbitrary JSON properties (`Props`)
- Slate-compatible JSON serialization/deserialization
- Paths, points, ranges, and the same path/point transform rules as Slate
- Character offsets (with UTF-16 conversion at the DOM boundary)
- Configurable schema for tags, inline elements, void elements and default blocks

**Editing engine**

- Operations: `insert_node`, `insert_text`, `remove_node`, `remove_text`, `merge_node`, `split_node`, `move_node`, `set_node`, `set_selection`
- Patches that are serializable and replayable
- High-level transforms:
  - insert / delete text, selection, words, blocks
  - split blocks (Enter), soft breaks (Shift+Enter), paste with block splitting
  - Enter splits the leaf block, so list items stay inside their list and paragraphs inside a blockquote stay inside the quote
  - marks (bold, italic, underline, strikethrough, inline code, colors)
  - block types, lists (wrap/unwrap), node set/unset, move
- Normalization:
  - elements always have a child
  - adjacent text nodes with equal properties merge
  - empty texts are removed when they have siblings
  - inline content wraps in the default block when needed
  - inline elements are surrounded by text
- Undo/redo with grouped transactions, inverse operations and selection restore
- Consecutive typing coalesces into one undo step until the selection moves or a non-text edit happens

**Leptos integration**

- `EditorHandle` wrapping `RwSignal<Editor>`
- Schema-driven rendering (`p`, headings, blockquotes, code, lists, links, images, dividers, …)
- Pluggable element renderers for application-specific blocks
- Block/node selection for void blocks: click an image, delete it, arrow out of it, or type to replace it
- Inline links: wrap a selection, edit the href, remove links, and prevent navigation while editing
- Inline void mentions: insert atoms, delete them as single units, and render them with custom components
- DOM selection ↔ model selection mapping via `data-text-path` attributes
- `beforeinput` handling for typing, deletion, formatting, undo/redo and IME-safe composition
- Paste/cut/copy handling with plain text
- File paste/drop hook for image uploads (`FileHandler`)
- Keyboard shortcuts: ⌘/Ctrl+B, I, U, Z, Shift+Z, Y, A; Tab inserts spaces
- Placeholder support, active-mark/block state helpers, JSON snapshot

## Quick start

```rust
use leptos::prelude::*;
use leptoslate::{Editable, EditorHandle, Element, Node, Text};

#[component]
fn App() -> impl IntoView {
    let editor = EditorHandle::new(vec![
        Node::Element(Element::new(
            "h1",
            vec![Node::Text(Text::new("Hello"))],
        )),
        Node::Element(Element::paragraph(vec![
            Node::Text(Text::new("Start writing…")),
        ])),
    ]);

    view! {
        <button
            on:mousedown=|ev| ev.prevent_default()
            on:click={
                let editor = editor.clone();
                move |_| editor.toggle_mark("bold", serde_json::json!(true))
            }
        >
            "B"
        </button>
        <Editable editor=editor placeholder="Start writing…" />
    }
}
```

The `on:mousedown` `prevent_default()` keeps the editor selection while clicking toolbar buttons.

### Using the handle

```rust
let editor = EditorHandle::empty();

editor.insert_text("Hello");
editor.toggle_mark("italic", serde_json::json!(true));
editor.insert_break();
editor.insert_text("world");

editor.set_block("h2");
editor.toggle_list("ul");
editor.undo();
editor.redo();

println!("{}", editor.to_json_pretty());
```

`EditorHandle::update` runs the closure with `&mut Editor`, so the entire core API is available:

```rust
editor.update(|state| {
    state.insert_text("…");
    state.toggle_mark("bold", serde_json::json!(true));
    state.select_all();
});
```

### Custom blocks

Register a Leptos view for any element type:

```rust
use leptos::prelude::*;
use leptoslate::{Editable, EditorHandle, Renderers};

let renderers = Renderers::new().element("callout", |ctx| {
    let class = ctx.class_with("callout");
    let node_path = ctx.node_path;
    let children = ctx.children;
    view! {
        <aside class=class data-node-path=node_path>
            {children}
        </aside>
    }
    .into_any()
});

view! { <Editable editor=editor renderers=renderers /> }
```

`ElementRenderContext` provides the element, path, rendered children, the editor handle, and inline/void flags. A custom renderer for a void element is responsible for setting `contenteditable="false"`. Use `editor.is_node_selected(&path)` inside a reactive class to show selected chrome.

Custom blocks are ordinary elements in the document, so they serialize, normalize and undo like everything else. The demo uses renderers for a callout, a todo block, and images with alignment/delete controls.

Insert them with the core API:

```rust
use leptoslate::{Element, Node};

editor.update(|state| {
    state.insert_block_after_selection(Node::Element(
        Element::new("callout", vec![Node::empty_text()])
            .with_property("tone", serde_json::json!("info")),
    ));
});
```

Void blocks (images, dividers) automatically get a paragraph after them so typing can continue. `remove_block_at` and `set_node_properties` are available for custom block toolbars.

### Links

Links are inline elements with an `href` property:

```rust
editor.select_point(point);      // or select a text range
editor.set_link("https://leptos.dev"); // wraps the selection or updates the link at the caret
editor.link_at_selection();      // Some(href) when the caret is inside a link
editor.remove_link();            // unwraps links intersecting the selection
```

The built-in renderer prevents link navigation during editing. The demo adds a Link toolbar button with an href bar for Apply/Remove.

### Mentions and inline voids

Inline void nodes are atoms that cannot be edited internally. They still live inside text, are surrounded by text nodes by normalization, and delete as a single unit:

```rust
let node = Node::Element(
    Element::new("mention", vec![Node::empty_text()])
        .with_property("id", serde_json::json!("leptos"))
        .with_property("name", serde_json::json!("Leptos")),
);

editor.insert_inline_node(node); // splits text at the caret and selects the text after it
editor.select_after_node(&path); // used when clicking a chip
```

Register a renderer for the element type to draw the chip. The demo includes a Mention picker with recipient props (`id`, `name`), renders mentions as inline chips, and supports Backspace/Delete as atom deletion. The built-in fallback renders void inline elements as non-editable spans.

### Block selection

Void blocks (images, dividers, embeds) are selected as nodes rather than text ranges:

```rust
editor.select_node(&path);          // clears any text selection
editor.is_node_selected(&path);     // reactive, for selected chrome
editor.delete_node_selection();     // Backspace/Delete
editor.move_selection_off_node(false); // ArrowRight
```

The built-in void renderer selects on click. Custom renderers can call `editor.select_node(&ctx.path)` and use `is_node_selected` in a reactive class. Backspace/Delete removes the block, Left/Right moves into adjacent text, and typing replaces the block.

### Images and dropped files

Images are ordinary block elements. The demo inserts them from a URL bar, a file picker, pasted files, or drag/drop:

```rust
use leptoslate::{Editable, FileHandler};

let files = FileHandler::new(move |files| {
    for file in files {
        // read/upload the file, then:
        // editor.update(|state| state.insert_block_after_selection(image_node(url)));
    }
});

view! { <Editable editor=editor on_files=files /> }
```

`Editable` already handles plain-text paste. When a paste or drop contains files, it calls `on_files` instead, so the application owns upload policy (data URL in the demo, object storage in production). Drag-over displays a drop outline.

### Slate-compatible values

```json
[
  {
    "type": "p",
    "children": [
      { "text": "Hello " },
      { "text": "world", "bold": true }
    ]
  }
]
```

```rust
use leptoslate::Editor;

let mut editor = Editor::from_json(json)?;
editor.select_all();
editor.insert_text("replaced");
let json = editor.to_json();
```

## Running the demo

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk
cd examples/demo
trunk serve
```

Then open <http://localhost:8080>.

## Tests

Rust tests:

```bash
cargo test --workspace
```

The core test suite covers serialization, typing, block splitting, merging, cross-block deletion, marks, undo/redo, normalization, lists, block conversion, node selection and Unicode offsets.

Browser tests use Playwright against the real Trunk build:

```bash
cd e2e
npm ci
npx playwright install chromium
npm test
```

The config starts `trunk build` and serves `examples/demo/dist`, so the tests exercise the same DOM, input events and WASM binary as the demo. Coverage includes typing and selection sync, Enter/Backspace, toolbar marks, undo coalescing, paste, custom blocks, image insert/select/delete/undo, image toolbar controls, built-in void blocks and IME composition reconciliation.

The demo exposes a small test hook:

```js
window.__leptoslate.json()       // Slate-shaped document JSON
window.__leptoslate.selection()  // text range + node selection
```

CI runs formatting, clippy, Rust tests, the wasm build and Playwright on every push/PR.

## How it maps to Slate

| Slate | leptoslate |
| --- | --- |
| `Node[]` | `Vec<Node>` |
| `Element` / `Text` | `Element` / `Text` with `Props` |
| `Path` | `Vec<usize>` |
| `Point` / `Range` | `Point` / `Range` |
| `Operation` | `Operation` (serde tagged) |
| `Editor.apply` | `Editor::apply_op` |
| `Transforms.*` | `Editor` methods |
| `editor.marks` | `Editor::marks` |
| `normalizeNode` | built-in normalization pass |
| DOM adapter + `Editable` | `leptoslate::Editable` |

The core has no DOM or Leptos dependency, so it can be used in tests, servers, collaborative backends or other frontend frameworks.

## Design notes

- **Character offsets.** The model counts Unicode scalar values, unlike Slate's UTF-16 offsets. `leptoslate-core::text` converts at the DOM boundary. This avoids splitting surrogate pairs inside Rust strings.
- **Transactions.** Each public transform groups operations into one undoable batch. Normalization operations are recorded too, so undo reverses the exact document state.
- **Rendering.** The renderer reads a `Memo` of the document tree, so selection-only updates (dragging a selection, toolbar state) do not rebuild the contenteditable DOM.
- **Input.** Editing is driven by `beforeinput`, which is prevented and translated into operations. IME composition is allowed in the DOM and reconciled on `compositionend`.

## Limitations

This is an early implementation, not a drop-in replacement for the entire Slate ecosystem:

- Client-side rendering only; no SSR/hydration adapter yet.
- A pragmatic core schema instead of Slate's plugin model. Inline links and void mentions are supported; typing `@` to trigger mention autocomplete is not implemented (the demo uses a picker).
- Rich HTML/Markdown fragment paste is tracked in #12.
- Element rendering is pluggable, but custom text/mark renderers (for example syntax highlighting) are not exposed yet.
- Node selection currently targets single blocks (images, dividers, embeds); multi-block and range-of-blocks selection is not implemented.
- Undoing the deletion of a block restores the document but places the caret at the document start rather than reselecting the restored block.
- No collaborative editing, OT/CRDT, or remote operation rebasing.
- IME composition is reconciled per text node, not with Slate's composition-aware range model.
- Markdown paste, HTML paste, drag-and-drop and rich clipboard formats are not implemented.
- List toggling operates on the top-level block containing the selection; list indentation/lift commands are not included.

## License

MIT
