//! Configurable rich-text preset for [`leptoslate`].
//!
//! `leptoslate` stays the lower-level Leptos binding. This crate provides a
//! batteries-included entry point whose defaults can be changed without
//! giving up custom renderers or behavior plugins.

use std::time::Duration;

use leptos::prelude::*;
pub mod fragment;
mod list;
mod paste;
mod plugins;
mod renderers;
mod table;

use leptoslate::core::{Feature, FeatureSet};
use leptoslate::Editable as LeptoslateEditable;
pub use list::{ListContext, ListHandle, ListTransforms, RichListFeature, RichListPlugin};
pub use paste::{PasteHandle, PasteTransforms};
pub use plugins::{RichClipboardPlugin, RichPastePlugin};
pub use renderers::{rich_renderers, RICH_STYLES};
pub use table::{RichTableFeature, RichTablePlugin, TableHandle, TableTransforms};

/// Configuration for [`RichEditor`].
#[derive(Clone)]
pub struct RichConfig {
    pub hover_bar: bool,
    pub selection_bubble: bool,
    pub paste_html: bool,
    pub paste_markdown: bool,
    pub renderers: leptoslate::Renderers,
    pub plugins: leptoslate::EditablePlugins,
    pub features: FeatureSet,
    /// Property keys used for the built-in bold/italic/underline/strikethrough
    /// and inline-code marks.
    pub marks: leptoslate::MarkSet,
    pub file_handler: Option<leptoslate::FileHandler>,
    pub mention_handler: Option<leptoslate::MentionHandler>,
    pub list_behaviour: bool,
    pub table_behaviour: bool,
    /// Optional document change callback, forwarded to [`leptoslate::Editable`].
    pub on_change: Option<Callback<String>>,
    /// Optional debounce applied to [`RichConfig::on_change`].
    pub change_debounce: Option<Duration>,
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
            features: FeatureSet::new()
                .with(RichListFeature)
                .with(RichTableFeature),
            marks: leptoslate::MarkSet::default(),
            file_handler: None,
            mention_handler: None,
            list_behaviour: true,
            table_behaviour: true,
            on_change: None,
            change_debounce: None,
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

    /// Configure the property keys used for bold, italic, underline,
    /// strikethrough and inline code.
    ///
    /// The same set drives rich paste, keyboard shortcuts, `beforeinput`
    /// formatting, the selection bubble and the default mark styles. The
    /// conventional names remain accepted on input.
    pub fn with_marks(mut self, marks: leptoslate::MarkSet) -> Self {
        self.marks = marks;
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

    pub fn with_list_behaviour(mut self, enabled: bool) -> Self {
        self.list_behaviour = enabled;
        self
    }

    pub fn with_table_behaviour(mut self, enabled: bool) -> Self {
        self.table_behaviour = enabled;
        self
    }

    /// Receive the document as Slate-shaped JSON after every content change.
    ///
    /// Selection-only updates are ignored. Combine with
    /// [`RichConfig::with_change_debounce`] to coalesce rapid edits.
    pub fn with_on_change(mut self, callback: Callback<String>) -> Self {
        self.on_change = Some(callback);
        self
    }

    /// Coalesce `on_change` notifications by this delay.
    ///
    /// A new change resets the timer, so only the latest document is
    /// delivered. A pending update is flushed when the editor unmounts.
    pub fn with_change_debounce(mut self, debounce: Duration) -> Self {
        self.change_debounce = Some(debounce);
        self
    }

    /// Add a model-level feature (schema rules and normalization hooks).
    pub fn with_feature<F>(mut self, feature: F) -> Self
    where
        F: Feature + 'static,
    {
        self.features.push(feature);
        self
    }

    /// Install the configured features on an editor handle.
    ///
    /// This is useful when an application renders a base [`leptoslate::Editable`]
    /// but still wants the rich schema/normalization rules.
    pub fn install(&self, editor: &leptoslate::EditorHandle) {
        let features = self.features.clone();
        editor.update(|editor| editor.set_features(features));
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
    /// Overrides [`RichConfig::on_change`] when set.
    #[prop(optional)]
    on_change: MaybeProp<Callback<String>>,
    /// Overrides [`RichConfig::change_debounce`] when set.
    #[prop(optional)]
    change_debounce: MaybeProp<Duration>,
) -> impl IntoView {
    // Features own model-level schema and normalization. Install them when the
    // editor mounts; the component body runs once per editor instance.
    let feature_editor = editor.clone();
    let features = config.features.clone();
    feature_editor.update(|editor| editor.set_features(features));

    let mut plugins = config.plugins.clone();
    plugins.push(RichClipboardPlugin);
    if config.paste_html || config.paste_markdown {
        plugins.push(
            RichPastePlugin::new(config.paste_html, config.paste_markdown)
                .with_marks(config.marks.clone()),
        );
    }
    if config.list_behaviour {
        plugins.push(RichListPlugin);
    }
    if config.table_behaviour {
        plugins.push(RichTablePlugin);
    }
    let renderers = rich_renderers().merge(config.renderers.clone());
    let on_change = on_change.get_untracked().or(config.on_change);
    let change_debounce = change_debounce.get_untracked().or(config.change_debounce);

    view! {
        <style>{RICH_STYLES}</style>
        <LeptoslateEditable
            editor=editor
            renderers=renderers
            marks=config.marks.clone()
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
            on_change=on_change.into()
            change_debounce=change_debounce.into()
        />
    }
}

pub use leptoslate;
pub use leptoslate::core;
pub use leptoslate::*;

pub mod prelude {
    pub use crate::{
        ListContext, ListHandle, ListTransforms, PasteHandle, PasteTransforms, RichConfig,
        RichEditor, RichListFeature, RichTableFeature, TableHandle, TableTransforms,
    };
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
            .with_list_behaviour(false)
            .with_table_behaviour(false)
            .with_plugin(NoopPlugin);

        assert!(!config.hover_bar);
        assert!(!config.selection_bubble);
        assert!(config.paste_markdown);
        assert!(!config.list_behaviour);
        assert!(!config.table_behaviour);
        assert!(!config.plugins.is_empty());
        assert!(!config.features.is_empty());
        assert!(config.on_change.is_none());
        assert!(config.change_debounce.is_none());
        assert_eq!(config.marks, leptoslate::MarkSet::default());
    }

    #[test]
    fn mark_configuration_is_forwarded() {
        let marks = leptoslate::MarkSet::new()
            .bold("strong")
            .italic("em")
            .underline("u");
        let config = RichConfig::new().with_marks(marks.clone());
        assert_eq!(config.marks, marks);
        assert_eq!(config.marks.bold_key(), "strong");
    }

    #[test]
    fn change_notification_configuration_is_forwarded() {
        let config = RichConfig::new().with_change_debounce(Duration::from_millis(250));
        assert_eq!(config.change_debounce, Some(Duration::from_millis(250)));

        let callback = Callback::new(|_json: String| {});
        let config = config.with_on_change(callback);
        assert!(config.on_change.is_some());
    }
}
