use relm4::adw;
use relm4::adw::prelude::*;
use relm4::gtk;
use relm4::prelude::*;

use crate::agent::library::builtin_prompts;

pub struct PromptPicker;

#[derive(Debug)]
pub enum PromptMsg {
    Pick(usize),
}

#[derive(Debug)]
pub enum PromptOutput {
    Selected(String),
}

#[relm4::component(pub)]
impl SimpleComponent for PromptPicker {
    type Init = ();
    type Input = PromptMsg;
    type Output = PromptOutput;

    view! {
        adw::Dialog {
            set_title: "Prompts",
            set_content_width: 480,
            set_content_height: 560,

            #[wrap(Some)]
            set_child = &adw::ToolbarView {
                add_top_bar = &adw::HeaderBar {
                    set_show_end_title_buttons: true,
                },

                #[wrap(Some)]
                set_content = &gtk::ScrolledWindow {
                    set_vexpand: true,

                    #[name(list)]
                    gtk::ListBox {
                        add_css_class: "boxed-list",
                        set_margin_all: 16,
                        set_selection_mode: gtk::SelectionMode::None,
                    }
                }
            }
        }
    }

    fn init(_init: Self::Init, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let model = PromptPicker;
        let widgets = view_output!();
        for (index, prompt) in builtin_prompts().iter().enumerate() {
            let row = adw::ActionRow::builder()
                .title(prompt.title)
                .subtitle(prompt.text)
                .activatable(true)
                .build();
            let badge = gtk::Label::new(Some(prompt.category));
            badge.add_css_class("dim-label");
            row.add_prefix(&badge);
            let sender = sender.clone();
            row.connect_activated(move |_| sender.input(PromptMsg::Pick(index)));
            widgets.list.append(&row);
        }
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        let PromptMsg::Pick(index) = message;
        if let Some(prompt) = builtin_prompts().get(index) {
            sender
                .output(PromptOutput::Selected(prompt.text.to_string()))
                .ok();
        }
    }
}
