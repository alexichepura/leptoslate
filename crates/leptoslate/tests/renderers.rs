use leptos::prelude::*;
use leptoslate::Renderers;

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
