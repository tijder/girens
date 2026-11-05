//! The sign-in page: plex.tv account, or server address and token.

use std::cell::RefCell;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};

/// What the user filled in.
#[derive(Debug, Clone)]
pub enum LoginRequest {
    /// Sign in on plex.tv in the browser.
    Browser,
    Account {
        login: String,
        password: String,
    },
    Server {
        url: String,
        token: String,
    },
}

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/login_view.ui")]
    pub struct LoginView {
        #[template_child]
        pub _browser_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _username_entry: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub _password_entry: TemplateChild<adw::PasswordEntryRow>,
        #[template_child]
        pub _connect_user_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _server_url_entry: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub _server_token_entry: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub _connect_url_button: TemplateChild<gtk::Button>,
        pub on_request: RefCell<Option<Box<dyn Fn(LoginRequest)>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LoginView {
        const NAME: &'static str = "login_view";
        type Type = super::LoginView;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LoginView {
        fn constructed(&self) {
            self.parent_constructed();
            let view = self.obj();

            for entry in view.entries() {
                entry.connect_changed(glib::clone!(
                    #[weak]
                    view,
                    move |_| view.set_error(false)
                ));
            }

            self._browser_button.connect_clicked(glib::clone!(
                #[weak]
                view,
                move |_| view.request(LoginRequest::Browser)
            ));

            let submit_account = glib::clone!(
                #[weak]
                view,
                move || {
                    let imp = view.imp();
                    view.request(LoginRequest::Account {
                        login: imp._username_entry.text().into(),
                        password: imp._password_entry.text().into(),
                    });
                }
            );
            let submit = submit_account.clone();
            self._connect_user_button.connect_clicked(move |_| submit());
            let submit = submit_account.clone();
            self._password_entry
                .connect_entry_activated(move |_| submit());

            let submit_server = glib::clone!(
                #[weak]
                view,
                move || {
                    let imp = view.imp();
                    view.request(LoginRequest::Server {
                        url: imp._server_url_entry.text().trim().into(),
                        token: imp._server_token_entry.text().trim().into(),
                    });
                }
            );
            let submit = submit_server.clone();
            self._connect_url_button.connect_clicked(move |_| submit());
            self._server_token_entry
                .connect_entry_activated(move |_| submit_server());
        }
    }

    impl WidgetImpl for LoginView {}
    impl BoxImpl for LoginView {}
}

glib::wrapper! {
    pub struct LoginView(ObjectSubclass<imp::LoginView>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl LoginView {
    fn entries(&self) -> [adw::EntryRow; 4] {
        let imp = self.imp();
        [
            imp._username_entry.get(),
            imp._password_entry.get().upcast(),
            imp._server_url_entry.get(),
            imp._server_token_entry.get(),
        ]
    }

    fn request(&self, request: LoginRequest) {
        if let Some(callback) = self.imp().on_request.borrow().as_ref() {
            callback(request);
        }
    }

    /// Called when the user submits one of the two forms.
    pub fn connect_request(&self, callback: impl Fn(LoginRequest) + 'static) {
        self.imp().on_request.replace(Some(Box::new(callback)));
    }

    /// Blocks the forms while a login attempt is running.
    pub fn set_busy(&self, busy: bool) {
        let imp = self.imp();
        imp._browser_button.set_sensitive(!busy);
        imp._connect_user_button.set_sensitive(!busy);
        imp._connect_url_button.set_sensitive(!busy);
        for entry in self.entries() {
            entry.set_sensitive(!busy);
        }
    }

    /// Marks the fields after a failed attempt.
    pub fn set_error(&self, error: bool) {
        for entry in self.entries() {
            if error {
                entry.add_css_class("error");
            } else {
                entry.remove_css_class("error");
            }
        }
    }

    /// Clears what was typed, e.g. after logging out.
    pub fn reset(&self) {
        for entry in self.entries() {
            entry.set_text("");
        }
        self.set_error(false);
        self.set_busy(false);
    }
}
