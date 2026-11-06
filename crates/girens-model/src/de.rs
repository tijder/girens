//! Plex is loose about JSON types.

use serde::de::Deserializer;
use serde::Deserialize;

/// Booleans are sent as `true`, `1` or `"1"`.
pub fn flag<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Flag {
        Bool(bool),
        Number(i64),
        String(String),
    }
    Ok(match Option::<Flag>::deserialize(deserializer)? {
        None => false,
        Some(Flag::Bool(value)) => value,
        Some(Flag::Number(value)) => value != 0,
        Some(Flag::String(value)) => value == "1" || value == "true",
    })
}
