use std::path::Path;

use relm4::adw;
use relm4::adw::prelude::*;
use relm4::gtk;
use relm4::gtk::gio::prelude::FileExt;
use relm4::gtk::prelude::IsA;

use crate::agent::tools::preview_text;
use crate::workspace::FileItem;

pub fn present(parent: &impl IsA<gtk::Widget>, item: &FileItem) {
    let dialog = adw::Dialog::new();
    dialog.set_title(&item.name);
    dialog.set_content_width(760);
    dialog.set_content_height(560);

    let view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let open = gtk::Button::from_icon_name("external-link-symbolic");
    open.set_tooltip_text(Some("Open with the system app"));
    let path = item.path.clone();
    open.connect_clicked(move |_| {
        let _ = gtk::gio::AppInfo::launch_default_for_uri(
            &format!("file://{}", path.display()),
            gtk::gio::AppLaunchContext::NONE,
        );
    });
    header.pack_start(&open);
    view.add_top_bar(&header);

    if item.is_image() {
        let picture = gtk::Picture::for_filename(&item.path);
        picture.set_content_fit(gtk::ContentFit::Contain);
        picture.set_hexpand(true);
        picture.set_vexpand(true);
        view.set_content(Some(&picture));
    } else {
        let text = gtk::TextView::new();
        text.set_editable(false);
        text.set_monospace(true);
        text.set_wrap_mode(gtk::WrapMode::WordChar);
        text.set_left_margin(12);
        text.set_right_margin(12);
        text.set_top_margin(12);
        text.set_bottom_margin(12);
        let body = preview_text(&item.path).unwrap_or_else(|error| error.to_string());
        text.buffer().set_text(&body);
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_child(Some(&text));
        scroll.set_vexpand(true);
        view.set_content(Some(&scroll));
    }

    dialog.set_child(Some(&view));
    dialog.present(Some(parent));
}

pub fn open_path(path: &Path) {
    let uri = gtk::gio::File::for_path(path).uri();
    let _ = gtk::gio::AppInfo::launch_default_for_uri(
        uri.as_str(),
        gtk::gio::AppLaunchContext::NONE,
    );
}
