//! The sidebar: server chooser and the list of destinations.

use std::cell::{Cell, RefCell};

use adw::subclass::prelude::*;
use girens_plex::model::{ItemKind, LibrarySection};
use gtk::prelude::*;
use gtk::{glib, CompositeTemplate};

use super::SectionGrid;
use crate::i18n::gettext;

/// Where a sidebar row leads.
#[derive(Debug, Clone)]
pub enum Destination {
    Player,
    Home,
    Playlists,
    /// The items with an offline copy.
    Downloads,
    Section(LibrarySection),
}

const PLAYER_ROW: i32 = 0;
const HOME_ROW: i32 = 1;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/sidebar_box.ui")]
    pub struct SidebarBox {
        #[template_child]
        pub _server_box: TemplateChild<gtk::DropDown>,
        #[template_child]
        pub _section_list: TemplateChild<gtk::ListBox>,
        /// Destination of each row, by row index.
        pub destinations: RefCell<Vec<Destination>>,
        /// Set while the server list is filled, to tell that apart from the
        /// user choosing a server.
        pub updating_servers: Cell<bool>,
        /// Set while the selection follows the page, not the user.
        pub following: Cell<bool>,
        /// Index of the row of the page on screen, if it has one.
        pub shown: Cell<Option<i32>>,
        pub on_destination: RefCell<Option<Box<dyn Fn(Destination)>>>,
        pub on_server: RefCell<Option<Box<dyn Fn(u32)>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SidebarBox {
        const NAME: &'static str = "sidebar_box";
        type Type = super::SidebarBox;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SidebarBox {
        fn constructed(&self) {
            self.parent_constructed();
            let sidebar = self.obj();

            // Only activating a row (a click, Enter) leads somewhere. The
            // list also selects a row that merely gets the focus, which
            // happens when the sidebar appears; that selection is undone.
            self._section_list.connect_row_activated(glib::clone!(
                #[weak]
                sidebar,
                move |_, row| sidebar.go_to(row.index())
            ));
            self._section_list.connect_row_selected(glib::clone!(
                #[weak]
                sidebar,
                move |_, _| {
                    if !sidebar.imp().following.get() {
                        sidebar.mark(sidebar.imp().shown.get());
                    }
                }
            ));

            self._server_box.connect_selected_notify(glib::clone!(
                #[weak]
                sidebar,
                move |dropdown| {
                    let imp = sidebar.imp();
                    if imp.updating_servers.get()
                        || dropdown.selected() == gtk::INVALID_LIST_POSITION
                    {
                        return;
                    }
                    if let Some(callback) = imp.on_server.borrow().as_ref() {
                        callback(dropdown.selected());
                    }
                }
            ));
        }
    }

    impl WidgetImpl for SidebarBox {}
    impl BoxImpl for SidebarBox {}
}

glib::wrapper! {
    pub struct SidebarBox(ObjectSubclass<imp::SidebarBox>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl SidebarBox {
    /// Called when the user selects a row.
    pub fn connect_destination(&self, callback: impl Fn(Destination) + 'static) {
        self.imp().on_destination.replace(Some(Box::new(callback)));
    }

    /// Called with the index of the server the user picked.
    pub fn connect_server_selected(&self, callback: impl Fn(u32) + 'static) {
        self.imp().on_server.replace(Some(Box::new(callback)));
    }

    /// Fills the server chooser. It is hidden when there is only one server.
    pub fn set_servers(&self, names: &[&str], active: Option<u32>) {
        let imp = self.imp();
        imp.updating_servers.set(true);
        imp._server_box
            .set_model(Some(&gtk::StringList::new(names)));
        imp._server_box
            .set_selected(active.unwrap_or(gtk::INVALID_LIST_POSITION));
        imp._server_box.set_visible(names.len() > 1);
        imp.updating_servers.set(false);
    }

    /// Rebuilds the list: Player (hidden until a video plays), Home,
    /// Playlists and one row per movie, show and music library.
    pub fn set_sections(&self, sections: &[LibrarySection]) {
        let imp = self.imp();
        imp._section_list.remove_all();
        let mut destinations = Vec::new();
        let mut add = |title: &str, icon: &str, destination: Destination| {
            imp._section_list.append(&SectionGrid::new(title, icon));
            destinations.push(destination);
        };

        add(
            &gettext("Player"),
            "media-playback-start-symbolic",
            Destination::Player,
        );
        add(&gettext("Home"), "user-home-symbolic", Destination::Home);
        add(
            &gettext("Playlists"),
            "view-list-symbolic",
            Destination::Playlists,
        );
        add(
            &gettext("Downloads"),
            "folder-download-symbolic",
            Destination::Downloads,
        );
        for section in sections {
            let icon = match section.kind {
                ItemKind::Movie => "camera-video-symbolic",
                ItemKind::Show => "video-display-symbolic",
                ItemKind::Artist => "audio-x-generic-symbolic",
                _ => continue,
            };
            add(&section.title, icon, Destination::Section(section.clone()));
        }

        imp.destinations.replace(destinations);
        self.set_player_visible(false);
        self.select_home();
    }

    /// The list for when the server cannot be reached: only what works
    /// without it.
    pub fn set_offline(&self) {
        let imp = self.imp();
        imp._section_list.remove_all();
        let rows = [
            (
                gettext("Player"),
                "media-playback-start-symbolic",
                Destination::Player,
            ),
            (
                gettext("Downloads"),
                "folder-download-symbolic",
                Destination::Downloads,
            ),
        ];
        let mut destinations = Vec::new();
        for (title, icon, destination) in rows {
            imp._section_list.append(&SectionGrid::new(&title, icon));
            destinations.push(destination);
        }
        imp.destinations.replace(destinations);
        self.set_servers(&[], None);
        self.set_player_visible(false);
        // The second row, which is Downloads here.
        self.select_home();
    }

    pub fn clear(&self) {
        let imp = self.imp();
        imp._section_list.remove_all();
        imp.destinations.borrow_mut().clear();
        self.set_servers(&[], None);
    }

    /// The Player row is only offered while a video is playing.
    pub fn set_player_visible(&self, visible: bool) {
        if let Some(row) = self.imp()._section_list.row_at_index(PLAYER_ROW) {
            row.set_visible(visible);
        }
    }

    /// Marks the row that leads to the page on screen, or none for pages
    /// that are not in the sidebar. Does not navigate.
    pub fn show_selected(&self, destination: Option<&Destination>) {
        let imp = self.imp();
        let index = destination.and_then(|wanted| {
            imp.destinations
                .borrow()
                .iter()
                .position(|row| match (row, wanted) {
                    (Destination::Home, Destination::Home)
                    | (Destination::Playlists, Destination::Playlists)
                    | (Destination::Downloads, Destination::Downloads)
                    | (Destination::Player, Destination::Player) => true,
                    (Destination::Section(a), Destination::Section(b)) => a.uuid == b.uuid,
                    _ => false,
                })
        });
        self.mark(index.map(|index| index as i32));
    }

    /// Selects the row at `index`, or none, without going anywhere.
    fn mark(&self, index: Option<i32>) {
        let imp = self.imp();
        let list = &imp._section_list;
        imp.shown.set(index);
        imp.following.set(true);
        match index.and_then(|index| list.row_at_index(index)) {
            Some(row) => list.select_row(Some(&row)),
            None => list.unselect_all(),
        }
        imp.following.set(false);
    }

    /// Opens the destination of the row at `index`.
    fn go_to(&self, index: i32) {
        let imp = self.imp();
        let destination = imp.destinations.borrow().get(index as usize).cloned();
        if let (Some(destination), Some(callback)) =
            (destination, imp.on_destination.borrow().as_ref())
        {
            callback(destination);
        }
    }

    pub fn select_home(&self) {
        self.select(HOME_ROW);
    }

    pub fn select_player(&self) {
        self.select(PLAYER_ROW);
    }

    /// Selects the row at `index`, as if the user clicked it.
    #[cfg(debug_assertions)]
    pub fn select_row(&self, index: i32) {
        self.select(index);
    }

    fn select(&self, index: i32) {
        self.mark(Some(index));
        self.go_to(index);
    }
}
