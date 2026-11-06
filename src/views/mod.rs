//! Views and widgets referenced from the Blueprint templates.
//!
//! gtk4-rs can only subclass widgets with a public class struct. Templates
//! that were rooted in `Gtk.ScrolledWindow`, `Gtk.Overlay`, `Gtk.Revealer` or
//! `Gtk.GridView` therefore have `Adw.Bin` as their parent now, with the
//! original widget as its child.

use gtk::prelude::*;

mod album_item;
mod album_view;
mod artist_view;
mod cover_box;
mod discover_view;
mod downloads_view;
mod list;
mod loading_view;
mod login_view;
mod media_box;
mod music_popover_menu;
mod photo_view;
mod player_view;
pub mod preferences;
mod profile_dialog;
mod queue_popover;
mod row_list;
mod search_view;
mod section_grid;
mod section_view;
mod show_view;
mod show_view_item;
mod sidebar_box;
mod sync_views;
mod theme_switcher;

pub use album_item::AlbumItem;
pub use album_view::AlbumView;
pub use artist_view::ArtistView;
pub use cover_box::CoverBox;
pub use discover_view::DiscoverView;
pub use downloads_view::DownloadsView;
pub use list::List;
pub use loading_view::LoadingView;
pub use login_view::{LoginRequest, LoginView};
pub use media_box::{MediaBoxMusic, MediaBoxVideoBottom, MediaBoxVideoTop, MediaControls};
pub use music_popover_menu::MusicPopoverMenu;
pub use photo_view::PhotoView;
pub use player_view::PlayerView;
pub use profile_dialog::ProfileDialog;
pub use queue_popover::QueuePopover;
pub use search_view::SearchView;
pub use section_grid::SectionGrid;
pub use section_view::SectionView;
pub use show_view::ShowView;
pub use show_view_item::ShowViewItem;
pub use sidebar_box::{Destination, SidebarBox};
pub use sync_views::{DownloadMenu, SyncDialog, SyncSettings};
pub use theme_switcher::ThemeSwitcher;

/// Makes every custom type known to GtkBuilder before a template uses it.
pub fn ensure_types() {
    LoginView::ensure_type();
    LoadingView::ensure_type();
    DiscoverView::ensure_type();
    ShowView::ensure_type();
    SectionView::ensure_type();
    SearchView::ensure_type();
    ArtistView::ensure_type();
    AlbumView::ensure_type();
    PlayerView::ensure_type();
    MediaBoxMusic::ensure_type();
    ThemeSwitcher::ensure_type();
    SidebarBox::ensure_type();
}
