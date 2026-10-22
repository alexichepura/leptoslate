//! Mention trigger hooks for the editable surface.

use leptos::callback::Callback;
use leptoslate_core::Range;

/// An active `@query` sequence under the caret.
#[derive(Clone, Debug, PartialEq)]
pub struct MentionTrigger {
    /// Text after `@` and before the caret.
    pub query: String,
    /// The text range covering `@query`. Replace it with the selected atom.
    pub range: Range,
}

/// Called whenever the active mention trigger changes.
///
/// `None` means the caret left the `@query` sequence or the picker was dismissed.
pub type MentionHandler = Callback<Option<MentionTrigger>>;
