//! Plex is loose about JSON types: identifiers arrive as strings in one
//! response and as numbers in the next.

use serde::de::{self, Deserializer};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(untagged)]
enum NumberOrString {
    Number(u64),
    String(String),
}

impl NumberOrString {
    fn into_u64<E: de::Error>(self) -> Result<u64, E> {
        match self {
            NumberOrString::Number(number) => Ok(number),
            NumberOrString::String(text) => text
                .parse()
                .map_err(|_| E::custom(format!("expected a numeric identifier, got {text:?}"))),
        }
    }
}

pub fn id<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    NumberOrString::deserialize(deserializer)?.into_u64()
}

pub fn optional_id<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u64>, D::Error> {
    Option::<NumberOrString>::deserialize(deserializer)?
        .map(NumberOrString::into_u64)
        .transpose()
}

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
