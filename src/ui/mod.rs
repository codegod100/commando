pub mod files;
pub mod log;
pub mod preview;
pub mod prompts;
pub mod settings;

use relm4::adw;
use relm4::adw::prelude::*;
use relm4::gtk;
use relm4::gtk::prelude::IsA;

use crate::agent::tools;

pub fn present_apps(parent: &impl IsA<gtk::Widget>) {
    let dialog = adw::Dialog::new();
    dialog.set_title("Apps");
    dialog.set_content_width(420);
    dialog.set_content_height(480);

    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());

    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_margin_top(16);
    list.set_margin_bottom(16);
    list.set_margin_start(16);
    list.set_margin_end(16);
    list.set_selection_mode(gtk::SelectionMode::None);

    let intro = adw::ActionRow::builder()
        .title("Built-in tools")
        .subtitle("Commando runs these locally. Custom MCP apps are not wired up yet.")
        .build();
    intro.set_activatable(false);
    list.append(&intro);

    for spec in tools::catalog() {
        let row = adw::ActionRow::builder()
            .title(spec.name.replace('_', " "))
            .subtitle(spec.description)
            .build();
        row.set_activatable(false);
        list.append(&row);
    }

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_child(Some(&list));
    view.set_content(Some(&scroll));
    dialog.set_child(Some(&view));
    dialog.present(Some(parent));
}
