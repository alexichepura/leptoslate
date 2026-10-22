//! Parsing HTML and Markdown fragments into editor nodes.

use serde_json::json;
use tl::{NodeHandle, Parser};

use crate::{
    node::{Element, Node, Text},
    props::Props,
};

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

/// Parse an HTML fragment into block nodes.
///
/// Common rich-text tags are supported: paragraphs, headings, lists,
/// blockquotes, code blocks, links, emphasis, line breaks, images and rules.
pub fn parse_html(html: &str) -> Vec<Node> {
    let Ok(dom) = tl::parse(html, tl::ParserOptions::default()) else {
        return Vec::new();
    };
    let parser = dom.parser();
    let builder = HtmlFragment { parser };
    builder.blocks(dom.children())
}

struct HtmlFragment<'a, 'p> {
    parser: &'p Parser<'a>,
}

impl<'a, 'p> HtmlFragment<'a, 'p> {
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
                    blocks.push(self.image(tag));
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
            "strong" | "b" => self.inline_children(tag, &marks_with(marks, "bold")),
            "em" | "i" => self.inline_children(tag, &marks_with(marks, "italic")),
            "u" => self.inline_children(tag, &marks_with(marks, "underline")),
            "s" | "strike" | "del" => {
                self.inline_children(tag, &marks_with(marks, "strikethrough"))
            }
            "code" => self.inline_children(tag, &marks_with(marks, "code")),
            "sup" => self.inline_children(tag, &marks_with(marks, "superscript")),
            "sub" => self.inline_children(tag, &marks_with(marks, "subscript")),
            "br" => vec![Node::text("\n")],
            "img" => vec![self.inline_image(tag)],
            "a" => {
                let children = self.inline_children(tag, marks);
                if children.is_empty() {
                    return Vec::new();
                }
                if let Some(href) = attr(tag, self.parser, "href") {
                    vec![Node::Element(
                        Element::new("a", children).with_property("href", json!(href)),
                    )]
                } else {
                    children
                }
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
                            out.push(self.inline_image(child));
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
            "img" => vec![self.image(tag)],
            "figure" => {
                if let Some(image) = tag.children().top().as_slice().iter().find_map(|handle| {
                    handle
                        .get(self.parser)
                        .and_then(tl::Node::as_tag)
                        .filter(|child| child.name().as_utf8_str() == "img")
                }) {
                    vec![self.image(image)]
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
                            "img" => out.push(self.inline_image(child)),
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
        let mut rows = Vec::new();
        self.table_rows(tag, &mut rows);
        rows
    }

    fn table_rows(&self, tag: &tl::HTMLTag<'a>, rows: &mut Vec<Node>) {
        for handle in tag.children().top().as_slice() {
            let Some(child) = handle.get(self.parser).and_then(tl::Node::as_tag) else {
                continue;
            };
            let name = child.name().as_utf8_str().to_ascii_lowercase();
            match name.as_str() {
                "tr" => rows.push(self.table_row(child)),
                "thead" | "tbody" | "tfoot" => self.table_rows(child, rows),
                _ => {}
            }
        }
    }

    fn table_row(&self, tag: &tl::HTMLTag<'a>) -> Node {
        let mut inline = Vec::new();
        for handle in tag.children().top().as_slice() {
            let Some(cell) = handle.get(self.parser).and_then(tl::Node::as_tag) else {
                continue;
            };
            let name = cell.name().as_utf8_str().to_ascii_lowercase();
            if !matches!(name.as_str(), "td" | "th") {
                continue;
            }
            if !inline.is_empty() {
                inline.push(Node::text(" | "));
            }
            let marks = if name == "th" {
                marks_with(&Props::new(), "bold")
            } else {
                Props::new()
            };
            inline.extend(self.inline_children(cell, &marks));
        }
        Node::Element(Element::new("p", inline))
    }

    fn inline_image(&self, tag: &tl::HTMLTag<'a>) -> Node {
        let src = attr(tag, self.parser, "src").unwrap_or_default();
        let alt = attr(tag, self.parser, "alt").unwrap_or_default();
        Node::Element(
            Element::new("inline-image", vec![Node::empty_text()])
                .with_property("src", json!(src))
                .with_property("alt", json!(alt)),
        )
    }

    fn image(&self, tag: &tl::HTMLTag<'a>) -> Node {
        let src = attr(tag, self.parser, "src").unwrap_or_default();
        let alt = attr(tag, self.parser, "alt").unwrap_or_default();
        Node::Element(
            Element::new("img", vec![Node::empty_text()])
                .with_property("src", json!(src))
                .with_property("alt", json!(alt)),
        )
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

/// Parse a Markdown fragment into blocks.
///
/// Supports headings, paragraphs, flat lists, blockquotes, fenced code,
/// horizontal rules and common inline marks/links.
pub fn parse_markdown(markdown: &str) -> Vec<Node> {
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
                inline_markdown(content, &Props::new()),
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
            let parsed = parse_markdown(&inner);
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

        if let Some(ordered) = list_marker(trimmed) {
            let element_type = if ordered { "ol" } else { "ul" };
            let mut items = Vec::new();
            while index < lines.len() {
                let line = lines[index].trim();
                match list_marker(line) {
                    Some(item_ordered) if item_ordered == ordered => {
                        let content = list_content(line).unwrap_or(line);
                        items.push(Node::Element(Element::new(
                            "li",
                            inline_markdown(content, &Props::new()),
                        )));
                        index += 1;
                    }
                    _ => break,
                }
            }
            blocks.push(Node::Element(Element::new(element_type, items)));
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
                inline_markdown(&paragraph, &Props::new()),
            )));
        }
    }

    blocks
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

fn inline_markdown(text: &str, marks: &Props) -> Vec<Node> {
    let mut out = Vec::new();
    let mut index = 0;
    let mut buffer = String::new();

    while index < text.len() {
        let rest = &text[index..];

        if let Some((mark, delimiter)) = inline_delimiter(rest) {
            let content_start = index + delimiter.len();
            if let Some(end) = text[content_start..].find(delimiter) {
                let content = &text[content_start..content_start + end];
                if !buffer.is_empty() {
                    push_text(&mut out, &buffer, marks, false);
                    buffer.clear();
                }
                let next_marks = if mark == "code" {
                    let mut properties = marks.clone();
                    properties.insert("code".into(), json!(true));
                    properties
                } else {
                    marks_with(marks, mark)
                };
                if mark == "code" {
                    out.push(Node::Text(Text {
                        text: content.to_string(),
                        properties: next_marks,
                    }));
                } else {
                    out.extend(inline_markdown(content, &next_marks));
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
                        push_text(&mut out, &buffer, marks, false);
                        buffer.clear();
                    }
                    let children = inline_markdown(label, marks);
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
        push_text(&mut out, &buffer, marks, false);
    }
    out
}

fn inline_delimiter(text: &str) -> Option<(&'static str, &'static str)> {
    if text.starts_with("**") {
        Some(("bold", "**"))
    } else if text.starts_with("__") {
        Some(("bold", "__"))
    } else if text.starts_with("~~") {
        Some(("strikethrough", "~~"))
    } else if text.starts_with('*') {
        Some(("italic", "*"))
    } else if text.starts_with('_') {
        Some(("italic", "_"))
    } else if text.starts_with('`') {
        Some(("code", "`"))
    } else {
        None
    }
}
