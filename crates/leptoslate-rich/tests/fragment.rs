//! HTML and Markdown parser tests for the rich preset.

use leptoslate::Node;
use leptoslate_rich::fragment::{parse_html, parse_markdown};
use leptoslate_rich::PasteTransforms;

#[test]
fn parses_html_fragments_into_slate_nodes() {
    let nodes = parse_html(
        r#"<h2>Heading</h2>
        <p>Hello <strong>bold</strong> and <a href="https://leptos.dev">a link</a>.</p>
        <ul><li>One</li><li>Two</li></ul>"#,
    );

    assert_eq!(nodes.len(), 3);
    assert_eq!(nodes[0].element_type(), Some("h2"));
    assert_eq!(nodes[0].string(), "Heading");

    let paragraph = nodes[1].as_element().unwrap();
    assert_eq!(paragraph.element_type, "p");
    assert!(paragraph.children.iter().any(|child| {
        child
            .as_text()
            .map(|text| text.text == "bold" && text.properties.get("bold").is_some())
            .unwrap_or(false)
    }));
    assert!(paragraph.children.iter().any(|child| {
        child
            .element_type()
            .map(|kind| kind == "a")
            .unwrap_or(false)
    }));

    let list = nodes[2].as_element().unwrap();
    assert_eq!(list.element_type, "ul");
    assert_eq!(list.children.len(), 2);
    assert_eq!(list.children[0].string(), "One");
    assert_eq!(list.children[1].string(), "Two");
}

#[test]
fn html_spans_preserve_copy_marks() {
    let nodes = parse_html(
        r#"<p><span data-marks="bold,italic">marked</span><span style="text-decoration-line:underline;color:#b4532a">styled</span></p>"#,
    );
    let paragraph = &nodes[0];
    let marked = paragraph.children()[0].as_text().unwrap();
    assert_eq!(marked.text, "marked");
    assert_eq!(
        marked.properties.get("bold"),
        Some(&serde_json::json!(true))
    );
    assert_eq!(
        marked.properties.get("italic"),
        Some(&serde_json::json!(true))
    );

    let styled = paragraph.children()[1].as_text().unwrap();
    assert_eq!(
        styled.properties.get("underline"),
        Some(&serde_json::json!(true))
    );
    assert_eq!(
        styled.properties.get("color"),
        Some(&serde_json::json!("#b4532a"))
    );
}

#[test]
fn html_paste_preserves_nested_lists_tables_and_inline_images() {
    let nodes = parse_html(
        r#"<ul><li>Parent<ul><li>Child</li></ul></li></ul>
        <table><tr><th>Name</th><th>Value</th></tr><tr><td>A</td><td>1</td></tr></table>
        <p>Before <img src="data:image/png;base64,abc" alt="inline"> after</p>"#,
    );

    let list = &nodes[0];
    assert_eq!(list.element_type(), Some("ul"));
    let item = &list.children()[0];
    assert_eq!(item.element_type(), Some("li"));
    assert_eq!(item.string(), "ParentChild");
    assert_eq!(item.children().len(), 2);
    assert_eq!(item.children()[0].string(), "Parent");
    assert_eq!(item.children()[1].element_type(), Some("ul"));

    let table = &nodes[1];
    assert_eq!(table.element_type(), Some("table"));
    assert_eq!(table.children()[0].element_type(), Some("thead"));
    let header_row = &table.children()[0].children()[0];
    assert_eq!(header_row.element_type(), Some("tr"));
    assert_eq!(header_row.children()[0].string(), "Name");
    assert_eq!(header_row.children()[0].element_type(), Some("th"));
    let body_row = &table.children()[1].children()[0];
    assert_eq!(body_row.children()[0].string(), "A");
    assert_eq!(body_row.children()[1].string(), "1");

    let paragraph = &nodes[2];
    assert!(paragraph.children().iter().any(|child| {
        child
            .as_element()
            .map(|element| {
                element.element_type == "inline-image"
                    && element.properties.get("src")
                        == Some(&serde_json::json!("data:image/png;base64,abc"))
            })
            .unwrap_or(false)
    }));
}

#[test]
fn html_tables_preserve_inline_marks_and_links() {
    let nodes = parse_html(
        r#"<table><thead><tr><th>Model</th></tr></thead><tbody><tr><td><strong>Example</strong> <a href="/zeekr">001</a></td></tr></tbody></table>"#,
    );
    let table = &nodes[0];
    assert_eq!(table.element_type(), Some("table"));
    assert_eq!(table.children()[0].element_type(), Some("thead"));
    let cell = &table.children()[1].children()[0].children()[0];
    assert_eq!(cell.element_type(), Some("td"));
    assert!(cell.children().iter().any(|child| {
        child
            .as_text()
            .map(|text| text.text == "Example" && text.properties.get("bold").is_some())
            .unwrap_or(false)
    }));
    let link = cell.children()[2].as_element().unwrap();
    assert_eq!(link.element_type, "a");
    assert_eq!(
        link.properties.get("href"),
        Some(&serde_json::json!("/zeekr"))
    );
    assert_eq!(link.children[0].string(), "001");
}

#[test]
fn parses_markdown_fragments_into_slate_nodes() {
    let nodes = parse_markdown(
        "# Title\n\nHello **bold** and [link](https://leptos.dev).\n\n- one\n- two\n\n> quote\n\n```rust\nlet x = 1;\n```",
    );

    assert_eq!(nodes.len(), 5);
    assert_eq!(nodes[0].element_type(), Some("h1"));
    assert_eq!(nodes[0].string(), "Title");
    assert_eq!(nodes[1].element_type(), Some("p"));
    assert!(nodes[1].as_element().unwrap().children.iter().any(|child| {
        child
            .as_text()
            .map(|text| text.text == "bold" && text.properties.get("bold").is_some())
            .unwrap_or(false)
    }));
    assert_eq!(nodes[2].element_type(), Some("ul"));
    assert_eq!(nodes[2].children().len(), 2);
    assert_eq!(nodes[3].element_type(), Some("blockquote"));
    assert_eq!(nodes[4].element_type(), Some("code"));
    assert_eq!(
        nodes[4].get_prop("language"),
        Some(&serde_json::json!("rust"))
    );
}

#[test]
fn parses_nested_markdown_lists() {
    let nodes = parse_markdown(
        "- parent\n  - child one\n  - child two\n- sibling\n\n1. first\n   1. nested ordered\n2. second",
    );
    assert_eq!(nodes.len(), 2);

    let list = &nodes[0];
    assert_eq!(list.element_type(), Some("ul"));
    assert_eq!(list.children().len(), 2);
    let parent = &list.children()[0];
    assert_eq!(parent.element_type(), Some("li"));
    assert_eq!(parent.children()[0].element_type(), Some("p"));
    assert_eq!(parent.children()[0].string(), "parent");
    let nested = &parent.children()[1];
    assert_eq!(nested.element_type(), Some("ul"));
    assert_eq!(nested.children().len(), 2);
    assert_eq!(nested.children()[0].string(), "child one");
    assert_eq!(nested.children()[1].string(), "child two");
    assert_eq!(list.children()[1].string(), "sibling");

    let ordered = &nodes[1];
    assert_eq!(ordered.element_type(), Some("ol"));
    let first = &ordered.children()[0];
    assert_eq!(first.children()[0].string(), "first");
    let nested = &first.children()[1];
    assert_eq!(nested.element_type(), Some("ol"));
    assert_eq!(nested.children()[0].string(), "nested ordered");
    assert_eq!(ordered.children()[1].string(), "second");
}

#[test]
fn html_sanitizes_unsafe_links_and_images() {
    let nodes = parse_html(
        r#"<p><a href="javascript:alert(1)">bad</a> <a href="/safe">good</a><img src="javascript:alert(1)"><img src="data:image/png;base64,abc"></p>"#,
    );
    let paragraph = &nodes[0];
    assert!(!paragraph.children().iter().any(|child| {
        child
            .as_element()
            .map(|element| element.element_type == "a")
            .unwrap_or(false)
            && child.string() == "bad"
    }));
    assert!(paragraph.children().iter().any(|child| {
        child
            .as_element()
            .map(|element| {
                element.element_type == "a"
                    && element.properties.get("href") == Some(&serde_json::json!("/safe"))
            })
            .unwrap_or(false)
    }));
    let images: Vec<_> = paragraph
        .children()
        .iter()
        .filter_map(Node::as_element)
        .filter(|element| element.element_type == "inline-image")
        .collect();
    assert_eq!(images.len(), 1);
    assert_eq!(
        images[0].properties.get("src"),
        Some(&serde_json::json!("data:image/png;base64,abc"))
    );
}

#[test]
fn insert_html_and_markdown_use_the_rich_parser() {
    use leptoslate::Editor;

    let mut editor = Editor::empty();
    editor.insert_html("<h2>Title</h2><p>Hello <strong>world</strong></p>");
    assert_eq!(editor.children[0].element_type(), Some("h2"));
    assert_eq!(editor.children[0].string(), "Title");
    assert_eq!(editor.children[1].element_type(), Some("p"));
    assert_eq!(editor.children[1].string(), "Hello world");
    assert!(editor.children[1].children().iter().any(|child| {
        child
            .as_text()
            .map(|text| text.text == "world" && text.properties.get("bold").is_some())
            .unwrap_or(false)
    }));

    let mut editor = Editor::empty();
    editor.insert_markdown("# Title\n\nHello **world**");
    assert_eq!(editor.children[0].element_type(), Some("h1"));
    assert_eq!(editor.children[0].string(), "Title");
    assert_eq!(editor.children[1].element_type(), Some("p"));
    assert_eq!(editor.children[1].string(), "Hello world");
    assert!(editor.children[1].children().iter().any(|child| {
        child
            .as_text()
            .map(|text| text.text == "world" && text.properties.get("bold").is_some())
            .unwrap_or(false)
    }));
}
