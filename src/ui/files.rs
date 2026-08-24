use relm4::factory::{DynamicIndex, FactoryComponent, FactorySender};
use relm4::gtk;
use relm4::gtk::prelude::*;

use crate::workspace::FileItem;

#[relm4::factory(pub)]
impl FactoryComponent for FileItem {
    type Init = FileItem;
    type Input = ();
    type Output = ();
    type CommandOutput = ();
    type ParentWidget = gtk::ListBox;

    view! {
        gtk::ListBoxRow {
            add_css_class: "file-row",
            set_activatable: true,

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 10,
                set_margin_start: 10,
                set_margin_end: 10,
                set_margin_top: 6,
                set_margin_bottom: 6,

                gtk::Image {
                    set_icon_name: Some(self.icon_name()),
                    set_pixel_size: 16,
                },

                gtk::Label {
                    set_label: &self.name,
                    set_halign: gtk::Align::Start,
                    set_hexpand: true,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    add_css_class: if self.is_dir { "heading" } else { "" },
                },

                gtk::Label {
                    set_label: &self.size_label,
                    add_css_class: "dim-label",
                    set_visible: !self.size_label.is_empty(),
                }
            }
        }
    }

    fn init_model(item: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        item
    }
}
