//! Who is using Girens: the profiles of the Plex Home to switch between, the
//! accounts that are signed in, and the details of the active one.

use std::cell::RefCell;

use adw::prelude::*;
use adw::subclass::prelude::*;
use girens_plex::myplex::Account;
use gtk::{gdk, glib, CompositeTemplate};

use crate::i18n::{gettext, gettext_f};
use crate::runtime;
use crate::store::{self, AccountKind, Profile};

type Callback<T> = RefCell<Option<Box<dyn Fn(T)>>>;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/profile_dialog.ui")]
    pub struct ProfileDialog {
        #[template_child]
        pub _profiles_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub _accounts_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub _add_button: TemplateChild<adw::ButtonRow>,
        #[template_child]
        pub _info_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub _username_value_label: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub _email_value_label: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub _subscriptionplan_value_label: TemplateChild<adw::ActionRow>,
        pub profile_rows: RefCell<Vec<adw::ActionRow>>,
        pub account_rows: RefCell<Vec<adw::ActionRow>>,
        pub on_profile: Callback<Profile>,
        pub on_account: Callback<String>,
        pub on_remove: Callback<String>,
        pub on_add: Callback<()>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ProfileDialog {
        const NAME: &'static str = "profile_dialog";
        type Type = super::ProfileDialog;
        type ParentType = adw::PreferencesDialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ProfileDialog {
        fn constructed(&self) {
            self.parent_constructed();
            let dialog = self.obj();
            self._add_button.connect_activated(glib::clone!(
                #[weak]
                dialog,
                move |_| dialog.choose(&dialog.imp().on_add, ())
            ));
        }
    }

    impl WidgetImpl for ProfileDialog {}
    impl AdwDialogImpl for ProfileDialog {}
    impl PreferencesDialogImpl for ProfileDialog {}
}

glib::wrapper! {
    pub struct ProfileDialog(ObjectSubclass<imp::ProfileDialog>)
        @extends adw::PreferencesDialog, adw::Dialog, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::ShortcutManager;
}

/// A round picture of someone: the initials, replaced by the avatar once it
/// is downloaded.
fn avatar(name: &str, thumb: Option<&str>) -> adw::Avatar {
    let avatar = adw::Avatar::new(32, Some(name), true);
    if let Some(url) = thumb.map(str::to_owned) {
        glib::spawn_future_local(glib::clone!(
            #[weak]
            avatar,
            async move {
                let Ok(bytes) = runtime::run(async move { girens_model::net::download(&url).await }).await
                else {
                    return;
                };
                if let Ok(texture) = gdk::Texture::from_bytes(&glib::Bytes::from_owned(bytes)) {
                    avatar.set_custom_image(Some(&texture));
                }
            }
        ));
    }
    avatar
}

/// The mark on the row of who is active.
fn active_mark() -> gtk::Image {
    let image = gtk::Image::from_icon_name("object-select-symbolic");
    image.set_tooltip_text(Some(&gettext("In use")));
    image
}

impl ProfileDialog {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Closes the dialog and tells what was chosen.
    fn choose<T>(&self, callback: &Callback<T>, value: T) {
        self.close();
        if let Some(callback) = callback.borrow().as_ref() {
            callback(value);
        }
    }

    /// Shows the details of the active profile; `None` when connected to a
    /// server by address, which has none.
    pub fn set_account(&self, account: Option<&Account>) {
        let imp = self.imp();
        imp._info_group.set_visible(account.is_some());
        let Some(account) = account else {
            return;
        };
        let name = account.username.as_deref().or(account.title.as_deref());
        imp._username_value_label
            .set_title(name.unwrap_or_default());
        imp._email_value_label
            .set_title(account.email.as_deref().unwrap_or_default());
        imp._email_value_label.set_visible(account.email.is_some());
        let plan = account
            .subscription
            .as_ref()
            .and_then(|subscription| subscription.plan.as_deref())
            .unwrap_or_default();
        imp._subscriptionplan_value_label.set_title(plan);
    }

    /// Offers the profiles of the Plex Home; nothing when the account is
    /// the only one in it.
    pub fn set_profiles(&self, profiles: &[Profile], active: Option<&str>) {
        let imp = self.imp();
        for row in imp.profile_rows.take() {
            imp._profiles_group.remove(&row);
        }
        imp._profiles_group.set_visible(profiles.len() > 1);
        if profiles.len() < 2 {
            return;
        }
        let mut rows = Vec::new();
        for profile in profiles {
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(&profile.title))
                .build();
            if profile.restricted {
                row.set_subtitle(&gettext("Managed profile"));
            }
            row.add_prefix(&avatar(&profile.title, profile.thumb.as_deref()));
            if profile.protected {
                let lock = gtk::Image::from_icon_name("system-lock-screen-symbolic");
                lock.set_tooltip_text(Some(&gettext("Protected with a PIN")));
                row.add_suffix(&lock);
            }
            if active == Some(profile.uuid.as_str()) {
                row.add_suffix(&active_mark());
            } else {
                row.set_activatable(true);
                row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
                let profile = profile.clone();
                row.connect_activated(glib::clone!(
                    #[weak(rename_to = dialog)]
                    self,
                    move |_| dialog.choose(&dialog.imp().on_profile, profile.clone())
                ));
            }
            imp._profiles_group.add(&row);
            rows.push(row);
        }
        imp.profile_rows.replace(rows);
    }

    /// Lists the accounts that are signed in.
    pub fn set_accounts(&self, accounts: &[store::Account], active: Option<&str>) {
        let imp = self.imp();
        for row in imp.account_rows.take() {
            imp._accounts_group.remove(&row);
        }
        // The button to add one stays last.
        imp._accounts_group.remove(&*imp._add_button);
        let mut rows = Vec::new();
        for account in accounts {
            let kind = match account.kind {
                AccountKind::Plex => gettext("Plex account"),
                AccountKind::Server => gettext("Server"),
                AccountKind::Ister => gettext("Ister account"),
            };
            let row = adw::ActionRow::new();
            if account.name.is_empty() {
                // Its name is not known until plex.tv has been asked once.
                row.set_title(&kind);
            } else {
                row.set_title(&glib::markup_escape_text(&account.name));
                row.set_subtitle(&kind);
            }
            row.add_prefix(&avatar(&account.name, account.thumb.as_deref()));
            if active == Some(account.id.as_str()) {
                row.add_suffix(&active_mark());
            } else {
                row.set_activatable(true);
                let id = account.id.clone();
                row.connect_activated(glib::clone!(
                    #[weak(rename_to = dialog)]
                    self,
                    move |_| dialog.choose(&dialog.imp().on_account, id.clone())
                ));
            }
            let sign_out = gtk::Button::builder()
                .icon_name("system-log-out-symbolic")
                .tooltip_text(gettext_f("Sign out {}", &[&account.name]))
                .valign(gtk::Align::Center)
                .css_classes(["flat"])
                .build();
            let id = account.id.clone();
            sign_out.connect_clicked(glib::clone!(
                #[weak(rename_to = dialog)]
                self,
                move |_| dialog.choose(&dialog.imp().on_remove, id.clone())
            ));
            row.add_suffix(&sign_out);
            imp._accounts_group.add(&row);
            rows.push(row);
        }
        imp._accounts_group.add(&*imp._add_button);
        imp.account_rows.replace(rows);
    }

    /// Another profile of the Plex Home was chosen.
    pub fn connect_profile(&self, callback: impl Fn(Profile) + 'static) {
        self.imp().on_profile.replace(Some(Box::new(callback)));
    }

    /// Another account was chosen; the argument is its id.
    pub fn connect_account(&self, callback: impl Fn(String) + 'static) {
        self.imp().on_account.replace(Some(Box::new(callback)));
    }

    /// The account with this id is to sign out.
    pub fn connect_remove(&self, callback: impl Fn(String) + 'static) {
        self.imp().on_remove.replace(Some(Box::new(callback)));
    }

    pub fn connect_add(&self, callback: impl Fn() + 'static) {
        self.imp()
            .on_add
            .replace(Some(Box::new(move |()| callback())));
    }
}

impl Default for ProfileDialog {
    fn default() -> Self {
        Self::new()
    }
}
