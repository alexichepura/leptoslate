//! The `Editable` component: contenteditable DOM, events and selection sync.

use leptos::html::Div;
use leptos::prelude::*;
use leptoslate_core::{Editor, Point, Range};
use serde_json::Value;
use wasm_bindgen::JsCast;
use web_sys::{
    ClipboardEvent, CompositionEvent, DragEvent, Element, InputEvent, KeyboardEvent, MouseEvent,
};

use crate::{
    dom,
    handle::EditorHandle,
    media::FileHandler,
    mention::{MentionHandler, MentionTrigger},
    render::render_nodes,
    renderers::Renderers,
    EDITOR_STYLES,
};

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
) -> impl IntoView {
    let content_ref = NodeRef::<Div>::new();
    let composing = RwSignal::new(false);
    let dragging = RwSignal::new(false);
    let mention_dismissed = RwSignal::new(false);
    let notified_trigger = RwSignal::new(None::<Option<MentionTrigger>>);

    let view_editor = editor.clone();
    let render_editor = editor.clone();
    let document = Memo::new(move |_| {
        view_editor
            .state
            .with(|state| (state.schema.clone(), state.children.clone()))
    });

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

        if keydown_editor.has_node_selection() {
            match key.as_str() {
                "Backspace" | "Delete" | "Enter" => {
                    ev.prevent_default();
                    keydown_editor.delete_node_selection();
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
                _ => {}
            }
            return;
        }

        if key == "Tab" {
            ev.prevent_default();
            keydown_editor.insert_text("    ");
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
    let drop_files = on_files.clone();
    let handle_drag_over = move |ev: DragEvent| {
        ev.prevent_default();
        dragging.set(true);
    };
    let handle_drag_leave = move |ev: DragEvent| {
        ev.prevent_default();
        dragging.set(false);
    };
    let handle_drop = move |ev: DragEvent| {
        ev.prevent_default();
        dragging.set(false);
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
        if files.is_empty() {
            return;
        }
        if let Some(handler) = &drop_files {
            handler.call(files);
        }
    };

    let copy_editor = editor.clone();
    let copy_ref = content_ref;
    let handle_copy = move |ev: ClipboardEvent| {
        let Some(root) = copy_ref.get() else {
            return;
        };
        sync_from_dom(&copy_editor, root.unchecked_ref());
        let text = copy_editor.selection_string().unwrap_or_default();
        if let Some(data) = ev.clipboard_data() {
            let _ = data.set_data("text/plain", &text);
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
        let text = cut_editor.selection_string().unwrap_or_default();
        if let Some(data) = ev.clipboard_data() {
            let _ = data.set_data("text/plain", &text);
        }
        ev.prevent_default();
        cut_editor.delete_selection();
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

    // Keep the browser caret in sync with the model after reactive renders.
    let effect_editor = editor.clone();
    Effect::new(move |_| {
        effect_editor.state.with(|_| ());
        if composing.get_untracked() {
            return;
        }
        let Some(root) = content_ref.get() else {
            return;
        };
        let root: &Element = root.unchecked_ref();
        if !dom::has_focus(root) {
            return;
        }
        if effect_editor
            .state
            .with_untracked(|editor| editor.has_node_selection())
        {
            return;
        }
        let selection = effect_editor
            .state
            .with_untracked(|editor| editor.selection.clone());
        if let Some(selection) = selection {
            if !dom::selection_matches_dom(root, &selection) {
                dom::apply_selection_to_dom(root, &selection);
            }
        }
    });

    let empty_editor = editor.clone();
    let selection_editor = editor.clone();
    let shell_class = if class.is_empty() {
        "ls-editor-shell".to_string()
    } else {
        format!("ls-editor-shell {class}")
    };

    view! {
        <style>{EDITOR_STYLES}</style>
        <div class=shell_class>
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
                {move || {
                    let (schema, children) = document.get();
                    render_nodes(
                        &renderers,
                        &render_editor,
                        &schema,
                        &children,
                        &[],
                    )
                }}
            </div>
        </div>
    }
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
