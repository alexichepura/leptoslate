use leptoslate_core::{props, Editor, Element, Node, Point, Range, Text};

fn p(text: &str) -> Node {
    Node::Element(Element::paragraph(vec![Node::Text(Text::new(text))]))
}

fn heading(text: &str) -> Node {
    Node::Element(Element::new("h1", vec![Node::Text(Text::new(text))]))
}

#[test]
fn parses_and_serializes_slate_json() {
    let json = r#"[
        {"type":"h1","children":[{"text":"Title"}]},
        {"type":"p","children":[{"text":"Hello ","bold":true},{"text":"world"}]}
    ]"#;
    let editor = Editor::from_json(json).unwrap();
    assert_eq!(editor.children.len(), 2);
    assert_eq!(editor.children[0].element_type(), Some("h1"));
    assert_eq!(editor.document_string(), "Title\nHello world\n");

    let round_trip = Editor::from_json(&editor.to_json()).unwrap();
    assert_eq!(editor.children, round_trip.children);
}

#[test]
fn starts_with_a_caret_at_the_document_start() {
    let editor = Editor::new(vec![p("hello")]);
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 0], 0)))
    );
}

#[test]
fn typing_moves_the_caret() {
    let mut editor = Editor::new(vec![p("hello")]);
    editor.select_point(Point::new(vec![0, 0], 5));
    editor.insert_text("!");
    assert_eq!(editor.document_string(), "hello!\n");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 0], 6)))
    );
}

#[test]
fn enter_splits_the_block() {
    let mut editor = Editor::new(vec![p("hello world")]);
    editor.select_point(Point::new(vec![0, 0], 5));
    editor.insert_break();
    assert_eq!(editor.children.len(), 2);
    assert_eq!(editor.children[0].string(), "hello");
    assert_eq!(editor.children[1].string(), " world");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![1, 0], 0)))
    );
}

#[test]
fn enter_at_end_creates_empty_block() {
    let mut editor = Editor::new(vec![p("hello")]);
    editor.select_point(Point::new(vec![0, 0], 5));
    editor.insert_break();
    assert_eq!(editor.children.len(), 2);
    assert_eq!(editor.children[1].string(), "");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![1, 0], 0)))
    );
}

#[test]
fn backspace_at_start_merges_blocks() {
    let mut editor = Editor::new(vec![p("one"), p("two")]);
    editor.select_point(Point::new(vec![1, 0], 0));
    editor.delete_backward();
    assert_eq!(editor.children.len(), 1);
    assert_eq!(editor.document_string(), "onetwo\n");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 0], 3)))
    );
}

#[test]
fn deleting_a_range_across_paragraphs_merges_them() {
    let mut editor = Editor::new(vec![p("hello"), p("middle"), p("world")]);
    editor.selection = Some(Range::new(
        Point::new(vec![0, 0], 2),
        Point::new(vec![2, 0], 3),
    ));
    editor.delete_selection();
    assert_eq!(editor.children.len(), 1);
    assert_eq!(editor.document_string(), "held\n");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 0], 2)))
    );
}

#[test]
fn selecting_a_partial_range() {
    let mut editor = Editor::new(vec![p("hello world")]);
    editor.selection = Some(Range::new(
        Point::new(vec![0, 0], 0),
        Point::new(vec![0, 0], 5),
    ));
    assert_eq!(editor.selection_string().as_deref(), Some("hello"));
    editor.delete_selection();
    assert_eq!(editor.document_string(), " world\n");
}

#[test]
fn bold_mark_formats_the_selection() {
    let mut editor = Editor::new(vec![p("hello world")]);
    editor.selection = Some(Range::new(
        Point::new(vec![0, 0], 0),
        Point::new(vec![0, 0], 5),
    ));
    editor.toggle_mark("bold", serde_json::Value::Bool(true));
    let element = editor.element(&[0]).unwrap();
    assert_eq!(element.children.len(), 2);
    assert_eq!(element.children[0].as_text().unwrap().text, "hello");
    assert_eq!(
        element.children[0]
            .as_text()
            .unwrap()
            .properties
            .get("bold"),
        Some(&serde_json::Value::Bool(true))
    );
    assert_eq!(element.children[1].as_text().unwrap().text, " world");
    assert!(element.children[1]
        .as_text()
        .unwrap()
        .properties
        .get("bold")
        .is_none());
}

#[test]
fn marks_apply_to_typed_text() {
    let mut editor = Editor::new(vec![p("hello")]);
    editor.select_point(Point::new(vec![0, 0], 5));
    editor.toggle_mark("bold", serde_json::Value::Bool(true));
    editor.insert_text("!");
    assert_eq!(editor.document_string(), "hello!\n");
    let element = editor.element(&[0]).unwrap();
    assert!(element.children.iter().any(|child| {
        child
            .as_text()
            .map(|text| {
                text.text == "!"
                    && text.properties.get("bold") == Some(&serde_json::Value::Bool(true))
            })
            .unwrap_or(false)
    }));
}

#[test]
fn undo_and_redo_typing() {
    let mut editor = Editor::new(vec![p("hello")]);
    editor.select_point(Point::new(vec![0, 0], 5));
    editor.insert_text("!");
    assert_eq!(editor.document_string(), "hello!\n");
    assert!(editor.undo());
    assert_eq!(editor.document_string(), "hello\n");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 0], 5)))
    );
    assert!(editor.redo());
    assert_eq!(editor.document_string(), "hello!\n");
}

#[test]
fn undo_a_block_split() {
    let mut editor = Editor::new(vec![p("hello world")]);
    editor.select_point(Point::new(vec![0, 0], 5));
    editor.insert_break();
    assert_eq!(editor.children.len(), 2);
    assert!(editor.undo());
    assert_eq!(editor.children.len(), 1);
    assert_eq!(editor.document_string(), "hello world\n");
    assert!(editor.redo());
    assert_eq!(editor.children.len(), 2);
}

#[test]
fn normalization_wraps_stray_text() {
    let editor = Editor::new(vec![Node::Text(Text::new("hello"))]);
    assert_eq!(editor.children.len(), 1);
    assert_eq!(editor.children[0].element_type(), Some("p"));
    assert_eq!(editor.document_string(), "hello\n");
}

#[test]
fn normalization_merges_adjacent_equal_text() {
    let editor = Editor::new(vec![Node::Element(Element::paragraph(vec![
        Node::Text(Text::new("a")),
        Node::Text(Text::new("b")),
    ]))]);
    assert_eq!(editor.children[0].children().len(), 1);
    assert_eq!(editor.document_string(), "ab\n");
}

#[test]
fn normalization_keeps_empty_text_in_empty_blocks() {
    let editor = Editor::new(vec![Node::Element(Element::new("p", vec![]))]);
    assert_eq!(editor.children[0].children().len(), 1);
    assert_eq!(editor.children[0].string(), "");
}

#[test]
fn selection_spans_handle_multiline_blocks() {
    let mut editor = Editor::new(vec![p("a"), heading("b"), p("c")]);
    editor.selection = Some(Range::new(
        Point::new(vec![0, 0], 0),
        Point::new(vec![2, 0], 1),
    ));
    assert_eq!(editor.selection_string().as_deref(), Some("abc"));
    editor.delete_selection();
    assert_eq!(editor.document_string(), "\n");
}

#[test]
fn move_nodes_can_be_undone() {
    let mut editor = Editor::new(vec![p("a"), p("b"), p("c")]);
    editor.move_nodes(&[0], &[2]);
    assert_eq!(editor.document_string(), "b\nc\na\n");
    assert!(editor.undo());
    assert_eq!(editor.document_string(), "a\nb\nc\n");
}

#[test]
fn undo_restores_a_cross_block_deletion() {
    let mut editor = Editor::new(vec![p("hello"), p("middle"), p("world")]);
    editor.selection = Some(Range::new(
        Point::new(vec![0, 0], 2),
        Point::new(vec![2, 0], 3),
    ));
    editor.delete_selection();
    assert_eq!(editor.children.len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.children.len(), 3);
    assert_eq!(editor.children[0].string(), "hello");
    assert_eq!(editor.children[1].string(), "middle");
    assert_eq!(editor.children[2].string(), "world");
}

#[test]
fn undo_of_backspace_merge_restores_both_blocks() {
    let mut editor = Editor::new(vec![p("one"), p("two")]);
    editor.select_point(Point::new(vec![1, 0], 0));
    editor.delete_backward();
    assert_eq!(editor.children.len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.children.len(), 2);
    assert_eq!(editor.children[0].string(), "one");
    assert_eq!(editor.children[1].string(), "two");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![1, 0], 0)))
    );
    assert!(editor.redo());
    assert_eq!(editor.children.len(), 1);
}

#[test]
fn undo_restores_merged_block_types_and_text_marks() {
    let mut editor = Editor::new(vec![
        p("one"),
        Node::Element(Element::new(
            "h1",
            vec![Node::Text(
                Text::new("two").with_property("bold", serde_json::json!(true)),
            )],
        )),
    ]);
    editor.select_point(Point::new(vec![1, 0], 0));
    editor.delete_backward();
    assert_eq!(editor.children.len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.children.len(), 2);
    assert_eq!(editor.children[1].element_type(), Some("h1"));
    let restored = editor.children[1].as_element().unwrap();
    assert_eq!(restored.children[0].string(), "two");
    assert_eq!(
        restored.children[0]
            .as_text()
            .unwrap()
            .properties
            .get("bold"),
        Some(&serde_json::json!(true))
    );
}

#[test]
fn consecutive_typing_is_one_undo_step() {
    let mut editor = Editor::new(vec![p("hello")]);
    editor.select_point(Point::new(vec![0, 0], 5));
    editor.insert_text("a");
    editor.insert_text("b");
    editor.insert_text("c");
    assert_eq!(editor.document_string(), "helloabc\n");
    assert!(editor.undo());
    assert_eq!(editor.document_string(), "hello\n");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 0], 5)))
    );
    assert!(editor.redo());
    assert_eq!(editor.document_string(), "helloabc\n");
}

#[test]
fn moving_the_selection_ends_a_typing_undo_group() {
    let mut editor = Editor::new(vec![p("ab")]);
    editor.select_point(Point::new(vec![0, 0], 0));
    editor.insert_text("x");
    editor.select_point(Point::new(vec![0, 0], 2));
    editor.insert_text("y");
    // Only the second typing run is undone.
    assert!(editor.undo());
    assert_eq!(editor.document_string(), "xab\n");
    assert!(editor.undo());
    assert_eq!(editor.document_string(), "ab\n");
}

#[test]
fn turning_a_mark_off_at_the_caret_affects_new_text() {
    let mut editor = Editor::new(vec![Node::Element(Element::paragraph(vec![Node::Text(
        Text::new("hello").with_property("bold", serde_json::json!(true)),
    )]))]);
    editor.select_point(Point::new(vec![0, 0], 2));
    editor.toggle_mark("bold", serde_json::Value::Bool(true));
    editor.insert_text("x");

    assert_eq!(editor.document_string(), "hexllo\n");
    let paragraph = editor.element(&[0]).unwrap();
    let inserted = paragraph
        .children
        .iter()
        .find(|node| node.string() == "x")
        .expect("inserted text node");
    assert!(inserted.as_text().unwrap().properties.get("bold").is_none());
}

#[test]
fn unicode_offsets_are_characters() {
    let mut editor = Editor::new(vec![p("hé🙂")]);
    // Three characters, five UTF-16 code units.
    assert_eq!(editor.text(&[0, 0]).unwrap().len(), 3);
    editor.select_point(Point::new(vec![0, 0], 2));
    editor.insert_text("x");
    assert_eq!(editor.document_string(), "héx🙂\n");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 0], 3)))
    );
}

#[test]
fn toggling_bullet_list_wraps_and_unwraps() {
    let mut editor = Editor::new(vec![p("one"), p("two")]);
    editor.selection = Some(Range::new(
        Point::new(vec![0, 0], 1),
        Point::new(vec![1, 0], 1),
    ));
    editor.toggle_list("ul");
    assert_eq!(editor.children.len(), 1);
    assert_eq!(editor.children[0].element_type(), Some("ul"));
    assert_eq!(editor.document_string(), "one\ntwo\n");
    assert_eq!(
        editor.selection,
        Some(Range::new(
            Point::new(vec![0, 0, 0], 1),
            Point::new(vec![0, 1, 0], 1),
        ))
    );

    editor.toggle_list("ul");
    assert_eq!(editor.children.len(), 2);
    assert_eq!(editor.children[0].element_type(), Some("p"));
}

#[test]
fn enter_inside_a_list_item_keeps_the_list() {
    let mut editor = Editor::new(vec![Node::Element(Element::new(
        "ul",
        vec![
            Node::Element(Element::new("li", vec![Node::Text(Text::new("one"))])),
            Node::Element(Element::new("li", vec![Node::Text(Text::new("two"))])),
        ],
    ))]);
    editor.select_point(Point::new(vec![0, 0, 0], 3));
    editor.insert_break();

    assert_eq!(editor.children.len(), 1);
    let ul = editor.children[0].as_element().unwrap();
    assert_eq!(ul.element_type, "ul");
    assert_eq!(ul.children.len(), 3);
    assert_eq!(ul.children[0].string(), "one");
    assert_eq!(ul.children[1].string(), "");
    assert_eq!(ul.children[2].string(), "two");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 1, 0], 0)))
    );
}

#[test]
fn enter_inside_a_blockquote_stays_inside_it() {
    let mut editor = Editor::new(vec![Node::Element(Element::new(
        "blockquote",
        vec![p("hello")],
    ))]);
    editor.select_point(Point::new(vec![0, 0, 0], 2));
    editor.insert_break();

    assert_eq!(editor.children.len(), 1);
    let quote = editor.children[0].as_element().unwrap();
    assert_eq!(quote.element_type, "blockquote");
    assert_eq!(quote.children.len(), 2);
    assert_eq!(quote.children[0].string(), "he");
    assert_eq!(quote.children[1].string(), "llo");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 1, 0], 0)))
    );
}

#[test]
fn set_block_changes_heading_level() {
    let mut editor = Editor::new(vec![p("hello")]);
    editor.select_point(Point::new(vec![0, 0], 2));
    editor.set_block("h2");
    assert_eq!(editor.children[0].element_type(), Some("h2"));
    assert_eq!(editor.document_string(), "hello\n");
    assert!(editor.undo());
    assert_eq!(editor.children[0].element_type(), Some("p"));
}

#[test]
fn set_block_unwraps_a_list_first() {
    let mut editor = Editor::new(vec![Node::Element(Element::new(
        "ul",
        vec![Node::Element(Element::new(
            "li",
            vec![Node::Text(Text::new("item"))],
        ))],
    ))]);
    editor.select_point(Point::new(vec![0, 0, 0], 2));
    editor.set_block("h2");
    assert_eq!(editor.children.len(), 1);
    assert_eq!(editor.children[0].element_type(), Some("h2"));
    assert_eq!(editor.document_string(), "item\n");
}

#[test]
fn inserting_with_active_marks_uses_text_properties() {
    let mut editor = Editor::new(vec![p("abc")]);
    editor.selection = Some(Range::new(
        Point::new(vec![0, 0], 0),
        Point::new(vec![0, 0], 2),
    ));
    editor.toggle_mark("italic", serde_json::Value::Bool(true));
    let marks = editor.marks_at_selection();
    assert_eq!(marks.get("italic"), Some(&serde_json::Value::Bool(true)));
}

#[test]
fn operations_serialize_with_slate_tags() {
    let operation = leptoslate_core::Operation::InsertText {
        path: vec![0, 1],
        offset: 2,
        text: "hi".into(),
    };
    let value = serde_json::to_value(&operation).unwrap();
    assert_eq!(value["type"], "insert_text");
    assert_eq!(value["path"], serde_json::json!([0, 1]));
    assert_eq!(value["offset"], 2);

    let decoded: leptoslate_core::Operation = serde_json::from_value(value).unwrap();
    assert_eq!(decoded, operation);
}

#[test]
fn props_macro_builds_json_objects() {
    let properties = props! { "bold" => true, "level" => 2 };
    assert_eq!(properties.get("bold"), Some(&serde_json::Value::Bool(true)));
    assert_eq!(properties.get("level"), Some(&serde_json::json!(2)));
}

#[test]
fn insert_block_after_selection_selects_the_new_block() {
    let mut editor = Editor::new(vec![p("hello")]);
    editor.select_point(Point::new(vec![0, 0], 2));
    let inserted = editor.insert_block_after_selection(Node::Element(
        Element::new("callout", vec![]).with_property("tone", serde_json::json!("info")),
    ));

    assert_eq!(inserted, Some(vec![1]));
    assert_eq!(editor.children.len(), 2);
    assert_eq!(editor.children[1].element_type(), Some("callout"));
    assert_eq!(editor.children[1].string(), "");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![1, 0], 0)))
    );
    assert!(editor.undo());
    assert_eq!(editor.children.len(), 1);
}

#[test]
fn inserting_a_void_block_creates_a_paragraph_after_it() {
    let mut editor = Editor::new(vec![p("hello")]);
    editor.select_point(Point::new(vec![0, 0], 5));
    let image = Node::Element(
        Element::new("img", vec![Node::empty_text()])
            .with_property("src", serde_json::json!("https://example.com/a.png")),
    );
    let inserted = editor.insert_block_after_selection(image);

    assert_eq!(inserted, Some(vec![1]));
    assert_eq!(editor.children.len(), 3);
    assert_eq!(editor.children[1].element_type(), Some("img"));
    assert_eq!(editor.children[2].element_type(), Some("p"));
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![2, 0], 0)))
    );
}

#[test]
fn removing_a_block_moves_the_caret_to_a_neighbour() {
    let mut editor = Editor::new(vec![
        p("before"),
        Node::Element(Element::empty("img")),
        p("after"),
    ]);
    editor.select_point(Point::new(vec![0, 0], 2));
    let caret = editor.remove_block_at(&[1]);

    assert_eq!(editor.children.len(), 2);
    assert_eq!(editor.children[0].string(), "before");
    assert_eq!(editor.children[1].string(), "after");
    assert_eq!(caret, Some(Point::new(vec![1, 0], 0)));
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![1, 0], 0)))
    );
}

#[test]
fn node_properties_can_be_changed_and_undone() {
    let mut editor = Editor::new(vec![Node::Element(
        Element::new("todo", vec![Node::Text(Text::new("task"))])
            .with_property("checked", serde_json::json!(false)),
    )]);
    let mut properties = leptoslate_core::Props::new();
    properties.insert("checked".into(), serde_json::json!(true));
    editor.set_node_properties(&[0], properties);
    assert_eq!(
        editor.element(&[0]).unwrap().properties.get("checked"),
        Some(&serde_json::json!(true))
    );
    assert!(editor.undo());
    assert_eq!(
        editor.element(&[0]).unwrap().properties.get("checked"),
        Some(&serde_json::json!(false))
    );
}

#[test]
fn inserting_a_block_inside_a_list_goes_after_the_list() {
    let mut editor = Editor::new(vec![
        p("before"),
        Node::Element(Element::new(
            "ul",
            vec![Node::Element(Element::new(
                "li",
                vec![Node::Text(Text::new("item"))],
            ))],
        )),
        p("after"),
    ]);
    editor.select_point(Point::new(vec![1, 0, 0], 2));
    let inserted = editor.insert_block_after_selection(Node::Element(Element::new(
        "callout",
        vec![Node::Text(Text::new("note"))],
    )));

    assert_eq!(inserted, Some(vec![2]));
    assert_eq!(editor.children.len(), 4);
    assert_eq!(editor.children[1].element_type(), Some("ul"));
    assert_eq!(editor.children[2].element_type(), Some("callout"));
    assert_eq!(editor.children[3].element_type(), Some("p"));
}

fn image_block() -> Node {
    Node::Element(Element::new("img", vec![Node::empty_text()]))
}

#[test]
fn node_and_text_selections_are_mutually_exclusive() {
    let mut editor = Editor::new(vec![p("before"), image_block(), p("after")]);
    editor.select_node(&[1]);

    assert_eq!(editor.node_selection(), Some([1].as_slice()));
    assert!(editor.selection.is_none());

    editor.select_point(Point::new(vec![0, 0], 1));
    assert!(editor.node_selection().is_none());
    assert!(editor.selection.is_some());

    editor.select_node(&[1]);
    assert!(editor.selection.is_none());
    assert!(editor.node_selection().is_some());
}

#[test]
fn deleting_a_selected_node_moves_the_caret_to_the_next_block() {
    let mut editor = Editor::new(vec![p("before"), image_block(), p("after")]);
    editor.select_node(&[1]);
    editor.delete_backward();

    assert_eq!(editor.children.len(), 2);
    assert_eq!(editor.children[1].string(), "after");
    assert!(editor.node_selection().is_none());
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![1, 0], 0)))
    );

    assert!(editor.undo());
    assert_eq!(editor.children.len(), 3);
    assert_eq!(editor.children[1].element_type(), Some("img"));
}

#[test]
fn typing_replaces_a_selected_node() {
    let mut editor = Editor::new(vec![p("before"), image_block(), p("after")]);
    editor.select_node(&[1]);
    editor.insert_text("x");

    assert_eq!(editor.children.len(), 2);
    assert_eq!(editor.children[1].string(), "xafter");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![1, 0], 1)))
    );
}

#[test]
fn node_selection_moves_with_structural_operations() {
    let mut editor = Editor::new(vec![p("one"), image_block(), p("two")]);
    editor.select_node(&[1]);
    editor.insert_node(&[0], p("zero"));
    assert_eq!(editor.node_selection(), Some([2].as_slice()));

    editor.remove_nodes(&[0], 1);
    assert_eq!(editor.node_selection(), Some([1].as_slice()));

    editor
        .apply_op(leptoslate_core::Operation::RemoveNode { path: vec![1] })
        .unwrap();
    assert!(editor.node_selection().is_none());
}

#[test]
fn moving_off_a_node_selects_adjacent_text() {
    let mut editor = Editor::new(vec![p("before"), image_block(), p("after")]);
    editor.select_node(&[1]);
    let point = editor.move_selection_off_node(false);
    assert_eq!(point, Some(Point::new(vec![2, 0], 0)));
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![2, 0], 0)))
    );
    assert!(editor.node_selection().is_none());

    editor.select_node(&[1]);
    let point = editor.move_selection_off_node(true);
    assert_eq!(point, Some(Point::new(vec![0, 0], 6)));
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 0], 6)))
    );
}

fn link_node(label: &str, href: &str) -> Node {
    Node::Element(
        Element::new("a", vec![Node::Text(Text::new(label))])
            .with_property("href", serde_json::json!(href)),
    )
}

#[test]
fn links_wrap_the_selection_and_update_the_href() {
    let mut editor = Editor::new(vec![p("hello world")]);
    editor.selection = Some(Range::new(
        Point::new(vec![0, 0], 0),
        Point::new(vec![0, 0], 5),
    ));

    assert!(editor.set_link("https://leptos.dev"));
    let link_path = editor
        .element(&[0])
        .unwrap()
        .children
        .iter()
        .position(|node| node.element_type() == Some("a"))
        .map(|index| vec![0, index])
        .expect("link element");
    let link = editor.element(&link_path).unwrap();
    assert_eq!(link.children[0].string(), "hello");
    assert_eq!(
        link.properties.get("href"),
        Some(&serde_json::json!("https://leptos.dev"))
    );

    assert_eq!(
        editor.link_at_selection(),
        Some((link_path.clone(), "https://leptos.dev".into()))
    );

    assert!(editor.set_link("https://example.com"));
    assert_eq!(
        editor.element(&link_path).unwrap().properties.get("href"),
        Some(&serde_json::json!("https://example.com"))
    );

    assert!(editor.remove_link());
    assert!(editor
        .element(&[0])
        .unwrap()
        .children
        .iter()
        .all(|node| node.element_type() != Some("a")));
    assert_eq!(editor.children[0].string(), "hello world");
}

#[test]
fn set_link_requires_a_selection_or_existing_link() {
    let mut editor = Editor::new(vec![p("hello")]);
    editor.select_point(Point::new(vec![0, 0], 1));
    assert!(!editor.set_link("https://leptos.dev"));
    assert!(editor.link_at_selection().is_none());
}

#[test]
fn backspace_and_delete_work_at_link_boundaries() {
    let mut editor = Editor::new(vec![Node::Element(Element::paragraph(vec![
        Node::Text(Text::new("ab")),
        link_node("cd", "https://leptos.dev"),
        Node::Text(Text::new("ef")),
    ]))]);

    editor.select_point(Point::new(vec![0, 1, 0], 0));
    editor.delete_backward();
    assert_eq!(editor.children[0].string(), "acdef");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 0], 1)))
    );

    editor.select_point(Point::new(vec![0, 1, 0], 2));
    editor.delete_forward();
    assert_eq!(editor.children[0].string(), "acdf");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 1, 0], 2)))
    );
}

#[test]
fn deleting_across_a_link_removes_the_link_element() {
    let mut editor = Editor::new(vec![Node::Element(Element::paragraph(vec![
        Node::Text(Text::new("ab")),
        link_node("cd", "https://leptos.dev"),
        Node::Text(Text::new("ef")),
    ]))]);
    editor.selection = Some(Range::new(
        Point::new(vec![0, 0], 1),
        Point::new(vec![0, 2], 1),
    ));
    editor.delete_selection();

    assert_eq!(editor.children[0].string(), "af");
    assert!(editor
        .element(&[0])
        .unwrap()
        .children
        .iter()
        .all(|node| node.element_type() != Some("a")));
}

#[test]
fn normalization_removes_empty_inline_elements() {
    let editor = Editor::new(vec![Node::Element(Element::paragraph(vec![
        Node::Element(Element::new("a", vec![Node::empty_text()])),
    ]))]);
    assert_eq!(editor.children[0].children().len(), 1);
    assert_eq!(editor.children[0].children()[0].string(), "");
    assert_eq!(editor.children[0].children()[0].element_type(), None);
}
