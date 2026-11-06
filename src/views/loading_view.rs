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
        #[template_child]
        pub _accounts_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _spinner: TemplateChild<adw::Spinner>,
        #[template_child]
        pub _action_button: TemplateChild<gtk::Button>,
        pub on_action: RefCell<Option<Box<dyn Fn()>>>,
        #[template_child]
        pub _waiting_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub _open_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _copy_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _cancel_button: TemplateChild<gtk::Button>,
        /// The page the user is signing in at, while waiting for that.
        pub waiting_for: RefCell<Option<String>>,
        pub on_cancel: RefCell<Option<Box<dyn Fn()>>>,
        pub on_logout: RefCell<Option<Box<dyn Fn()>>>,
        pub on_accounts: RefCell<Option<Box<dyn Fn()>>>,
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
            self._action_button.connect_clicked(glib::clone!(
                #[weak]
                view,
                move |_| {
                    if let Some(callback) = view.imp().on_action.borrow().as_ref() {
                        callback();
                    }
                }
            ));
            self._cancel_button.connect_clicked(glib::clone!(
                #[weak]
                view,
                move |_| {
                    if let Some(callback) = view.imp().on_cancel.borrow().as_ref() {
                        callback();
                    }
                }
            ));
            self._open_button.connect_clicked(glib::clone!(
                #[weak]
                view,
                move |_| {
                    let Some(url) = view.imp().waiting_for.borrow().clone() else {
                        return;
                    };
                    let window = view.root().and_downcast::<gtk::Window>();
                    gtk::UriLauncher::new(&url).launch(
                        window.as_ref(),
                        gtk::gio::Cancellable::NONE,
                        |result| {
                            if let Err(err) = result {
                                eprintln!("Could not open the browser: {err}");
                            }
                        },
                    );
                }
            ));
            self._copy_button.connect_clicked(glib::clone!(
                #[weak]
                view,
                move |_| {
                    if let Some(url) = view.imp().waiting_for.borrow().as_deref() {
                        view.clipboard().set_text(url);
                    }
                }
            ));
            self._accounts_button.connect_clicked(glib::clone!(
                #[weak]
                view,
                move |_| {
                    if let Some(callback) = view.imp().on_accounts.borrow().as_ref() {
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

    /// Waits for the user to sign in at `url` in the browser: offers to
    /// open that page again, to copy its address for another browser, and
    /// to give up. `None` is the usual page again.
    pub fn set_waiting(&self, url: Option<&str>) {
        let imp = self.imp();
        imp.waiting_for.replace(url.map(str::to_owned));
        imp._waiting_box.set_visible(url.is_some());
        imp._logout_button.set_visible(url.is_none());
    }

    /// Instead of being busy, offers the one thing to do next, as a button
    /// called `label`. `None` is the usual page again.
    pub fn set_action(&self, label: Option<&str>) {
        let imp = self.imp();
        if let Some(label) = label {
            imp._action_button.set_label(label);
        }
        imp._action_button.set_visible(label.is_some());
        imp._spinner.set_visible(label.is_none());
    }

    /// Doing what [`Self::set_action`] offers.
    pub fn connect_action(&self, callback: impl Fn() + 'static) {
        self.imp().on_action.replace(Some(Box::new(callback)));
    }

    #[cfg(debug_assertions)]
    pub fn debug_press(&self, button: &str) {
        let imp = self.imp();
        match button {
            "cancel" => imp._cancel_button.emit_clicked(),
            "copy" => imp._copy_button.emit_clicked(),
            _ => eprintln!("GIRENS_DEBUG_WAITING: no button {button:?}"),
        }
    }

    /// Giving up the sign-in that is being waited for.
    pub fn connect_cancel(&self, callback: impl Fn() + 'static) {
        self.imp().on_cancel.replace(Some(Box::new(callback)));
    }

    /// Signing out of the account that is being connected.
    pub fn connect_logout(&self, callback: impl Fn() + 'static) {
        self.imp().on_logout.replace(Some(Box::new(callback)));
    }

    /// Going to the other accounts.
    pub fn connect_accounts(&self, callback: impl Fn() + 'static) {
        self.imp().on_accounts.replace(Some(Box::new(callback)));
    }

    /// Whether there are other accounts to go to.
    pub fn set_accounts_visible(&self, visible: bool) {
        self.imp()._accounts_button.set_visible(visible);
    }
}
