//! Pluggable element rendering.
//!
//! The built-in renderer maps schema element types to HTML tags. Applications
//! can register their own Leptos views for specific element types, which is how
//! custom blocks (callouts, todos, media, embeds, …) are built.

use std::{collections::HashMap, sync::Arc};

use leptos::prelude::AnyView;
use leptoslate_core::{props, Element, Path};

use crate::handle::EditorHandle;

type ElementRenderFn = Arc<dyn Fn(ElementRenderContext) -> AnyView + Send + Sync>;

/// Everything a custom element renderer needs.
///
/// `children` contains the already-rendered child views (text spans, nested
/// blocks). Void elements have no children.
pub struct ElementRenderContext {
    /// The element being rendered.
    pub element: Element,
    /// The element's path in the document.
    pub path: Path,
    /// Dot-separated path, useful for the `data-node-path` attribute.
    pub node_path: String,
    /// Default library class for this element.
    pub class: String,
    /// Rendered children. Empty for void elements.
    pub children: Vec<AnyView>,
    /// A handle for node-local actions (toggle a property, delete the node, …).
    pub editor: EditorHandle,
    pub is_inline: bool,
    pub is_void: bool,
}

impl ElementRenderContext {
    pub fn element_type(&self) -> &str {
        &self.element.element_type
    }

    pub fn class_with(&self, extra: &str) -> String {
        if extra.is_empty() {
            self.class.clone()
        } else {
            format!("{} {}", self.class, extra)
        }
    }

    pub fn prop_str(&self, key: &str) -> Option<&str> {
        props::get_str(&self.element.properties, key)
    }

    pub fn prop_bool(&self, key: &str) -> bool {
        props::get_bool(&self.element.properties, key)
    }
}

/// A registry of custom element renderers.
///
/// ```rust,ignore
/// let renderers = Renderers::new().element("callout", |ctx| {
///     let class = ctx.class_with("callout");
///     let node_path = ctx.node_path;
///     let children = ctx.children;
///     view! {
///         <aside class=class data-node-path=node_path>
///             {children}
///         </aside>
///     }.into_any()
/// });
/// ```
#[derive(Clone, Default)]
pub struct Renderers {
    elements: Arc<HashMap<String, ElementRenderFn>>,
}

impl Renderers {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a renderer for an element type.
    ///
    /// A renderer replaces the built-in tag rendering for that type. For void
    /// elements it is responsible for setting `contenteditable="false"` and for
    /// keeping the element non-editable.
    pub fn element<F>(mut self, element_type: impl Into<String>, render: F) -> Self
    where
        F: Fn(ElementRenderContext) -> AnyView + Send + Sync + 'static,
    {
        Arc::make_mut(&mut self.elements).insert(element_type.into(), Arc::new(render));
        self
    }

    pub(crate) fn get(&self, element_type: &str) -> Option<ElementRenderFn> {
        self.elements.get(element_type).cloned()
    }
}
