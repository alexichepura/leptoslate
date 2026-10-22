use leptos::prelude::*;
use leptoslate::{FileHandler, Renderers};

#[test]
fn custom_element_renderers_can_be_registered() {
    let _renderers = Renderers::new()
        .element("callout", |ctx| {
            let class = ctx.class_with("callout");
            let node_path = ctx.node_path;
            let children = ctx.children;
            view! {
                <aside class=class data-node-path=node_path>
                    {children}
                </aside>
            }
            .into_any()
        })
        .element("todo", |ctx| {
            let children = ctx.children;
            view! { <div class="todo">{children}</div> }.into_any()
        });
}

#[test]
fn custom_text_and_mark_renderers_can_be_registered() {
    let _renderers = Renderers::new()
        .text(|ctx| {
            let class = ctx.class;
            let text_path = ctx.text_path;
            let style = ctx.style;
            let text = ctx.text.text;
            view! {
                <span class=class data-text-path=text_path style=style>
                    {text}
                </span>
            }
            .into_any()
        })
        .mark_style("highlight", |value| {
            format!(
                "background:{}",
                value.as_str().unwrap_or("rgba(255, 230, 0, .35)")
            )
        });
}

#[test]
fn file_handlers_are_cloneable() {
    let handler = FileHandler::new(|_files| {});
    let _clone = handler.clone();
}
