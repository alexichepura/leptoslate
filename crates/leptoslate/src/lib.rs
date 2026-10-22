//! A Slate-inspired rich text editor for [Leptos](https://leptos.dev).
//!
//! ```rust,ignore
//! use leptos::prelude::*;
//! use leptoslate::{Editable, EditorHandle};
//!
//! #[component]
//! fn App() -> impl IntoView {
//!     let editor = EditorHandle::empty();
//!     view! { <Editable editor=editor /> }
//! }
//! ```
//!
//! The document model, operations, normalization and history live in
//! [`leptoslate_core`]. This crate renders it into a `contenteditable`
//! surface, maps DOM selections to model selections, and handles input events.

pub mod dom;
pub mod handle;
mod media;
mod mention;
mod render;
mod renderers;

mod editable;

pub use editable::{keep_selection, select_point, Editable};
pub use handle::EditorHandle;
pub use media::FileHandler;
pub use mention::{MentionHandler, MentionTrigger};
pub use renderers::{ElementRenderContext, Renderers};

pub use leptoslate_core as core;
pub use leptoslate_core::{
    Affinity, Editor, Element, ElementRule, Node, Operation, Path, Point, Props, Range, Schema,
    SlateError, Text,
};

/// Base styles for the editable surface. The component injects these
/// automatically; they can be overridden by application CSS.
pub const EDITOR_STYLES: &str = r#"
.ls-editor-shell {
    position: relative;
}

.ls-editor {
    position: relative;
    outline: none;
    white-space: pre-wrap;
    overflow-wrap: break-word;
    min-height: 4rem;
}

.ls-editor[data-empty="true"]::before {
    content: attr(data-placeholder);
    position: absolute;
    top: 0;
    left: 0;
    pointer-events: none;
    color: #9ca3af;
    user-select: none;
}

.ls-text {
    white-space: pre-wrap;
}

.ls-text:empty::before {
    content: "\200b";
}

.ls-editor.ls-dragging {
    outline: 2px dashed rgba(180, 83, 42, 0.55);
    outline-offset: -6px;
    background: rgba(180, 83, 42, 0.04);
}

.ls-editor.ls-node-selected {
    caret-color: transparent;
}

.ls-void.ls-selected {
    outline: 2px solid #b4532a;
    outline-offset: 4px;
    border-radius: 4px;
}

.ls-block {
    display: block;
    min-height: 1.4em;
}

.ls-block ul,
.ls-block ol {
    padding-left: 1.5em;
    margin: 0.25em 0;
}

.ls-block li {
    display: list-item;
}

.ls-block pre,
pre.ls-block {
    white-space: pre-wrap;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    margin: 0.5em 0;
    padding: 0.6em 0.8em;
    border-radius: 6px;
    background: rgba(127, 127, 127, 0.12);
}

.ls-void {
    user-select: none;
}

.ls-void img {
    max-width: 100%;
}

hr.ls-void {
    border: none;
    border-top: 1px solid currentColor;
    opacity: 0.25;
    margin: 1em 0;
}
"#;
