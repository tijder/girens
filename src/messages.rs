//! Failure messages that are used inside `glib::clone!` blocks.
//!
//! xgettext does not look inside macro invocations, so a `gettext("…")`
//! written there would never reach the translators. Each message is
//! therefore a small function here.

use crate::i18n::gettext;

pub fn open_item() -> String {
    gettext("Could not open the item")
}

pub fn load_item() -> String {
    gettext("Could not load the item")
}

pub fn change_watched() -> String {
    gettext("Could not change the watched state")
}

pub fn load_on_deck() -> String {
    gettext("Could not load on deck")
}

pub fn load_recently_added() -> String {
    gettext("Could not load recently added")
}

pub fn load_sorts() -> String {
    gettext("Could not load the sort options")
}

pub fn load_library() -> String {
    gettext("Could not load the library")
}

pub fn load_playlists() -> String {
    gettext("Could not load the playlists")
}

pub fn search() -> String {
    gettext("Search failed")
}

pub fn load_show() -> String {
    gettext("Could not load the show")
}

pub fn load_episodes() -> String {
    gettext("Could not load the episodes")
}

pub fn load_artist() -> String {
    gettext("Could not load the artist")
}

pub fn load_synced_item() -> String {
    gettext("Could not load a synced item")
}

pub fn load_album() -> String {
    gettext("Could not load the album")
}
