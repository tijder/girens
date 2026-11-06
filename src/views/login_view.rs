//! The sign-in page: a plex.tv account in the browser or by password, a
//! Plex server by address and token, or an Ister server by its address.

use std::cell::RefCell;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};

use crate::i18n::gettext;

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
    /// Sign in to the Ister server at `address`, in the browser.
    Ister {
        address: String,
    },
}

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/login_view.ui")]
    pub struct LoginView {
        #[template_child]
        pub _cancel_button: TemplateChild<gtk::Button>,
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
        #[template_child]
        pub _ister_address_entry: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub _connect_ister_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _error_banner: TemplateChild<adw::Banner>,
        #[template_child]
        pub _title_label: TemplateChild<gtk::Label>,
        pub on_request: RefCell<Option<Box<dyn Fn(LoginRequest)>>>,
        pub on_cancel: RefCell<Option<Box<dyn Fn()>>>,
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
                    move |_| view.set_error(None)
                ));
            }

            self._cancel_button.connect_clicked(glib::clone!(
                #[weak]
                view,
                move |_| {
                    if let Some(callback) = view.imp().on_cancel.borrow().as_ref() {
                        callback();
                    }
                }
            ));

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

            let submit_ister = glib::clone!(
                #[weak]
                view,
                move || {
                    let address = view.imp()._ister_address_entry.text().trim().to_owned();
                    if !address.is_empty() {
                        view.request(LoginRequest::Ister { address });
                    }
                }
            );
            let submit = submit_ister.clone();
            self._connect_ister_button.connect_clicked(move |_| submit());
            self._ister_address_entry
                .connect_entry_activated(move |_| submit_ister());
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
    fn entries(&self) -> [adw::EntryRow; 5] {
        let imp = self.imp();
        [
            imp._username_entry.get(),
            imp._password_entry.get().upcast(),
            imp._server_url_entry.get(),
            imp._server_token_entry.get(),
            imp._ister_address_entry.get(),
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

    /// Called when the user would rather stay with who is signed in.
    pub fn connect_cancel(&self, callback: impl Fn() + 'static) {
        self.imp().on_cancel.replace(Some(Box::new(callback)));
    }

    /// Whether there is someone signed in to go back to; the page then
    /// adds an account rather than welcoming the first.
    pub fn set_cancellable(&self, cancellable: bool) {
        let imp = self.imp();
        imp._cancel_button.set_visible(cancellable);
        imp._title_label.set_label(&if cancellable {
            gettext("Add an account")
        } else {
            gettext("Sign in")
        });
    }

    /// Blocks the forms while a login attempt is running.
    pub fn set_busy(&self, busy: bool) {
        let imp = self.imp();
        imp._browser_button.set_sensitive(!busy);
        imp._connect_user_button.set_sensitive(!busy);
        imp._connect_url_button.set_sensitive(!busy);
        imp._connect_ister_button.set_sensitive(!busy);
        for entry in self.entries() {
            entry.set_sensitive(!busy);
        }
    }

    /// Opens the form for an Ister server with its address filled in, to
    /// sign in again where a session ran out.
    pub fn offer_ister(&self, address: &str) {
        let imp = self.imp();
        imp._ister_address_entry.set_text(address);
        imp._ister_address_entry.grab_focus();
    }

    /// Tells why signing in did not work, or takes that away again.
    pub fn set_error(&self, message: Option<&str>) {
        let banner = &self.imp()._error_banner;
        if let Some(message) = message {
            banner.set_title(message);
        }
        banner.set_revealed(message.is_some());
    }

    /// Clears what was typed, e.g. after logging out.
    pub fn reset(&self) {
        for entry in self.entries() {
            entry.set_text("");
        }
        self.set_error(None);
        self.set_busy(false);
    }
}
