use leptos::prelude::*;
use leptoslate::{Editable, EditorHandle, Element, FileHandler, Node, Props, Renderers, Text};
use serde_json::{json, Value};
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::{KeyboardEvent, MouseEvent};

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

fn callout(value: &str, tone: &str) -> Node {
    Node::Element(Element::new("callout", vec![text(value)]).with_property("tone", json!(tone)))
}

fn todo(value: &str, checked: bool) -> Node {
    Node::Element(Element::new("todo", vec![text(value)]).with_property("checked", json!(checked)))
}

fn image(src: &str) -> Node {
    Node::Element(
        Element::new("img", vec![Node::empty_text()])
            .with_property("src", json!(src))
            .with_property("align", json!("center")),
    )
}

fn focus_editor() {
    if let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.query_selector(".ls-editor").ok().flatten())
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = element.focus();
    }
}

fn read_image_file(file: web_sys::File, editor: EditorHandle) {
    if file.size() > 8_000_000.0 {
        return;
    }
    let Ok(reader) = web_sys::FileReader::new() else {
        return;
    };
    let loaded = reader.clone();
    let onload = wasm_bindgen::closure::Closure::once_into_js(move || {
        if let Ok(result) = loaded.result() {
            if let Some(src) = result.as_string() {
                focus_editor();
                let image = image(&src);
                editor.update(|state| {
                    state.insert_block_after_selection(image);
                });
            }
        }
    });
    reader.set_onload(Some(onload.unchecked_ref()));
    let _ = reader.read_as_data_url(&file);
}

fn demo_renderers() -> Renderers {
    Renderers::new()
        .element("callout", |ctx| {
            let tone = ctx.prop_str("tone").unwrap_or("info").to_string();
            let class = ctx.class_with(&format!("ls-callout ls-callout-{tone}"));
            let element_type = ctx.element_type().to_string();
            let node_path = ctx.node_path;
            let children = ctx.children;
            view! {
                <aside class=class data-node-path=node_path data-type=element_type>
                    {children}
                </aside>
            }
            .into_any()
        })
        .element("todo", |ctx| {
            let class = ctx.class_with("ls-todo");
            let checked = ctx.prop_bool("checked");
            let element_type = ctx.element_type().to_string();
            let node_path = ctx.node_path;
            let children = ctx.children;
            let editor = ctx.editor.clone();
            let path = ctx.path.clone();
            let toggle_editor = editor.clone();
            let toggle_path = path.clone();

            view! {
                <div class=class data-node-path=node_path data-type=element_type>
                    <button
                        type="button"
                        class="ls-todo-check"
                        class:checked=checked
                        contenteditable="false"
                        aria-label="Toggle todo"
                        aria-pressed=if checked { "true" } else { "false" }
                        on:mousedown=move |ev: MouseEvent| {
                            ev.prevent_default();
                            let mut properties = Props::new();
                            properties.insert("checked".into(), Value::Bool(!checked));
                            let path = path.clone();
                            editor.update(|state| state.set_node_properties(&path, properties));
                        }
                        on:keydown=move |ev: KeyboardEvent| {
                            if ev.key() == "Enter" || ev.key() == " " {
                                ev.prevent_default();
                                let mut properties = Props::new();
                                properties.insert("checked".into(), Value::Bool(!checked));
                                let path = toggle_path.clone();
                                toggle_editor.update(|state| state.set_node_properties(&path, properties));
                            }
                        }
                    >
                        {if checked { "✓" } else { "" }}
                    </button>
                    <div class="ls-todo-content">{children}</div>
                </div>
            }
            .into_any()
        })
        .element("img", |ctx| {
            let src = ctx.prop_str("src").unwrap_or("").to_string();
            let alt = ctx.prop_str("alt").unwrap_or("").to_string();
            let align = ctx.prop_str("align").unwrap_or("center").to_string();
            let next_align = match align.as_str() {
                "left" => "center",
                "center" => "right",
                _ => "left",
            };
            let base_class = ctx.class_with(&format!("ls-media ls-media-{align}"));
            let element_type = ctx.element_type().to_string();
            let node_path = ctx.node_path;
            let path = ctx.path.clone();
            let editor = ctx.editor.clone();
            let align_path = path.clone();
            let align_editor = editor.clone();
            let delete_editor = editor.clone();
            let delete_path = path.clone();
            let select_editor = editor.clone();
            let select_path = path.clone();
            let class_editor = editor.clone();
            let class_path = path;
            let class = move || {
                if class_editor.is_node_selected(&class_path) {
                    format!("{base_class} ls-selected")
                } else {
                    base_class.clone()
                }
            };

            view! {
                <figure
                    class=class
                    data-node-path=node_path
                    data-type=element_type
                    contenteditable="false"
                    on:click=move |ev| {
                        ev.prevent_default();
                        select_editor.select_node(&select_path);
                    }
                >
                    <img src=src alt=alt draggable="false" />
                    <figcaption class="ls-media-tools">
                        <button
                            type="button"
                            contenteditable="false"
                            title="Cycle alignment"
                            aria-label="Cycle image alignment"
                            on:mousedown=move |ev: MouseEvent| {
                                ev.prevent_default();
                                ev.stop_propagation();
                                let mut properties = Props::new();
                                properties.insert("align".into(), Value::String(next_align.into()));
                                let path = align_path.clone();
                                align_editor.update(|state| state.set_node_properties(&path, properties));
                            }
                        >
                            "◧"
                        </button>
                        <button
                            type="button"
                            contenteditable="false"
                            title="Delete image"
                            aria-label="Delete image"
                            on:mousedown=move |ev: MouseEvent| {
                                ev.prevent_default();
                                ev.stop_propagation();
                                let path = delete_path.clone();
                                delete_editor.update(|state| {
                                    state.remove_block_at(&path);
                                });
                            }
                        >
                            "✕"
                        </button>
                    </figcaption>
                </figure>
            }
            .into_any()
        })
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
        callout(
            "Custom blocks are ordinary elements with a registered Leptos renderer. This callout, the todo below and images all use the same API.",
            "info",
        ),
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
        todo("Try inserting a custom block from the toolbar", false),
        todo("Swap the renderer without touching the document model", true),
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

fn install_test_hook(editor: EditorHandle) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let hook = js_sys::Object::new();

    let json_editor = editor.clone();
    let json = Closure::<dyn Fn() -> JsValue>::new(move || {
        JsValue::from_str(&json_editor.state.with_untracked(|state| state.to_json()))
    });
    let _ = js_sys::Reflect::set(&hook, &"json".into(), json.as_ref().unchecked_ref());
    json.forget();

    let selection = Closure::<dyn Fn() -> JsValue>::new(move || {
        let value = editor.state.with_untracked(|state| {
            serde_json::json!({
                "selection": state.selection,
                "nodeSelection": state.node_selection,
            })
        });
        JsValue::from_str(&value.to_string())
    });
    let _ = js_sys::Reflect::set(
        &hook,
        &"selection".into(),
        selection.as_ref().unchecked_ref(),
    );
    selection.forget();

    let _ = js_sys::Reflect::set(&window, &"__leptoslate".into(), &hook.into());
}

#[component]
fn App() -> impl IntoView {
    let editor = EditorHandle::new(initial_document());
    install_test_hook(editor.clone());
    let renderers = demo_renderers();
    let show_json = RwSignal::new(false);
    let show_image = RwSignal::new(false);
    let image_url = RwSignal::new(String::new());
    let files_handler = {
        let editor = editor.clone();
        FileHandler::new(move |files| {
            for file in files {
                if file.type_().starts_with("image/") {
                    read_image_file(file, editor.clone());
                }
            }
        })
    };

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
                state.insert_block_after_selection(Node::Element(Element::empty("hr")));
            });
        }
    };

    let insert_callout = {
        let editor = editor.clone();
        move |_| {
            editor.update(|state| {
                let node = Node::Element(
                    Element::new("callout", vec![Node::empty_text()])
                        .with_property("tone", json!("info")),
                );
                state.insert_block_after_selection(node);
            });
        }
    };

    let insert_todo = {
        let editor = editor.clone();
        move |_| {
            editor.update(|state| {
                let node = Node::Element(
                    Element::new("todo", vec![Node::empty_text()])
                        .with_property("checked", json!(false)),
                );
                state.insert_block_after_selection(node);
            });
        }
    };

    let insert_image = {
        let editor = editor.clone();
        move |_| {
            let src = image_url.get_untracked().trim().to_string();
            if src.is_empty() {
                return;
            }
            focus_editor();
            let image = image(&src);
            editor.update(|state| {
                state.insert_block_after_selection(image);
            });
            image_url.set(String::new());
            show_image.set(false);
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

                    <div class="divider"></div>

                    <div class="group">
                        <button
                            class="tool wide"
                            title="Insert callout"
                            on:mousedown=move |ev| ev.prevent_default()
                            on:click=insert_callout
                        >
                            "Callout"
                        </button>
                        <button
                            class="tool wide"
                            title="Insert todo"
                            on:mousedown=move |ev| ev.prevent_default()
                            on:click=insert_todo
                        >
                            "Todo"
                        </button>
                        <button
                            class="tool wide"
                            title="Insert image"
                            on:mousedown=move |ev| ev.prevent_default()
                            on:click=move |_| show_image.update(|value| *value = !*value)
                        >
                            "Image"
                        </button>
                    </div>
                </div>

                <div class="media-bar" class:hidden=move || !show_image.get()>
                    <input
                        class="media-input"
                        type="url"
                        placeholder="https://example.com/image.jpg"
                        prop:value=move || image_url.get()
                        on:input=move |ev| image_url.set(event_target_value(&ev))
                    />
                    <button
                        class="tool"
                        on:mousedown=move |ev| ev.prevent_default()
                        on:click=insert_image
                    >
                        "Insert"
                    </button>
                    <label class="tool wide file-tool">
                        "Upload"
                        <input
                            class="file-input"
                            type="file"
                            accept="image/*"
                            multiple
                            on:change={
                                let files = files_handler.clone();
                                move |ev: web_sys::Event| {
                                    let Some(input) = ev
                                        .target()
                                        .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
                                    else {
                                        return;
                                    };
                                    if let Some(files_list) = input.files() {
                                        let selected = (0..files_list.length())
                                            .filter_map(|index| files_list.get(index))
                                            .collect();
                                        files.call(selected);
                                    }
                                    input.set_value("");
                                }
                            }
                        />
                    </label>
                    <span class="media-hint">"or paste / drop images"</span>
                    <button
                        class="link-tool"
                        on:mousedown=move |ev| ev.prevent_default()
                        on:click=move |_| show_image.set(false)
                    >
                        "Cancel"
                    </button>
                </div>

                <Editable
                    editor=editor.clone()
                    renderers=renderers
                    on_files=files_handler
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
