//! Table transforms, schema and normalization hooks.

use leptoslate::core::FeatureSet;
use leptoslate::{Editor, Node, Point, Range};
use leptoslate_rich::{ListTransforms, RichListFeature, RichTableFeature, TableTransforms};

fn table_editor(children: Vec<Node>) -> Editor {
    Editor::new_with_features(
        children,
        FeatureSet::new()
            .with(RichListFeature)
            .with(RichTableFeature),
    )
}

fn table_doc() -> Editor {
    table_editor(
        serde_json::from_str(
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
        .unwrap(),
    )
}

#[test]
fn feature_installs_table_schema_rules() {
    let editor = table_editor(vec![Node::empty_text()]);
    assert_eq!(editor.schema.tag_for("table"), "table");
    assert_eq!(editor.schema.tag_for("tbody"), "tbody");
    assert_eq!(editor.schema.tag_for("tr"), "tr");
    assert_eq!(editor.schema.tag_for("th"), "th");
    assert_eq!(editor.schema.tag_for("td"), "td");
}

#[test]
fn insert_table_creates_optional_header_and_undoes() {
    let mut editor = table_editor(vec![Node::Element(leptoslate::Element::paragraph(vec![
        Node::Text(leptoslate::Text::new("start")),
    ]))]);
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
    editor.toggle_list("ul");

    assert_eq!(editor.element(&[0, 0, 0, 0]).unwrap().element_type, "td");
    assert_eq!(editor.children[0].element_type(), Some("table"));
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
    let editor = table_editor(nodes.clone());

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
fn feature_removes_empty_table_containers() {
    let nodes: Vec<Node> = serde_json::from_str(
        r#"[
            {"type":"table","children":[]},
            {"type":"table","children":[
                {"type":"tbody","children":[]},
                {"type":"tbody","children":[
                    {"type":"tr","children":[]},
                    {"type":"tr","children":[
                        {"type":"td","children":[{"text":"ok"}]}
                    ]}
                ]}
            ]}
        ]"#,
    )
    .unwrap();
    let editor = table_editor(nodes);
    assert_eq!(editor.children.len(), 1);
    let table = editor.element(&[0]).unwrap();
    assert_eq!(table.element_type, "table");
    assert_eq!(table.children.len(), 1);
    assert_eq!(table.children[0].children().len(), 1);
}

#[test]
fn table_cells_stay_inline_containers_when_first_child_is_a_block() {
    // The cell content model is inline even if a block tries to sneak in.
    let nodes: Vec<Node> = serde_json::from_str(
        r#"[{"type":"table","children":[{"type":"tbody","children":[
            {"type":"tr","children":[
                {"type":"td","children":[{"type":"p","children":[{"text":"nested"}]}]}
            ]}
        ]}]}]"#,
    )
    .unwrap();
    let editor = table_editor(nodes);
    let cell = editor.element(&[0, 0, 0, 0]).unwrap();
    assert_eq!(cell.element_type, "td");
    assert_eq!(cell.children.len(), 1);
    assert!(cell.children[0].is_text());
    assert_eq!(Node::Element(cell.clone()).string(), "nested");
}
