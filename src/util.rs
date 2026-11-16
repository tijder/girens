//! Small helpers shared by the views and by what is kept on disk.

use std::path::Path;

use girens_model::ItemId;
use gtk::glib;

/// Replaces the file at `path` by `contents` in one step: whoever reads it,
/// also after a crash half way, finds the old file or the new one, never a
/// part of it. With `durable` the data is on the disk before the old file
/// goes, which costs a moment.
pub fn write_atomic(path: &Path, contents: &[u8], durable: bool) -> std::io::Result<()> {
    use std::io::Write;

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut partial = path.as_os_str().to_owned();
    partial.push(".new");
    let partial = std::path::PathBuf::from(partial);
    let written = (|| {
        let mut file = std::fs::File::create(&partial)?;
        file.write_all(contents)?;
        if durable {
            file.sync_all()?;
        }
        std::fs::rename(&partial, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    written
}

/// An id as part of a file name. Servers choose their ids, so anything but
/// letters, digits, `-` and `_` is written as `%` and two hex digits: a name
/// cannot leave its folder, and the `.` stays free for what follows the id.
pub fn id_file_name(id: ItemId) -> String {
    let mut name = String::new();
    for byte in id.as_str().bytes() {
        if byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' {
            name.push(byte as char);
        } else {
            name.push_str(&format!("%{byte:02X}"));
        }
    }
    name
}

/// The id [`id_file_name`] wrote as `name`; `None` for anything else.
pub fn id_from_file_name(name: &str) -> Option<ItemId> {
    let mut bytes = Vec::with_capacity(name.len());
    let mut rest = name.bytes();
    while let Some(byte) = rest.next() {
        if byte == b'%' {
            let hex = [rest.next()?, rest.next()?];
            let hex = std::str::from_utf8(&hex).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
        } else if byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' {
            bytes.push(byte);
        } else {
            return None;
        }
    }
    if bytes.is_empty() {
        return None;
    }
    Some(ItemId::new(std::str::from_utf8(&bytes).ok()?))
}

/// `3:07` or `1:02:03` for a duration in milliseconds.
pub fn format_duration(milliseconds: u64) -> String {
    let seconds = milliseconds / 1000;
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// Joins the details that are known into one line.
pub fn detail_line(details: impl IntoIterator<Item = Option<String>>) -> String {
    let details: Vec<String> = details.into_iter().flatten().collect();
    details.join(" · ")
}

/// `02 Jan 2020` (in the user's language) for a `2020-01-02` date.
pub fn format_date(iso_date: &str) -> Option<String> {
    let mut parts = iso_date.splitn(3, '-').map(str::parse::<i32>);
    let (year, month, day) = (
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    );
    let date = glib::DateTime::from_local(year, month, day, 0, 0, 0.0).ok()?;
    date.format("%d %b %Y").ok().map(Into::into)
}

/// The rating key at the end of `/library/metadata/<key>`.
pub fn rating_key_from_path(path: &str) -> Option<ItemId> {
    let key: u64 = path.trim_end_matches('/').rsplit('/').next()?.parse().ok()?;
    Some(key.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(format_duration(0), "0:00");
        assert_eq!(format_duration(187_000), "3:07");
        assert_eq!(format_duration(3_723_000), "1:02:03");
    }

    #[test]
    fn ids_stay_inside_their_folder_and_come_back_as_they_were() {
        // What servers hand out today reads as it is.
        assert_eq!(id_file_name(123.into()), "123");
        let uuid = "bff9d7c2-d8bb-458c-a04f-033e6ce2cf62";
        assert_eq!(id_file_name(uuid.into()), uuid);

        let odd = "../../etc/passwd.tmp";
        let name = id_file_name(odd.into());
        assert_eq!(name, "%2E%2E%2F%2E%2E%2Fetc%2Fpasswd%2Etmp");
        assert_eq!(id_from_file_name(&name), Some(odd.into()));
        assert_eq!(id_from_file_name(&id_file_name("é%".into())), Some("é%".into()));

        // Not an id: the description or the partial copy beside a copy.
        assert_eq!(id_from_file_name("123.json"), None);
        assert_eq!(id_from_file_name("123.tmp"), None);
        assert_eq!(id_from_file_name(""), None);
        assert_eq!(id_from_file_name("%2"), None);
    }

    #[test]
    fn a_file_is_replaced_in_one_step() {
        let path = std::env::temp_dir().join(format!("girens-util-test-{}", std::process::id()));
        write_atomic(&path, b"first", true).unwrap();
        write_atomic(&path, b"second", false).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        let mut partial = path.clone().into_os_string();
        partial.push(".new");
        assert!(!Path::new(&partial).exists());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn rating_keys() {
        assert_eq!(rating_key_from_path("/library/metadata/123"), Some(123.into()));
        assert_eq!(rating_key_from_path("/library/metadata/123/"), Some(123.into()));
        assert_eq!(rating_key_from_path("/library/sections"), None);
    }
}
