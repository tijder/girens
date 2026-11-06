//! Turning the answers of the server into the items Girens shows.

use std::collections::HashMap;

use girens_model::model::{Item, ItemKind, LibrarySection, Media, Part, Stream, Tag};
use girens_model::ItemId;

use crate::wire;

/// An id as Girens keeps it: the server only has UUIDs, which do not tell
/// what they are of, so the kind goes in front.
pub fn id(kind: ItemKind, uuid: &str) -> ItemId {
    ItemId::new(&format!("{}-{uuid}", kind.as_str()))
}

/// The kind and the UUID in an id made by [`id`].
pub fn split(id: ItemId) -> Option<(ItemKind, &'static str)> {
    let (kind, uuid) = id.as_str().split_once('-')?;
    let kind = [
        ItemKind::Movie,
        ItemKind::Show,
        ItemKind::Season,
        ItemKind::Episode,
        ItemKind::Artist,
        ItemKind::Album,
        ItemKind::Track,
        ItemKind::Playlist,
    ]
    .into_iter()
    .find(|candidate| candidate.as_str() == kind)?;
    Some((kind, uuid))
}

/// The three-letter code of a language given with two or three letters, so
/// that `nl`, `nld` and `dut` compare as the same.
fn language_code(language: &str) -> String {
    let language = language.trim().to_ascii_lowercase();
    let language = language.split(['-', '_']).next().unwrap_or_default();
    let code = match language {
        "en" => "eng",
        "nl" | "dut" => "nld",
        "de" | "ger" => "deu",
        "fr" | "fre" => "fra",
        "es" => "spa",
        "it" => "ita",
        "pt" => "por",
        "sv" => "swe",
        "da" => "dan",
        "no" | "nb" | "nob" => "nor",
        "fi" => "fin",
        "pl" => "pol",
        "ru" => "rus",
        "tr" => "tur",
        "ja" => "jpn",
        "ko" => "kor",
        "zh" | "chi" => "zho",
        "cs" | "cze" => "ces",
        "el" | "gre" => "ell",
        "hu" => "hun",
        "ro" | "rum" => "ron",
        "uk" => "ukr",
        "ar" => "ara",
        "he" => "heb",
        "hi" => "hin",
        other => other,
    };
    code.to_owned()
}

fn same_language(a: &str, b: &str) -> bool {
    !a.is_empty() && language_code(a) == language_code(b)
}

/// The streams the user chose for a file while this program runs. The
/// server does not keep them; mpv switches by itself.
#[derive(Debug, Clone, Copy, Default)]
pub struct Selection {
    pub audio: Option<ItemId>,
    /// `Some(None)` is the choice for no subtitles.
    pub subtitle: Option<Option<ItemId>>,
}

/// What mapping depends on besides the answer itself.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// Languages for titles and descriptions, most wanted first.
    pub languages: Vec<String>,
    pub settings: wire::UserSettings,
    pub selections: HashMap<ItemId, Selection>,
}

fn image_url(image: &wire::Image) -> String {
    format!(
        "{}/images/{}/download",
        image.directory.serving_node.url.trim_end_matches('/'),
        image.id
    )
}

/// The image of one type to show: the one an admin chose, else the first.
fn image(images: &[wire::Image], kind: &str) -> Option<String> {
    let mut of_kind = images
        .iter()
        .filter(|image| image.kind.eq_ignore_ascii_case(kind));
    let first = of_kind.next()?;
    let chosen = std::iter::once(first)
        .chain(of_kind)
        .find(|image| image.selected)
        .unwrap_or(first);
    Some(image_url(chosen))
}

fn cover(images: &[wire::Image]) -> Option<String> {
    image(images, "COVER")
}

fn background(images: &[wire::Image]) -> Option<String> {
    image(images, "BACKGROUND")
}

impl Context {
    /// The description in the language the user reads best.
    fn metadata<'a>(&self, all: &'a [wire::Metadata]) -> Option<&'a wire::Metadata> {
        let in_language = |wanted: &str| {
            all.iter().find(|metadata| {
                metadata
                    .language
                    .as_deref()
                    .is_some_and(|language| same_language(language, wanted))
            })
        };
        self.languages
            .iter()
            .find_map(|language| in_language(language))
            .or_else(|| in_language("eng"))
            // What was read from the file itself has no language.
            .or_else(|| all.iter().rev().find(|metadata| metadata.title.is_some()))
            .or(all.first())
    }

    fn describe(&self, item: &mut Item, all: &[wire::Metadata]) {
        let Some(metadata) = self.metadata(all) else {
            return;
        };
        if let Some(title) = metadata.title.as_deref().filter(|title| !title.is_empty()) {
            item.title = title.to_owned();
        }
        item.summary = metadata.description.clone().filter(|text| !text.is_empty());
        item.originally_available_at = metadata.released.clone();
        item.genres = metadata
            .genre
            .as_deref()
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|genre| !genre.is_empty())
            .map(|genre| Tag {
                tag: genre.to_owned(),
            })
            .collect();
    }

    fn streams(&self, file: &wire::MediaFile, base: &str) -> Vec<Stream> {
        let file_id = ItemId::new(&file.id);
        let all: Vec<&wire::MediaFileStream> = file.media_file_streams.iter().flatten().collect();
        let mut streams: Vec<Stream> = Vec::new();
        for stream in &all {
            let (stream_type, key) = match stream.codec_type.as_str() {
                "VIDEO" => (1, None),
                "AUDIO" => (2, None),
                "SUBTITLE" | "EXTERNAL_SUBTITLE" => {
                    // Pictures instead of text cannot be had as a file.
                    let pictures = matches!(
                        stream.codec_name.as_str(),
                        "hdmv_pgs_subtitle" | "pgssub" | "dvd_subtitle" | "dvdsub" | "dvb_subtitle" | "xsub"
                    );
                    // A file beside the video replaces the same subtitles
                    // inside it.
                    let replaced = stream.codec_type == "SUBTITLE"
                        && all.iter().any(|other| {
                            other.codec_type == "EXTERNAL_SUBTITLE"
                                && other.stream_index == stream.stream_index
                        });
                    if pictures || replaced {
                        continue;
                    }
                    (3, Some(format!("{base}/sub_{}.srt", stream.id)))
                }
                _ => continue,
            };
            let codec = stream.codec_name.to_ascii_uppercase();
            // "eng · Surround 5.1": the language and what the file calls it.
            let named: Vec<&str> = [stream.language.as_deref(), stream.title.as_deref()]
                .into_iter()
                .flatten()
                .filter(|part| !part.is_empty() && *part != "und")
                .collect();
            let name = if named.is_empty() {
                "Unknown".to_owned()
            } else {
                named.join(" · ")
            };
            let display = match stream_type {
                3 => name,
                _ => format!("{name} ({codec})"),
            };
            streams.push(Stream {
                id: ItemId::new(&stream.id),
                stream_type,
                index: stream.stream_index,
                key,
                codec: Some(stream.codec_name.clone()),
                title: stream.title.clone(),
                display_title: Some(display.clone()),
                extended_display_title: Some(display),
                language: stream.language.clone(),
                language_code: stream.language.as_deref().map(language_code),
                selected: false,
            });
        }

        let selection = self.selections.get(&file_id).copied().unwrap_or_default();
        let preferred = |streams: &[Stream], stream_type: u32, languages: &[String]| {
            languages.iter().find_map(|wanted| {
                streams
                    .iter()
                    .find(|stream| {
                        stream.stream_type == stream_type
                            && stream
                                .language
                                .as_deref()
                                .is_some_and(|language| same_language(language, wanted))
                    })
                    .map(|stream| stream.id)
            })
        };
        let first_audio = streams
            .iter()
            .find(|stream| stream.stream_type == 2)
            .map(|stream| stream.id);
        let audio = selection
            .audio
            .filter(|id| streams.iter().any(|stream| stream.id == *id))
            .or_else(|| preferred(&streams, 2, &self.settings.preferred_audio_languages))
            .or(first_audio);
        let subtitle = match selection.subtitle {
            Some(choice) => choice,
            None => preferred(&streams, 3, &self.settings.preferred_subtitle_languages).filter(
                |subtitle| {
                    // Not what is already being spoken, when the user asked
                    // for that.
                    let language = |id: ItemId| {
                        streams
                            .iter()
                            .find(|stream| stream.id == id)
                            .and_then(|stream| stream.language.clone())
                            .unwrap_or_default()
                    };
                    !(self.settings.hide_subtitles_matching_audio
                        && audio.is_some_and(|audio| {
                            same_language(&language(audio), &language(*subtitle))
                        }))
                },
            ),
        };
        for stream in &mut streams {
            stream.selected = Some(stream.id) == audio || Some(stream.id) == subtitle;
        }
        streams
    }

    fn media(&self, files: &[wire::MediaFile]) -> Vec<Media> {
        files
            .iter()
            .map(|file| {
                let base = format!(
                    "{}/hls/{}",
                    file.directory.serving_node.url.trim_end_matches('/'),
                    file.id
                );
                let container = file.format.as_deref().map(str::to_ascii_lowercase);
                let height = file
                    .media_file_streams
                    .iter()
                    .flatten()
                    .find(|stream| stream.codec_type == "VIDEO")
                    .map(|stream| stream.height)
                    .filter(|height| *height > 0);
                Media {
                    id: ItemId::new(&file.id),
                    container: container.clone(),
                    video_resolution: height.map(|height| height.to_string()),
                    parts: vec![Part {
                        id: ItemId::new(&file.id),
                        streams: self.streams(file, &base),
                        key: base,
                        container,
                        size: file.size.map(|size| size as u64),
                    }],
                }
            })
            .collect()
    }

    pub fn movie(&self, movie: &wire::Movie) -> Item {
        let rating_key = id(ItemKind::Movie, &movie.id);
        let mut item = Item {
            rating_key,
            key: rating_key.to_string(),
            kind: ItemKind::Movie,
            title: movie.name.clone(),
            year: movie.release_year.filter(|year| *year > 0),
            user_rating: movie.rating.map(|rating| rating as f32),
            thumb: cover(&movie.images),
            art: background(&movie.images),
            media: self.media(&movie.media_file),
            ..Item::default()
        };
        item.duration = movie
            .media_file
            .first()
            .and_then(|file| file.duration_in_milliseconds);
        watched(&mut item, &movie.watch_status);
        self.describe(&mut item, &movie.metadata);
        item
    }

    pub fn show(&self, show: &wire::Show) -> Item {
        let rating_key = id(ItemKind::Show, &show.id);
        let mut item = Item {
            rating_key,
            key: rating_key.to_string(),
            kind: ItemKind::Show,
            title: show.name.clone(),
            year: show.release_year.filter(|year| *year > 0),
            user_rating: show.rating.map(|rating| rating as f32),
            thumb: cover(&show.images),
            art: background(&show.images),
            ..Item::default()
        };
        if let Some(seasons) = &show.seasons {
            let episodes = || seasons.iter().flat_map(|season| season.episodes.iter());
            item.child_count = Some(seasons.len() as u32);
            item.leaf_count = Some(episodes().count() as u32);
            item.viewed_leaf_count = Some(
                episodes()
                    .filter(|episode| episode.watch_status.iter().any(|status| status.watched))
                    .count() as u32,
            );
        }
        self.describe(&mut item, &show.metadata);
        item
    }

    /// The seasons of a show, which the server only describes by number.
    pub fn seasons(&self, show: &wire::Show) -> Vec<Item> {
        let parent = self.show(show);
        let mut seasons: Vec<&wire::SeasonBrief> =
            show.seasons.iter().flatten().collect();
        seasons.sort_by_key(|season| season.number);
        seasons
            .into_iter()
            .map(|season| {
                let watched = season
                    .episodes
                    .iter()
                    .filter(|episode| episode.watch_status.iter().any(|status| status.watched))
                    .count();
                season_item(&parent, &season.id, season.number, season.episodes.len(), watched)
            })
            .collect()
    }

    pub fn season(&self, season: &wire::Season) -> Item {
        let parent = self.show(&season.show);
        let watched = season
            .episodes
            .iter()
            .filter(|episode| episode.watch_status.iter().any(|status| status.watched))
            .count();
        season_item(&parent, &season.id, season.number, season.episodes.len(), watched)
    }

    pub fn episode(&self, episode: &wire::Episode) -> Item {
        let rating_key = id(ItemKind::Episode, &episode.id);
        let mut item = Item {
            rating_key,
            key: rating_key.to_string(),
            kind: ItemKind::Episode,
            title: format!("Episode {}", episode.number),
            index: Some(episode.number),
            parent_index: episode.season.as_ref().map(|season| season.number),
            parent_rating_key: episode
                .season
                .as_ref()
                .map(|season| id(ItemKind::Season, &season.id)),
            parent_title: episode
                .season
                .as_ref()
                .map(|season| format!("Season {}", season.number)),
            grandparent_rating_key: episode
                .show
                .as_ref()
                .map(|show| id(ItemKind::Show, &show.id)),
            grandparent_title: episode.show.as_ref().map(|show| show.name.clone()),
            grandparent_thumb: episode.show.as_ref().and_then(|show| cover(&show.images)),
            user_rating: episode.rating.map(|rating| rating as f32),
            // A still of the episode, when there is one.
            thumb: background(&episode.images).or_else(|| cover(&episode.images)),
            art: episode
                .show
                .as_ref()
                .and_then(|show| background(&show.images)),
            media: self.media(&episode.media_file),
            ..Item::default()
        };
        // An episode that shares its file with others is only its slice.
        let slice = episode
            .media_file_parts
            .first()
            .map(|part| part.duration_in_milliseconds as u64)
            .filter(|duration| *duration > 0);
        item.duration = slice.or_else(|| {
            episode
                .media_file
                .first()
                .and_then(|file| file.duration_in_milliseconds)
        });
        watched(&mut item, &episode.watch_status);
        self.describe(&mut item, &episode.metadata);
        item
    }

    pub fn artist(&self, person: &wire::Person) -> Item {
        let rating_key = id(ItemKind::Artist, &person.id);
        let mut item = Item {
            rating_key,
            key: rating_key.to_string(),
            kind: ItemKind::Artist,
            title: person.name.clone(),
            thumb: cover(&person.images),
            art: background(&person.images),
            child_count: person.albums.as_ref().map(|albums| albums.len() as u32),
            ..Item::default()
        };
        self.describe(&mut item, &person.metadata);
        // A person is called by name, whatever a description is titled.
        item.title.clone_from(&person.name);
        item
    }

    pub fn album(&self, album: &wire::Album) -> Item {
        let rating_key = id(ItemKind::Album, &album.id);
        Item {
            rating_key,
            key: rating_key.to_string(),
            kind: ItemKind::Album,
            title: album.name.clone(),
            year: album.release_year.filter(|year| *year > 0),
            user_rating: album.rating.map(|rating| rating as f32),
            thumb: cover(&album.images),
            parent_title: Some(album.artist.name.clone()),
            parent_rating_key: Some(id(ItemKind::Artist, &album.artist.id)),
            leaf_count: album.tracks.as_ref().map(|tracks| tracks.len() as u32),
            ..Item::default()
        }
    }

    pub fn track(&self, track: &wire::Track) -> Item {
        let rating_key = id(ItemKind::Track, &track.id);
        let cover = cover(&track.album.images);
        let mut item = Item {
            rating_key,
            key: rating_key.to_string(),
            kind: ItemKind::Track,
            title: format!("Track {}", track.number),
            index: Some(track.number),
            parent_index: Some(track.disc_number.max(1)),
            parent_title: Some(track.album.name.clone()),
            parent_rating_key: Some(id(ItemKind::Album, &track.album.id)),
            parent_thumb: cover.clone(),
            thumb: cover,
            grandparent_title: Some(track.artist.name.clone()),
            grandparent_rating_key: Some(id(ItemKind::Artist, &track.artist.id)),
            user_rating: track.rating.map(|rating| rating as f32),
            view_count: track.play_count.unwrap_or(0),
            // Only the first of several versions is played.
            media: self.media(&track.media_file[..track.media_file.len().min(1)]),
            ..Item::default()
        };
        item.duration = track
            .media_file
            .first()
            .and_then(|file| file.duration_in_milliseconds);
        self.describe(&mut item, &track.metadata);
        item
    }

    /// The movie, episode or track of an entry.
    pub fn entry(&self, entry: &wire::Entry) -> Option<Item> {
        match (&entry.movie, &entry.episode, &entry.track) {
            (Some(movie), _, _) => Some(self.movie(movie)),
            (_, Some(episode), _) => Some(self.episode(episode)),
            (_, _, Some(track)) => Some(self.track(track)),
            _ => None,
        }
    }

    pub fn playlist(&self, playlist: &wire::Playlist) -> Item {
        let rating_key = id(ItemKind::Playlist, &playlist.id);
        let audio = playlist.library_type == "MUSIC";
        let cover = playlist.cover_images.first().map(image_url);
        Item {
            rating_key,
            key: rating_key.to_string(),
            kind: ItemKind::Playlist,
            title: playlist.name.clone(),
            leaf_count: playlist.item_count,
            playlist_type: Some(if audio { "audio" } else { "video" }.to_owned()),
            smart: playlist.kind == "SMART",
            composite: cover.clone(),
            thumb: cover,
            ..Item::default()
        }
    }

    pub fn search_result(&self, result: &wire::SearchResult) -> Option<Item> {
        Some(match result {
            wire::SearchResult::Movie(movie) => self.movie(movie),
            wire::SearchResult::Show(show) => self.show(show),
            wire::SearchResult::Episode(episode) => self.episode(episode),
            wire::SearchResult::Person(person) => self.artist(person),
            wire::SearchResult::Album(album) => self.album(album),
            wire::SearchResult::Track(track) => self.track(track),
            wire::SearchResult::Other => return None,
        })
    }
}

fn season_item(show: &Item, uuid: &str, number: u32, episodes: usize, watched: usize) -> Item {
    let rating_key = id(ItemKind::Season, uuid);
    Item {
        rating_key,
        key: rating_key.to_string(),
        kind: ItemKind::Season,
        title: format!("Season {number}"),
        index: Some(number),
        parent_rating_key: Some(show.rating_key),
        parent_title: Some(show.title.clone()),
        parent_thumb: show.thumb.clone(),
        thumb: show.thumb.clone(),
        art: show.art.clone(),
        leaf_count: Some(episodes as u32),
        viewed_leaf_count: Some(watched as u32),
        ..Item::default()
    }
}

/// How often the item was played and where the last play stopped.
fn watched(item: &mut Item, statuses: &[wire::WatchStatus]) {
    item.view_count = statuses.iter().filter(|status| status.watched).count() as u32;
    let latest = statuses
        .iter()
        .max_by(|a, b| a.updated_at.cmp(&b.updated_at));
    if let Some(latest) = latest.filter(|latest| !latest.watched) {
        item.view_offset = latest.progress_in_milliseconds;
    }
}

/// The libraries Girens can show; books, podcasts, comics and games are
/// left out.
pub fn section(library: &wire::Library) -> Option<LibrarySection> {
    let kind = match library.kind.as_str() {
        "MOVIE" => ItemKind::Movie,
        "SHOW" => ItemKind::Show,
        "MUSIC" => ItemKind::Artist,
        _ => return None,
    };
    Some(LibrarySection {
        key: id(kind, &library.id),
        uuid: library.id.clone(),
        title: library.name.clone(),
        kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_tells_the_kind() {
        let uuid = "6f1c2a0e-0000-4000-8000-000000000001";
        let episode = id(ItemKind::Episode, uuid);
        assert_eq!(episode.as_str(), format!("episode-{uuid}"));
        assert_eq!(split(episode), Some((ItemKind::Episode, uuid)));
        assert_eq!(split(ItemId::new(uuid)), None);
        assert_eq!(split(ItemId::new("42")), None);
    }

    #[test]
    fn languages_compare_across_their_codes() {
        assert!(same_language("nl", "nld"));
        assert!(same_language("dut", "nl-NL"));
        assert!(same_language("ENG", "en"));
        assert!(!same_language("en", "nl"));
        assert!(!same_language("", ""));
    }
}
