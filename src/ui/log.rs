use relm4::factory::{DynamicIndex, FactoryComponent, FactorySender};
use relm4::gtk;
use relm4::gtk::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    User,
    Status,
    Tool,
    ToolError,
    Assistant,
    Done,
    Error,
}

#[derive(Debug, Clone)]
pub struct LogItem {
    pub kind: LogKind,
    pub text: String,
    pub detail: Option<String>,
}

impl LogItem {
    pub fn new(kind: LogKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        let detail = detail.into();
        if !detail.is_empty() && detail != self.text {
            self.detail = Some(detail);
        }
        self
    }

    fn prefix(&self) -> &'static str {
        match self.kind {
            LogKind::User => "you",
            LogKind::Status => "…",
            LogKind::Tool => "›",
            LogKind::ToolError => "✕",
            LogKind::Assistant => "✦",
            LogKind::Done => "✓",
            LogKind::Error => "!",
        }
    }

    fn style_class(&self) -> &'static str {
        match self.kind {
            LogKind::User => "log-user",
            LogKind::Status => "log-status",
            LogKind::Tool => "log-tool",
            LogKind::ToolError => "log-tool-error",
            LogKind::Assistant => "log-assistant",
            LogKind::Done => "log-done",
            LogKind::Error => "log-error",
        }
    }
}

#[relm4::factory(pub)]
impl FactoryComponent for LogItem {
    type Init = LogItem;
    type Input = ();
    type Output = ();
    type CommandOutput = ();
    type ParentWidget = gtk::Box;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 2,
            add_css_class: "log-item",
            add_css_class: self.style_class(),

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,

                gtk::Label {
                    set_label: self.prefix(),
                    add_css_class: "log-prefix",
                    set_valign: gtk::Align::Start,
                },

                gtk::Label {
                    set_label: &self.text,
                    set_wrap: true,
                    set_xalign: 0.0,
                    set_selectable: true,
                    set_hexpand: true,
                    add_css_class: "log-text",
                }
            },

            gtk::Label {
                set_label: self.detail.as_deref().unwrap_or(""),
                set_visible: self.detail.is_some(),
                set_wrap: true,
                set_xalign: 0.0,
                set_selectable: true,
                add_css_class: "log-detail",
                set_margin_start: 22,
            }
        }
    }

    fn init_model(item: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        item
    }
}
