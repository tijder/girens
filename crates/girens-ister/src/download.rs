//! Keeping a copy of an item for watching without the server.
//!
//! The server only streams: an item comes as a list of short files. A copy
//! is that list and those files in a folder, which a player opens like the
//! stream itself.

use std::path::Path;
use std::time::Duration;

use girens_model::model::Item;
use girens_model::{Error, Result};
use url::Url;

use crate::IsterServer;

/// The list a copy starts at, inside its folder.
pub const MASTER: &str = "master.m3u8";
/// A part is made by the server when it is first asked for, which can take
/// a while.
const PART_TIMEOUT: Duration = Duration::from_secs(180);

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> Error + '_ {
    move |source| Error::Io {
        path: path.display().to_string(),
        source,
    }
}

/// The name of the file an address ends in, without what follows the `?`.
fn file_name(address: &str) -> &str {
    let path = address.split(['?', '#']).next().unwrap_or(address);
    path.rsplit('/').next().unwrap_or(path)
}

/// The value of `name="…"` in a line of a playlist.
fn attribute<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let start = line.find(&format!("{name}=\""))? + name.len() + 2;
    let length = line[start..].find('"')?;
    Some(&line[start..start + length])
}

/// A master playlist cut down to one video variant and its audio, with the
/// lists it names: `(the local text, [(local name, address)])`.
fn localise_master(text: &str, base: &Url) -> Result<(String, Vec<(String, Url)>)> {
    let lines: Vec<&str> = text.lines().collect();
    // The first variant is the best one the server offers.
    let variant = lines
        .iter()
        .position(|line| line.starts_with("#EXT-X-STREAM-INF"))
        .ok_or_else(|| Error::Server("the stream lists nothing to play".into()))?;
    let audio_group = attribute(lines[variant], "AUDIO");
    let mut local = String::new();
    let mut lists = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        if line.starts_with("#EXT-X-STREAM-INF") {
            // Only the chosen variant, with the line that names its list.
            if index == variant {
                if let Some(address) = lines.get(index + 1) {
                    let name = file_name(address).to_owned();
                    local.push_str(line);
                    local.push('\n');
                    local.push_str(&name);
                    local.push('\n');
                    lists.push((name, base.join(address)?));
                }
            }
            index += 2;
            continue;
        }
        if line.starts_with("#EXT-X-MEDIA") {
            let wanted = line.contains("TYPE=AUDIO")
                && (audio_group.is_none() || attribute(line, "GROUP-ID") == audio_group);
            if let (true, Some(address)) = (wanted, attribute(line, "URI")) {
                let name = file_name(address).to_owned();
                local.push_str(&line.replace(address, &name));
                local.push('\n');
                lists.push((name, base.join(address)?));
            }
        } else {
            local.push_str(line);
            local.push('\n');
        }
        index += 1;
    }
    Ok((local, lists))
}

/// A list of parts with the parts it names: `(the local text, [(local
/// name, address)])`.
fn localise_list(text: &str, base: &Url) -> Result<(String, Vec<(String, Url)>)> {
    let mut local = String::new();
    let mut parts = Vec::new();
    for line in text.lines() {
        if line.is_empty() || line.starts_with('#') {
            local.push_str(line);
        } else {
            let name = file_name(line).to_owned();
            local.push_str(&name);
            parts.push((name, base.join(line)?));
        }
        local.push('\n');
    }
    Ok((local, parts))
}

impl IsterServer {
    async fn fetch(&self, url: &Url) -> Result<Vec<u8>> {
        let response = self
            .http()
            .get(url.clone())
            .timeout(PART_TIMEOUT)
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(Error::Status {
                status,
                // The address carries a token.
                url: file_name(url.as_str()).to_owned(),
            });
        }
        Ok(response.bytes().await?.to_vec())
    }

    async fn fetch_text(&self, url: &Url) -> Result<String> {
        Ok(String::from_utf8_lossy(&self.fetch(url).await?).into_owned())
    }

    /// Copies `item` into the folder `dir`: the file as it is, or converted
    /// with `converted` (and always when it cannot be streamed as it is).
    /// `progress` hears how many of the parts are there. What an earlier
    /// attempt left in `dir` is kept, so that a copy can be continued.
    pub async fn download_stream(
        &self,
        item: &Item,
        converted: bool,
        dir: &Path,
        progress: impl Fn(u64, u64) + Send,
    ) -> Result<()> {
        // A list does not tell what the file holds.
        let item = self.item(item.rating_key).await?;
        let master = Url::parse(&self.stream_address(&item, !converted)?)?;
        tokio::fs::create_dir_all(dir).await.map_err(io(dir))?;

        let (local_master, lists) = localise_master(&self.fetch_text(&master).await?, &master)?;
        let mut parts = Vec::new();
        for (name, address) in &lists {
            let (local, of_list) = localise_list(&self.fetch_text(address).await?, address)?;
            let path = dir.join(name);
            tokio::fs::write(&path, local).await.map_err(io(&path))?;
            parts.extend(of_list);
        }

        let total = parts.len() as u64;
        for (done, (name, address)) in parts.iter().enumerate() {
            let path = dir.join(name);
            let there = tokio::fs::metadata(&path)
                .await
                .is_ok_and(|file| file.len() > 0);
            if !there {
                let bytes = self.fetch(address).await?;
                // Whole or not at all: a part that broke off is asked for
                // again next time.
                let partial = dir.join(format!("{name}.part"));
                tokio::fs::write(&partial, bytes).await.map_err(io(&partial))?;
                tokio::fs::rename(&partial, &path).await.map_err(io(&path))?;
            }
            progress(done as u64 + 1, total);
        }

        // Subtitles that are text, named so that a player can tell the
        // language; a copy is still good without them.
        for stream in item.streams(girens_model::model::StreamKind::Subtitle) {
            let (false, Some(key)) = (stream.pictures, stream.key.as_deref()) else {
                continue;
            };
            let language = stream.language_code.as_deref().unwrap_or("und");
            let path = dir.join(format!("sub_{language}_{}.srt", stream.id));
            match Url::parse(&self.download_url(key)) {
                Ok(address) => match self.fetch(&address).await {
                    Ok(bytes) => {
                        if let Err(err) = tokio::fs::write(&path, bytes).await {
                            eprintln!("Could not keep subtitles: {err}");
                        }
                    }
                    Err(err) => eprintln!("Could not fetch subtitles: {err}"),
                },
                Err(err) => eprintln!("Could not fetch subtitles: {err}"),
            }
        }

        // Last, so that a folder with this list in it is a whole copy.
        let path = dir.join(MASTER);
        tokio::fs::write(&path, local_master).await.map_err(io(&path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_master_keeps_one_variant_and_its_audio() {
        let base = Url::parse("http://node/hls/file/master.m3u8?token=t").unwrap();
        let text = "#EXTM3U\n#EXT-X-VERSION:6\n\
            #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"audio-192k\",LANGUAGE=\"nld\",NAME=\"nld\",DEFAULT=YES,URI=\"stream_audio_1_192k.m3u8?token=t\"\n\
            #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"audio-copy\",LANGUAGE=\"nld\",NAME=\"nld\",URI=\"stream_audio_1_copy.m3u8?token=t\"\n\
            #EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subs\",URI=\"stream_sub_1.m3u8?token=t\"\n\
            #EXT-X-STREAM-INF:BANDWIDTH=2000000,RESOLUTION=1280x720,AUDIO=\"audio-192k\"\n\
            stream_video_720p.m3u8?token=t\n\
            #EXT-X-STREAM-INF:BANDWIDTH=1000000,RESOLUTION=854x480,AUDIO=\"audio-192k\"\n\
            stream_video_480p.m3u8?token=t\n";
        let (local, lists) = localise_master(text, &base).unwrap();
        let names: Vec<&str> = lists.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["stream_audio_1_192k.m3u8", "stream_video_720p.m3u8"]);
        assert_eq!(
            lists[1].1.as_str(),
            "http://node/hls/file/stream_video_720p.m3u8?token=t"
        );
        // Nothing in the copy points at the server or carries a token.
        assert!(!local.contains("token") && !local.contains("480p") && !local.contains("copy"));
        assert!(local.contains("URI=\"stream_audio_1_192k.m3u8\""));
        assert!(local.contains("\nstream_video_720p.m3u8\n"));
    }

    #[test]
    fn a_list_names_its_parts_without_the_server() {
        let base = Url::parse("http://node/hls/file/stream_video_copy.m3u8?token=t").unwrap();
        let text = "#EXTM3U\n#EXTINF:5.12,\nseg_video_copy_00000.ts?token=t\n#EXTINF:5.12,\nseg_video_copy_00001.ts?token=t\n#EXT-X-ENDLIST\n";
        let (local, parts) = localise_list(text, &base).unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1].0, "seg_video_copy_00001.ts");
        assert_eq!(parts[1].1.as_str(), "http://node/hls/file/seg_video_copy_00001.ts?token=t");
        assert_eq!(
            local,
            "#EXTM3U\n#EXTINF:5.12,\nseg_video_copy_00000.ts\n#EXTINF:5.12,\nseg_video_copy_00001.ts\n#EXT-X-ENDLIST\n"
        );
    }
}
