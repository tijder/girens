//! The sign-in page: a plex.tv account in the browser or by password, a
//! Plex server by address and token, or an Ister server by its address.

use std::cell::{Cell, RefCell};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};

use crate::i18n::{gettext, gettext_f};

/// How long the user has to stop typing before an address is looked up.
const LOOKUP_DELAY: std::time::Duration = std::time::Duration::from_millis(700);

/// What the user filled in.
#[derive(Debug, Clone)]
pub enum LoginRequest {
    /// Sign in on plex.tv in the browser.
    Browser,
    Account {
        login: String,
        password: String,
        /// Of the authenticator, for an account with two-factor
        /// authentication; empty otherwise.
        verification_code: String,
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
        pub _code_entry: TemplateChild<adw::EntryRow>,
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
        pub _ister_row: TemplateChild<adw::ActionRow>,
        /// Waits for the user to stop typing before the address is looked up.
        pub ister_lookup: RefCell<Option<glib::SourceId>>,
        pub on_ister_address: RefCell<Option<Box<dyn Fn(String)>>>,
        #[template_child]
        pub _error_banner: TemplateChild<adw::Banner>,
        #[template_child]
        pub _title_label: TemplateChild<gtk::Label>,
        pub on_request: RefCell<Option<Box<dyn Fn(LoginRequest)>>>,
        pub on_cancel: RefCell<Option<Box<dyn Fn()>>>,
        /// While a login attempt runs the forms are blocked.
        pub busy: Cell<bool>,
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
                    move |_| {
                        view.set_error(None);
                        view.update_buttons();
                    }
                ));
            }

            self._cancel_button.connect_clicked(glib::clone!(
                #[weak]
                view,
                move |_| view.cancel()
            ));
            // Escape goes back too, when there is somewhere to go back to.
            let escape = gtk::Shortcut::new(
                gtk::ShortcutTrigger::parse_string("Escape"),
                Some(gtk::CallbackAction::new(glib::clone!(
                    #[weak]
                    view,
                    #[upgrade_or]
                    glib::Propagation::Proceed,
                    move |_, _| {
                        if view.is_mapped() && view.imp()._cancel_button.is_visible() {
                            view.cancel();
                            glib::Propagation::Stop
                        } else {
                            glib::Propagation::Proceed
                        }
                    }
                ))),
            );
            let shortcuts = gtk::ShortcutController::new();
            shortcuts.set_scope(gtk::ShortcutScope::Global);
            shortcuts.add_shortcut(escape);
            view.add_controller(shortcuts);

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
                    if !view.account_filled_in() {
                        return;
                    }
                    view.request(LoginRequest::Account {
                        login: imp._username_entry.text().trim().into(),
                        password: imp._password_entry.text().into(),
                        verification_code: imp._code_entry.text().trim().into(),
                    });
                }
            );
            let submit = submit_account.clone();
            self._connect_user_button.connect_clicked(move |_| submit());
            // Enter goes on to the next field; from the last two it signs
            // in, as the code is only there for some.
            self._username_entry.connect_entry_activated(glib::clone!(
                #[weak]
                view,
                move |_| {
                    view.imp()._password_entry.grab_focus();
                }
            ));
            let submit = submit_account.clone();
            self._password_entry
                .connect_entry_activated(move |_| submit());
            let submit = submit_account.clone();
            self._code_entry.connect_entry_activated(move |_| submit());

            let submit_server = glib::clone!(
                #[weak]
                view,
                move || {
                    let imp = view.imp();
                    if !view.server_filled_in() {
                        return;
                    }
                    view.request(LoginRequest::Server {
                        url: imp._server_url_entry.text().trim().into(),
                        token: imp._server_token_entry.text().trim().into(),
                    });
                }
            );
            let submit = submit_server.clone();
            self._connect_url_button.connect_clicked(move |_| submit());
            self._server_url_entry.connect_entry_activated(glib::clone!(
                #[weak]
                view,
                move |_| {
                    view.imp()._server_token_entry.grab_focus();
                }
            ));
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
            self._ister_address_entry.connect_changed(glib::clone!(
                #[weak]
                view,
                move |_| view.look_up_ister_soon()
            ));
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
    fn entries(&self) -> [adw::EntryRow; 6] {
        let imp = self.imp();
        [
            imp._username_entry.get(),
            imp._password_entry.get().upcast(),
            imp._code_entry.get(),
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

    fn cancel(&self) {
        if let Some(callback) = self.imp().on_cancel.borrow().as_ref() {
            callback();
        }
    }

    fn account_filled_in(&self) -> bool {
        let imp = self.imp();
        !imp._username_entry.text().trim().is_empty() && !imp._password_entry.text().is_empty()
    }

    fn server_filled_in(&self) -> bool {
        let imp = self.imp();
        !imp._server_url_entry.text().trim().is_empty()
            && !imp._server_token_entry.text().trim().is_empty()
    }

    /// Each form can only be sent once it is filled in, and not while a
    /// login attempt runs.
    fn update_buttons(&self) {
        let imp = self.imp();
        let free = !imp.busy.get();
        imp._browser_button.set_sensitive(free);
        imp._connect_user_button
            .set_sensitive(free && self.account_filled_in());
        imp._connect_url_button
            .set_sensitive(free && self.server_filled_in());
        imp._connect_ister_button
            .set_sensitive(free && !self.ister_address().is_empty());
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
        self.imp().busy.set(busy);
        for entry in self.entries() {
            entry.set_sensitive(!busy);
        }
        self.update_buttons();
    }

    /// Opens the form for an Ister server with its address filled in, to
    /// sign in again where a session ran out.
    pub fn offer_ister(&self, address: &str) {
        let imp = self.imp();
        imp._ister_address_entry.set_text(address);
        imp._ister_address_entry.grab_focus();
    }

    /// Has the address of the Ister server looked up once the user has
    /// stopped typing for a moment.
    fn look_up_ister_soon(&self) {
        let imp = self.imp();
        if let Some(pending) = imp.ister_lookup.take() {
            pending.remove();
        }
        self.set_ister_found(None);
        let address = imp._ister_address_entry.text().trim().to_owned();
        if address.len() < 4 {
            return;
        }
        let lookup = glib::timeout_add_local_once(
            LOOKUP_DELAY,
            glib::clone!(
                #[weak(rename_to = view)]
                self,
                move || {
                    view.imp().ister_lookup.take();
                    view.set_ister_looking();
                    if let Some(callback) = view.imp().on_ister_address.borrow().as_ref() {
                        callback(address);
                    }
                }
            ),
        );
        imp.ister_lookup.replace(Some(lookup));
    }

    /// Called with the address of an Ister server once it was typed, to
    /// look it up and answer with [`Self::set_ister_found`].
    pub fn connect_ister_address(&self, callback: impl Fn(String) + 'static) {
        self.imp().on_ister_address.replace(Some(Box::new(callback)));
    }

    /// What is typed as the address of an Ister server now.
    pub fn ister_address(&self) -> String {
        self.imp()._ister_address_entry.text().trim().to_owned()
    }

    /// Tells what was found at the address: `Some(Ok(name))` for a server,
    /// `Some(Err(()))` for none, `None` while that is not known.
    pub fn set_ister_found(&self, found: Option<Result<&str, ()>>) {
        let imp = self.imp();
        let entry = &imp._ister_address_entry;
        entry.remove_css_class("success");
        entry.remove_css_class("warning");
        let subtitle = match found {
            Some(Ok(name)) => {
                entry.add_css_class("success");
                gettext_f("Found {}. Opens your browser to sign in", &[name])
            }
            Some(Err(())) => {
                entry.add_css_class("warning");
                gettext("No Ister server answers at this address")
            }
            None => gettext("Opens your browser to sign in"),
        };
        imp._ister_row.set_subtitle(&glib::markup_escape_text(&subtitle));
    }

    /// Tells that the address is being looked up.
    fn set_ister_looking(&self) {
        self.set_ister_found(None);
        self.imp()
            ._ister_row
            .set_subtitle(&glib::markup_escape_text(&gettext("Looking up the server…")));
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
