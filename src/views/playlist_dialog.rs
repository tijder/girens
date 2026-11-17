//! The dialog that asks which playlist a track is to be added to.

use adw::prelude::*;
use girens_model::model::Item;
use girens_model::ItemId;
use gtk::glib;

use crate::i18n::{gettext, gettext_f};
use crate::runtime;
use crate::session::Session;

/// Asks which playlist the track with this rating key goes on, and adds it.
pub fn present(parent: &impl IsA<gtk::Widget>, track: ItemId) {
    let Some(server) = Session::current().server() else {
        return;
    };

    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .valign(gtk::Align::Start)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&list)
        .build();
    let spinner = adw::Spinner::builder()
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .width_request(32)
        .height_request(32)
        .margin_top(48)
        .margin_bottom(48)
        .build();
    let empty = adw::StatusPage::builder()
        .icon_name("view-list-symbolic")
        .title(gettext("No Playlists"))
        .description(gettext("There is no playlist for music that can be added to."))
        .css_classes(["compact"])
        .build();
    let stack = gtk::Stack::new();
    stack.add_child(&spinner);
    stack.add_child(&scrolled);
    stack.add_child(&empty);

    let page = adw::ToolbarView::new();
    page.add_top_bar(&adw::HeaderBar::new());
    page.set_content(Some(&stack));
    let dialog = adw::Dialog::builder()
        .title(gettext("Add to Playlist"))
        .content_width(380)
        // It takes its size when it opens, before the playlists are known.
        .content_height(440)
        .child(&page)
        .build();

    glib::spawn_future_local(glib::clone!(
        #[weak]
        dialog,
        async move {
            let playlists = {
                let server = server.clone();
                runtime::run(async move { server.playlists().await }).await
            };
            let playlists: Vec<Item> = match playlists {
                Ok(playlists) => playlists,
                Err(err) => {
                    dialog.close();
                    crate::notify::failure(&crate::messages::load_playlists(), &err);
                    return;
                }
            };
            // A list that fills itself takes nothing, and a track does not
            // go on a list of videos.
            let playlists: Vec<Item> = playlists
                .into_iter()
                .filter(|playlist| !playlist.smart)
                .filter(|playlist| playlist.playlist_type.as_deref() == Some("audio"))
                .collect();
            if playlists.is_empty() {
                stack.set_visible_child(&empty);
                return;
            }
            for playlist in playlists {
                let row = adw::ActionRow::builder()
                    .title(glib::markup_escape_text(&playlist.title))
                    .activatable(true)
                    .build();
                row.add_suffix(&gtk::Image::from_icon_name("list-add-symbolic"));
                let server = server.clone();
                row.connect_activated(glib::clone!(
                    #[weak]
                    dialog,
                    move |_| {
                        dialog.close();
                        add(server.clone(), playlist.rating_key, playlist.title.clone(), track);
                    }
                ));
                list.append(&row);
            }
            stack.set_visible_child(&scrolled);
        }
    ));
    dialog.present(Some(parent));
}

fn add(server: crate::backend::Server, playlist: ItemId, title: String, track: ItemId) {
    glib::spawn_future_local(async move {
        let added = runtime::run(async move {
            let track = server.item(track).await?;
            server.add_to_playlist(playlist, &track).await
        })
        .await;
        match added {
            Ok(()) => crate::notify::message(&gettext_f("Added to “{}”", &[&title])),
            Err(err) => {
                crate::notify::failure(&gettext("Could not add to the playlist"), &err)
            }
        }
    });
}
