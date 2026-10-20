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
- DOM selection ↔ model selection mapping via `data-text-path` attributes
- `beforeinput` handling for typing, deletion, formatting, undo/redo and IME-safe composition
- Paste/cut/copy handling with plain text
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

```bash
cargo test --workspace
```

The core test suite covers serialization, typing, block splitting, merging, cross-block deletion, marks, undo/redo, normalization, lists, block conversion and Unicode offsets.

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
- A pragmatic core schema instead of Slate's plugin model. Inline elements are supported by the schema/renderer, but rich inline editing (links with editable text, mentions) is minimal.
- Element rendering is schema-driven HTML tags; custom Leptos components per element type are not pluggable yet.
- No collaborative editing, OT/CRDT, or remote operation rebasing.
- IME composition is reconciled per text node, not with Slate's composition-aware range model.
- Markdown paste, HTML paste, drag-and-drop and rich clipboard formats are not implemented.
- List toggling operates on the top-level block containing the selection; list indentation/lift commands are not included.

## License

MIT
