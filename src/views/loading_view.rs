//! Shown while connecting, and when connecting failed.

use std::cell::RefCell;

use adw::subclass::prelude::*;
use gtk::prelude::*;
use gtk::{glib, CompositeTemplate};

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/loading_view.ui")]
    pub struct LoadingView {
        #[template_child]
        pub _loading_text_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _logout_button: TemplateChild<gtk::Button>,
        pub on_logout: RefCell<Option<Box<dyn Fn()>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LoadingView {
        const NAME: &'static str = "loading_view";
        type Type = super::LoadingView;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LoadingView {
        fn constructed(&self) {
            self.parent_constructed();
            let view = self.obj();
            self._logout_button.connect_clicked(glib::clone!(
                #[weak]
                view,
                move |_| {
                    if let Some(callback) = view.imp().on_logout.borrow().as_ref() {
                        callback();
                    }
                }
            ));
        }
    }

    impl WidgetImpl for LoadingView {}
    impl BoxImpl for LoadingView {}
}

glib::wrapper! {
    pub struct LoadingView(ObjectSubclass<imp::LoadingView>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl LoadingView {
    pub fn set_text(&self, text: &str) {
        self.imp()._loading_text_label.set_text(text);
    }

    pub fn connect_logout(&self, callback: impl Fn() + 'static) {
        self.imp().on_logout.replace(Some(Box::new(callback)));
    }
}
