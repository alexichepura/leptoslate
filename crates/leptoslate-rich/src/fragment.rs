//! Parsing HTML and Markdown fragments into editor nodes.

use serde_json::json;
use tl::{NodeHandle, Parser};

use leptoslate::core::{is_safe_href, is_safe_image_src, MarkKind, MarkSet};
use leptoslate::{Element, Node, Props, Text};

fn marks_with(marks: &Props, key: &str) -> Props {
    let mut next = marks.clone();
    next.insert(key.to_string(), json!(true));
    next
}

fn push_text(out: &mut Vec<Node>, text: &str, marks: &Props, preserve: bool) {
    if text.is_empty() {
        return;
    }
    let text = if preserve {
        text.to_string()
    } else {
        normalize_whitespace(text)
    };
    if text.is_empty() {
        return;
    }
    out.push(Node::Text(Text {
        text,
        properties: marks.clone(),
    }));
}

fn normalize_whitespace(text: &str) -> String {
    let mut out = String::new();
    let mut last_whitespace = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !last_whitespace {
                out.push(' ');
            }
            last_whitespace = true;
        } else {
            out.push(ch);
            last_whitespace = false;
        }
    }
    out
}

fn attr(tag: &tl::HTMLTag<'_>, parser: &Parser<'_>, key: &str) -> Option<String> {
    let _ = parser;
    tag.attributes()
        .get(key)
        .flatten()
        .map(|value| value.as_utf8_str().to_string())
        .filter(|value| !value.is_empty())
}

/// Parse an HTML fragment into block nodes using the default mark keys.
///
/// Common rich-text tags are supported: paragraphs, headings, lists,
/// blockquotes, code blocks, links, emphasis, line breaks, images and rules.
pub fn parse_html(html: &str) -> Vec<Node> {
    parse_html_with_marks(html, &MarkSet::default())
}

/// Parse an HTML fragment, writing marks with the configured property keys.
///
/// Conventional property names in `data-marks` attributes and inline styles
/// are still accepted and resolve to the configured keys.
pub fn parse_html_with_marks(html: &str, marks: &MarkSet) -> Vec<Node> {
    let Ok(dom) = tl::parse(html, tl::ParserOptions::default()) else {
        return Vec::new();
    };
    let parser = dom.parser();
    let builder = HtmlFragment { parser, marks };
    builder.blocks(dom.children())
}

struct HtmlFragment<'a, 'p, 'm> {
    parser: &'p Parser<'a>,
    marks: &'m MarkSet,
}

impl<'a, 'p, 'm> HtmlFragment<'a, 'p, 'm> {
    fn blocks(&self, handles: &[NodeHandle]) -> Vec<Node> {
        let mut blocks = Vec::new();
        let mut inline = Vec::new();
        for handle in handles {
            if let Some(node) = handle.get(self.parser) {
                self.collect(node, &mut blocks, &mut inline, &Props::new(), false);
            }
        }
        self.flush(&mut blocks, &mut inline);
        blocks
    }

    fn collect(
        &self,
        node: &tl::Node<'a>,
        blocks: &mut Vec<Node>,
        inline: &mut Vec<Node>,
        marks: &Props,
        preserve: bool,
    ) {
        match node {
            tl::Node::Raw(bytes) => {
                push_text(inline, bytes.as_utf8_str().as_ref(), marks, preserve)
            }
            tl::Node::Tag(tag) => {
                let name = tag.name().as_utf8_str().to_ascii_lowercase();
                if name == "img" {
                    self.flush(blocks, inline);
                    if let Some(image) = self.image(tag) {
                        blocks.push(image);
                    }
                } else if is_block(&name) {
                    self.flush(blocks, inline);
                    blocks.extend(self.block(tag, &name));
                } else if name == "br" {
                    inline.push(Node::text("\n"));
                } else {
                    inline.extend(self.inline(tag, &name, marks));
                }
            }
            tl::Node::Comment(_) => {}
        }
    }

    fn flush(&self, blocks: &mut Vec<Node>, inline: &mut Vec<Node>) {
        if inline.is_empty() {
            return;
        }
        let has_content = inline
            .iter()
            .any(|node| node.element_type().is_some() || !node.string().trim().is_empty());
        if has_content {
            blocks.push(Node::Element(Element::new("p", std::mem::take(inline))));
        } else {
            inline.clear();
        }
    }

    fn inline(&self, tag: &tl::HTMLTag<'a>, name: &str, marks: &Props) -> Vec<Node> {
        match name {
            "strong" | "b" => {
                self.inline_children(tag, &marks_with(marks, self.marks.key(MarkKind::Bold)))
            }
            "em" | "i" => {
                self.inline_children(tag, &marks_with(marks, self.marks.key(MarkKind::Italic)))
            }
            "u" => {
                self.inline_children(tag, &marks_with(marks, self.marks.key(MarkKind::Underline)))
            }
            "s" | "strike" | "del" => self.inline_children(
                tag,
                &marks_with(marks, self.marks.key(MarkKind::Strikethrough)),
            ),
            "code" => self.inline_children(tag, &marks_with(marks, self.marks.key(MarkKind::Code))),
            "sup" => self.inline_children(tag, &marks_with(marks, "superscript")),
            "sub" => self.inline_children(tag, &marks_with(marks, "subscript")),
            "br" => vec![Node::text("\n")],
            "img" => self.inline_image(tag).into_iter().collect(),
            "a" => {
                let children = self.inline_children(tag, marks);
                if children.is_empty() {
                    return Vec::new();
                }
                if let Some(href) = attr(tag, self.parser, "href") {
                    if is_safe_href(&href) {
                        vec![Node::Element(
                            Element::new("a", children).with_property("href", json!(href)),
                        )]
                    } else {
                        children
                    }
                } else {
                    children
                }
            }
            "span" => {
                let marks = self.span_marks(tag, marks);
                self.inline_children(tag, &marks)
            }
            "script" | "style" | "template" => Vec::new(),
            _ => self.inline_children(tag, marks),
        }
    }

    fn inline_children(&self, tag: &tl::HTMLTag<'a>, marks: &Props) -> Vec<Node> {
        let mut out = Vec::new();
        for handle in tag.children().top().as_slice() {
            if let Some(node) = handle.get(self.parser) {
                match node {
                    tl::Node::Raw(bytes) => {
                        push_text(&mut out, bytes.as_utf8_str().as_ref(), marks, false)
                    }
                    tl::Node::Tag(child) => {
                        let name = child.name().as_utf8_str().to_ascii_lowercase();
                        if name == "br" {
                            out.push(Node::text("\n"));
                        } else if name == "img" {
                            if let Some(image) = self.inline_image(child) {
                                out.push(image);
                            }
                        } else {
                            out.extend(self.inline(child, &name, marks));
                        }
                    }
                    tl::Node::Comment(_) => {}
                }
            }
        }
        out
    }

    fn span_marks(&self, tag: &tl::HTMLTag<'a>, marks: &Props) -> Props {
        let mut marks = marks.clone();
        if let Some(value) = attr(tag, self.parser, "data-marks") {
            for key in value.split(',') {
                let key = key.trim();
                if !key.is_empty() {
                    let key = self.marks.resolve(key).unwrap_or(key);
                    marks.insert(key.to_string(), json!(true));
                }
            }
        }
        if let Some(style) = attr(tag, self.parser, "style") {
            for declaration in style.split(';') {
                let Some((name, value)) = declaration.split_once(':') else {
                    continue;
                };
                let name = name.trim().to_ascii_lowercase();
                let value = value.trim();
                match name.as_str() {
                    "font-weight" => {
                        if value.eq_ignore_ascii_case("bold")
                            || value
                                .parse::<u32>()
                                .map(|weight| weight >= 600)
                                .unwrap_or(false)
                        {
                            marks.insert(self.marks.bold_key().into(), json!(true));
                        }
                    }
                    "font-style" if value.eq_ignore_ascii_case("italic") => {
                        marks.insert(self.marks.italic_key().into(), json!(true));
                    }
                    "text-decoration" | "text-decoration-line" => {
                        if value.contains("underline") {
                            marks.insert(self.marks.underline_key().into(), json!(true));
                        }
                        if value.contains("line-through") {
                            marks.insert(self.marks.strikethrough_key().into(), json!(true));
                        }
                    }
                    "font-family" if value.contains("monospace") || value.contains("Menlo") => {
                        marks.insert(self.marks.code_key().into(), json!(true));
                    }
                    "vertical-align" if value.eq_ignore_ascii_case("super") => {
                        marks.insert("superscript".into(), json!(true));
                    }
                    "vertical-align" if value.eq_ignore_ascii_case("sub") => {
                        marks.insert("subscript".into(), json!(true));
                    }
                    "color" => {
                        marks.insert("color".into(), json!(value));
                    }
                    "background" | "background-color" => {
                        marks.insert("background".into(), json!(value));
                    }
                    _ => {}
                }
            }
        }
        marks
    }

    fn block(&self, tag: &tl::HTMLTag<'a>, name: &str) -> Vec<Node> {
        match name {
            "p" => vec![Node::Element(Element::new(
                "p",
                self.inline_children(tag, &Props::new()),
            ))],
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => vec![Node::Element(Element::new(
                name,
                self.inline_children(tag, &Props::new()),
            ))],
            "blockquote" => {
                if self.has_block_children(tag) {
                    vec![Node::Element(Element::new(
                        "blockquote",
                        self.blocks(tag.children().top().as_slice()),
                    ))]
                } else {
                    vec![Node::Element(Element::new(
                        "blockquote",
                        self.inline_children(tag, &Props::new()),
                    ))]
                }
            }
            "pre" => {
                let mut text = tag.inner_text(self.parser).to_string();
                if text.ends_with('\n') {
                    text.pop();
                }
                vec![Node::Element(Element::new(
                    "code",
                    vec![Node::Text(Text::new(text))],
                ))]
            }
            "ul" | "ol" => vec![self.list(tag, name == "ol")],
            "li" => vec![self.list_item(tag)],
            "hr" => vec![Node::Element(Element::empty("hr"))],
            "img" => self.image(tag).into_iter().collect(),
            "figure" => {
                if let Some(image) = tag.children().top().as_slice().iter().find_map(|handle| {
                    handle
                        .get(self.parser)
                        .and_then(tl::Node::as_tag)
                        .filter(|child| child.name().as_utf8_str() == "img")
                }) {
                    self.image(image).into_iter().collect()
                } else {
                    self.container(tag)
                }
            }
            "html" | "body" | "div" | "section" | "article" | "header" | "footer" | "main"
            | "aside" | "nav" | "address" | "figcaption" => self.container(tag),
            "table" => self.table(tag),
            "style" | "script" | "meta" | "title" | "head" | "link" => Vec::new(),
            _ => self.container(tag),
        }
    }

    fn container(&self, tag: &tl::HTMLTag<'a>) -> Vec<Node> {
        if self.has_block_children(tag) {
            self.blocks(tag.children().top().as_slice())
        } else {
            vec![Node::Element(Element::new(
                "p",
                self.inline_children(tag, &Props::new()),
            ))]
        }
    }

    fn has_block_children(&self, tag: &tl::HTMLTag<'a>) -> bool {
        tag.children().top().as_slice().iter().any(|handle| {
            handle
                .get(self.parser)
                .and_then(tl::Node::as_tag)
                .map(|child| is_block(child.name().as_utf8_str().as_ref()))
                .unwrap_or(false)
        })
    }

    fn list(&self, tag: &tl::HTMLTag<'a>, ordered: bool) -> Node {
        let mut items = Vec::new();
        for handle in tag.children().top().as_slice() {
            if let Some(item) = handle
                .get(self.parser)
                .and_then(tl::Node::as_tag)
                .filter(|child| child.name().as_utf8_str() == "li")
            {
                items.push(self.list_item(item));
            }
        }
        Node::Element(Element::new(if ordered { "ol" } else { "ul" }, items))
    }

    fn list_item(&self, tag: &tl::HTMLTag<'a>) -> Node {
        let inline = self.inline_children_without_lists(tag, &Props::new());
        let mut nested = Vec::new();
        for handle in tag.children().top().as_slice() {
            if let Some(child) = handle
                .get(self.parser)
                .and_then(tl::Node::as_tag)
                .filter(|child| matches!(child.name().as_utf8_str().as_ref(), "ul" | "ol"))
            {
                nested.push(self.list(child, child.name().as_utf8_str() == "ol"));
            }
        }

        if nested.is_empty() {
            Node::Element(Element::new("li", inline))
        } else {
            let mut children = Vec::new();
            if !inline.is_empty() {
                children.push(Node::Element(Element::new("p", inline)));
            }
            children.extend(nested);
            Node::Element(Element::new("li", children))
        }
    }

    fn inline_children_without_lists(&self, tag: &tl::HTMLTag<'a>, marks: &Props) -> Vec<Node> {
        let mut out = Vec::new();
        for handle in tag.children().top().as_slice() {
            if let Some(node) = handle.get(self.parser) {
                match node {
                    tl::Node::Raw(bytes) => {
                        push_text(&mut out, bytes.as_utf8_str().as_ref(), marks, false)
                    }
                    tl::Node::Tag(child) => {
                        let name = child.name().as_utf8_str().to_ascii_lowercase();
                        match name.as_str() {
                            "ul" | "ol" => {}
                            "br" => out.push(Node::text("\n")),
                            "img" => {
                                if let Some(image) = self.inline_image(child) {
                                    out.push(image);
                                }
                            }
                            _ => out.extend(self.inline(child, &name, marks)),
                        }
                    }
                    tl::Node::Comment(_) => {}
                }
            }
        }
        out
    }

    fn table(&self, tag: &tl::HTMLTag<'a>) -> Vec<Node> {
        let mut children = Vec::new();
        let mut loose_rows = Vec::new();
        for handle in tag.children().top().as_slice() {
            let Some(child) = handle.get(self.parser).and_then(tl::Node::as_tag) else {
                continue;
            };
            let name = child.name().as_utf8_str().to_ascii_lowercase();
            match name.as_str() {
                "thead" | "tbody" | "tfoot" => {
                    if let Some(section) = self.table_section(child, &name) {
                        children.push(section);
                    }
                }
                "tr" => {
                    if let Some(row) = self.table_row(child) {
                        loose_rows.push(row);
                    }
                }
                _ => {}
            }
        }
        if !loose_rows.is_empty() {
            let header_count = if children.is_empty() {
                loose_rows
                    .iter()
                    .take_while(|row| {
                        !row.children().is_empty()
                            && row
                                .children()
                                .iter()
                                .all(|cell| cell.element_type() == Some("th"))
                    })
                    .count()
            } else {
                0
            };
            if header_count > 0 {
                let body = loose_rows.split_off(header_count);
                children.push(Node::Element(Element::new("thead", loose_rows)));
                if !body.is_empty() {
                    children.push(Node::Element(Element::new("tbody", body)));
                }
            } else {
                children.push(Node::Element(Element::new("tbody", loose_rows)));
            }
        }

        if children.is_empty() {
            let text = normalize_whitespace(tag.inner_text(self.parser).as_ref())
                .trim()
                .to_string();
            return if text.is_empty() {
                Vec::new()
            } else {
                vec![Node::Element(Element::new("p", vec![Node::text(text)]))]
            };
        }

        vec![Node::Element(Element::new("table", children))]
    }

    fn table_section(&self, tag: &tl::HTMLTag<'a>, name: &str) -> Option<Node> {
        let rows: Vec<Node> = tag
            .children()
            .top()
            .as_slice()
            .iter()
            .filter_map(|handle| handle.get(self.parser).and_then(tl::Node::as_tag))
            .filter(|child| child.name().as_utf8_str() == "tr")
            .filter_map(|row| self.table_row(row))
            .collect();
        if rows.is_empty() {
            None
        } else {
            Some(Node::Element(Element::new(name, rows)))
        }
    }

    fn table_row(&self, tag: &tl::HTMLTag<'a>) -> Option<Node> {
        let cells: Vec<Node> = tag
            .children()
            .top()
            .as_slice()
            .iter()
            .filter_map(|handle| handle.get(self.parser).and_then(tl::Node::as_tag))
            .filter_map(|cell| {
                let name = cell.name().as_utf8_str().to_ascii_lowercase();
                matches!(name.as_str(), "td" | "th").then(|| self.table_cell(cell, &name))
            })
            .collect();
        if cells.is_empty() {
            None
        } else {
            Some(Node::Element(Element::new("tr", cells)))
        }
    }

    fn table_cell(&self, tag: &tl::HTMLTag<'a>, name: &str) -> Node {
        let mut children = self.inline_children(tag, &Props::new());
        if children.is_empty() {
            children.push(Node::empty_text());
        }
        Node::Element(Element::new(name, children))
    }

    fn inline_image(&self, tag: &tl::HTMLTag<'a>) -> Option<Node> {
        let src = attr(tag, self.parser, "src").unwrap_or_default();
        if !is_safe_image_src(&src) {
            return None;
        }
        let alt = attr(tag, self.parser, "alt").unwrap_or_default();
        Some(Node::Element(
            Element::new("inline-image", vec![Node::empty_text()])
                .with_property("src", json!(src))
                .with_property("alt", json!(alt)),
        ))
    }

    fn image(&self, tag: &tl::HTMLTag<'a>) -> Option<Node> {
        let src = attr(tag, self.parser, "src").unwrap_or_default();
        if !is_safe_image_src(&src) {
            return None;
        }
        let alt = attr(tag, self.parser, "alt").unwrap_or_default();
        Some(Node::Element(
            Element::new("img", vec![Node::empty_text()])
                .with_property("src", json!(src))
                .with_property("alt", json!(alt)),
        ))
    }
}

fn is_block(name: &str) -> bool {
    matches!(
        name,
        "p" | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "blockquote"
            | "pre"
            | "ul"
            | "ol"
            | "li"
            | "hr"
            | "figure"
            | "figcaption"
            | "html"
            | "body"
            | "div"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "main"
            | "aside"
            | "nav"
            | "address"
            | "table"
            | "style"
            | "script"
            | "meta"
            | "title"
            | "head"
            | "link"
    )
}

/// Parse a Markdown fragment into blocks using the default mark keys.
///
/// Supports headings, paragraphs, flat lists, blockquotes, fenced code,
/// horizontal rules and common inline marks/links.
pub fn parse_markdown(markdown: &str) -> Vec<Node> {
    parse_markdown_with_marks(markdown, &MarkSet::default())
}

/// Parse a Markdown fragment, writing marks with the configured property keys.
pub fn parse_markdown_with_marks(markdown: &str, marks: &MarkSet) -> Vec<Node> {
    let text = markdown.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    let mut blocks = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim();

        if trimmed.is_empty() {
            index += 1;
            continue;
        }

        if let Some((language, code)) = fenced_code(&lines, &mut index) {
            let mut node = Element::new("code", vec![Node::Text(Text::new(code))]);
            if !language.is_empty() {
                node = node.with_property("language", json!(language));
            }
            blocks.push(Node::Element(node));
            continue;
        }

        if let Some((level, content)) = heading(trimmed) {
            let element_type = format!("h{level}");
            blocks.push(Node::Element(Element::new(
                element_type,
                inline_markdown(content, marks, &Props::new()),
            )));
            index += 1;
            continue;
        }

        if is_rule(trimmed) {
            blocks.push(Node::Element(Element::empty("hr")));
            index += 1;
            continue;
        }

        if trimmed.starts_with('>') {
            let mut inner = String::new();
            while index < lines.len() && lines[index].trim_start().starts_with('>') {
                let line = lines[index].trim_start();
                let content = line.strip_prefix('>').unwrap_or(line);
                inner.push_str(content.strip_prefix(' ').unwrap_or(content));
                inner.push('\n');
                index += 1;
            }
            let parsed = parse_markdown_with_marks(&inner, marks);
            if parsed.len() == 1 {
                if let Node::Element(element) = &parsed[0] {
                    if element.element_type == "p" {
                        blocks.push(Node::Element(Element::new(
                            "blockquote",
                            element.children.clone(),
                        )));
                        continue;
                    }
                }
            }
            blocks.push(Node::Element(Element::new("blockquote", parsed)));
            continue;
        }

        if list_marker(trimmed).is_some() {
            let indent = indentation(line);
            blocks.push(parse_markdown_list(&lines, &mut index, indent, marks));
            continue;
        }

        let mut paragraph = String::new();
        while index < lines.len() {
            let line = lines[index];
            let trimmed = line.trim();
            if trimmed.is_empty()
                || fenced_code_start(trimmed)
                || heading(trimmed).is_some()
                || is_rule(trimmed)
                || trimmed.starts_with('>')
                || list_marker(trimmed).is_some()
            {
                break;
            }
            if !paragraph.is_empty() {
                paragraph.push(' ');
            }
            paragraph.push_str(trimmed);
            index += 1;
        }
        if !paragraph.is_empty() {
            blocks.push(Node::Element(Element::new(
                "p",
                inline_markdown(&paragraph, marks, &Props::new()),
            )));
        }
    }

    blocks
}

fn indentation(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn parse_markdown_list(lines: &[&str], index: &mut usize, indent: usize, marks: &MarkSet) -> Node {
    let first = lines[*index];
    let Some(ordered) = first.get(indent..).and_then(list_marker) else {
        return Node::Element(Element::new("ul", vec![]));
    };
    let element_type = if ordered { "ol" } else { "ul" };
    let mut items = Vec::new();

    while *index < lines.len() {
        let line = lines[*index];
        if line.trim().is_empty() || indentation(line) != indent {
            break;
        }
        let content = &line[indent..];
        let Some(item_ordered) = list_marker(content) else {
            break;
        };
        if item_ordered != ordered {
            break;
        }

        let mut inline = inline_markdown(
            list_content(content).unwrap_or(content),
            marks,
            &Props::new(),
        );
        *index += 1;
        let mut nested = Vec::new();

        while *index < lines.len() {
            let next = lines[*index];
            if next.trim().is_empty() {
                break;
            }
            let next_indent = indentation(next);
            if next_indent <= indent {
                break;
            }
            let next_content = &next[next_indent..];
            if list_marker(next_content).is_some() {
                nested.push(parse_markdown_list(lines, index, next_indent, marks));
            } else {
                // Treat an indented continuation as another run of inline content.
                if !inline.is_empty() {
                    inline.push(Node::text(" "));
                }
                inline.extend(inline_markdown(next.trim(), marks, &Props::new()));
                *index += 1;
            }
        }

        if nested.is_empty() {
            items.push(Node::Element(Element::new("li", inline)));
        } else {
            let mut children = Vec::new();
            if !inline.is_empty() {
                children.push(Node::Element(Element::new("p", inline)));
            }
            children.extend(nested);
            items.push(Node::Element(Element::new("li", children)));
        }
    }

    Node::Element(Element::new(element_type, items))
}

fn fenced_code_start(line: &str) -> bool {
    line.starts_with("```") || line.starts_with("~~~")
}

fn fenced_code(lines: &[&str], index: &mut usize) -> Option<(String, String)> {
    let line = lines.get(*index)?.trim_start();
    let fence = if line.starts_with("```") {
        "```"
    } else if line.starts_with("~~~") {
        "~~~"
    } else {
        return None;
    };
    let language = line[fence.len()..].trim().to_string();
    *index += 1;
    let mut code = Vec::new();
    while *index < lines.len() {
        let line = lines[*index];
        if line.trim_start().starts_with(fence) {
            *index += 1;
            return Some((language, code.join("\n")));
        }
        code.push(line);
        *index += 1;
    }
    Some((language, code.join("\n")))
}

fn heading(line: &str) -> Option<(usize, &str)> {
    let hashes = line.chars().take_while(|ch| *ch == '#').count();
    if (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') {
        Some((hashes, line[hashes + 1..].trim()))
    } else {
        None
    }
}

fn is_rule(line: &str) -> bool {
    let cleaned: String = line.chars().filter(|ch| !ch.is_whitespace()).collect();
    cleaned.len() >= 3
        && (cleaned.chars().all(|ch| ch == '-')
            || cleaned.chars().all(|ch| ch == '*')
            || cleaned.chars().all(|ch| ch == '_'))
}

fn list_marker(line: &str) -> Option<bool> {
    if line.starts_with("- ") || line.starts_with("* ") || line.starts_with("+ ") {
        return Some(false);
    }
    let digits = line.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digits > 0 && line[digits..].starts_with(". ") {
        Some(true)
    } else {
        None
    }
}

fn list_content(line: &str) -> Option<&str> {
    if let Some(content) = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("+ "))
    {
        return Some(content);
    }
    let digits = line.chars().take_while(|ch| ch.is_ascii_digit()).count();
    line.get(digits..)?.strip_prefix(". ")
}

fn inline_markdown(text: &str, mark_set: &MarkSet, properties: &Props) -> Vec<Node> {
    let mut out = Vec::new();
    let mut index = 0;
    let mut buffer = String::new();

    while index < text.len() {
        let rest = &text[index..];

        if let Some((kind, delimiter)) = inline_delimiter(rest) {
            let content_start = index + delimiter.len();
            if let Some(end) = text[content_start..].find(delimiter) {
                let content = &text[content_start..content_start + end];
                if !buffer.is_empty() {
                    push_text(&mut out, &buffer, properties, false);
                    buffer.clear();
                }
                let key = mark_set.key(kind);
                let next_properties = if kind == MarkKind::Code {
                    let mut properties = properties.clone();
                    properties.insert(key.into(), json!(true));
                    properties
                } else {
                    marks_with(properties, key)
                };
                if kind == MarkKind::Code {
                    out.push(Node::Text(Text {
                        text: content.to_string(),
                        properties: next_properties,
                    }));
                } else {
                    out.extend(inline_markdown(content, mark_set, &next_properties));
                }
                index = content_start + end + delimiter.len();
                continue;
            }
        }

        if rest.starts_with('[') {
            if let Some(close) = rest.find("](") {
                if let Some(end) = rest[close + 2..].find(')') {
                    let label = &rest[1..close];
                    let href = &rest[close + 2..close + 2 + end];
                    if !buffer.is_empty() {
                        push_text(&mut out, &buffer, properties, false);
                        buffer.clear();
                    }
                    let children = inline_markdown(label, mark_set, properties);
                    out.push(Node::Element(
                        Element::new("a", children).with_property("href", json!(href)),
                    ));
                    index += close + 2 + end + 1;
                    continue;
                }
            }
        }

        let ch = rest.chars().next().unwrap();
        buffer.push(ch);
        index += ch.len_utf8();
    }

    if !buffer.is_empty() {
        push_text(&mut out, &buffer, properties, false);
    }
    out
}

fn inline_delimiter(text: &str) -> Option<(MarkKind, &'static str)> {
    if text.starts_with("**") {
        Some((MarkKind::Bold, "**"))
    } else if text.starts_with("__") {
        Some((MarkKind::Bold, "__"))
    } else if text.starts_with("~~") {
        Some((MarkKind::Strikethrough, "~~"))
    } else if text.starts_with('*') {
        Some((MarkKind::Italic, "*"))
    } else if text.starts_with('_') {
        Some((MarkKind::Italic, "_"))
    } else if text.starts_with('`') {
        Some((MarkKind::Code, "`"))
    } else {
        None
    }
}
