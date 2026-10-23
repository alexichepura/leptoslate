//! The `Editable` component: contenteditable DOM, events and selection sync.

use leptos::html::Div;
use leptos::prelude::*;
use leptoslate_core::{Editor, Element as SlateElement, Node, Point, Range};
use send_wrapper::SendWrapper;
use serde_json::Value;
use wasm_bindgen::{closure::Closure, JsCast};
use web_sys::{
    ClipboardEvent, CompositionEvent, DragEvent, Element, InputEvent, KeyboardEvent, MouseEvent,
};

const MODEL_CLIPBOARD_MIME: &str = "application/x-leptoslate+json";

use crate::{
    dom,
    handle::EditorHandle,
    media::FileHandler,
    mention::{MentionHandler, MentionTrigger},
    render::render_node,
    renderers::Renderers,
    EDITOR_STYLES,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct BlockKey {
    index: usize,
    hash: u64,
}

/// A Slate-style rich text editor surface for Leptos.
///
/// The component renders the value from an [`EditorHandle`] into a
/// `contenteditable` container and translates browser input events into
/// document operations. Use the handle (or toolbar buttons) to apply marks and
/// block transforms.
#[component]
pub fn Editable(
    /// The editor state to render and edit.
    editor: EditorHandle,
    /// Extra class names for the outer shell.
    #[prop(optional, into)]
    class: String,
    /// Placeholder shown when the document is empty.
    #[prop(optional, into)]
    placeholder: String,
    /// Custom element renderers for application-specific blocks.
    #[prop(optional)]
    renderers: Renderers,
    /// Receives files pasted or dropped onto the editor.
    #[prop(optional)]
    on_files: Option<FileHandler>,
    /// Parse `text/html` clipboard data into blocks and marks.
    #[prop(default = true)]
    paste_html: bool,
    /// Parse plain text as Markdown when no HTML is available.
    #[prop(default = false)]
    paste_markdown: bool,
    /// Called when an `@query` mention trigger starts, changes, ends, or is dismissed.
    #[prop(optional)]
    on_mention: Option<MentionHandler>,
    /// Show a floating block-action bar on hover.
    #[prop(default = false)]
    hover_bar: bool,
    /// Show a floating formatting bar for text selections.
    #[prop(default = false)]
    selection_bubble: bool,
) -> impl IntoView {
    let content_ref = NodeRef::<Div>::new();
    let shell_ref = NodeRef::<Div>::new();
    let composing = RwSignal::new(false);
    let dragging = RwSignal::new(false);
    let mention_dismissed = RwSignal::new(false);
    let notified_trigger = RwSignal::new(None::<Option<MentionTrigger>>);
    let hovered_block = RwSignal::new(None::<Vec<usize>>);
    let hover_style = RwSignal::new(String::new());
    let bubble_visible = RwSignal::new(false);
    let bubble_style = RwSignal::new(String::new());
    let drag_block = RwSignal::new(None::<usize>);
    let drop_index = RwSignal::new(None::<usize>);
    let drop_style = RwSignal::new(String::new());
    let selection_listener = StoredValue::new(None::<SendWrapper<Closure<dyn FnMut()>>>);

    let render_editor = editor.clone();
    let schema_editor = editor.clone();
    let schema = Memo::new(move |_| schema_editor.state.with(|state| state.schema.clone()));

    let beforeinput_editor = editor.clone();
    let beforeinput_ref = content_ref;
    let beforeinput_composing = composing;
    let handle_beforeinput = move |ev: InputEvent| {
        let Some(root) = beforeinput_ref.get() else {
            return;
        };
        let root: &Element = root.unchecked_ref();

        if beforeinput_editor.has_node_selection() {
            ev.prevent_default();
            match ev.input_type().as_str() {
                "insertText" | "insertReplacementText" => {
                    if let Some(data) = ev.data() {
                        beforeinput_editor.insert_text(&data);
                    }
                }
                "deleteContentBackward"
                | "deleteContentForward"
                | "deleteByCut"
                | "insertParagraph"
                | "insertLineBreak" => {
                    beforeinput_editor.delete_node_selection();
                }
                "historyUndo" => {
                    beforeinput_editor.undo();
                }
                "historyRedo" => {
                    beforeinput_editor.redo();
                }
                _ => {}
            }
            return;
        }

        if beforeinput_editor.in_table_cell()
            && matches!(
                ev.input_type().as_str(),
                "insertParagraph" | "insertLineBreak"
            )
        {
            ev.prevent_default();
            return;
        }

        sync_from_dom(&beforeinput_editor, root);

        if beforeinput_composing.get_untracked() {
            return;
        }

        let input_type = ev.input_type();

        // IME composition is handled on `compositionend`.
        if input_type.contains("Composition") {
            return;
        }

        ev.prevent_default();

        match input_type.as_str() {
            "insertText" | "insertReplacementText" => {
                if let Some(data) = ev.data() {
                    if !data.is_empty() {
                        beforeinput_editor.insert_text(&data);
                    }
                }
            }
            "insertParagraph" => beforeinput_editor.insert_break(),
            "insertLineBreak" => beforeinput_editor.insert_soft_break(),
            "deleteContentBackward" | "deleteContentForward" | "deleteByCut" => {
                if input_type == "deleteContentBackward" {
                    beforeinput_editor.delete_backward();
                } else if input_type == "deleteContentForward" {
                    beforeinput_editor.delete_forward();
                } else {
                    beforeinput_editor.delete_selection();
                }
            }
            "deleteWordBackward" => beforeinput_editor.delete_word_backward(),
            "deleteWordForward" => beforeinput_editor.delete_word_forward(),
            "deleteSoftLineBackward" | "deleteHardLineBackward" => {
                beforeinput_editor.delete_backward()
            }
            "deleteSoftLineForward" | "deleteHardLineForward" => {
                beforeinput_editor.delete_forward()
            }
            "formatBold" => beforeinput_editor.toggle_mark("bold", Value::Bool(true)),
            "formatItalic" => beforeinput_editor.toggle_mark("italic", Value::Bool(true)),
            "formatUnderline" => beforeinput_editor.toggle_mark("underline", Value::Bool(true)),
            "historyUndo" => {
                beforeinput_editor.undo();
            }
            "historyRedo" => {
                beforeinput_editor.redo();
            }
            _ => {}
        }
    };

    let keydown_editor = editor.clone();
    let keydown_mention_editor = editor.clone();
    let keydown_mention_handler = on_mention;
    let keydown_mention_dismissed = mention_dismissed;
    let handle_keydown = move |ev: KeyboardEvent| {
        if ev.is_composing() || ev.key_code() == 229 {
            return;
        }
        let modifier = ev.meta_key() || ev.ctrl_key();
        let key = ev.key();
        let lower = key.to_ascii_lowercase();

        if key == "Escape" && keydown_mention_handler.is_some() {
            let active = keydown_mention_editor
                .state
                .with_untracked(|state| mention_trigger_at(state).is_some());
            if active {
                ev.prevent_default();
                keydown_mention_dismissed.set(true);
                if let Some(handler) = &keydown_mention_handler {
                    handler.run(None);
                }
                return;
            }
        }

        if keydown_editor.in_table_cell() {
            match key.as_str() {
                "Enter" => {
                    ev.prevent_default();
                    return;
                }
                "Tab" if !modifier && !ev.alt_key() => {
                    ev.prevent_default();
                    keydown_editor.move_table_cell(if ev.shift_key() { -1 } else { 1 });
                    return;
                }
                "Backspace" if keydown_editor.at_table_cell_start() => {
                    ev.prevent_default();
                    return;
                }
                "Delete" if keydown_editor.at_table_cell_end() => {
                    ev.prevent_default();
                    return;
                }
                _ => {}
            }
        }

        if keydown_editor.has_node_selection() {
            match key.as_str() {
                "Backspace" | "Delete" | "Enter" => {
                    ev.prevent_default();
                    keydown_editor.delete_node_selection();
                    return;
                }
                "ArrowUp" if ev.shift_key() => {
                    ev.prevent_default();
                    keydown_editor.extend_node_selection(-1);
                    return;
                }
                "ArrowDown" if ev.shift_key() => {
                    ev.prevent_default();
                    keydown_editor.extend_node_selection(1);
                    return;
                }
                "ArrowUp" => {
                    ev.prevent_default();
                    keydown_editor.move_node_selection(-1);
                    return;
                }
                "ArrowDown" => {
                    ev.prevent_default();
                    keydown_editor.move_node_selection(1);
                    return;
                }
                "ArrowLeft" => {
                    ev.prevent_default();
                    keydown_editor.move_selection_off_node(true);
                    return;
                }
                "ArrowRight" => {
                    ev.prevent_default();
                    keydown_editor.move_selection_off_node(false);
                    return;
                }
                "Escape" => {
                    ev.prevent_default();
                    keydown_editor.clear_node_selection();
                    return;
                }
                _ => {
                    let printable = !modifier
                        && !ev.alt_key()
                        && key.chars().count() == 1
                        && !key.chars().next().map(char::is_control).unwrap_or(true);
                    if printable {
                        ev.prevent_default();
                        keydown_editor.insert_text(&key);
                        return;
                    }
                }
            }
        }

        if modifier {
            match lower.as_str() {
                "b" => {
                    ev.prevent_default();
                    keydown_editor.toggle_mark("bold", Value::Bool(true));
                }
                "i" => {
                    ev.prevent_default();
                    keydown_editor.toggle_mark("italic", Value::Bool(true));
                }
                "u" => {
                    ev.prevent_default();
                    keydown_editor.toggle_mark("underline", Value::Bool(true));
                }
                "z" => {
                    ev.prevent_default();
                    if ev.shift_key() {
                        keydown_editor.redo();
                    } else {
                        keydown_editor.undo();
                    }
                }
                "y" => {
                    ev.prevent_default();
                    keydown_editor.redo();
                }
                "a" => {
                    ev.prevent_default();
                    keydown_editor.select_all();
                }
                "d" if keydown_editor.has_node_selection() => {
                    ev.prevent_default();
                    keydown_editor.duplicate_node_selection();
                }
                _ => {}
            }
            return;
        }

        if key == "Tab" {
            ev.prevent_default();
            if ev.shift_key() {
                keydown_editor.outdent_list();
            } else if !keydown_editor.indent_list() {
                keydown_editor.insert_text("    ");
            }
        }
    };

    let paste_editor = editor.clone();
    let paste_ref = content_ref;
    let paste_files = on_files.clone();
    let handle_paste = move |ev: ClipboardEvent| {
        ev.prevent_default();
        let Some(root) = paste_ref.get() else {
            return;
        };
        sync_from_dom(&paste_editor, root.unchecked_ref());

        let data = ev.clipboard_data();
        let files = data
            .as_ref()
            .and_then(|data| data.files())
            .map(|files| collect_files(&files))
            .unwrap_or_default();
        if !files.is_empty() {
            if let Some(handler) = &paste_files {
                handler.call(files);
            }
            return;
        }

        if let Some(html) = data
            .as_ref()
            .and_then(|data| data.get_data("text/html").ok())
            .filter(|html| !html.trim().is_empty())
        {
            if let Some(model) = extract_model_marker(&html) {
                if let Ok(nodes) = serde_json::from_str::<Vec<Node>>(&model) {
                    if !nodes.is_empty() {
                        paste_editor.insert_fragment(nodes);
                        return;
                    }
                }
            }
        }

        if let Some(model) = data
            .as_ref()
            .and_then(|data| data.get_data(MODEL_CLIPBOARD_MIME).ok())
            .filter(|model| !model.trim().is_empty())
        {
            if let Ok(nodes) = serde_json::from_str::<Vec<Node>>(&model) {
                if !nodes.is_empty() {
                    paste_editor.insert_fragment(nodes);
                    return;
                }
            }
        }

        let html = data
            .as_ref()
            .and_then(|data| data.get_data("text/html").ok())
            .unwrap_or_default();
        if paste_html && !html.trim().is_empty() {
            let nodes = leptoslate_core::fragment::parse_html(&html);
            if !nodes.is_empty() {
                paste_editor.insert_fragment(nodes);
                return;
            }
        }

        let text = data
            .and_then(|data| data.get_data("text/plain").ok())
            .unwrap_or_default();
        if text.is_empty() {
            return;
        }

        if paste_markdown {
            let nodes = leptoslate_core::fragment::parse_markdown(&text);
            if !nodes.is_empty() {
                paste_editor.insert_fragment(nodes);
                return;
            }
        }

        paste_editor.insert_plain_text(&text);
    };

    let drop_editor = editor.clone();
    let drop_ref = content_ref;
    let drop_shell_ref = shell_ref;
    let drop_files = on_files.clone();
    let handle_drag_over = move |ev: DragEvent| {
        ev.prevent_default();

        if drag_block.get_untracked().is_some() {
            dragging.set(false);
            let (Some(content), Some(shell)) = (drop_ref.get(), drop_shell_ref.get()) else {
                return;
            };
            let count = drop_editor
                .state
                .with_untracked(|state| state.children.len());
            if let Some((boundary, style)) =
                block_drop_target(content.unchecked_ref(), shell.unchecked_ref(), &ev, count)
            {
                drop_style.set(style);
                drop_index.set(Some(boundary));
            }
            return;
        }

        dragging.set(true);
    };
    let handle_drag_leave = move |ev: DragEvent| {
        ev.prevent_default();
        if drag_block.get_untracked().is_none() {
            dragging.set(false);
        }
    };
    let handle_drop = move |ev: DragEvent| {
        ev.prevent_default();
        dragging.set(false);

        if let Some(from) = drag_block.get_untracked() {
            let (Some(content), Some(shell)) = (drop_ref.get(), drop_shell_ref.get()) else {
                drag_block.set(None);
                drop_index.set(None);
                return;
            };
            let count = drop_editor
                .state
                .with_untracked(|state| state.children.len());
            if let Some((boundary, _)) =
                block_drop_target(content.unchecked_ref(), shell.unchecked_ref(), &ev, count)
            {
                let to = if from < boundary {
                    boundary - 1
                } else {
                    boundary
                };
                if to < count && to != from {
                    drop_editor.update(|state| {
                        state.move_block(from, to);
                    });
                }
            }
            drag_block.set(None);
            drop_index.set(None);
            if let Some(root) = drop_ref.get() {
                let _ = root.focus();
            }
            return;
        }

        drop_index.set(None);
        let Some(root) = drop_ref.get() else {
            return;
        };
        sync_from_dom(&drop_editor, root.unchecked_ref());
        let Some(data) = ev.data_transfer() else {
            return;
        };
        let files = data
            .files()
            .map(|files| collect_files(&files))
            .unwrap_or_default();
        if !files.is_empty() {
            if let Some(handler) = &drop_files {
                handler.call(files);
            }
            return;
        }

        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                if let Some(position) =
                    document.caret_position_from_point(ev.client_x() as f32, ev.client_y() as f32)
                {
                    if let (Some(node), Ok(range)) =
                        (position.offset_node(), document.create_range())
                    {
                        let offset = position.offset();
                        if range.set_start(&node, offset).is_ok()
                            && range.set_end(&node, offset).is_ok()
                        {
                            if let Ok(Some(selection)) = window.get_selection() {
                                let _ = selection.remove_all_ranges();
                                let _ = selection.add_range(&range);
                            }
                        }
                    }
                }
            }
        }
        sync_from_dom(&drop_editor, root.unchecked_ref());

        let html = data.get_data("text/html").unwrap_or_default();
        if paste_html && !html.trim().is_empty() {
            let nodes = leptoslate_core::fragment::parse_html(&html);
            if !nodes.is_empty() {
                drop_editor.insert_fragment(nodes);
                return;
            }
        }

        let text = data.get_data("text/plain").unwrap_or_default();
        if !text.is_empty() {
            drop_editor.insert_plain_text(&text);
        }
    };

    let copy_editor = editor.clone();
    let copy_ref = content_ref;
    let handle_copy = move |ev: ClipboardEvent| {
        let Some(root) = copy_ref.get() else {
            return;
        };
        sync_from_dom(&copy_editor, root.unchecked_ref());
        if !copy_editor.has_node_selection() {
            // Let the browser copy the selected DOM, including its inline
            // styles and link/table markup.
            return;
        }

        let text = copy_editor.node_selection_string().unwrap_or_default();
        let html = selection_html(&copy_editor, root.unchecked_ref());
        let model = selection_model_json(&copy_editor);
        let html = match (&html, &model) {
            (Some(html), Some(model)) => Some(model_marker_html(model, html)),
            (Some(html), None) => Some(html.clone()),
            (None, Some(model)) => Some(model_marker_html(model, "")),
            (None, None) => None,
        };
        if let Some(data) = ev.clipboard_data() {
            let _ = data.set_data("text/plain", &text);
            if let Some(html) = &html {
                let _ = data.set_data("text/html", html);
            }
            if let Some(model) = &model {
                let _ = data.set_data(MODEL_CLIPBOARD_MIME, model);
            }
        }
        ev.prevent_default();
    };

    let cut_editor = editor.clone();
    let cut_ref = content_ref;
    let handle_cut = move |ev: ClipboardEvent| {
        let Some(root) = cut_ref.get() else {
            return;
        };
        sync_from_dom(&cut_editor, root.unchecked_ref());

        if !cut_editor.has_node_selection() {
            // Use the browser's rich copy for the current DOM selection, then
            // delete through the model so the document stays in sync.
            if let Some(document) = web_sys::window().and_then(|window| window.document()) {
                if let Ok(document) = document.dyn_into::<web_sys::HtmlDocument>() {
                    let _ = document.exec_command("copy");
                }
            }
            ev.prevent_default();
            cut_editor.delete_selection();
            return;
        }

        let text = cut_editor.node_selection_string().unwrap_or_default();
        let html = selection_html(&cut_editor, root.unchecked_ref());
        let model = selection_model_json(&cut_editor);
        let html = match (&html, &model) {
            (Some(html), Some(model)) => Some(model_marker_html(model, html)),
            (Some(html), None) => Some(html.clone()),
            (None, Some(model)) => Some(model_marker_html(model, "")),
            (None, None) => None,
        };
        if let Some(data) = ev.clipboard_data() {
            let _ = data.set_data("text/plain", &text);
            if let Some(html) = &html {
                let _ = data.set_data("text/html", html);
            }
            if let Some(model) = &model {
                let _ = data.set_data(MODEL_CLIPBOARD_MIME, model);
            }
        }
        ev.prevent_default();
        cut_editor.delete_node_selection();
    };

    let hover_editor = editor.clone();
    let hover_content_ref = content_ref;
    let hover_shell_ref = shell_ref;
    let handle_hover = move |ev: MouseEvent| {
        if !hover_bar {
            return;
        }
        if hover_editor.state.with_untracked(|state| {
            state
                .selection
                .as_ref()
                .map(|range| !range.is_collapsed())
                .unwrap_or(false)
        }) {
            hovered_block.set(None);
            return;
        }
        let Some(content) = hover_content_ref.get() else {
            return;
        };
        let Some(target) = ev
            .target()
            .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
        else {
            return;
        };
        let Some(node) = target.closest("[data-node-path]").ok().flatten() else {
            web_sys::console::log_1(
                &format!("HOVER no node target={}", target.class_name()).into(),
            );
            return;
        };
        web_sys::console::log_1(
            &format!(
                "HOVER node={} attr={:?}",
                node.class_name(),
                node.get_attribute("data-node-path")
            )
            .into(),
        );
        let Some(path) = node
            .get_attribute("data-node-path")
            .as_deref()
            .and_then(dom::parse_path)
        else {
            return;
        };
        let Some(block_index) = path.first().copied() else {
            return;
        };
        let Some(block) = content
            .query_selector(&format!("[data-node-path=\"{block_index}\"]"))
            .ok()
            .flatten()
        else {
            return;
        };
        let Some(shell) = hover_shell_ref.get() else {
            return;
        };
        let block_rect = block.get_bounding_client_rect();
        let shell_rect = shell.get_bounding_client_rect();
        let left = (block_rect.left() - shell_rect.left() - 36.0).max(2.0);
        let bar_top = (block_rect.top() - shell_rect.top()).max(0.0) + 2.0;
        hover_style.set(format!("left:{left}px;top:{bar_top}px;"));
        hovered_block.set(Some(vec![block_index]));
    };

    let add_block = {
        let editor = editor.clone();
        let hovered = hovered_block;
        let content = content_ref;
        move |_| {
            let Some(path) = hovered.get_untracked() else {
                return;
            };
            let Some(block_type) = editor
                .state
                .with_untracked(|state| Some(state.schema.default_block.clone()))
            else {
                return;
            };
            if let Some(root) = content.get() {
                let _ = root.focus();
            }
            editor.update(|state| {
                state.insert_block_after(&path, Node::Element(SlateElement::empty(&block_type)));
            });
            hovered.set(None);
        }
    };

    let duplicate_block = {
        let editor = editor.clone();
        let hovered = hovered_block;
        let content = content_ref;
        move |_| {
            let Some(path) = hovered.get_untracked() else {
                return;
            };
            if let Some(root) = content.get() {
                let _ = root.focus();
            }
            if editor.has_node_selection() && editor.is_node_selected(&path) {
                editor.duplicate_node_selection();
                hovered.set(None);
                return;
            }
            let node = editor
                .state
                .with_untracked(|state| state.node(&path).cloned());
            let Some(node) = node else {
                return;
            };
            editor.update(|state| {
                state.insert_block_after(&path, node);
            });
            hovered.set(None);
        }
    };

    let delete_block = {
        let editor = editor.clone();
        let hovered = hovered_block;
        let content = content_ref;
        move |_| {
            let Some(path) = hovered.get_untracked() else {
                return;
            };
            if let Some(root) = content.get() {
                let _ = root.focus();
            }
            if editor.has_node_selection() && editor.is_node_selected(&path) {
                editor.delete_node_selection();
            } else {
                editor.update(|state| {
                    state.remove_block_at(&path);
                });
            }
            hovered.set(None);
        }
    };

    let select_block = {
        let editor = editor.clone();
        let hovered = hovered_block;
        let content = content_ref;
        move |ev: MouseEvent| {
            ev.prevent_default();
            let Some(path) = hovered.get_untracked() else {
                return;
            };
            if let Some(root) = content.get() {
                let _ = root.focus();
            }
            if ev.shift_key() {
                editor.extend_node_selection_to(&path);
            } else {
                editor.select_node(&path);
            }
            hovered.set(None);
        }
    };

    let drag_start = {
        let hovered = hovered_block;
        move |ev: DragEvent| {
            let Some(path) = hovered.get_untracked() else {
                return;
            };
            let Some(index) = path.first().copied() else {
                return;
            };
            drag_block.set(Some(index));
            if let Some(data) = ev.data_transfer() {
                data.set_effect_allowed("move");
                let _ = data.set_data("text/plain", "block");
            }
        }
    };

    let drag_end = move |_ev: DragEvent| {
        drag_block.set(None);
        drop_index.set(None);
    };

    let move_block_key = {
        let editor = editor.clone();
        let hovered = hovered_block;
        move |ev: KeyboardEvent| {
            let direction = match ev.key().as_str() {
                "ArrowUp" => -1isize,
                "ArrowDown" => 1,
                _ => return,
            };
            ev.prevent_default();
            let Some(path) = hovered.get_untracked() else {
                return;
            };
            let Some(from) = path.first().copied() else {
                return;
            };
            let to = from as isize + direction;
            if to < 0 {
                return;
            }
            let to = to as usize;
            editor.update(|state| {
                state.move_block(from, to);
            });
            hovered.set(None);
        }
    };

    let mousedown_editor = editor.clone();
    let handle_mousedown = move |_ev: MouseEvent| {
        mousedown_editor.clear_node_selection();
    };

    let click_ref = content_ref;
    let handle_click = move |_ev: MouseEvent| {
        // `contenteditable="false"` children do not always focus the editor on
        // click. Focus after the click so we do not retarget the click itself
        // (which happens when focus causes the page to scroll).
        if let Some(root) = click_ref.get() {
            let _ = root.focus();
        }
    };

    let mouseup_editor = editor.clone();
    let mouseup_ref = content_ref;
    let handle_mouseup = move |_ev: MouseEvent| {
        if let Some(root) = mouseup_ref.get() {
            sync_from_dom(&mouseup_editor, root.unchecked_ref());
        }
    };

    let keyup_editor = editor.clone();
    let keyup_ref = content_ref;
    let keyup_composing = composing;
    let handle_keyup = move |_ev: KeyboardEvent| {
        if keyup_composing.get_untracked() {
            return;
        }
        if let Some(root) = keyup_ref.get() {
            sync_from_dom(&keyup_editor, root.unchecked_ref());
        }
    };

    let input_editor = editor.clone();
    let input_ref = content_ref;
    let input_composing = composing;
    let handle_input = move |_ev: web_sys::Event| {
        if input_composing.get_untracked() {
            return;
        }
        let Some(root) = input_ref.get() else {
            return;
        };
        let root: &Element = root.unchecked_ref();

        // In case a browser mutated the DOM without a handled `beforeinput`,
        // reconcile the focused text node with the model.
        if let Some((path, dom_text)) = dom::focused_text_span(root) {
            let model_text = input_editor.read(|editor| {
                editor
                    .text(&path)
                    .map(|text| text.text.clone())
                    .unwrap_or_default()
            });
            if model_text != dom_text {
                input_editor.update(|editor| editor.set_text_at_path(&path, &dom_text));
            }
        }
        sync_from_dom(&input_editor, root);
    };

    let composing_start = composing;
    let handle_composition_start = move |_ev: CompositionEvent| {
        composing_start.set(true);
    };

    let composition_editor = editor.clone();
    let composition_ref = content_ref;
    let composition_composing = composing;
    let handle_composition_end = move |_ev: CompositionEvent| {
        composition_composing.set(false);
        let Some(root) = composition_ref.get() else {
            return;
        };
        let root: &Element = root.unchecked_ref();

        if let Some((path, dom_text)) = dom::focused_text_span(root) {
            composition_editor.update(|editor| editor.set_text_at_path(&path, &dom_text));
        }
        sync_from_dom(&composition_editor, root);
    };

    if on_mention.is_some() {
        let mention_editor = editor.clone();
        let mention_handler = on_mention;
        Effect::new(move |_| {
            mention_editor.state.with(|state| {
                let trigger = mention_trigger_at(state);

                if trigger.is_none() && mention_dismissed.get_untracked() {
                    mention_dismissed.set(false);
                }

                if mention_dismissed.get_untracked() {
                    if notified_trigger.get_untracked() != Some(None) {
                        if let Some(handler) = &mention_handler {
                            handler.run(None);
                        }
                        notified_trigger.set(Some(None));
                    }
                    return;
                }

                if notified_trigger.get_untracked() != Some(trigger.clone()) {
                    if let Some(handler) = &mention_handler {
                        handler.run(trigger.clone());
                    }
                    notified_trigger.set(Some(trigger));
                }
            });
        });
    }

    let listener_editor = editor.clone();
    let listener_ref = content_ref;
    let listener_composing = composing;
    Effect::new(move |_| {
        let Some(root) = listener_ref.get() else {
            return;
        };
        if selection_listener.with_value(|listener| listener.is_some()) {
            return;
        }
        let root = SendWrapper::new(root);
        let editor = listener_editor.clone();
        let listener = Closure::<dyn FnMut()>::new(move || {
            if listener_composing.get_untracked() {
                return;
            }
            let root: &Element = root.unchecked_ref();
            if !dom::has_focus(root) {
                return;
            }
            sync_from_dom(&editor, root);
        });
        if let Some(document) = web_sys::window().and_then(|window| window.document()) {
            let _ = document.add_event_listener_with_callback(
                "selectionchange",
                listener.as_ref().unchecked_ref(),
            );
        }
        selection_listener.set_value(Some(SendWrapper::new(listener)));
    });
    on_cleanup(move || {
        selection_listener.with_value(|listener| {
            if let Some(listener) = listener {
                if let Some(document) = web_sys::window().and_then(|window| window.document()) {
                    let _ = document.remove_event_listener_with_callback(
                        "selectionchange",
                        listener.as_ref().unchecked_ref(),
                    );
                }
            }
        });
    });

    let dismiss_hover = {
        let hovered = hovered_block;
        let bubble = bubble_visible;
        let drop = drop_index;
        window_event_listener_untyped("scroll", move |_| {
            hovered.set(None);
            bubble.set(false);
            drop.set(None);
        })
    };
    let dismiss_resize = {
        let hovered = hovered_block;
        let bubble = bubble_visible;
        let drop = drop_index;
        window_event_listener_untyped("resize", move |_| {
            hovered.set(None);
            bubble.set(false);
            drop.set(None);
        })
    };
    on_cleanup(move || {
        dismiss_hover.remove();
        dismiss_resize.remove();
    });

    // Keep the browser caret in sync with the model after reactive renders.
    let effect_editor = editor.clone();
    let effect_shell_ref = shell_ref;
    Effect::new(move |_| {
        effect_editor.state.with(|_| ());
        if composing.get_untracked() {
            bubble_visible.set(false);
            return;
        }
        let Some(root) = content_ref.get() else {
            bubble_visible.set(false);
            return;
        };
        let root: &Element = root.unchecked_ref();
        if !dom::has_focus(root) {
            bubble_visible.set(false);
            return;
        }
        if effect_editor
            .state
            .with_untracked(|editor| editor.has_node_selection())
        {
            bubble_visible.set(false);
            return;
        }
        let selection = effect_editor
            .state
            .with_untracked(|editor| editor.selection.clone());
        if let Some(selection) = &selection {
            if !dom::selection_matches_dom(root, selection) {
                dom::apply_selection_to_dom(root, selection);
            }
        }

        let expanded = selection
            .as_ref()
            .map(|range| !range.is_collapsed())
            .unwrap_or(false);
        if !selection_bubble || !expanded {
            bubble_visible.set(false);
            return;
        }

        let Some(browser_selection) =
            web_sys::window().and_then(|window| window.get_selection().ok().flatten())
        else {
            bubble_visible.set(false);
            return;
        };
        if browser_selection.range_count() == 0 {
            bubble_visible.set(false);
            return;
        }
        let Ok(range) = browser_selection.get_range_at(0) else {
            bubble_visible.set(false);
            return;
        };
        let rect = range.get_bounding_client_rect();
        let Some(shell) = effect_shell_ref.get() else {
            bubble_visible.set(false);
            return;
        };
        let shell_rect = shell.get_bounding_client_rect();
        let center = rect.left() - shell_rect.left() + rect.width() / 2.0;
        let left = center.clamp(86.0, (shell_rect.width() - 86.0).max(86.0));
        let mut top = rect.top() - shell_rect.top() - 44.0;
        if top < 4.0 {
            top = rect.bottom() - shell_rect.top() + 8.0;
        }
        bubble_style.set(format!("left:{left}px;top:{top}px;"));
        bubble_visible.set(true);
    });

    let bubble_bold = {
        let editor = editor.clone();
        move |_| editor.toggle_mark("bold", Value::Bool(true))
    };
    let bubble_italic = {
        let editor = editor.clone();
        move |_| editor.toggle_mark("italic", Value::Bool(true))
    };
    let bubble_underline = {
        let editor = editor.clone();
        move |_| editor.toggle_mark("underline", Value::Bool(true))
    };
    let bubble_strike = {
        let editor = editor.clone();
        move |_| editor.toggle_mark("strikethrough", Value::Bool(true))
    };
    let bubble_code = {
        let editor = editor.clone();
        move |_| editor.toggle_mark("code", Value::Bool(true))
    };
    let bubble_bold_active = {
        let editor = editor.clone();
        move || {
            editor
                .state
                .with(|state| state.has_mark("bold", &Value::Bool(true)))
        }
    };
    let bubble_italic_active = {
        let editor = editor.clone();
        move || {
            editor
                .state
                .with(|state| state.has_mark("italic", &Value::Bool(true)))
        }
    };
    let bubble_underline_active = {
        let editor = editor.clone();
        move || {
            editor
                .state
                .with(|state| state.has_mark("underline", &Value::Bool(true)))
        }
    };
    let bubble_strike_active = {
        let editor = editor.clone();
        move || {
            editor
                .state
                .with(|state| state.has_mark("strikethrough", &Value::Bool(true)))
        }
    };
    let bubble_code_active = {
        let editor = editor.clone();
        move || {
            editor
                .state
                .with(|state| state.has_mark("code", &Value::Bool(true)))
        }
    };
    let bubble_bold_pressed = {
        let editor = editor.clone();
        move || {
            editor
                .state
                .with(|state| state.has_mark("bold", &Value::Bool(true)))
        }
    };
    let bubble_italic_pressed = {
        let editor = editor.clone();
        move || {
            editor
                .state
                .with(|state| state.has_mark("italic", &Value::Bool(true)))
        }
    };
    let bubble_underline_pressed = {
        let editor = editor.clone();
        move || {
            editor
                .state
                .with(|state| state.has_mark("underline", &Value::Bool(true)))
        }
    };
    let bubble_strike_pressed = {
        let editor = editor.clone();
        move || {
            editor
                .state
                .with(|state| state.has_mark("strikethrough", &Value::Bool(true)))
        }
    };
    let bubble_code_pressed = {
        let editor = editor.clone();
        move || {
            editor
                .state
                .with(|state| state.has_mark("code", &Value::Bool(true)))
        }
    };

    let empty_editor = editor.clone();
    let selection_editor = editor.clone();
    let live_editor = editor.clone();
    let live_selection = move || {
        live_editor.state.with(|state| {
            if state.has_node_selection() {
                let count = state.node_selections().len();
                if count > 1 {
                    format!("{count} blocks selected")
                } else {
                    "Block selected".to_string()
                }
            } else {
                String::new()
            }
        })
    };
    let shell_class = if class.is_empty() {
        "ls-editor-shell".to_string()
    } else {
        format!("ls-editor-shell {class}")
    };

    view! {
        <style>{EDITOR_STYLES}</style>
        <div
            node_ref=shell_ref
            class=shell_class
            on:mouseleave=move |_| {
                hovered_block.set(None);
                drag_block.set(None);
                drop_index.set(None);
            }
        >
            <div class="ls-sr-only" aria-live="polite" aria-atomic="true">
                {live_selection}
            </div>
            <div
                node_ref=content_ref
                class="ls-editor"
                contenteditable="true"
                spellcheck="true"
                data-placeholder=placeholder
                data-empty=move || {
                    if empty_editor.state.with(|editor| editor.is_empty()) {
                        "true"
                    } else {
                        "false"
                    }
                }
                on:beforeinput=handle_beforeinput
                on:input=handle_input
                on:mousemove=handle_hover
                on:keydown=handle_keydown
                on:keyup=handle_keyup
                on:mousedown=handle_mousedown
                on:click=handle_click
                on:mouseup=handle_mouseup
                on:paste=handle_paste
                on:copy=handle_copy
                on:cut=handle_cut
                on:dragover=handle_drag_over
                on:dragleave=handle_drag_leave
                on:drop=handle_drop
                class:ls-dragging=move || dragging.get()
                class:ls-node-selected=move || {
                    selection_editor.state.with(|editor| editor.has_node_selection())
                }
                on:compositionstart=handle_composition_start
                on:compositionend=handle_composition_end
            >
                <For
                    each={{
                        let editor = render_editor.clone();
                        move || {
                            editor.state.with(|state| {
                                state
                                    .children
                                    .iter()
                                    .enumerate()
                                    .map(|(index, node)| BlockKey {
                                        index,
                                        hash: node.content_hash(),
                                    })
                                    .collect::<Vec<_>>()
                            })
                        }
                    }}
                    key={|key: &BlockKey| (key.index, key.hash)}
                    children={
                        let renderers = renderers.clone();
                        let editor = render_editor.clone();
                        move |key: BlockKey| {
                            let renderers = renderers.clone();
                            let editor = editor.clone();
                            let index = key.index;
                            let node = editor
                                .state
                                .with_untracked(|state| state.children.get(index).cloned());
                            view! {
                                {move || {
                                    let schema = schema.get();
                                    match &node {
                                        Some(node) => render_node(
                                            &renderers,
                                            &editor,
                                            &schema,
                                            node,
                                            &[index],
                                        ),
                                        None => ().into_any(),
                                    }
                                }}
                            }
                            .into_any()
                        }
                    }
                />
            </div>
            <div
                class="ls-hover-bar"
                role="toolbar"
                aria-label="Block actions"
                class:visible=move || hovered_block.get().is_some()
                style=move || hover_style.get()
                on:mousedown=move |ev: MouseEvent| {
                    if let Some(target) = ev
                        .target()
                        .and_then(|target| target.dyn_into::<Element>().ok())
                    {
                        if target.closest(".ls-drag-handle").ok().flatten().is_some() {
                            return;
                        }
                    }
                    ev.prevent_default();
                }
            >
                <span
                    class="ls-drag-handle"
                    draggable="true"
                    role="button"
                    tabindex="0"
                    title="Drag to move (↑/↓)"
                    aria-label="Drag to move"
                    on:dragstart=drag_start
                    on:dragend=drag_end
                    on:keydown=move_block_key
                >
                    "⠿"
                </span>
                <button
                    type="button"
                    title="Select block"
                    aria-label="Select block"
                    on:click=select_block
                >
                    "▣"
                </button>
                <button
                    type="button"
                    title="Add block below"
                    aria-label="Add block below"
                    on:click=add_block
                >
                    "+"
                </button>
                <button
                    type="button"
                    title="Duplicate block"
                    aria-label="Duplicate block"
                    on:click=duplicate_block
                >
                    "⧉"
                </button>
                <button
                    type="button"
                    title="Delete block"
                    aria-label="Delete block"
                    on:click=delete_block
                >
                    "✕"
                </button>
            </div>
            <div
                class="ls-drop-indicator"
                class:visible=move || drop_index.get().is_some()
                style=move || drop_style.get()
            ></div>
            <div
                class="ls-selection-bubble"
                role="toolbar"
                aria-label="Text formatting"
                class:visible=move || bubble_visible.get()
                style=move || bubble_style.get()
                on:mousedown=move |ev| ev.prevent_default()
            >
                <button
                    type="button"
                    class="ls-bubble-mark"
                    class:active=bubble_bold_active
                    aria-pressed=move || if bubble_bold_pressed() { "true" } else { "false" }
                    title="Bold"
                    aria-label="Bold"
                    on:click=bubble_bold
                >
                    "B"
                </button>
                <button
                    type="button"
                    class="ls-bubble-mark"
                    class:active=bubble_italic_active
                    aria-pressed=move || if bubble_italic_pressed() { "true" } else { "false" }
                    title="Italic"
                    aria-label="Italic"
                    on:click=bubble_italic
                >
                    "I"
                </button>
                <button
                    type="button"
                    class="ls-bubble-mark"
                    class:active=bubble_underline_active
                    aria-pressed=move || if bubble_underline_pressed() { "true" } else { "false" }
                    title="Underline"
                    aria-label="Underline"
                    on:click=bubble_underline
                >
                    "U"
                </button>
                <button
                    type="button"
                    class="ls-bubble-mark"
                    class:active=bubble_strike_active
                    aria-pressed=move || if bubble_strike_pressed() { "true" } else { "false" }
                    title="Strikethrough"
                    aria-label="Strikethrough"
                    on:click=bubble_strike
                >
                    "S"
                </button>
                <span class="ls-bubble-divider"></span>
                <button
                    type="button"
                    class="ls-bubble-mark"
                    class:active=bubble_code_active
                    aria-pressed=move || if bubble_code_pressed() { "true" } else { "false" }
                    title="Inline code"
                    aria-label="Inline code"
                    on:click=bubble_code
                >
                    "<>"
                </button>
            </div>
        </div>
    }
}

fn block_drop_target(
    content: &Element,
    shell: &Element,
    event: &DragEvent,
    count: usize,
) -> Option<(usize, String)> {
    let target = event
        .target()
        .and_then(|target| target.dyn_into::<Element>().ok());
    let block = target.and_then(|target| target.closest("[data-node-path]").ok().flatten());

    let shell_rect = shell.get_bounding_client_rect();
    let (boundary, line_y, left, width) = if let Some(block) = block {
        let path = block
            .get_attribute("data-node-path")
            .as_deref()
            .and_then(dom::parse_path)?;
        let index = *path.first()?;
        let block = content
            .query_selector(&format!("[data-node-path=\"{index}\"]"))
            .ok()
            .flatten()
            .unwrap_or(block);
        let rect = block.get_bounding_client_rect();
        let after = event.client_y() as f64 > rect.top() + rect.height() / 2.0;
        (
            index + usize::from(after),
            if after { rect.bottom() } else { rect.top() },
            rect.left(),
            rect.width(),
        )
    } else {
        let rect = content.get_bounding_client_rect();
        (count, rect.bottom(), rect.left(), rect.width())
    };

    let boundary = boundary.min(count);
    let style = format!(
        "left:{}px;width:{}px;top:{}px;",
        left - shell_rect.left(),
        width,
        line_y - shell_rect.top()
    );
    Some((boundary, style))
}

fn mention_trigger_at(editor: &Editor) -> Option<MentionTrigger> {
    let range = editor.selection.as_ref()?;
    if !range.is_collapsed() {
        return None;
    }
    let point = range.anchor.clone();
    let text = editor.text(&point.path)?;
    let chars: Vec<char> = text.text.chars().take(point.offset).collect();

    let mut start = None;
    for index in (0..chars.len()).rev() {
        let ch = chars[index];
        if ch == '@' {
            start = Some(index);
            break;
        }
        if ch.is_whitespace() {
            break;
        }
    }

    let start = start?;
    if start > 0 && chars[start - 1].is_alphanumeric() {
        return None;
    }
    if point.path.len() < 2 {
        return None;
    }

    let parent = &point.path[..point.path.len() - 1];
    if editor
        .element(parent)
        .map(|element| editor.schema.is_inline(&element.element_type))
        .unwrap_or(false)
    {
        return None;
    }

    let block = editor.leaf_block_path(&point)?;
    if editor
        .element(&block)
        .map(|element| element.element_type == "code")
        .unwrap_or(false)
    {
        return None;
    }

    let query: String = chars[start + 1..].iter().collect();
    Some(MentionTrigger {
        query,
        range: Range::new(
            Point::new(point.path.clone(), start),
            Point::new(point.path.clone(), point.offset),
        ),
    })
}

fn collect_files(files: &web_sys::FileList) -> Vec<web_sys::File> {
    (0..files.length())
        .filter_map(|index| files.get(index))
        .collect()
}

fn selection_html(editor: &EditorHandle, root: &Element) -> Option<String> {
    let paths = editor.state.with_untracked(|state| state.node_selections());
    if paths.is_empty() {
        return None;
    }
    let html = paths
        .iter()
        .filter_map(|path| {
            root.query_selector(&format!("[data-node-path=\"{}\"]", dom::path_string(path)))
                .ok()
                .flatten()
        })
        .map(|element| element.outer_html())
        .collect::<Vec<_>>()
        .join("");
    (!html.trim().is_empty()).then_some(html)
}

fn model_marker_html(model: &str, html: &str) -> String {
    format!(
        "<div data-leptoslate-model=\"{}\">{}</div>",
        hex_encode(model.as_bytes()),
        html
    )
}

fn extract_model_marker(html: &str) -> Option<String> {
    let marker = "data-leptoslate-model=\"";
    let start = html.find(marker)? + marker.len();
    let end = html[start..].find('"')? + start;
    let bytes = hex_decode(&html[start..end])?;
    String::from_utf8(bytes).ok()
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char);
    }
    out
}

fn hex_decode(value: &str) -> Option<Vec<u8>> {
    if value.len() % 2 != 0 {
        return None;
    }
    value
        .as_bytes()
        .chunks(2)
        .map(|pair| {
            let high = hex_digit(pair[0])?;
            let low = hex_digit(pair[1])?;
            Some((high << 4) | low)
        })
        .collect()
}

fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn selection_model_json(editor: &EditorHandle) -> Option<String> {
    let nodes = editor.state.with_untracked(|state| {
        state
            .node_selections()
            .iter()
            .filter_map(|path| state.node(path).cloned())
            .collect::<Vec<_>>()
    });
    (!nodes.is_empty())
        .then(|| serde_json::to_string(&nodes).ok())
        .flatten()
}

fn sync_from_dom(editor: &EditorHandle, root: &Element) {
    let Some(range) = dom::selection_from_dom(root) else {
        return;
    };
    editor.state.update(|state| {
        if state.has_node_selection() {
            return;
        }
        if state.selection.as_ref() != Some(&range) {
            state.selection = Some(range);
            state.marks = None;
            state.history.break_coalescing();
        }
    });
}

/// A convenience for toolbar buttons that should not steal editor focus.
pub fn keep_selection(ev: &web_sys::MouseEvent) {
    ev.prevent_default();
}

/// Move the caret to a specific document point (used by custom widgets).
pub fn select_point(editor: &EditorHandle, point: Point) {
    editor.update(|state| state.select_point(point));
}

#[cfg(test)]
mod tests {
    use super::mention_trigger_at;
    use leptoslate_core::{Editor, Element, Node, Point, Text};

    fn paragraph(text: &str) -> Node {
        Node::Element(Element::paragraph(vec![Node::Text(Text::new(text))]))
    }

    #[test]
    fn detects_a_mention_query_under_the_caret() {
        let mut editor = Editor::new(vec![paragraph("hello @lep")]);
        editor.select_point(Point::new(vec![0, 0], 10));
        let trigger = mention_trigger_at(&editor).expect("trigger");
        assert_eq!(trigger.query, "lep");
        assert_eq!(
            trigger.range,
            leptoslate_core::Range::new(Point::new(vec![0, 0], 6), Point::new(vec![0, 0], 10))
        );
    }

    #[test]
    fn ignores_email_like_at_signs_and_whitespace() {
        let mut editor = Editor::new(vec![paragraph("a@b")]);
        editor.select_point(Point::new(vec![0, 0], 3));
        assert!(mention_trigger_at(&editor).is_none());

        let mut editor = Editor::new(vec![paragraph("@lep done")]);
        editor.select_point(Point::new(vec![0, 0], 9));
        assert!(mention_trigger_at(&editor).is_none());
    }

    #[test]
    fn ignores_code_blocks() {
        let mut editor = Editor::new(vec![Node::Element(Element::new(
            "code",
            vec![Node::Text(Text::new("@lep"))],
        ))]);
        editor.select_point(Point::new(vec![0, 0], 4));
        assert!(mention_trigger_at(&editor).is_none());
    }
}
