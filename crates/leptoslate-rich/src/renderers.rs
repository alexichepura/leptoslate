//! Rich preset renderers and CSS for lists and tables.
//!
//! The base `Editable` renderer is intentionally minimal. This module provides
//! the semantic list and table markup used by the rich preset and owns the
//! matching CSS.

use leptos::prelude::*;
use leptoslate::{ElementRenderContext, Renderers};

/// CSS owned by the rich preset for list and table markup.
pub const RICH_STYLES: &str = r#"
.ls-block.ls-list {
    padding-left: 1.5em;
    margin: 0.25em 0;
}

.ls-block.ls-list-item {
    display: list-item;
}

.ls-table-wrap {
    max-width: 100%;
    overflow-x: auto;
    margin: 0.75em 0;
}

.ls-table {
    width: 100%;
    border-collapse: collapse;
    display: table;
}

.ls-table-head {
    display: table-header-group;
}

.ls-table-body {
    display: table-row-group;
}

.ls-table-foot {
    display: table-footer-group;
}

.ls-table-row {
    display: table-row;
}

.ls-table-cell {
    display: table-cell;
}

.ls-table-wrap th,
.ls-table-wrap td {
    padding: 0.4em 0.6em;
    border: 1px solid rgba(17, 24, 39, 0.15);
    text-align: left;
    vertical-align: top;
}

.ls-table-wrap th {
    background: rgba(127, 127, 127, 0.08);
}
"#;

/// Renderers for the rich preset's list and table elements.
///
/// Combine with application renderers through [`Renderers::merge`] so custom
/// renderers still win:
///
/// ```rust,ignore
/// let renderers = rich_renderers().merge(custom_renderers);
/// ```
pub fn rich_renderers() -> Renderers {
    Renderers::new()
        .element("ul", rich_element)
        .element("ol", rich_element)
        .element("li", rich_element)
        .element("table", rich_element)
        .element("thead", rich_element)
        .element("tbody", rich_element)
        .element("tfoot", rich_element)
        .element("tr", rich_element)
        .element("th", rich_element)
        .element("td", rich_element)
}

fn rich_element(ctx: ElementRenderContext) -> AnyView {
    let ElementRenderContext {
        element,
        node_path,
        class,
        children,
        ..
    } = ctx;
    let element_type = element.element_type.clone();

    macro_rules! tag {
        ($name:ident, $class:expr) => {{
            let class = $class;
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
        }};
    }

    match element_type.as_str() {
        "ul" | "ol" => {
            let class = format!("{class} ls-list");
            if element_type == "ol" {
                tag!(ol, class)
            } else {
                tag!(ul, class)
            }
        }
        "li" => tag!(li, format!("{class} ls-list-item")),
        "table" => {
            let class = "ls-table".to_string();
            view! {
                <div class="ls-table-wrap">
                    <table
                        class=class
                        data-node-path=node_path
                        data-type=element_type
                    >
                        {children}
                    </table>
                </div>
            }
            .into_any()
        }
        "thead" => tag!(thead, "ls-table-head".to_string()),
        "tbody" => tag!(tbody, "ls-table-body".to_string()),
        "tfoot" => tag!(tfoot, "ls-table-foot".to_string()),
        "tr" => tag!(tr, "ls-table-row".to_string()),
        "th" => tag!(th, "ls-table-cell ls-table-heading".to_string()),
        "td" => tag!(td, "ls-table-cell".to_string()),
        _ => tag!(div, class),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rich_renderers_cover_lists_and_tables() {
        let renderers = rich_renderers();
        for element_type in [
            "ul", "ol", "li", "table", "thead", "tbody", "tfoot", "tr", "th", "td",
        ] {
            assert!(
                renderers.has_element(element_type),
                "missing renderer for {element_type}"
            );
        }
    }

    #[test]
    fn rich_styles_own_table_and_list_css() {
        assert!(RICH_STYLES.contains(".ls-table"));
        assert!(RICH_STYLES.contains(".ls-block.ls-list"));
        assert!(RICH_STYLES.contains(".ls-block.ls-list-item"));
        assert!(!leptoslate::EDITOR_STYLES.contains(".ls-table-wrap"));
    }

    #[test]
    fn user_renderers_override_rich_defaults() {
        let renderers = rich_renderers().merge(Renderers::new().element("table", |ctx| {
            let children = ctx.children;
            view! { <div class="custom-table">{children}</div> }.into_any()
        }));
        assert!(renderers.has_element("table"));
        assert!(renderers.has_element("ul"));
    }
}
