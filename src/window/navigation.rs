//! Moving between pages, with a history to go back through.

use super::*;
use girens_plex::model::LibrarySection;

/// A page of the main view and what it shows.
#[derive(Debug, Clone)]
pub enum Location {
    Home,
    Playlists,
    Downloads,
    Section(LibrarySection),
    Search(String),
    Show(u64),
    Artist(u64),
    Album(u64),
    Player,
}

impl Location {
    /// The sidebar row that leads here, for the pages that have one.
    fn destination(&self) -> Option<Destination> {
        Some(match self {
            Location::Home => Destination::Home,
            Location::Playlists => Destination::Playlists,
            Location::Downloads => Destination::Downloads,
            Location::Section(section) => Destination::Section(section.clone()),
            Location::Player => Destination::Player,
            _ => return None,
        })
    }
}

impl From<Destination> for Location {
    fn from(destination: Destination) -> Self {
        match destination {
            Destination::Home => Location::Home,
            Destination::Playlists => Location::Playlists,
            Destination::Downloads => Location::Downloads,
            Destination::Section(section) => Location::Section(section),
            Destination::Player => Location::Player,
        }
    }
}

impl PlexWindow {
    pub(super) fn setup_navigation(&self) {
        let imp = self.imp();
        imp._back_button.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| window.go_back()
        ));

        // The "back" button of a mouse.
        let back_click = gtk::GestureClick::builder().button(8).build();
        back_click.connect_pressed(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _, _, _| window.go_back()
        ));
        self.add_controller(back_click);

        let shortcuts = gtk::ShortcutController::new();
        shortcuts.set_scope(gtk::ShortcutScope::Global);
        shortcuts.add_shortcut(gtk::Shortcut::new(
            gtk::ShortcutTrigger::parse_string("<Alt>Left"),
            Some(gtk::CallbackAction::new(glib::clone!(
                #[weak(rename_to = window)]
                self,
                #[upgrade_or]
                glib::Propagation::Proceed,
                move |_, _| {
                    window.go_back();
                    glib::Propagation::Stop
                }
            ))),
        ));
        self.add_controller(shortcuts);
    }

    /// Shows the results of a search for `query`.
    pub fn search(&self, query: &str) {
        self.open(Location::Search(query.to_owned()));
    }

    /// Goes to a page chosen in the sidebar: a fresh start for the history.
    pub(super) fn open_root(&self, location: Location) {
        self.imp().history.borrow_mut().clear();
        self.open(location);
    }

    /// Goes to a page, remembering where the user came from.
    pub(super) fn open(&self, location: Location) {
        {
            let mut history = self.imp().history.borrow_mut();
            // Refining a search or reopening the player replaces the entry
            // instead of piling up.
            let replaces = matches!(
                (history.last(), &location),
                (Some(Location::Search(_)), Location::Search(_))
                    | (Some(Location::Player), Location::Player)
            );
            if replaces {
                history.pop();
            }
            history.push(location.clone());
        }
        self.display(&location, true);
    }

    /// Returns to the page before the current one.
    pub(super) fn go_back(&self) {
        let previous = {
            let mut history = self.imp().history.borrow_mut();
            if history.len() < 2 {
                return;
            }
            history.pop();
            history.last().cloned()
        };
        if let Some(previous) = previous {
            self.display(&previous, false);
        }
    }

    /// Leaves the player page when playback ended.
    pub(super) fn leave_player(&self) {
        let on_player = matches!(self.imp().history.borrow().last(), Some(Location::Player));
        if !on_player {
            return;
        }
        if self.imp().history.borrow().len() > 1 {
            self.go_back();
        } else {
            self.open_root(Location::Home);
        }
    }

    /// Shows `location`. A page that is revisited (`fresh` is false) keeps
    /// what it had loaded, and with that its scroll position.
    fn display(&self, location: &Location, fresh: bool) {
        let imp = self.imp();
        let page: gtk::Widget = match location {
            Location::Home => {
                if fresh {
                    imp._discover_view.refresh();
                }
                imp._discover_view.get().upcast()
            }
            Location::Playlists => {
                if fresh || !imp._section_view.shows_playlists() {
                    imp._section_view.show_playlists();
                }
                imp._section_view.get().upcast()
            }
            Location::Downloads => {
                let view = imp.downloads_view.get_or_init(|| {
                    let view = views::DownloadsView::default();
                    imp.page_stack.add(&view);
                    view
                });
                view.refresh();
                view.clone().upcast()
            }
            Location::Section(section) => {
                // Does nothing when this library is already loaded.
                imp._section_view.show_section(section);
                imp._section_view.get().upcast()
            }
            Location::Search(query) => {
                if fresh || imp._search_view.query() != *query {
                    imp._search_view.search(query);
                }
                imp._search_view.get().upcast()
            }
            Location::Show(key) => {
                if fresh || imp._show_view.rating_key() != Some(*key) {
                    imp._show_view.show(*key);
                }
                imp._show_view.get().upcast()
            }
            Location::Artist(key) => {
                if fresh || imp._artist_view.rating_key() != Some(*key) {
                    imp._artist_view.show(*key);
                }
                imp._artist_view.get().upcast()
            }
            Location::Album(key) => {
                if fresh || imp._album_view.rating_key() != Some(*key) {
                    imp._album_view.show(*key);
                }
                imp._album_view.get().upcast()
            }
            Location::Player => imp._player_view.get().upcast(),
        };
        imp.page_stack.set_visible_child(&page);
        imp._sidebar_box
            .show_selected(location.destination().as_ref());
        imp._back_button.set_visible(imp.history.borrow().len() > 1);
    }
}
