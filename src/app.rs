use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use relm4::adw;
use relm4::adw::prelude::*;
use relm4::factory::FactoryVecDeque;
use relm4::gtk;
use relm4::prelude::*;

use crate::agent::llm::ChatMessage;
use crate::agent::tools::preview_text;
use crate::agent::{self, AgentEvent, AgentRequest};
use crate::config::{display_path, Config};
use crate::ui::log::{LogItem, LogKind};
use crate::ui::prompts::{PromptOutput, PromptPicker};
use crate::ui::settings::{SettingsDialog, SettingsMsg, SettingsOutput};
use crate::ui::{self, preview};
use crate::workspace::{self, FileItem};

pub struct App {
    config: Config,
    workspace: PathBuf,
    file_items: Vec<FileItem>,
    files: FactoryVecDeque<FileItem>,
    log: FactoryVecDeque<LogItem>,
    log_empty: bool,
    running: bool,
    status: String,
    knowledge: Vec<PathBuf>,
    stop: Arc<AtomicBool>,
    turns: Vec<(String, String)>,
    pending_prompt: Option<String>,
    last_assistant: String,
    settings: Controller<SettingsDialog>,
    prompts: Controller<PromptPicker>,
}

#[derive(Debug)]
pub enum AppMsg {
    Submit,
    UsePrompt(String),
    PickWorkspace,
    WorkspacePicked(PathBuf),
    GoParent,
    RefreshFiles,
    FileActivated(i32),
    OpenInFiles,
    OpenPrompts,
    OpenApps,
    AddKnowledge,
    KnowledgePicked(Vec<PathBuf>),
    ClearKnowledge,
    OpenSettings,
    ConfigSaved(Config),
    NewChat,
    Stop,
    Agent(AgentEvent),
}

#[relm4::component(pub)]
impl Component for App {
    type Init = ();
    type Input = AppMsg;
    type Output = ();
    type CommandOutput = ();

    view! {
        #[name(window)]
        adw::ApplicationWindow {
            set_title: Some("Commando"),
            set_default_width: 1280,
            set_default_height: 840,

            adw::ToolbarView {
                add_top_bar = &adw::HeaderBar {
                    pack_start = &gtk::Button {
                        set_icon_name: "folder-open-symbolic",
                        set_tooltip_text: Some("Choose workspace"),
                        connect_clicked => AppMsg::PickWorkspace,
                    },
                    pack_start = &gtk::Button {
                        add_css_class: "flat",
                        add_css_class: "workspace-chip",
                        #[watch]
                        set_label: &display_path(&model.workspace),
                        set_tooltip_text: Some("Choose workspace"),
                        connect_clicked => AppMsg::PickWorkspace,
                    },
                    #[wrap(Some)]
                    set_title_widget = &gtk::Label {
                        set_label: "Commando",
                        add_css_class: "commando-title",
                    },
                    pack_end = &gtk::Button {
                        set_icon_name: "emblem-system-symbolic",
                        set_tooltip_text: Some("Settings"),
                        connect_clicked => AppMsg::OpenSettings,
                    },
                    pack_end = &gtk::Button {
                        set_icon_name: "list-add-symbolic",
                        set_tooltip_text: Some("New chat"),
                        connect_clicked => AppMsg::NewChat,
                    },
                },

                #[wrap(Some)]
                set_content = &gtk::Paned {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_shrink_start_child: false,
                        set_shrink_end_child: false,
                        set_wide_handle: true,
                        set_position: 820,

                        #[wrap(Some)]
                        set_start_child = &gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 12,
                            set_margin_start: 18,
                            set_margin_end: 12,
                            set_margin_top: 12,
                            set_margin_bottom: 16,

                            gtk::Box {
                                add_css_class: "prompt-shell",
                                set_spacing: 8,
                                set_hexpand: true,

                                #[name(prompt)]
                                gtk::Entry {
                                    set_placeholder_text: Some("Describe the outcome in plain English…"),
                                    set_hexpand: true,
                                    #[watch]
                                    set_sensitive: !model.running,
                                    connect_activate => AppMsg::Submit,
                                },

                                gtk::Button {
                                    add_css_class: "suggested-action",
                                    add_css_class: "send-button",
                                    #[watch]
                                    set_icon_name: if model.running {
                                        "media-playback-stop-symbolic"
                                    } else {
                                        "go-next-symbolic"
                                    },
                                    #[watch]
                                    set_tooltip_text: Some(if model.running {
                                        "Stop"
                                    } else {
                                        "Run"
                                    }),
                                    connect_clicked[sender] => move |_| {
                                        sender.input(AppMsg::Submit);
                                    },
                                }
                            },

                            gtk::Box {
                                set_spacing: 8,
                                gtk::Button {
                                    add_css_class: "toolbar-chip",
                                    set_label: "Prompts",
                                    connect_clicked => AppMsg::OpenPrompts,
                                },
                                gtk::Button {
                                    add_css_class: "toolbar-chip",
                                    set_label: "Apps",
                                    connect_clicked => AppMsg::OpenApps,
                                },
                                gtk::Button {
                                    add_css_class: "toolbar-chip",
                                    add_css_class: "suggested-action",
                                    #[watch]
                                    set_label: &knowledge_label(&model.knowledge),
                                    connect_clicked => AppMsg::AddKnowledge,
                                },
                                gtk::Button {
                                    add_css_class: "flat",
                                    set_label: "Clear",
                                    #[watch]
                                    set_visible: !model.knowledge.is_empty(),
                                    connect_clicked => AppMsg::ClearKnowledge,
                                },
                                gtk::Box {
                                    set_hexpand: true,
                                },
                                gtk::Button {
                                    add_css_class: "flat",
                                    add_css_class: "model-button",
                                    #[watch]
                                    set_label: &model.config.model,
                                    set_tooltip_text: Some("Change model"),
                                    connect_clicked => AppMsg::OpenSettings,
                                },
                                gtk::Label {
                                    #[watch]
                                    set_label: &model.status,
                                    #[watch]
                                    set_css_classes: &if model.running {
                                        ["status-busy"]
                                    } else {
                                        ["status-ready"]
                                    },
                                },
                            },

                            #[name(stack)]
                            gtk::Stack {
                                set_vexpand: true,

                                add_child = &gtk::Box {
                                    add_css_class: "welcome",
                                    set_orientation: gtk::Orientation::Vertical,
                                    set_valign: gtk::Align::Center,
                                    set_halign: gtk::Align::Start,
                                    set_spacing: 10,

                                    gtk::Label {
                                        set_label: "AI that executes.",
                                        add_css_class: "welcome-title",
                                        set_halign: gtk::Align::Start,
                                    },
                                    gtk::Label {
                                        set_label: "Most assistants talk. Commando opens folders, runs commands, writes files, and reports back — on Linux.",
                                        add_css_class: "welcome-copy",
                                        set_wrap: true,
                                        set_xalign: 0.0,
                                    },
                                    gtk::Label {
                                        set_label: "Try: Sort my Downloads by file type",
                                        add_css_class: "dim-label",
                                        set_halign: gtk::Align::Start,
                                        set_margin_top: 8,
                                    },
                                } -> {
                                    set_name: "welcome",
                                },

                                #[name(log_scroll)]
                                add_child = &gtk::ScrolledWindow {
                                    add_css_class: "log-scroller",
                                    set_vexpand: true,
                                    set_hexpand: true,
                                    set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),

                                    #[local_ref]
                                    log_box -> gtk::Box {
                                        set_orientation: gtk::Orientation::Vertical,
                                        set_spacing: 6,
                                    }
                                } -> {
                                    set_name: "log",
                                }
                            }
                        },

                        #[wrap(Some)]
                        set_end_child = &gtk::Box {
                            add_css_class: "files-pane",
                            set_orientation: gtk::Orientation::Vertical,
                            set_width_request: 280,

                            gtk::Box {
                                set_margin_start: 12,
                                set_margin_end: 8,
                                set_margin_top: 10,
                                set_margin_bottom: 6,
                                set_spacing: 6,

                                gtk::Button {
                                    set_icon_name: "go-up-symbolic",
                                    set_tooltip_text: Some("Parent folder"),
                                    connect_clicked => AppMsg::GoParent,
                                },
                                gtk::Label {
                                    #[watch]
                                    set_label: &display_path(&model.workspace),
                                    set_ellipsize: gtk::pango::EllipsizeMode::Start,
                                    set_xalign: 0.0,
                                    set_hexpand: true,
                                    add_css_class: "heading",
                                },
                                gtk::Button {
                                    set_icon_name: "view-refresh-symbolic",
                                    set_tooltip_text: Some("Refresh"),
                                    connect_clicked => AppMsg::RefreshFiles,
                                },
                                gtk::Button {
                                    set_icon_name: "document-open-symbolic",
                                    set_tooltip_text: Some("Open in Files"),
                                    connect_clicked => AppMsg::OpenInFiles,
                                },
                            },

                            gtk::ScrolledWindow {
                                set_vexpand: true,
                                #[local_ref]
                                file_list -> gtk::ListBox {
                                    set_selection_mode: gtk::SelectionMode::Single,
                                    add_css_class: "navigation-sidebar",
                                    connect_row_activated[sender] => move |_, row| {
                                        sender.input(AppMsg::FileActivated(row.index()));
                                    },
                                }
                            }
                        }
                }
            }
        }
    }

    fn init(_init: Self::Init, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let config = Config::load();
        apply_color_scheme(config.force_dark);
        let workspace = if config.workspace.exists() {
            config.workspace.clone()
        } else {
            crate::config::default_workspace()
        };

        let files = FactoryVecDeque::builder()
            .launch_default()
            .detach();
        let log = FactoryVecDeque::builder()
            .launch_default()
            .detach();

        let settings = SettingsDialog::builder()
            .launch(config.clone())
            .forward(sender.input_sender(), |msg| match msg {
                SettingsOutput::Saved(config) => AppMsg::ConfigSaved(config),
            });
        let prompts = PromptPicker::builder()
            .launch(())
            .forward(sender.input_sender(), |msg| match msg {
                PromptOutput::Selected(text) => AppMsg::UsePrompt(text),
            });

        let mut model = App {
            config,
            workspace,
            file_items: Vec::new(),
            files,
            log,
            log_empty: true,
            running: false,
            status: "Ready".into(),
            knowledge: Vec::new(),
            stop: Arc::new(AtomicBool::new(false)),
            turns: Vec::new(),
            pending_prompt: None,
            last_assistant: String::new(),
            settings,
            prompts,
        };
        model.reload_files();

        let file_list = model.files.widget();
        let log_box = model.log.widget();
        let widgets = view_output!();
        widgets.stack.set_visible_child_name("welcome");
        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::Input,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        match message {
            AppMsg::Submit => {
                if self.running {
                    sender.input(AppMsg::Stop);
                } else {
                    let text = widgets.prompt.text().to_string();
                    widgets.prompt.set_text("");
                    self.start_run(text, &sender);
                }
            }
            AppMsg::UsePrompt(text) => {
                self.prompts.widget().close();
                widgets.prompt.set_text(&text);
                widgets.prompt.grab_focus();
            }
            AppMsg::PickWorkspace => pick_workspace(root, sender.clone()),
            AppMsg::WorkspacePicked(path) => {
                self.workspace = path.clone();
                self.config.workspace = path;
                let _ = self.config.save();
                self.reload_files();
            }
            AppMsg::GoParent => {
                if let Some(parent) = self.workspace.parent() {
                    self.workspace = parent.to_path_buf();
                    self.config.workspace = self.workspace.clone();
                    let _ = self.config.save();
                    self.reload_files();
                }
            }
            AppMsg::RefreshFiles => self.reload_files(),
            AppMsg::FileActivated(index) => {
                if let Some(item) = self.file_items.get(index as usize).cloned() {
                    if item.is_dir {
                        self.workspace = item.path;
                        self.config.workspace = self.workspace.clone();
                        let _ = self.config.save();
                        self.reload_files();
                    } else {
                        preview::present(root, &item);
                    }
                }
            }
            AppMsg::OpenInFiles => preview::open_path(&self.workspace),
            AppMsg::OpenPrompts => self.prompts.widget().present(Some(root)),
            AppMsg::OpenApps => ui::present_apps(root),
            AppMsg::AddKnowledge => pick_knowledge(root, sender.clone()),
            AppMsg::KnowledgePicked(paths) => {
                for path in paths {
                    if !self.knowledge.contains(&path) {
                        self.knowledge.push(path);
                    }
                }
            }
            AppMsg::ClearKnowledge => self.knowledge.clear(),
            AppMsg::OpenSettings => {
                self.settings.emit(SettingsMsg::Load(self.config.clone()));
                self.settings.widget().present(Some(root));
            }
            AppMsg::ConfigSaved(config) => {
                apply_color_scheme(config.force_dark);
                self.config = config;
            }
            AppMsg::NewChat => {
                self.stop.store(true, Ordering::Relaxed);
                self.running = false;
                self.status = "Ready".into();
                self.turns.clear();
                self.pending_prompt = None;
                self.last_assistant.clear();
                self.log.guard().clear();
                self.log_empty = true;
            }
            AppMsg::Stop => {
                self.stop.store(true, Ordering::Relaxed);
                self.status = "Stopping…".into();
            }
            AppMsg::Agent(event) => self.on_agent(event, widgets),
        }
        widgets.stack.set_visible_child_name(if self.log_empty { "welcome" } else { "log" });
        relm4::Component::update_view(self, widgets, sender);
    }
}

impl App {
    fn reload_files(&mut self) {
        let mut guard = self.files.guard();
        guard.clear();
        match workspace::list_workspace(&self.workspace) {
            Ok(items) => {
                self.file_items = items.clone();
                for item in items {
                    guard.push_back(item);
                }
            }
            Err(error) => {
                self.file_items.clear();
                tracing::warn!(%error, "failed to list workspace");
            }
        }
    }

    fn start_run(&mut self, prompt: String, sender: &ComponentSender<Self>) {
        let prompt = prompt.trim().to_string();
        if prompt.is_empty() {
            return;
        }
        if let Some(hint) = self.config.ready_hint() {
            self.push_log(LogItem::new(LogKind::Error, hint));
            return;
        }
        self.stop = Arc::new(AtomicBool::new(false));
        self.running = true;
        self.status = "Running".into();
        self.pending_prompt = Some(prompt.clone());
        self.last_assistant.clear();
        self.push_log(LogItem::new(LogKind::User, prompt.clone()));

        let mut history = Vec::new();
        for (user, assistant) in &self.turns {
            history.push(ChatMessage::User(user.clone()));
            history.push(ChatMessage::Assistant {
                text: assistant.clone(),
                tool_calls: Vec::new(),
            });
        }
        let knowledge = knowledge_blobs(&self.knowledge);
        let request = AgentRequest {
            prompt,
            knowledge,
            config: self.config.clone(),
            workspace: self.workspace.clone(),
            history,
            stop: self.stop.clone(),
        };
        let sender = sender.clone();
        relm4::spawn(async move {
            agent::run(request, move |event| sender.input(AppMsg::Agent(event))).await;
        });
    }

    fn on_agent(&mut self, event: AgentEvent, widgets: &AppWidgets) {
        match event {
            AgentEvent::Status(text) => {
                self.status = text.clone();
                self.push_log(LogItem::new(LogKind::Status, text));
            }
            AgentEvent::ToolStart { name, summary } => {
                self.push_log(LogItem::new(LogKind::Tool, format!("{name} · {summary}")));
            }
            AgentEvent::ToolResult {
                ok,
                summary,
                detail,
            } => {
                let kind = if ok {
                    LogKind::Tool
                } else {
                    LogKind::ToolError
                };
                self.push_log(LogItem::new(kind, summary).with_detail(clip(&detail, 800)));
                self.reload_files();
            }
            AgentEvent::Assistant(text) => {
                self.last_assistant = text.clone();
                self.push_log(LogItem::new(LogKind::Assistant, text));
            }
            AgentEvent::Done { summary, elapsed } => {
                if let Some(user) = self.pending_prompt.take() {
                    let answer = if self.last_assistant.is_empty() {
                        summary.clone()
                    } else {
                        self.last_assistant.clone()
                    };
                    self.turns.push((user, answer));
                }
                self.running = false;
                self.status = "Ready".into();
                self.push_log(LogItem::new(
                    LogKind::Done,
                    format!("{summary} · {:.1}s", elapsed.as_secs_f32()),
                ));
                self.reload_files();
            }
            AgentEvent::Failed(error) => {
                self.pending_prompt = None;
                self.running = false;
                self.status = "Ready".into();
                self.push_log(LogItem::new(LogKind::Error, error));
            }
        }
        let adj = widgets.log_scroll.vadjustment();
        adj.set_value(adj.upper());
    }

    fn push_log(&mut self, item: LogItem) {
        self.log_empty = false;
        self.log.guard().push_back(item);
    }
}

fn knowledge_label(paths: &[PathBuf]) -> String {
    if paths.is_empty() {
        "+ Add Knowledge".into()
    } else {
        format!("+ Knowledge ({})", paths.len())
    }
}

fn knowledge_blobs(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .filter_map(|path| {
            let body = preview_text(path).ok()?;
            Some(format!(
                "<knowledge path=\"{}\">\n{body}\n</knowledge>",
                path.display()
            ))
        })
        .collect()
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

fn apply_color_scheme(force_dark: bool) {
    adw::StyleManager::default().set_color_scheme(if force_dark {
        adw::ColorScheme::ForceDark
    } else {
        adw::ColorScheme::Default
    });
}

fn pick_workspace(window: &adw::ApplicationWindow, sender: ComponentSender<App>) {
    let dialog = gtk::FileDialog::builder()
        .title("Choose workspace")
        .modal(true)
        .build();
    dialog.select_folder(Some(window), gtk::gio::Cancellable::NONE, move |result| {
        if let Ok(file) = result {
            if let Some(path) = file.path() {
                sender.input(AppMsg::WorkspacePicked(path));
            }
        }
    });
}

fn pick_knowledge(window: &adw::ApplicationWindow, sender: ComponentSender<App>) {
    let dialog = gtk::FileDialog::builder()
        .title("Add knowledge")
        .modal(true)
        .build();
    dialog.open_multiple(Some(window), gtk::gio::Cancellable::NONE, move |result| {
        let Ok(files) = result else {
            return;
        };
        let mut paths = Vec::new();
        for index in 0..files.n_items() {
            if let Some(file) = files.item(index).and_downcast::<gtk::gio::File>() {
                if let Some(path) = file.path() {
                    paths.push(path);
                }
            }
        }
        if !paths.is_empty() {
            sender.input(AppMsg::KnowledgePicked(paths));
        }
    });
}
