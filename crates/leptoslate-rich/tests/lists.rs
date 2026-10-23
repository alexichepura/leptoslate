//! List transforms and list feature hooks.

use leptoslate::{Editor, Element, Node, Point, Range, Text};
use leptoslate_rich::{ListTransforms, RichListFeature};

fn list(list_type: &str, items: Vec<&str>) -> Node {
    Node::Element(Element::new(
        list_type,
        items
            .into_iter()
            .map(|item| Node::Element(Element::new("li", vec![Node::Text(Text::new(item))])))
            .collect(),
    ))
}

fn list_editor(children: Vec<Node>) -> Editor {
    let mut editor = Editor::new(children);
    editor.add_feature(RichListFeature);
    editor
}

#[test]
fn feature_installs_list_schema_rules() {
    let editor = list_editor(vec![list("ul", vec!["one"])]);
    assert!(editor.schema.rule("ul").is_some());
    assert!(editor.schema.rule("ol").is_some());
    assert!(editor.schema.rule("li").is_some());
    assert_eq!(editor.schema.tag_for("li"), "li");
}

#[test]
fn toggling_bullet_list_wraps_and_unwraps() {
    let mut editor = list_editor(vec![
        Node::Element(Element::paragraph(vec![Node::Text(Text::new("one"))])),
        Node::Element(Element::paragraph(vec![Node::Text(Text::new("two"))])),
    ]);
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
fn switching_list_type_changes_the_list_element() {
    let mut editor = list_editor(vec![list("ul", vec!["one", "two"])]);
    editor.select_point(Point::new(vec![0, 0, 0], 1));

    editor.toggle_list("ol");
    assert_eq!(editor.element(&[0]).unwrap().element_type, "ol");
    assert_eq!(editor.element(&[0]).unwrap().children.len(), 2);

    editor.toggle_list("ul");
    assert_eq!(editor.element(&[0]).unwrap().element_type, "ul");
}

#[test]
fn enter_inside_a_list_item_keeps_the_list() {
    let mut editor = list_editor(vec![list("ul", vec!["one", "two"])]);
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
fn set_block_unwraps_a_list_first() {
    let mut editor = list_editor(vec![list("ul", vec!["item"])]);
    editor.select_point(Point::new(vec![0, 0, 0], 2));
    editor.set_block("h2");
    assert_eq!(editor.children.len(), 1);
    assert_eq!(editor.children[0].element_type(), Some("h2"));
    assert_eq!(editor.document_string(), "item\n");
}

#[test]
fn inserting_a_block_inside_a_list_goes_after_the_list() {
    let mut editor = list_editor(vec![
        Node::Element(Element::paragraph(vec![Node::Text(Text::new("before"))])),
        list("ul", vec!["item"]),
        Node::Element(Element::paragraph(vec![Node::Text(Text::new("after"))])),
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

#[test]
fn indenting_a_list_item_nests_it_under_the_previous_item() {
    let mut editor = list_editor(vec![list("ul", vec!["one", "two", "three"])]);
    editor.select_point(Point::new(vec![0, 1, 0], 1));

    assert!(editor.indent_list());
    let ul = editor.element(&[0]).unwrap();
    assert_eq!(ul.children.len(), 2);
    assert_eq!(ul.children[0].string(), "onetwo");
    assert_eq!(ul.children[1].string(), "three");
    let first = ul.children[0].as_element().unwrap();
    assert_eq!(first.children.len(), 2);
    assert_eq!(first.children[0].element_type(), Some("p"));
    assert_eq!(first.children[0].string(), "one");
    assert_eq!(first.children[1].element_type(), Some("ul"));
    assert_eq!(first.children[1].children().len(), 1);
    assert_eq!(first.children[1].children()[0].string(), "two");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 0, 1, 0, 0], 0)))
    );

    assert!(editor.undo());
    assert_eq!(editor.element(&[0]).unwrap().children.len(), 3);
}

#[test]
fn indenting_multiple_list_items_nests_the_selection() {
    let mut editor = list_editor(vec![list("ul", vec!["one", "two", "three", "four"])]);
    editor.selection = Some(Range::new(
        Point::new(vec![0, 1, 0], 0),
        Point::new(vec![0, 3, 0], 5),
    ));

    assert!(editor.indent_list());
    assert_eq!(editor.document_string(), "one\ntwo\nthree\nfour\n");
    assert_eq!(editor.element(&[0]).unwrap().children.len(), 1);
    assert_eq!(editor.node(&[0, 0]).unwrap().string(), "onetwothreefour");
    assert_eq!(editor.element(&[0, 0, 1]).unwrap().children.len(), 1);
    assert_eq!(editor.node(&[0, 0, 1, 0]).unwrap().string(), "twothreefour");
    assert_eq!(editor.element(&[0, 0, 1, 0, 1]).unwrap().children.len(), 1);
    assert_eq!(
        editor.node(&[0, 0, 1, 0, 1, 0]).unwrap().string(),
        "threefour"
    );

    assert!(editor.undo());
    assert_eq!(editor.element(&[0]).unwrap().children.len(), 4);
}

#[test]
fn outdenting_multiple_nested_items_preserves_order() {
    let nested = Node::Element(Element::new(
        "ul",
        vec![
            Node::Element(Element::new("li", vec![Node::Text(Text::new("one"))])),
            Node::Element(Element::new(
                "li",
                vec![
                    Node::Text(Text::new("two")),
                    Node::Element(Element::new(
                        "ul",
                        vec![
                            Node::Element(Element::new(
                                "li",
                                vec![Node::Text(Text::new("child one"))],
                            )),
                            Node::Element(Element::new(
                                "li",
                                vec![Node::Text(Text::new("child two"))],
                            )),
                        ],
                    )),
                ],
            )),
        ],
    ));
    let mut editor = list_editor(vec![nested]);
    editor.selection = Some(Range::new(
        Point::new(vec![0, 1, 1, 0, 0], 0),
        Point::new(vec![0, 1, 1, 1, 0], 9),
    ));

    assert!(editor.outdent_list());
    let ul = editor.element(&[0]).unwrap();
    assert_eq!(ul.children.len(), 4);
    assert_eq!(ul.children[0].string(), "one");
    assert_eq!(ul.children[1].string(), "two");
    assert_eq!(ul.children[2].string(), "child one");
    assert_eq!(ul.children[3].string(), "child two");
    assert_eq!(ul.children[1].children().len(), 1);
}

#[test]
fn cannot_indent_the_first_list_item() {
    let mut editor = list_editor(vec![list("ul", vec!["one", "two"])]);
    editor.select_point(Point::new(vec![0, 0, 0], 0));
    assert!(!editor.indent_list());
}

#[test]
fn outdenting_a_nested_item_moves_it_into_the_parent_list() {
    let mut editor = list_editor(vec![Node::Element(Element::new(
        "ul",
        vec![
            Node::Element(Element::new("li", vec![Node::Text(Text::new("one"))])),
            Node::Element(Element::new(
                "li",
                vec![
                    Node::Text(Text::new("two")),
                    Node::Element(Element::new(
                        "ul",
                        vec![Node::Element(Element::new(
                            "li",
                            vec![Node::Text(Text::new("child"))],
                        ))],
                    )),
                ],
            )),
        ],
    ))]);
    editor.select_point(Point::new(vec![0, 1, 1, 0, 0], 0));

    assert!(editor.outdent_list());
    let ul = editor.element(&[0]).unwrap();
    assert_eq!(ul.children.len(), 3);
    assert_eq!(ul.children[1].string(), "two");
    assert_eq!(ul.children[2].string(), "child");
    assert_eq!(ul.children[1].children().len(), 1);
    assert_eq!(ul.children[1].children()[0].element_type(), Some("p"));
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 2, 0], 0)))
    );
}

#[test]
fn outdenting_a_top_level_item_lifts_it_to_a_paragraph() {
    let mut editor = list_editor(vec![list("ul", vec!["one", "two"])]);
    editor.select_point(Point::new(vec![0, 1, 0], 0));

    assert!(editor.outdent_list());
    assert_eq!(editor.children.len(), 2);
    assert_eq!(editor.children[0].element_type(), Some("ul"));
    assert_eq!(editor.children[0].children().len(), 1);
    assert_eq!(editor.children[1].element_type(), Some("p"));
    assert_eq!(editor.children[1].string(), "two");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![1, 0], 0)))
    );
}

#[test]
fn outdenting_the_only_list_item_removes_the_list() {
    let mut editor = list_editor(vec![list("ul", vec!["one"])]);
    editor.select_point(Point::new(vec![0, 0, 0], 0));
    assert!(editor.outdent_list());
    assert_eq!(editor.children.len(), 1);
    assert_eq!(editor.children[0].element_type(), Some("p"));
    assert_eq!(editor.children[0].string(), "one");
}

#[test]
fn backspace_at_the_first_list_item_lifts_it_before_the_list() {
    let mut editor = list_editor(vec![list("ul", vec!["one", "two"])]);
    editor.select_point(Point::new(vec![0, 0, 0], 0));

    editor.delete_backward();
    assert_eq!(editor.children.len(), 2);
    assert_eq!(editor.children[0].element_type(), Some("p"));
    assert_eq!(editor.children[0].string(), "one");
    assert_eq!(editor.children[1].element_type(), Some("ul"));
    assert_eq!(editor.children[1].children().len(), 1);
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 0], 0)))
    );
}

#[test]
fn backspace_at_a_later_list_item_merges_it_into_the_previous_item() {
    let mut editor = list_editor(vec![list("ul", vec!["one", "two"])]);
    editor.select_point(Point::new(vec![0, 1, 0], 0));

    editor.delete_backward();
    assert_eq!(editor.children.len(), 1);
    assert_eq!(editor.children[0].children().len(), 1);
    assert_eq!(editor.children[0].string(), "onetwo");
    assert_eq!(
        editor.selection,
        Some(Range::collapsed(Point::new(vec![0, 0, 0], 3)))
    );
}

#[test]
fn enter_inside_a_list_item_with_block_children_creates_a_new_item() {
    let mut editor = list_editor(vec![Node::Element(Element::new(
        "ul",
        vec![Node::Element(Element::new(
            "li",
            vec![
                Node::Element(Element::paragraph(vec![Node::Text(Text::new("one"))])),
                list("ul", vec!["child"]),
            ],
        ))],
    ))]);
    editor.select_point(Point::new(vec![0, 0, 0, 0], 2));
    editor.insert_break();

    let outer = editor.element(&[0]).unwrap();
    assert_eq!(outer.children.len(), 2);
    assert_eq!(outer.children[0].string(), "on");
    assert_eq!(outer.children[0].children().len(), 1);
    assert_eq!(outer.children[0].children()[0].element_type(), Some("p"));
    assert_eq!(outer.children[1].string(), "echild");
    assert_eq!(outer.children[1].children().len(), 2);
    assert_eq!(outer.children[1].children()[0].element_type(), Some("p"));
    assert_eq!(outer.children[1].children()[1].element_type(), Some("ul"));
}

#[test]
fn list_context_reports_the_item_and_position() {
    let editor = list_editor(vec![list("ol", vec!["one", "two"])]);
    let context = editor
        .list_context(&Point::new(vec![0, 1, 0], 1))
        .expect("inside list item");
    assert_eq!(context.item, vec![0, 1]);
    assert_eq!(context.list, vec![0]);
    assert_eq!(context.index, 1);
    assert_eq!(context.list_type, "ol");
}
