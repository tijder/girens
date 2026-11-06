//! Small formatting helpers shared by the views.

use girens_model::ItemId;
use gtk::glib;

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
    fn rating_keys() {
        assert_eq!(rating_key_from_path("/library/metadata/123"), Some(123.into()));
        assert_eq!(rating_key_from_path("/library/metadata/123/"), Some(123.into()));
        assert_eq!(rating_key_from_path("/library/sections"), None);
    }
}
