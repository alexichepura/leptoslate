//! Configurable rich-text preset for [`leptoslate`].
//!
//! `leptoslate` stays the lower-level Leptos binding. This crate provides a
//! batteries-included entry point whose defaults can be changed without
//! giving up custom renderers or behavior plugins.

use leptos::prelude::*;
mod plugins;

use leptoslate::Editable as LeptoslateEditable;
pub use plugins::{RichClipboardPlugin, RichPastePlugin};

/// Configuration for [`RichEditor`].
#[derive(Clone)]
pub struct RichConfig {
    pub hover_bar: bool,
    pub selection_bubble: bool,
    pub paste_html: bool,
    pub paste_markdown: bool,
    pub renderers: leptoslate::Renderers,
    pub plugins: leptoslate::EditablePlugins,
    pub file_handler: Option<leptoslate::FileHandler>,
    pub mention_handler: Option<leptoslate::MentionHandler>,
}

impl Default for RichConfig {
    fn default() -> Self {
        Self {
            hover_bar: true,
            selection_bubble: true,
            paste_html: true,
            paste_markdown: false,
            renderers: leptoslate::Renderers::default(),
            plugins: leptoslate::EditablePlugins::default(),
            file_handler: None,
            mention_handler: None,
        }
    }
}

impl RichConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_hover_bar(mut self, enabled: bool) -> Self {
        self.hover_bar = enabled;
        self
    }

    pub fn with_selection_bubble(mut self, enabled: bool) -> Self {
        self.selection_bubble = enabled;
        self
    }

    pub fn with_html_paste(mut self, enabled: bool) -> Self {
        self.paste_html = enabled;
        self
    }

    pub fn with_markdown_paste(mut self, enabled: bool) -> Self {
        self.paste_markdown = enabled;
        self
    }

    pub fn with_renderers(mut self, renderers: leptoslate::Renderers) -> Self {
        self.renderers = renderers;
        self
    }

    pub fn with_plugins(mut self, plugins: leptoslate::EditablePlugins) -> Self {
        self.plugins = plugins;
        self
    }

    pub fn with_file_handler(mut self, handler: leptoslate::FileHandler) -> Self {
        self.file_handler = Some(handler);
        self
    }

    pub fn with_mention_handler(mut self, handler: leptoslate::MentionHandler) -> Self {
        self.mention_handler = Some(handler);
        self
    }

    pub fn with_plugin<P>(mut self, plugin: P) -> Self
    where
        P: leptoslate::EditablePlugin + 'static,
    {
        self.plugins.push(plugin);
        self
    }
}

/// A rich editor using the [`RichConfig`] defaults.
///
/// The component remains fully composable: replace `renderers`, add behavior
/// plugins, or disable floating UI and paste modes through the config.
#[component]
pub fn RichEditor(
    editor: leptoslate::EditorHandle,
    #[prop(optional)] config: RichConfig,
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] placeholder: String,
) -> impl IntoView {
    let mut plugins = config.plugins.clone();
    plugins.push(RichClipboardPlugin);
    if config.paste_html || config.paste_markdown {
        plugins.push(RichPastePlugin::new(
            config.paste_html,
            config.paste_markdown,
        ));
    }
    let renderers = config.renderers.clone();

    view! {
        <LeptoslateEditable
            editor=editor
            renderers=renderers
            plugins=plugins
            hover_bar=config.hover_bar
            selection_bubble=config.selection_bubble
            class=class
            placeholder=placeholder
            on_files=config
                .file_handler
                .unwrap_or_else(|| leptoslate::FileHandler::new(|_| {}))
            on_mention=config
                .mention_handler
                .unwrap_or_else(|| Callback::new(|_| {}))
        />
    }
}

pub use leptoslate;
pub use leptoslate::core;
pub use leptoslate::*;

pub mod prelude {
    pub use crate::{RichConfig, RichEditor};
    pub use leptoslate::core::*;
    pub use leptoslate::*;
}

#[cfg(test)]
mod tests {
    use super::*;
    use leptoslate::EditablePlugin;

    struct NoopPlugin;

    impl EditablePlugin for NoopPlugin {}

    #[test]
    fn rich_config_is_composable() {
        let config = RichConfig::new()
            .with_hover_bar(false)
            .with_selection_bubble(false)
            .with_markdown_paste(true)
            .with_plugin(NoopPlugin);

        assert!(!config.hover_bar);
        assert!(!config.selection_bubble);
        assert!(config.paste_markdown);
        assert!(!config.plugins.is_empty());
    }
}
