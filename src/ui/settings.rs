use relm4::adw;
use relm4::adw::prelude::*;
use relm4::gtk;
use relm4::prelude::*;

use crate::config::{Config, Provider};

pub struct SettingsDialog {
    pub config: Config,
    providers: gtk::StringList,
}

#[derive(Debug)]
pub enum SettingsMsg {
    Load(Config),
    ProviderChanged(u32),
    ModelChanged(String),
    BaseUrlChanged(String),
    ApiKeyChanged(String),
    TimeoutChanged(f64),
    IterationsChanged(f64),
    DarkToggled(bool),
}

#[derive(Debug)]
pub enum SettingsOutput {
    Saved(Config),
}

#[relm4::component(pub)]
impl Component for SettingsDialog {
    type Init = Config;
    type Input = SettingsMsg;
    type Output = SettingsOutput;
    type CommandOutput = ();

    view! {
        #[name(dialog)]
        adw::PreferencesDialog {
            set_title: "Settings",
            set_search_enabled: false,

            add = &adw::PreferencesPage {
                set_title: "General",
                set_icon_name: Some("emblem-system-symbolic"),

                add = &adw::PreferencesGroup {
                    set_title: "Model",
                    set_description: Some("Ollama is local and free. Other providers need an API key."),

                    #[name(provider_row)]
                    adw::ComboRow {
                        set_title: "Provider",
                        set_model: Some(&model.providers),
                        set_selected: model.config.provider.index(),
                        connect_selected_notify[sender] => move |row| {
                            sender.input(SettingsMsg::ProviderChanged(row.selected()));
                        },
                    },

                    #[name(model_row)]
                    adw::EntryRow {
                        set_title: "Model",
                        #[watch]
                        set_tooltip_text: Some(&model.config.provider.suggested_models().join(", ")),
                        set_text: &model.config.model,
                        connect_changed[sender] => move |row| {
                            sender.input(SettingsMsg::ModelChanged(row.text().to_string()));
                        },
                    },

                    #[name(base_row)]
                    adw::EntryRow {
                        set_title: "Base URL",
                        set_text: &model.config.base_url,
                        connect_changed[sender] => move |row| {
                            sender.input(SettingsMsg::BaseUrlChanged(row.text().to_string()));
                        },
                    },

                    #[name(key_row)]
                    adw::PasswordEntryRow {
                        set_title: "API key",
                        set_text: &model.config.api_key,
                        connect_changed[sender] => move |row| {
                            sender.input(SettingsMsg::ApiKeyChanged(row.text().to_string()));
                        },
                    },
                },

                add = &adw::PreferencesGroup {
                    set_title: "Agent",

                    #[name(timeout_row)]
                    adw::SpinRow {
                        set_title: "Command timeout",
                        set_subtitle: "Seconds before a shell command is killed",
                        set_adjustment: Some(&gtk::Adjustment::new(
                            model.config.timeout_secs as f64,
                            5.0,
                            600.0,
                            5.0,
                            15.0,
                            0.0,
                        )),
                        connect_changed[sender] => move |row| {
                            sender.input(SettingsMsg::TimeoutChanged(row.value()));
                        },
                    },

                    #[name(iter_row)]
                    adw::SpinRow {
                        set_title: "Max tool steps",
                        set_subtitle: "How many tool calls one prompt may make",
                        set_adjustment: Some(&gtk::Adjustment::new(
                            model.config.max_iterations as f64,
                            1.0,
                            80.0,
                            1.0,
                            5.0,
                            0.0,
                        )),
                        connect_changed[sender] => move |row| {
                            sender.input(SettingsMsg::IterationsChanged(row.value()));
                        },
                    },

                    adw::SwitchRow {
                        set_title: "Force dark theme",
                        set_active: model.config.force_dark,
                        connect_active_notify[sender] => move |row| {
                            sender.input(SettingsMsg::DarkToggled(row.is_active()));
                        },
                    },
                },
            },
        }
    }

    fn init(config: Self::Init, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let providers = gtk::StringList::new(
            &Provider::ALL
                .iter()
                .map(|provider| provider.as_label())
                .collect::<Vec<_>>(),
        );
        let model = SettingsDialog { config, providers };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::Input,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            SettingsMsg::Load(config) => {
                self.config = config;
                widgets.provider_row.set_selected(self.config.provider.index());
                widgets.model_row.set_text(&self.config.model);
                widgets.base_row.set_text(&self.config.base_url);
                widgets.key_row.set_text(&self.config.api_key);
                widgets.timeout_row.set_value(self.config.timeout_secs as f64);
                widgets.iter_row.set_value(self.config.max_iterations as f64);
            }
            SettingsMsg::ProviderChanged(index) => {
                let provider = Provider::from_index(index);
                if provider != self.config.provider {
                    self.config.provider = provider;
                    self.config.apply_provider_defaults();
                    widgets.model_row.set_text(&self.config.model);
                    widgets.base_row.set_text(&self.config.base_url);
                    persist(&self.config, &sender);
                }
            }
            SettingsMsg::ModelChanged(value) => {
                self.config.model = value;
                persist(&self.config, &sender);
            }
            SettingsMsg::BaseUrlChanged(value) => {
                self.config.base_url = value;
                persist(&self.config, &sender);
            }
            SettingsMsg::ApiKeyChanged(value) => {
                self.config.api_key = value;
                persist(&self.config, &sender);
            }
            SettingsMsg::TimeoutChanged(value) => {
                self.config.timeout_secs = value as u64;
                persist(&self.config, &sender);
            }
            SettingsMsg::IterationsChanged(value) => {
                self.config.max_iterations = value as u32;
                persist(&self.config, &sender);
            }
            SettingsMsg::DarkToggled(value) => {
                self.config.force_dark = value;
                persist(&self.config, &sender);
            }
        }
        relm4::Component::update_view(self, widgets, sender);
    }
}

fn persist(config: &Config, sender: &ComponentSender<SettingsDialog>) {
    let _ = config.save();
    sender.output(SettingsOutput::Saved(config.clone())).ok();
}
