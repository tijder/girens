//! Account details and the logout button.

use std::cell::RefCell;

use adw::prelude::*;
use adw::subclass::prelude::*;
use girens_plex::myplex::Account;
use gtk::{glib, CompositeTemplate};

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/profile_dialog.ui")]
    pub struct ProfileDialog {
        #[template_child]
        pub _username_value_label: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub _email_value_label: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub _subscriptionplan_value_label: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub _logout_button: TemplateChild<gtk::Button>,
        pub on_logout: RefCell<Option<Box<dyn Fn()>>>,
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
            self._logout_button.connect_clicked(glib::clone!(
                #[weak]
                dialog,
                move |_| {
                    if let Some(callback) = dialog.imp().on_logout.borrow().as_ref() {
                        callback();
                    }
                    dialog.close();
                }
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

impl ProfileDialog {
    /// `account` is `None` when connected by server address only.
    pub fn new(account: Option<&Account>) -> Self {
        let dialog: Self = glib::Object::new();
        let imp = dialog.imp();
        if let Some(account) = account {
            imp._username_value_label
                .set_title(account.username.as_deref().unwrap_or_default());
            imp._email_value_label
                .set_title(account.email.as_deref().unwrap_or_default());
            let plan = account
                .subscription
                .as_ref()
                .and_then(|subscription| subscription.plan.as_deref())
                .unwrap_or_default();
            imp._subscriptionplan_value_label.set_title(plan);
        }
        dialog
    }

    pub fn connect_logout(&self, callback: impl Fn() + 'static) {
        self.imp().on_logout.replace(Some(Box::new(callback)));
    }
}
