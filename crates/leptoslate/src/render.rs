//! Schema-driven rendering of Slate nodes to Leptos views.

use leptos::prelude::*;
use leptoslate_core::{
    props::{get_bool, get_str},
    Element, Node, Props, Schema, Text,
};
use web_sys::MouseEvent;

use crate::{
    dom::path_string,
    handle::EditorHandle,
    renderers::{ElementRenderContext, Renderers},
};

pub(crate) fn render_nodes(
    renderers: &Renderers,
    editor: &EditorHandle,
    schema: &Schema,
    nodes: &[Node],
    parent: &[usize],
) -> Vec<AnyView> {
    nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let mut path = parent.to_vec();
            path.push(index);
            render_node(renderers, editor, schema, node, &path)
        })
        .collect()
}

pub(crate) fn render_node(
    renderers: &Renderers,
    editor: &EditorHandle,
    schema: &Schema,
    node: &Node,
    path: &[usize],
) -> AnyView {
    match node {
        Node::Text(text) => render_text(text, path),
        Node::Element(element) => render_element(renderers, editor, schema, element, path),
    }
}

fn render_text(text: &Text, path: &[usize]) -> AnyView {
    let style = mark_style(&text.properties);
    let class = "ls-text";
    let path = path_string(path);
    view! {
        <span class=class data-text-path=path style=style>{text.text.clone()}</span>
    }
    .into_any()
}

fn render_element(
    renderers: &Renderers,
    editor: &EditorHandle,
    schema: &Schema,
    element: &Element,
    path: &[usize],
) -> AnyView {
    let rule = schema
        .rule(&element.element_type)
        .cloned()
        .unwrap_or_else(|| leptoslate_core::ElementRule::block("div"));

    let node_path = path_string(path);
    let element_type = element.element_type.clone();

    let children: Vec<AnyView> = if rule.void {
        Vec::new()
    } else {
        element
            .children
            .iter()
            .enumerate()
            .map(|(index, child)| {
                let mut child_path = path.to_vec();
                child_path.push(index);
                render_node(renderers, editor, schema, child, &child_path)
            })
            .collect()
    };

    let mut class = if rule.inline {
        format!("ls-inline ls-inline-{}", element.element_type)
    } else {
        format!("ls-block ls-block-{}", element.element_type)
    };
    if rule.void {
        class.push_str(" ls-void");
    }

    if let Some(renderer) = renderers.get(&element.element_type) {
        return renderer(ElementRenderContext {
            element: element.clone(),
            path: path.to_vec(),
            node_path,
            class,
            children,
            editor: editor.clone(),
            is_inline: rule.inline,
            is_void: rule.void,
        });
    }

    if rule.void {
        return render_void(editor, element, path, &node_path, class);
    }

    let tag = rule.tag.clone();
    if rule.inline {
        return render_inline(
            &tag,
            &element.element_type,
            class,
            node_path,
            children,
            &element.properties,
        );
    }

    render_block(&tag, class, node_path, element_type, children)
}

fn render_block(
    tag: &str,
    class: String,
    node_path: String,
    element_type: String,
    children: Vec<AnyView>,
) -> AnyView {
    macro_rules! block {
        ($name:ident) => {
            view! {
                <$name
                    class=class
                    data-node-path=node_path
                    data-type=element_type
                >
                    {children}
                </$name>
            }
            .into_any()
        };
    }

    match tag {
        "p" => block!(p),
        "h1" => block!(h1),
        "h2" => block!(h2),
        "h3" => block!(h3),
        "h4" => block!(h4),
        "h5" => block!(h5),
        "h6" => block!(h6),
        "blockquote" => block!(blockquote),
        "pre" => block!(pre),
        "ul" => block!(ul),
        "ol" => block!(ol),
        "li" => block!(li),
        "section" => block!(section),
        "article" => block!(article),
        "figure" => block!(figure),
        "figcaption" => block!(figcaption),
        "header" => block!(header),
        "footer" => block!(footer),
        _ => block!(div),
    }
}

fn render_inline(
    tag: &str,
    element_type: &str,
    class: String,
    node_path: String,
    children: Vec<AnyView>,
    properties: &Props,
) -> AnyView {
    let data_type = element_type.to_string();
    macro_rules! inline {
        ($name:ident) => {
            view! {
                <$name
                    class=class
                    data-node-path=node_path
                    data-type=data_type
                >
                    {children}
                </$name>
            }
            .into_any()
        };
    }

    match tag {
        "a" => {
            let href = get_str(properties, "href").unwrap_or("#").to_string();
            let prevent_navigation = move |ev: MouseEvent| ev.prevent_default();
            view! {
                <a
                    class=class
                    href=href
                    data-node-path=node_path
                    data-type=data_type
                    on:click=prevent_navigation
                >
                    {children}
                </a>
            }
            .into_any()
        }
        "strong" => inline!(strong),
        "em" => inline!(em),
        "code" => inline!(code),
        "mark" => inline!(mark),
        _ => inline!(span),
    }
}

fn render_void(
    editor: &EditorHandle,
    element: &Element,
    path: &[usize],
    node_path: &str,
    base_class: String,
) -> AnyView {
    let element_type = element.element_type.clone();
    let class_editor = editor.clone();
    let class_path = path.to_vec();
    let class = move || {
        if class_editor.is_node_selected(&class_path) {
            format!("{base_class} ls-selected")
        } else {
            base_class.clone()
        }
    };
    let click_editor = editor.clone();
    let click_path = path.to_vec();
    let select = move |ev: MouseEvent| {
        ev.prevent_default();
        click_editor.select_node(&click_path);
    };

    match element_type.as_str() {
        "img" => {
            let src = get_str(&element.properties, "src")
                .unwrap_or("")
                .to_string();
            let alt = get_str(&element.properties, "alt")
                .unwrap_or("")
                .to_string();
            view! {
                <img
                    class=class
                    src=src
                    alt=alt
                    data-node-path=node_path
                    data-type=element_type
                    contenteditable="false"
                    draggable="false"
                    on:click=select
                />
            }
            .into_any()
        }
        "hr" => view! {
            <hr
                class=class
                data-node-path=node_path
                data-type=element_type
                contenteditable="false"
                on:click=select
            />
        }
        .into_any(),
        _ => view! {
            <div
                class=class
                data-node-path=node_path
                data-type=element_type
                contenteditable="false"
                on:click=select
            ></div>
        }
        .into_any(),
    }
}

fn mark_style(properties: &Props) -> String {
    let mut styles: Vec<String> = Vec::new();

    if get_bool(properties, "bold") {
        styles.push("font-weight:700".into());
    }
    if get_bool(properties, "italic") {
        styles.push("font-style:italic".into());
    }
    if get_bool(properties, "underline") {
        styles.push("text-decoration-line:underline".into());
    }
    if get_bool(properties, "strikethrough") {
        styles.push("text-decoration-line:line-through".into());
    }
    if get_bool(properties, "code") {
        styles.push("font-family:ui-monospace,SFMono-Regular,Menlo,monospace".into());
        styles.push("background:rgba(127,127,127,.14)".into());
        styles.push("border-radius:3px".into());
        styles.push("padding:0 2px".into());
    }
    if get_bool(properties, "superscript") {
        styles.push("vertical-align:super".into());
        styles.push("font-size:.75em".into());
    }
    if get_bool(properties, "subscript") {
        styles.push("vertical-align:sub".into());
        styles.push("font-size:.75em".into());
    }
    if let Some(color) = get_str(properties, "color") {
        if safe_color(color) {
            styles.push(format!("color:{color}"));
        }
    }
    if let Some(background) = get_str(properties, "background") {
        if safe_color(background) {
            styles.push(format!("background:{background}"));
        }
    }

    styles.join(";")
}

fn safe_color(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.chars().all(|ch| {
            ch.is_ascii_alphanumeric()
                || matches!(ch, '#' | '(' | ')' | ',' | '.' | '%' | ' ' | '-' | '_')
        })
}
