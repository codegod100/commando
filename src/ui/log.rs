use relm4::factory::{DynamicIndex, FactoryComponent, FactorySender};
use relm4::gtk;
use relm4::gtk::prelude::*;
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};

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

    fn display_markup(&self) -> String {
        if self.kind == LogKind::Assistant {
            markdown_to_pango(&self.text)
        } else {
            gtk::glib::markup_escape_text(&self.text).to_string()
        }
    }
}

/// Convert the subset of Markdown supported by Pango into safe label markup.
///
/// Text and link destinations are always escaped before being included, so
/// model output cannot inject arbitrary Pango markup.
fn markdown_to_pango(markdown: &str) -> String {
    let mut output = String::new();
    let mut lists: Vec<Option<u64>> = Vec::new();

    for event in Parser::new(markdown) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {}
                Tag::Heading { level, .. } => {
                    let size = match level {
                        HeadingLevel::H1 => "xx-large",
                        HeadingLevel::H2 => "x-large",
                        _ => "large",
                    };
                    output.push_str(&format!("<span size=\"{size}\" weight=\"bold\">"));
                }
                Tag::BlockQuote(_) => output.push_str("<span style=\"italic\">"),
                Tag::CodeBlock(_) => output.push_str("<tt>"),
                Tag::List(start) => lists.push(start),
                Tag::Item => {
                    if let Some(counter) = lists.last_mut() {
                        match counter {
                            Some(number) => {
                                output.push_str(&format!("{number}. "));
                                *number += 1;
                            }
                            None => output.push_str("• "),
                        }
                    }
                }
                Tag::Emphasis => output.push_str("<i>"),
                Tag::Strong => output.push_str("<b>"),
                Tag::Strikethrough => output.push_str("<s>"),
                Tag::Link { dest_url, .. } => {
                    let url = gtk::glib::markup_escape_text(&dest_url);
                    output.push_str(&format!("<a href=\"{url}\">"));
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => output.push_str("\n\n"),
                TagEnd::Heading(_) => output.push_str("</span>\n"),
                TagEnd::BlockQuote(_) => output.push_str("</span>\n"),
                TagEnd::CodeBlock => output.push_str("</tt>\n"),
                TagEnd::List(_) => {
                    lists.pop();
                    output.push('\n');
                }
                TagEnd::Item => output.push('\n'),
                TagEnd::Emphasis => output.push_str("</i>"),
                TagEnd::Strong => output.push_str("</b>"),
                TagEnd::Strikethrough => output.push_str("</s>"),
                TagEnd::Link => output.push_str("</a>"),
                _ => {}
            },
            Event::Text(text) => {
                output.push_str(&gtk::glib::markup_escape_text(&text));
            }
            Event::Code(text) => {
                output.push_str("<tt>");
                output.push_str(&gtk::glib::markup_escape_text(&text));
                output.push_str("</tt>");
            }
            Event::SoftBreak => output.push(' '),
            Event::HardBreak => output.push('\n'),
            Event::Rule => output.push_str("────────\n"),
            _ => {}
        }
    }

    output.trim_end().to_string()
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
                    set_markup: &self.display_markup(),
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

#[cfg(test)]
mod tests {
    use super::markdown_to_pango;

    #[test]
    fn formats_common_markdown_and_escapes_model_output() {
        let rendered = markdown_to_pango("# Result\n\n**Done** with `<unsafe>`.");

        assert!(rendered.contains("<span size=\"xx-large\" weight=\"bold\">Result</span>"));
        assert!(rendered.contains("<b>Done</b>"));
        assert!(rendered.contains("<tt>&lt;unsafe&gt;</tt>"));
    }

    #[test]
    fn formats_lists_and_links() {
        let rendered = markdown_to_pango("- one\n- [two](https://example.com?a=1&b=2)");

        assert!(rendered.contains("• one"));
        assert!(rendered.contains("<a href=\"https://example.com?a=1&amp;b=2\">two</a>"));
    }
}
