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

mod change;
pub mod dom;
pub mod handle;
mod media;
mod mention;
mod plugins;
mod render;
mod renderers;

mod editable;

pub use change::document_fingerprint;
pub use editable::{keep_selection, select_point, Editable};
pub use handle::EditorHandle;
pub use media::FileHandler;
pub use mention::{MentionHandler, MentionTrigger};
pub use plugins::{EditablePlugin, EditablePlugins};
pub use renderers::{ElementRenderContext, Renderers};

pub use leptoslate_core as core;
pub use leptoslate_core::{
    Affinity, Editor, Element, ElementRule, MarkKind, MarkSet, Node, Operation, Path, Point, Props,
    Range, Schema, SlateError, Text,
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

.ls-sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
}

.ls-hover-bar button:focus-visible,
.ls-hover-bar .ls-drag-handle:focus-visible,
.ls-selection-bubble button:focus-visible {
    outline: 2px solid #b4532a;
    outline-offset: 1px;
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

.ls-block.ls-block-selected {
    background: rgba(180, 83, 42, 0.08);
    box-shadow: inset 3px 0 0 #b4532a;
    border-radius: 3px;
}

.ls-hover-bar {
    position: absolute;
    z-index: 30;
    display: flex;
    flex-direction: column;
    gap: 1px;
    width: 28px;
    padding: 2px;
    border: 1px solid rgba(17, 24, 39, 0.09);
    border-radius: 8px;
    background: #fff;
    box-shadow: 0 8px 24px -12px rgba(17, 24, 39, 0.35);
    opacity: 0;
    pointer-events: none;
    transition: opacity 0.1s ease;
}

.ls-hover-bar.visible {
    opacity: 1;
    pointer-events: auto;
}

.ls-hover-bar button {
    width: 22px;
    height: 22px;
    padding: 0;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: #6b7280;
    font-family: inherit;
    font-size: 13px;
    line-height: 1;
    cursor: pointer;
}

.ls-hover-bar button:hover {
    background: #f3f4f6;
    color: #1f2430;
}

.ls-hover-bar .ls-drag-handle {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border-radius: 5px;
    color: #6b7280;
    font-size: 13px;
    line-height: 1;
    cursor: grab;
    user-select: none;
}

.ls-hover-bar .ls-drag-handle:hover {
    background: #f3f4f6;
    color: #1f2430;
}

.ls-hover-bar .ls-drag-handle:active {
    cursor: grabbing;
}

.ls-drop-indicator {
    position: absolute;
    z-index: 28;
    height: 2px;
    border-radius: 2px;
    background: #b4532a;
    box-shadow: 0 0 0 2px rgba(180, 83, 42, 0.12);
    opacity: 0;
    pointer-events: none;
    transition: opacity 0.08s ease;
}

.ls-drop-indicator.visible {
    opacity: 1;
}

.ls-selection-bubble {
    position: absolute;
    z-index: 35;
    display: flex;
    align-items: center;
    gap: 1px;
    padding: 3px;
    border: 1px solid rgba(17, 24, 39, 0.1);
    border-radius: 9px;
    background: #fff;
    box-shadow: 0 10px 30px -14px rgba(17, 24, 39, 0.45);
    opacity: 0;
    pointer-events: none;
    transform: translateX(-50%);
    transition: opacity 0.1s ease;
}

.ls-selection-bubble.visible {
    opacity: 1;
    pointer-events: auto;
}

.ls-selection-bubble .ls-bubble-mark {
    min-width: 26px;
    height: 26px;
    padding: 0 5px;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: #4b5563;
    font-family: inherit;
    font-size: 12px;
    font-weight: 600;
    line-height: 1;
    cursor: pointer;
}

.ls-selection-bubble .ls-bubble-mark:hover {
    background: #f3f4f6;
    color: #111827;
}

.ls-selection-bubble .ls-bubble-mark.active {
    background: #fdeee5;
    color: #b4532a;
}

.ls-bubble-divider {
    width: 1px;
    height: 18px;
    margin: 0 2px;
    background: #e5e7eb;
}

.ls-block {
    display: block;
    min-height: 1.4em;
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
