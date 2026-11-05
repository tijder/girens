//! MPRIS: lets the desktop show what is playing and control it (media keys,
//! the shell's media controls, `playerctl`). Replaces `legacy/mpris.py`.

use std::rc::Rc;

use girens_plex::model::{Item, ItemKind};
use gtk::prelude::*;
use gtk::{gio, glib};
use mpris_server::{Metadata, PlaybackStatus, Time, TrackId};

use crate::player::{Event, Player};
use crate::session::Session;
use crate::{config, images};

/// Bus name suffix; the Flatpak may own `org.mpris.MediaPlayer2.Girens`.
const BUS_NAME: &str = "Girens";

/// Registers the MPRIS service. `raise` is called when a controller asks to
/// bring the window to the front.
pub fn start(player: &Player, raise: impl Fn() + 'static) {
    let player = player.clone();
    glib::spawn_future_local(async move {
        let service = match mpris_server::Player::builder(BUS_NAME)
            .identity("Girens")
            .desktop_entry(config::APP_ID)
            .can_raise(true)
            .can_play(true)
            .can_pause(true)
            .can_seek(true)
            .can_control(true)
            .build()
            .await
        {
            Ok(service) => Rc::new(service),
            Err(err) => {
                eprintln!("MPRIS is unavailable: {err}");
                return;
            }
        };

        service.connect_raise(move |_| raise());
        let controlled = player.clone();
        service.connect_play_pause(move |_| controlled.play_pause());
        let controlled = player.clone();
        service.connect_play(move |_| controlled.resume());
        let controlled = player.clone();
        service.connect_pause(move |_| controlled.pause());
        let controlled = player.clone();
        service.connect_stop(move |_| controlled.stop());
        let controlled = player.clone();
        service.connect_next(move |_| controlled.next());
        let controlled = player.clone();
        service.connect_previous(move |_| controlled.previous());
        let controlled = player.clone();
        service.connect_seek(move |_, offset| controlled.seek_by(offset.as_secs() as i32));
        let controlled = player.clone();
        service.connect_set_position(move |_, _, position| {
            controlled.seek_to(position.as_millis().max(0) as u64)
        });
        let controlled = player.clone();
        service.connect_set_shuffle(move |_, shuffle| controlled.set_shuffle(shuffle));
        service.connect_set_volume(|_, volume| {
            let percent = (volume * 100.0).round().clamp(0.0, 100.0) as i32;
            if let Err(err) = gio::Settings::new(config::APP_ID).set_int("volume-level", percent) {
                eprintln!("Could not save the volume: {err}");
            }
        });

        let reporting = service.clone();
        let observed = player.clone();
        player.connect(move |event| {
            let (service, event, player) = (reporting.clone(), event.clone(), observed.clone());
            glib::spawn_future_local(async move {
                if let Err(err) = report(&service, &event, &player).await {
                    eprintln!("Could not update MPRIS: {err}");
                }
            });
        });

        service.run().await;
    });
}

async fn report(
    service: &mpris_server::Player,
    event: &Event,
    player: &Player,
) -> mpris_server::zbus::Result<()> {
    match event {
        Event::Started { item, .. } => {
            service.set_metadata(metadata(item)).await?;
            service.set_playback_status(PlaybackStatus::Playing).await?;
            service.set_can_go_next(player.has_next()).await?;
            service.set_can_go_previous(player.has_previous()).await?;
            service.set_shuffle(player.is_shuffled()).await?;
        }
        Event::Paused(paused) => {
            service
                .set_playback_status(if *paused {
                    PlaybackStatus::Paused
                } else {
                    PlaybackStatus::Playing
                })
                .await?;
        }
        Event::Position(position) => service.set_position(Time::from_millis(*position as i64)),
        Event::QueueChanged => {
            service.set_can_go_next(player.has_next()).await?;
            service.set_can_go_previous(player.has_previous()).await?;
            service.set_shuffle(player.is_shuffled()).await?;
        }
        Event::Stopped => {
            service.set_playback_status(PlaybackStatus::Stopped).await?;
            service.set_metadata(Metadata::new()).await?;
        }
        Event::AskResume(_) => {}
    }
    Ok(())
}

fn metadata(item: &Item) -> Metadata {
    let mut builder = Metadata::builder().title(item.title.clone());
    if let Ok(id) = TrackId::try_from(format!("/nl/g4d/Girens/track/{}", item.rating_key)) {
        builder = builder.trackid(id);
    }
    if let Some(duration) = item.duration {
        builder = builder.length(Time::from_millis(duration as i64));
    }
    match item.kind {
        ItemKind::Track => {
            if let Some(artist) = item.grandparent_title.clone() {
                builder = builder.artist([artist]);
            }
            if let Some(album) = item.parent_title.clone() {
                builder = builder.album(album);
            }
        }
        ItemKind::Episode => {
            if let Some(show) = item.grandparent_title.clone() {
                builder = builder.artist([show]);
            }
            if let Some(season) = item.parent_title.clone() {
                builder = builder.album(season);
            }
        }
        _ => {}
    }
    // Covers are cached under the key of the item they belong to (the show
    // for an episode); see `views::cover_box`.
    if let Some(server) = Session::current().server() {
        let key = match item.kind {
            ItemKind::Episode => item.grandparent_rating_key.unwrap_or(item.rating_key),
            _ => item.rating_key,
        };
        if let Some(path) = images::cached_cover(&server, key) {
            builder = builder.art_url(gio::File::for_path(path).uri().to_string());
        }
    }
    builder.build()
}
