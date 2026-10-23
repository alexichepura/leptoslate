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

fn table_doc() -> Editor {
    Editor::from_json(
        r#"[{"type":"table","children":[{"type":"tbody","children":[
            {"type":"tr","children":[
                {"type":"td","children":[{"text":"A"}]},
                {"type":"td","children":[{"text":"B"}]}
            ]},
            {"type":"tr","children":[
                {"type":"td","children":[{"text":"C"}]},
                {"type":"td","children":[{"text":"D"}]}
            ]}
        ]}]}]"#,
    )
    .unwrap()
}

#[test]
fn insert_table_creates_optional_header_and_undoes() {
    let mut editor = Editor::empty();
    editor.select_point(Point::new(vec![0, 0], 0));
    let inserted = editor.insert_table(3, 2, true).expect("table path");
    assert_eq!(inserted, vec![1]);

    let table = editor.element(&[1]).unwrap();
    assert_eq!(table.element_type, "table");
    assert_eq!(table.children[0].element_type(), Some("thead"));
    assert_eq!(table.children[0].children()[0].children().len(), 2);
    assert_eq!(table.children[1].element_type(), Some("tbody"));
    assert_eq!(table.children[1].children().len(), 2);
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![1, 0, 0, 0, 0], 0)))
    );

    assert!(editor.undo());
    assert_eq!(editor.children.len(), 1);

    editor.select_point(Point::new(vec![0, 0], 0));
    let inserted = editor.insert_table(2, 3, false).expect("table path");
    let table = editor.element(&inserted).unwrap();
    assert_eq!(table.children[0].element_type(), Some("tbody"));
    assert_eq!(table.children[0].children().len(), 2);
    assert_eq!(table.children[0].children()[0].children().len(), 3);
}

#[test]
fn table_cell_navigation_and_boundaries() {
    let mut editor = table_doc();
    editor.select_point(Point::new(vec![0, 0, 0, 0, 0], 0));

    assert_eq!(editor.table_cell_at_selection(), Some(vec![0, 0, 0, 0]));
    assert_eq!(editor.table_at_selection(), Some(vec![0]));
    assert!(editor.at_table_cell_start(editor.selection.as_ref().unwrap().start()));
    assert!(!editor.at_table_cell_end(editor.selection.as_ref().unwrap().start()));

    assert!(editor.move_table_cell(1));
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 0, 0, 1, 0], 0)))
    );
    assert!(editor.move_table_cell(1));
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 0, 1, 0, 0], 0)))
    );
    assert!(editor.move_table_cell(-1));
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 0, 0, 1, 0], 0)))
    );

    editor.select_point(Point::new(vec![0, 0, 0, 0, 0], 1));
    assert!(editor.at_table_cell_end(editor.selection.as_ref().unwrap().start()));
}

#[test]
fn table_row_and_column_commands_are_undoable() {
    let mut editor = table_doc();
    editor.select_point(Point::new(vec![0, 0, 0, 0, 0], 0));

    assert!(editor.insert_table_row_after());
    assert_eq!(editor.children[0].children()[0].children().len(), 3);
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 0, 1, 0, 0], 0)))
    );
    assert!(editor.undo());
    assert_eq!(editor.children[0].children()[0].children().len(), 2);

    editor.select_point(Point::new(vec![0, 0, 0, 0, 0], 0));
    assert!(editor.insert_table_column_after());
    for row in editor.children[0].children()[0].children() {
        assert_eq!(row.children().len(), 3);
    }
    assert!(editor.undo());

    editor.select_point(Point::new(vec![0, 0, 1, 0, 0], 0));
    assert!(editor.delete_table_row());
    assert_eq!(editor.children[0].children()[0].children().len(), 1);
    assert!(editor.undo());

    editor.select_point(Point::new(vec![0, 0, 0, 0, 0], 0));
    assert!(editor.delete_table_column());
    for row in editor.children[0].children()[0].children() {
        assert_eq!(row.children().len(), 1);
    }
    assert!(!editor.delete_table_column());
    assert!(editor.undo());
}

#[test]
fn block_transforms_do_not_escape_table_cells() {
    let mut editor = table_doc();
    editor.select_point(Point::new(vec![0, 0, 0, 0, 0], 0));
    editor.set_block("h2");

    assert_eq!(editor.element(&[0, 0, 0, 0]).unwrap().element_type, "td");
    assert_eq!(editor.children[0].element_type(), Some("table"));
}

struct HeadingFeature;

impl leptoslate_core::Feature for HeadingFeature {
    fn install_schema(&self, schema: &mut leptoslate_core::Schema) {
        schema.set_rule("h5", leptoslate_core::ElementRule::block("h5"));
    }

    fn normalize(&self, editor: &Editor) -> Option<leptoslate_core::Operation> {
        let is_title = editor
            .children
            .first()
            .and_then(Node::as_element)
            .map(|element| {
                element.element_type == "p"
                    && element
                        .children
                        .iter()
                        .any(|child| child.string() == "Feature title")
            })
            .unwrap_or(false);
        if !is_title {
            return None;
        }
        let mut new_properties = props::Props::new();
        new_properties.insert("type".into(), serde_json::json!("h2"));
        Some(leptoslate_core::Operation::SetNode {
            path: vec![0],
            properties: props::Props::new(),
            new_properties,
        })
    }
}

#[test]
fn feature_can_stop_enter_at_a_custom_depth() {
    struct StopFeature;

    impl leptoslate_core::Feature for StopFeature {
        fn split_stop_depth(&self, _editor: &Editor, point: &Point) -> Option<usize> {
            (point.path.len() > 2).then_some(2)
        }
    }

    let nested = Node::Element(Element::new(
        "section",
        vec![Node::Element(Element::new("div", vec![p("hello")]))],
    ));
    let mut editor = Editor::new(vec![nested]);
    editor.add_feature(StopFeature);
    editor.select_point(Point::new(vec![0, 0, 0, 0], 2));
    editor.insert_break();

    let section = editor.element(&[0]).unwrap();
    assert_eq!(section.children.len(), 2);
    assert_eq!(section.children[0].children().len(), 1);
    assert_eq!(section.children[0].string(), "he");
    assert_eq!(section.children[1].string(), "llo");
}

#[test]
fn feature_can_take_over_backspace_at_a_block_start() {
    struct SilentBackspaceFeature;

    impl leptoslate_core::Feature for SilentBackspaceFeature {
        fn handle_backspace(&self, _editor: &mut Editor, _point: &Point) -> bool {
            true
        }
    }

    let mut editor = Editor::new(vec![p("one"), p("two")]);
    editor.add_feature(SilentBackspaceFeature);
    editor.select_point(Point::new(vec![1, 0], 0));
    editor.delete_backward();

    assert_eq!(editor.children.len(), 2);
    assert_eq!(editor.document_string(), "one\ntwo\n");
}

#[test]
fn feature_can_adjust_where_a_new_block_lands() {
    struct WrapFeature;

    impl leptoslate_core::Feature for WrapFeature {
        fn adjust_insert_block(&self, editor: &Editor, block: &mut leptoslate_core::Path) {
            let parent = vec![0];
            if block.len() > 1 && editor.element(&parent).is_some() {
                *block = parent;
            }
        }
    }

    let wrapper = Node::Element(Element::new("aside", vec![p("note")]));
    let mut editor = Editor::new(vec![wrapper]);
    editor.add_feature(WrapFeature);
    editor.select_point(Point::new(vec![0, 0, 0], 2));
    let inserted = editor.insert_block_after_selection(Node::Element(Element::new(
        "callout",
        vec![Node::Text(Text::new("new"))],
    )));

    assert_eq!(inserted, Some(vec![1]));
    assert_eq!(editor.children.len(), 2);
    assert_eq!(editor.children[0].element_type(), Some("aside"));
    assert_eq!(editor.children[1].element_type(), Some("callout"));
}

#[test]
fn feature_can_install_schema_and_normalize() {
    let mut editor = Editor::new(vec![p("Feature title")]);
    assert_eq!(editor.children[0].element_type(), Some("p"));

    editor.add_feature(HeadingFeature);
    assert_eq!(editor.children[0].element_type(), Some("h2"));
    assert!(editor.schema.rule("h5").is_some());

    let set = leptoslate_core::FeatureSet::new().with(HeadingFeature);
    assert!(!set.is_empty());
    editor.set_features(set);
    assert_eq!(editor.children[0].element_type(), Some("h2"));
}

#[test]
fn table_documents_are_preserved_and_normalized() {
    let json = r#"[
        {"type":"h4","children":[{"text":"Specs"}]},
        {"type":"table","children":[
            {"type":"thead","children":[
                {"type":"tr","children":[
                    {"type":"th","children":[{"text":"Name"}]},
                    {"type":"th","children":[{"text":"Value"}]}
                ]}
            ]},
            {"type":"tbody","children":[
                {"type":"tr","children":[
                    {"type":"td","children":[{"text":"Range"}]},
                    {"type":"td","children":[{"text":"500 km"}]}
                ]}
            ]}
        ]}
    ]"#;
    let nodes: Vec<Node> = serde_json::from_str(json).unwrap();
    let editor = Editor::new(nodes.clone());

    assert_eq!(editor.children, nodes);
    assert!(editor.schema.is_table("table"));
    assert!(editor.schema.is_table_section("tbody"));
    assert!(editor.schema.is_table_row("tr"));
    assert!(editor.schema.is_table_cell("th"));
    assert!(editor.schema.is_table_cell("td"));
    assert_eq!(
        editor.children[1].children()[1].element_type(),
        Some("tbody")
    );
    assert_eq!(
        editor.children[1].children()[1].children()[0].string(),
        "Range500 km"
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
fn node_content_hashes_track_text_and_marks() {
    let plain = p("hello");
    let same = p("hello");
    let changed = p("hello!");
    assert_eq!(plain.content_hash(), same.content_hash());
    assert_ne!(plain.content_hash(), changed.content_hash());

    let mut marked = Text::new("hello");
    marked
        .properties
        .insert("bold".into(), serde_json::json!(true));
    assert_ne!(plain.content_hash(), Node::Text(marked).content_hash());
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
    assert_eq!(editor.node_selection(), Some([1].as_slice()));
    assert!(editor.selection.is_none());
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
fn node_selection_ranges_cover_top_level_blocks() {
    let mut editor = Editor::new(vec![p("one"), p("two"), p("three"), p("four")]);
    editor.select_node_range(&[1], &[3]);

    assert_eq!(editor.node_selection(), Some([1].as_slice()));
    assert_eq!(editor.node_selection_end(), Some([3].as_slice()));
    assert_eq!(editor.node_selections(), vec![vec![1], vec![2], vec![3]]);
    assert!(editor.is_node_selected(&[2]));
    assert!(!editor.is_node_selected(&[0]));
    assert!(!editor.is_node_selected(&[2, 0]));
    assert_eq!(
        editor.node_selection_string().as_deref(),
        Some("two\nthree\nfour")
    );

    assert!(editor.extend_node_selection(-1));
    assert_eq!(editor.node_selection_end(), Some([2].as_slice()));
    assert!(editor.move_node_selection(-1));
    assert_eq!(editor.node_selection(), Some([1].as_slice()));
    assert_eq!(editor.node_selection_end(), None);
}

#[test]
fn deleting_a_node_range_removes_all_blocks_and_undo_restores_it() {
    let mut editor = Editor::new(vec![p("one"), p("two"), p("three"), p("four")]);
    editor.select_node_range(&[1], &[2]);
    assert!(editor.delete_node_selection());

    assert_eq!(editor.children.len(), 2);
    assert_eq!(editor.children[0].string(), "one");
    assert_eq!(editor.children[1].string(), "four");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![1, 0], 0)))
    );
    assert!(!editor.has_node_selection());

    assert!(editor.undo());
    assert_eq!(editor.children.len(), 4);
    assert_eq!(editor.node_selections(), vec![vec![1], vec![2]]);
    assert!(editor.selection.is_none());

    assert!(editor.redo());
    assert_eq!(editor.children.len(), 2);
    assert!(!editor.has_node_selection());
}

#[test]
fn duplicating_a_node_range_selects_the_copies() {
    let mut editor = Editor::new(vec![p("one"), p("two"), p("three")]);
    editor.select_node_range(&[1], &[2]);
    assert!(editor.duplicate_node_selection());

    assert_eq!(editor.children.len(), 5);
    assert_eq!(editor.children[1].string(), "two");
    assert_eq!(editor.children[2].string(), "three");
    assert_eq!(editor.children[3].string(), "two");
    assert_eq!(editor.children[4].string(), "three");
    assert_eq!(editor.node_selections(), vec![vec![3], vec![4]]);

    assert!(editor.undo());
    assert_eq!(editor.children.len(), 3);
    assert_eq!(editor.node_selections(), vec![vec![1], vec![2]]);
}

#[test]
fn node_selection_range_moves_with_structural_operations() {
    let mut editor = Editor::new(vec![p("one"), image_block(), p("two"), p("three")]);
    editor.select_node_range(&[1], &[2]);

    editor.insert_node(&[0], p("zero"));
    assert_eq!(editor.node_selection(), Some([2].as_slice()));
    assert_eq!(editor.node_selection_end(), Some([3].as_slice()));

    editor.remove_nodes(&[0], 1);
    assert_eq!(editor.node_selection(), Some([1].as_slice()));
    assert_eq!(editor.node_selection_end(), Some([2].as_slice()));

    editor
        .apply_op(leptoslate_core::Operation::RemoveNode { path: vec![1] })
        .unwrap();
    assert!(editor.node_selection().is_none());
    assert!(editor.node_selection_end().is_none());
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
fn unsafe_link_and_image_urls_are_rejected() {
    use leptoslate_core::fragment::parse_html;

    assert!(!leptoslate_core::is_safe_href("javascript:alert(1)"));
    assert!(!leptoslate_core::is_safe_href(
        "data:text/html,<script>x</script>"
    ));
    assert!(leptoslate_core::is_safe_href("/docs"));
    assert!(leptoslate_core::is_safe_href("https://leptos.dev"));
    assert!(leptoslate_core::is_safe_href("mailto:hello@example.com"));
    assert!(leptoslate_core::is_safe_href("#section"));

    let mut editor = Editor::new(vec![p("hello")]);
    editor.selection = Some(Range::new(
        Point::new(vec![0, 0], 0),
        Point::new(vec![0, 0], 5),
    ));
    assert!(!editor.set_link("javascript:alert(1)"));
    assert!(editor.link_at_selection().is_none());

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
fn update_link_changes_href_and_text_and_keeps_first_text_marks() {
    let mut first = Text::new("he");
    first
        .properties
        .insert("bold".into(), serde_json::json!(true));
    let mut second = Text::new("re");
    second
        .properties
        .insert("italic".into(), serde_json::json!(true));
    let link = Element::new("a", vec![Node::Text(first), Node::Text(second)])
        .with_property("href", serde_json::json!("/old"));
    let mut editor = Editor::new(vec![Node::Element(Element::paragraph(vec![
        Node::Text(Text::new("go ")),
        Node::Element(link),
        Node::Text(Text::new(" now")),
    ]))]);

    editor.select_point(Point::new(vec![0, 1, 0], 1));
    assert!(editor.update_link_at_selection("/new", "there"));

    let link = editor.element(&[0, 1]).unwrap();
    assert_eq!(
        link.properties.get("href"),
        Some(&serde_json::json!("/new"))
    );
    assert_eq!(link.children.len(), 1);
    assert_eq!(link.children[0].string(), "there");
    assert_eq!(
        link.children[0].as_text().unwrap().properties.get("bold"),
        Some(&serde_json::json!(true))
    );
}

#[test]
fn updating_only_the_link_href_preserves_existing_children() {
    let mut first = Text::new("he");
    first
        .properties
        .insert("bold".into(), serde_json::json!(true));
    let mut second = Text::new("re");
    second
        .properties
        .insert("italic".into(), serde_json::json!(true));
    let link = Element::new("a", vec![Node::Text(first), Node::Text(second)])
        .with_property("href", serde_json::json!("/old"));
    let mut editor = Editor::new(vec![Node::Element(Element::paragraph(vec![
        Node::Text(Text::new("go ")),
        Node::Element(link),
        Node::Text(Text::new(" now")),
    ]))]);

    editor.select_point(Point::new(vec![0, 1, 0], 1));
    assert_eq!(
        editor.link_at_selection(),
        Some((vec![0, 1], "/old".into()))
    );
    assert!(editor.update_link_at_selection("/new", "here"));

    let link = editor.element(&[0, 1]).unwrap();
    assert_eq!(link.children.len(), 2);
    assert_eq!(
        link.children[0].as_text().unwrap().properties.get("bold"),
        Some(&serde_json::json!(true))
    );
    assert_eq!(
        link.children[1].as_text().unwrap().properties.get("italic"),
        Some(&serde_json::json!(true))
    );
    assert!(!editor.update_link_at_selection("/new", ""));
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

fn mention(name: &str) -> Node {
    Node::Element(
        Element::new("mention", vec![Node::empty_text()])
            .with_property("id", serde_json::json!(name.to_lowercase()))
            .with_property("name", serde_json::json!(name)),
    )
}

#[test]
fn inline_void_nodes_are_inserted_between_text_and_survive_undo() {
    let mut editor = Editor::new(vec![p("hello")]);
    editor.select_point(Point::new(vec![0, 0], 2));
    let inserted = editor.insert_inline_node(mention("Leptos"));
    assert_eq!(inserted, Some(vec![0, 1]));

    let children = &editor.element(&[0]).unwrap().children;
    assert_eq!(children.len(), 3);
    assert_eq!(children[0].string(), "he");
    assert_eq!(children[1].element_type(), Some("mention"));
    assert_eq!(
        children[1].get_prop("name"),
        Some(&serde_json::json!("Leptos"))
    );
    assert_eq!(children[2].string(), "llo");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 2], 0)))
    );

    assert!(editor.undo());
    assert_eq!(editor.children[0].string(), "hello");
}

#[test]
fn inline_void_insertion_at_edges_keeps_surrounding_text() {
    let mut editor = Editor::new(vec![p("ab")]);
    editor.select_point(Point::new(vec![0, 0], 0));
    editor.insert_inline_node(mention("Start"));
    let children = editor.element(&[0]).unwrap().children.clone();
    assert_eq!(children[1].element_type(), Some("mention"));
    assert_eq!(children[2].string(), "ab");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 2], 0)))
    );

    editor.select_point(Point::new(vec![0, 2], 2));
    editor.insert_inline_node(mention("End"));
    let children = editor.element(&[0]).unwrap().children.clone();
    assert_eq!(children[1].element_type(), Some("mention"));
    assert_eq!(children[2].string(), "ab");
    assert_eq!(children[3].element_type(), Some("mention"));
    assert_eq!(children[4].string(), "");
}

#[test]
fn backspace_and_delete_remove_inline_voids_as_atoms() {
    let mut editor = Editor::new(vec![Node::Element(Element::paragraph(vec![
        Node::Text(Text::new("a")),
        mention("Atom"),
        Node::Text(Text::new("b")),
    ]))]);

    editor.select_point(Point::new(vec![0, 2], 0));
    editor.delete_backward();
    assert_eq!(editor.children[0].string(), "ab");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 0], 1)))
    );

    editor.select_point(Point::new(vec![0, 0], 1));
    editor.delete_forward();
    assert_eq!(editor.children[0].string(), "a");
}

#[test]
fn selecting_across_an_inline_void_removes_it() {
    let mut editor = Editor::new(vec![Node::Element(Element::paragraph(vec![
        Node::Text(Text::new("a")),
        mention("Atom"),
        Node::Text(Text::new("b")),
    ]))]);
    editor.selection = Some(Range::new(
        Point::new(vec![0, 0], 1),
        Point::new(vec![0, 2], 0),
    ));
    editor.delete_selection();

    assert_eq!(editor.children[0].string(), "ab");
    assert!(editor
        .element(&[0])
        .unwrap()
        .children
        .iter()
        .all(|node| node.element_type() != Some("mention")));
}

#[test]
fn selecting_after_an_inline_void_lands_in_following_text() {
    let mut editor = Editor::new(vec![Node::Element(Element::paragraph(vec![
        Node::Text(Text::new("a")),
        mention("Atom"),
        Node::Text(Text::new("b")),
    ]))]);
    let point = editor.select_after_node(&[0, 1]);
    assert_eq!(point, Some(Point::new(vec![0, 2], 0)));
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 2], 0)))
    );
}

#[test]
fn parses_html_fragments_into_slate_nodes() {
    use leptoslate_core::fragment::parse_html;

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
    use leptoslate_core::fragment::parse_html;

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
    use leptoslate_core::fragment::parse_html;

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
    use leptoslate_core::fragment::parse_html;

    let nodes = parse_html(
        r#"<table><thead><tr><th>Model</th></tr></thead><tbody><tr><td><strong>Zeekr</strong> <a href="/zeekr">001</a></td></tr></tbody></table>"#,
    );
    let table = &nodes[0];
    assert_eq!(table.element_type(), Some("table"));
    assert_eq!(table.children()[0].element_type(), Some("thead"));
    let cell = &table.children()[1].children()[0].children()[0];
    assert_eq!(cell.element_type(), Some("td"));
    assert!(cell.children().iter().any(|child| {
        child
            .as_text()
            .map(|text| text.text == "Zeekr" && text.properties.get("bold").is_some())
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
    use leptoslate_core::fragment::parse_markdown;

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
    use leptoslate_core::fragment::parse_markdown;

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
fn inserting_an_inline_fragment_merges_into_the_current_block() {
    let mut editor = Editor::new(vec![p("hello")]);
    editor.select_point(Point::new(vec![0, 0], 2));
    editor.insert_fragment(vec![Node::Element(Element::paragraph(vec![
        Node::Text(Text::new("X")),
        Node::Text(Text::new("Y").with_property("bold", serde_json::json!(true))),
    ]))]);

    assert_eq!(editor.children.len(), 1);
    assert_eq!(editor.children[0].string(), "heXYllo");
    assert_eq!(
        editor.element(&[0]).unwrap().children[1].get_prop("bold"),
        Some(&serde_json::json!(true))
    );
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 2], 0)))
    );
}

#[test]
fn inserting_a_block_fragment_splits_the_current_block() {
    let mut editor = Editor::new(vec![p("hello")]);
    editor.select_point(Point::new(vec![0, 0], 2));
    editor.insert_fragment(vec![
        Node::Element(Element::new("h2", vec![Node::Text(Text::new("Title"))])),
        Node::Element(Element::paragraph(vec![Node::Text(Text::new("Body"))])),
    ]);

    assert_eq!(editor.children.len(), 4);
    assert_eq!(editor.children[0].string(), "he");
    assert_eq!(editor.children[1].element_type(), Some("h2"));
    assert_eq!(editor.children[2].string(), "Body");
    assert_eq!(editor.children[3].string(), "llo");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![3, 0], 0)))
    );
}

#[test]
fn inserting_a_fragment_at_the_block_start_does_not_leave_an_empty_block() {
    let mut editor = Editor::new(vec![p("hello")]);
    editor.select_point(Point::new(vec![0, 0], 0));
    editor.insert_fragment(vec![Node::Element(Element::new(
        "h2",
        vec![Node::Text(Text::new("Title"))],
    ))]);

    assert_eq!(editor.children.len(), 2);
    assert_eq!(editor.children[0].element_type(), Some("h2"));
    assert_eq!(editor.children[1].string(), "hello");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![1, 0], 0)))
    );
}

#[test]
fn inserts_a_block_after_an_explicit_block_path() {
    let mut editor = Editor::new(vec![p("one"), p("two")]);
    let inserted = editor.insert_block_after(&[0], p("middle"));

    assert_eq!(inserted, Some(vec![1]));
    assert_eq!(editor.children.len(), 3);
    assert_eq!(editor.children[0].string(), "one");
    assert_eq!(editor.children[1].string(), "middle");
    assert_eq!(editor.children[2].string(), "two");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![1, 0], 0)))
    );

    assert!(editor.undo());
    assert_eq!(editor.children.len(), 2);
}

#[test]
fn moves_top_level_blocks_and_undoes() {
    let mut editor = Editor::new(vec![p("a"), p("b"), p("c")]);
    editor.select_point(Point::new(vec![0, 0], 0));

    assert!(editor.move_block(0, 2));
    assert_eq!(editor.document_string(), "b\nc\na\n");
    assert!(editor.undo());
    assert_eq!(editor.document_string(), "a\nb\nc\n");

    assert!(editor.move_block(2, 0));
    assert_eq!(editor.document_string(), "c\na\nb\n");
    assert!(!editor.move_block(0, 0));
    assert!(!editor.move_block(9, 0));
}

fn list(list_type: &str, items: Vec<&str>) -> Node {
    Node::Element(Element::new(
        list_type,
        items
            .into_iter()
            .map(|item| Node::Element(Element::new("li", vec![Node::Text(Text::new(item))])))
            .collect(),
    ))
}

#[test]
fn normalization_turns_mixed_list_item_content_into_blocks() {
    let editor = Editor::new(vec![Node::Element(Element::new(
        "ul",
        vec![Node::Element(Element::new(
            "li",
            vec![Node::Text(Text::new("parent")), list("ul", vec!["child"])],
        ))],
    ))]);
    let li = editor.element(&[0, 0]).unwrap();
    assert_eq!(li.children.len(), 2);
    assert_eq!(li.children[0].element_type(), Some("p"));
    assert_eq!(li.children[0].string(), "parent");
    assert_eq!(li.children[1].element_type(), Some("ul"));
}
