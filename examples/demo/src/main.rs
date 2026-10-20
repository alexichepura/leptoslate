use leptos::prelude::*;
use leptoslate::{Editable, EditorHandle, Element, Node, Text};
use serde_json::{json, Value};

fn text(value: &str) -> Node {
    Node::Text(Text::new(value))
}

fn marked(value: &str, marks: &[(&str, Value)]) -> Node {
    let mut node = Text::new(value);
    for (key, mark) in marks {
        node.properties.insert((*key).to_string(), mark.clone());
    }
    Node::Text(node)
}

fn paragraph(children: Vec<Node>) -> Node {
    Node::Element(Element::paragraph(children))
}

fn heading(level: &str, value: &str) -> Node {
    Node::Element(Element::new(level, vec![text(value)]))
}

fn list_item(children: Vec<Node>) -> Node {
    Node::Element(Element::new("li", children))
}

fn initial_document() -> Vec<Node> {
    let bold = json!(true);
    let italic = json!(true);
    let code = json!(true);

    vec![
        heading("h1", "Leptoslate"),
        paragraph(vec![
            text("A Rust rewrite of "),
            marked("Slate", &[("bold", bold.clone())]),
            text(" for "),
            marked("Leptos", &[("italic", italic.clone())]),
            text(". The document model, operations, normalization engine and undo history are pure Rust; this page is the Leptos view layer."),
        ]),
        paragraph(vec![
            text("Select some text and try the toolbar. The value is plain "),
            marked("serde", &[("code", code.clone())]),
            text("-compatible JSON, so it can be persisted, diffed or sent to a server unchanged."),
        ]),
        Node::Element(Element::new(
            "ul",
            vec![
                list_item(vec![text("Type, select and format text")]),
                list_item(vec![
                    text("Use "),
                    marked("⌘B", &[("code", code.clone())]),
                    text(" / "),
                    marked("⌘I", &[("code", code.clone())]),
                    text(" / "),
                    marked("⌘U", &[("code", code.clone())]),
                    text(" for marks"),
                ]),
                list_item(vec![text("Paste multi-line text to split blocks")]),
                list_item(vec![text("Undo with ⌘Z and redo with ⇧⌘Z")]),
            ],
        )),
        Node::Element(Element::new(
            "blockquote",
            vec![text(
                "The editor value is a tree of elements and text nodes — exactly the shape Slate uses.",
            )],
        )),
        Node::Element(Element::new(
            "code",
            vec![Node::Text(Text::new(
                "let mut editor = Editor::empty();\neditor.insert_text(\"Hello\");\neditor.toggle_mark(\"bold\", json!(true));",
            ))],
        )),
        paragraph(vec![
            text("Press "),
            marked("Enter", &[("code", code.clone())]),
            text(" to split a block, or "),
            marked("Shift+Enter", &[("code", code.clone())]),
            text(" for a soft line break. Images, dividers and inline links are supported by the schema."),
        ]),
        Node::Element(Element::empty("hr")),
        paragraph(vec![
            marked("Tip: ", &[("bold", bold), ("italic", italic)]),
            text("the JSON panel at the bottom is live."),
        ]),
    ]
}

#[component]
fn MarkButton(
    editor: EditorHandle,
    mark: &'static str,
    label: &'static str,
    #[prop(optional)] title: &'static str,
) -> impl IntoView {
    let editor_click = editor.clone();
    let active = {
        let editor = editor.clone();
        move || {
            editor
                .state
                .with(|state| state.has_mark(mark, &Value::Bool(true)))
        }
    };
    let title = if title.is_empty() { label } else { title };

    view! {
        <button
            class="tool"
            class:active=active
            title=title
            on:mousedown=move |ev| ev.prevent_default()
            on:click=move |_| editor_click.toggle_mark(mark, Value::Bool(true))
        >
            {label}
        </button>
    }
}

#[component]
fn BlockButton(
    editor: EditorHandle,
    block_type: &'static str,
    label: &'static str,
    #[prop(optional)] title: &'static str,
) -> impl IntoView {
    let editor_click = editor.clone();
    let active = {
        let editor = editor.clone();
        move || {
            editor.state.with(|state| {
                state
                    .selection
                    .as_ref()
                    .and_then(|range| state.leaf_block_path(range.start()))
                    .and_then(|path| state.element(&path))
                    .map(|element| element.element_type == block_type)
                    .unwrap_or(false)
            })
        }
    };
    let title = if title.is_empty() { label } else { title };

    view! {
        <button
            class="tool"
            class:active=active
            title=title
            on:mousedown=move |ev| ev.prevent_default()
            on:click=move |_| editor_click.set_block(block_type)
        >
            {label}
        </button>
    }
}

#[component]
fn ListButton(editor: EditorHandle, list_type: &'static str, label: &'static str) -> impl IntoView {
    let editor_click = editor.clone();
    let active = {
        let editor = editor.clone();
        move || {
            editor.state.with(|state| {
                let Some(range) = &state.selection else {
                    return false;
                };
                let top = range.start().path.first().copied().unwrap_or(0);
                state
                    .element(&[top])
                    .map(|element| element.element_type == list_type)
                    .unwrap_or(false)
            })
        }
    };

    view! {
        <button
            class="tool"
            class:active=active
            title=label
            on:mousedown=move |ev| ev.prevent_default()
            on:click=move |_| editor_click.toggle_list(list_type)
        >
            {label}
        </button>
    }
}

#[component]
fn App() -> impl IntoView {
    let editor = EditorHandle::new(initial_document());
    let show_json = RwSignal::new(false);

    let undo_disabled = {
        let editor = editor.clone();
        move || !editor.state.with(|state| state.can_undo())
    };
    let redo_disabled = {
        let editor = editor.clone();
        move || !editor.state.with(|state| state.can_redo())
    };

    let insert_divider = {
        let editor = editor.clone();
        move |_| {
            editor.update(|state| {
                let Some(top) = state
                    .selection
                    .as_ref()
                    .map(|range| range.start().path.first().copied().unwrap_or(0))
                else {
                    return;
                };
                let divider_at = vec![top + 1];
                let paragraph_at = vec![top + 2];
                state.insert_node(&divider_at, Node::Element(Element::empty("hr")));
                state.insert_node(&paragraph_at, Node::Element(Element::empty("p")));
                if let Some(point) = state.first_text_point_in(&paragraph_at) {
                    state.select_point(point);
                }
            });
        }
    };

    let clear = {
        let editor = editor.clone();
        move |_| {
            editor.set_value(vec![Node::Element(Element::empty("p"))]);
        }
    };

    let word_count = {
        let editor = editor.clone();
        move || {
            editor
                .state
                .with(|state| state.document_string().split_whitespace().count())
        }
    };

    let json = {
        let editor = editor.clone();
        move || editor.state.with(|state| state.to_json_pretty())
    };

    view! {
        <main class="page">
            <header class="masthead">
                <div>
                    <p class="eyebrow">"Rust · Leptos · Slate"</p>
                    <h1>"leptoslate"</h1>
                    <p class="subtitle">
                        "A framework-agnostic rich text engine with a contenteditable view."
                    </p>
                </div>
                <div class="status">
                    <span class="dot"></span>
                    <span>{word_count} " words"</span>
                </div>
            </header>

            <section class="surface">
                <div class="toolbar" role="toolbar" aria-label="Formatting">
                    <div class="group">
                        <button
                            class="tool"
                            title="Undo (⌘Z)"
                            disabled=undo_disabled
                            on:mousedown=move |ev| ev.prevent_default()
                            on:click={
                                let editor = editor.clone();
                                move |_| {
                                    editor.undo();
                                }
                            }
                        >
                            "↺"
                        </button>
                        <button
                            class="tool"
                            title="Redo (⇧⌘Z)"
                            disabled=redo_disabled
                            on:mousedown=move |ev| ev.prevent_default()
                            on:click={
                                let editor = editor.clone();
                                move |_| {
                                    editor.redo();
                                }
                            }
                        >
                            "↻"
                        </button>
                    </div>

                    <div class="divider"></div>

                    <div class="group">
                        <MarkButton editor=editor.clone() mark="bold" label="B" title="Bold (⌘B)" />
                        <MarkButton editor=editor.clone() mark="italic" label="I" title="Italic (⌘I)" />
                        <MarkButton editor=editor.clone() mark="underline" label="U" title="Underline (⌘U)" />
                        <MarkButton editor=editor.clone() mark="strikethrough" label="S" title="Strikethrough" />
                        <MarkButton editor=editor.clone() mark="code" label="<>" title="Inline code" />
                    </div>

                    <div class="divider"></div>

                    <div class="group">
                        <BlockButton editor=editor.clone() block_type="p" label="¶" title="Paragraph" />
                        <BlockButton editor=editor.clone() block_type="h1" label="H1" title="Heading 1" />
                        <BlockButton editor=editor.clone() block_type="h2" label="H2" title="Heading 2" />
                        <BlockButton editor=editor.clone() block_type="blockquote" label="❝" title="Block quote" />
                        <BlockButton editor=editor.clone() block_type="code" label="{ }" title="Code block" />
                    </div>

                    <div class="divider"></div>

                    <div class="group">
                        <ListButton editor=editor.clone() list_type="ul" label="• List" />
                        <ListButton editor=editor.clone() list_type="ol" label="1. List" />
                        <button
                            class="tool"
                            title="Insert divider"
                            on:mousedown=move |ev| ev.prevent_default()
                            on:click=insert_divider
                        >
                            "―"
                        </button>
                    </div>
                </div>

                <Editable
                    editor=editor.clone()
                    placeholder="Start writing…"
                />

                <footer class="surface-footer">
                    <span class="shortcuts">"⌘B bold · ⌘I italic · ⌘U underline · ⌘Z undo"</span>
                    <div class="footer-actions">
                        <span>{word_count} " words"</span>
                        <button
                            class="link-tool"
                            on:mousedown=move |ev| ev.prevent_default()
                            on:click={
                                let editor = editor.clone();
                                move |_| editor.set_value(initial_document())
                            }
                        >
                            "Reset"
                        </button>
                        <button
                            class="link-tool"
                            on:mousedown=move |ev| ev.prevent_default()
                            on:click=clear.clone()
                        >
                            "Clear"
                        </button>
                        <button
                            class="link-tool"
                            on:mousedown=move |ev| ev.prevent_default()
                            on:click=move |_| show_json.update(|value| *value = !*value)
                        >
                            "JSON"
                        </button>
                    </div>
                </footer>
            </section>

            <Show when=move || show_json.get()>
                <section class="json-panel">
                    <div class="json-header">
                        <span>"Document JSON"</span>
                        <span class="json-hint">"slate-compatible value"</span>
                    </div>
                    <pre>{json}</pre>
                </section>
            </Show>

            <footer class="page-footer">
                <span>"leptoslate-core: model · ops · normalize · history"</span>
                <span>"leptoslate: contenteditable · selection mapping · input"</span>
            </footer>
        </main>
    }
}

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}
