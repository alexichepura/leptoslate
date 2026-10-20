//! UTF-8 / UTF-16 offset helpers.
//!
//! The document model uses offsets measured in Unicode scalar values (`char`s),
//! which is the natural unit in Rust. Browsers report `Selection` offsets in
//! UTF-16 code units, so the DOM integration converts at the boundary.

/// Number of characters (Unicode scalar values) in `text`.
pub fn char_len(text: &str) -> usize {
    text.chars().count()
}

/// Convert a UTF-16 code unit offset into a character offset.
///
/// Offsets past the end of the string are clamped to the string length. An
/// offset pointing into the middle of a surrogate pair is rounded down.
pub fn utf16_to_char_index(text: &str, utf16_index: usize) -> usize {
    let mut utf16 = 0;
    for (char_index, ch) in text.chars().enumerate() {
        if utf16 >= utf16_index {
            return char_index;
        }
        utf16 += ch.len_utf16();
        if utf16 > utf16_index {
            return char_index;
        }
    }
    text.chars().count()
}

/// Convert a character offset into a UTF-16 code unit offset.
pub fn char_to_utf16_index(text: &str, char_index: usize) -> usize {
    text.chars().take(char_index).map(char::len_utf16).sum()
}

/// Byte index for a character offset, clamped to the end of the string.
pub fn char_to_byte_index(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(byte, _)| byte)
        .unwrap_or(text.len())
}

/// Slice by character offsets, clamped to the string length.
pub fn char_slice(text: &str, start: usize, end: usize) -> &str {
    let start = start.min(char_len(text));
    let end = end.max(start).min(char_len(text));
    &text[char_to_byte_index(text, start)..char_to_byte_index(text, end)]
}

/// Replace the range `[start, end)` of `text` with `replacement`.
pub fn splice(text: &mut String, start: usize, end: usize, replacement: &str) {
    let len = char_len(text);
    let start = start.min(len);
    let end = end.max(start).min(len);
    let start_byte = char_to_byte_index(text, start);
    let end_byte = char_to_byte_index(text, end);
    text.replace_range(start_byte..end_byte, replacement);
}

/// The character immediately before `offset`, if any.
pub fn char_before(text: &str, offset: usize) -> Option<(usize, char)> {
    if offset == 0 {
        None
    } else {
        text.chars().nth(offset - 1).map(|ch| (offset - 1, ch))
    }
}

/// The character at `offset`, if any.
pub fn char_at(text: &str, offset: usize) -> Option<char> {
    text.chars().nth(offset)
}

/// Find the start of the word preceding `offset` (whitespace delimited).
pub fn word_start_before(text: &str, offset: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut i = offset.min(chars.len());
    while i > 0 && chars[i - 1].is_whitespace() {
        i -= 1;
    }
    while i > 0 && !chars[i - 1].is_whitespace() {
        i -= 1;
    }
    i
}

/// Find the end of the word following `offset` (whitespace delimited).
pub fn word_end_after(text: &str, offset: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut i = offset.min(chars.len());
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    while i < chars.len() && !chars[i].is_whitespace() {
        i += 1;
    }
    i
}
