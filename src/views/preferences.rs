//! The preferences dialog. Everything in it is stored in GSettings, which
//! the player reads.

use adw::prelude::*;
use gtk::{gio, glib};

use crate::config;
use crate::i18n::gettext;

/// Stored value of `transcode-media-to-resolution` for each choice.
const RESOLUTIONS: [(&str, &str); 4] = [
    ("1080p", "1920x1080"),
    ("720p", "1280x720"),
    ("480p", "854x480"),
    ("240p", "427x240"),
];

pub fn present(parent: &impl IsA<gtk::Widget>) {
    let settings = gio::Settings::new(config::APP_ID);

    let music_clips = adw::SwitchRow::builder()
        .title(gettext("Prefer music clips"))
        .subtitle(gettext(
            "When playing music and the track has a music clip, play that instead of the audio.",
        ))
        .build();
    settings
        .bind("prefer-music-clips", &music_clips, "active")
        .build();

    let transcode = adw::SwitchRow::builder()
        .title(gettext("Transcode media"))
        .subtitle(gettext("Transcode media instead of playing directly."))
        .build();
    settings
        .bind("play-media-direct", &transcode, "active")
        .invert_boolean()
        .build();

    let labels: Vec<&str> = RESOLUTIONS.iter().map(|(label, _)| *label).collect();
    let resolution = adw::ComboRow::builder()
        .title(gettext("Resolution"))
        .model(&gtk::StringList::new(&labels))
        .build();
    let stored = settings.string("transcode-media-to-resolution");
    let selected = RESOLUTIONS
        .iter()
        .position(|(_, value)| *value == stored)
        .unwrap_or(0);
    resolution.set_selected(selected as u32);
    resolution.connect_selected_notify(glib::clone!(
        #[strong]
        settings,
        move |row| {
            let Some((_, value)) = RESOLUTIONS.get(row.selected() as usize) else {
                return;
            };
            if let Err(err) = settings.set_string("transcode-media-to-resolution", value) {
                eprintln!("Could not save the transcode resolution: {err}");
            }
        }
    ));
    // The resolution only matters while transcoding.
    transcode
        .bind_property("active", &resolution, "sensitive")
        .sync_create()
        .build();

    let playback = adw::PreferencesGroup::builder()
        .title(gettext("Playback"))
        .build();
    playback.add(&music_clips);
    playback.add(&transcode);
    playback.add(&resolution);

    let page = adw::PreferencesPage::new();
    page.add(&playback);
    let dialog = adw::PreferencesDialog::builder()
        .title(gettext("Preferences"))
        .build();
    dialog.add(&page);
    dialog.present(Some(parent));
}
