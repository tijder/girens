//! One row of the sidebar: an icon and a title.

use adw::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/section_grid.ui")]
    pub struct SectionGrid {
        #[template_child]
        pub _title_button: TemplateChild<gtk::Label>,
        #[template_child]
        pub _image: TemplateChild<gtk::Image>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SectionGrid {
        const NAME: &'static str = "section_grid";
        type Type = super::SectionGrid;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SectionGrid {}
    impl WidgetImpl for SectionGrid {}
    impl BoxImpl for SectionGrid {}
}

glib::wrapper! {
    pub struct SectionGrid(ObjectSubclass<imp::SectionGrid>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl SectionGrid {
    pub fn new(title: &str, icon_name: &str) -> Self {
        let row: Self = glib::Object::new();
        row.imp()._title_button.set_label(title);
        row.imp()._image.set_icon_name(Some(icon_name));
        row
    }
}
