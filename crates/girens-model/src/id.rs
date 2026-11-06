//! Identifiers of items, libraries and queues.

use std::collections::HashSet;
use std::fmt;
use std::sync::Mutex;

use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};

/// What a server calls an item: a number for Plex, a UUID for Ister.
///
/// The text is kept once for the life of the process, which makes an id as
/// cheap to copy and compare as the number it used to be. The ids a session
/// sees are few and short, so nothing is ever given back.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ItemId(&'static str);

static KNOWN: Mutex<Option<HashSet<&'static str>>> = Mutex::new(None);

impl ItemId {
    pub fn new(text: &str) -> Self {
        let mut known = KNOWN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let known = known.get_or_insert_with(HashSet::new);
        if let Some(kept) = known.get(text) {
            return Self(kept);
        }
        let kept: &'static str = Box::leak(text.to_owned().into_boxed_str());
        known.insert(kept);
        Self(kept)
    }

    /// An id that is known when compiling.
    pub const fn fixed(text: &'static str) -> Self {
        Self(text)
    }

    pub fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for ItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl fmt::Debug for ItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.0, f)
    }
}

impl std::str::FromStr for ItemId {
    type Err = std::convert::Infallible;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(text))
    }
}

impl From<&str> for ItemId {
    fn from(text: &str) -> Self {
        Self::new(text)
    }
}

impl From<String> for ItemId {
    fn from(text: String) -> Self {
        Self::new(&text)
    }
}

impl From<u64> for ItemId {
    fn from(number: u64) -> Self {
        Self::new(&number.to_string())
    }
}

impl Serialize for ItemId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0)
    }
}

/// Plex sends an id as a number in one response and as text in the next,
/// and files written by older versions hold numbers.
impl<'de> Deserialize<'de> for ItemId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct IdVisitor;

        impl Visitor<'_> for IdVisitor {
            type Value = ItemId;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an identifier")
            }

            fn visit_str<E: de::Error>(self, text: &str) -> Result<ItemId, E> {
                Ok(ItemId::new(text))
            }

            fn visit_u64<E: de::Error>(self, number: u64) -> Result<ItemId, E> {
                Ok(ItemId::from(number))
            }

            fn visit_i64<E: de::Error>(self, number: i64) -> Result<ItemId, E> {
                Ok(ItemId::new(&number.to_string()))
            }
        }

        deserializer.deserialize_any(IdVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_numbers_and_text_and_writes_text() {
        let from_number: ItemId = serde_json::from_str("42").unwrap();
        let from_text: ItemId = serde_json::from_str("\"42\"").unwrap();
        assert_eq!(from_number, from_text);
        assert_eq!(from_number, ItemId::from(42));
        assert_eq!(serde_json::to_string(&from_number).unwrap(), "\"42\"");

        let uuid: ItemId = serde_json::from_str("\"6f1c2a0e-0000-4000-8000-000000000001\"").unwrap();
        assert_eq!(uuid.to_string(), "6f1c2a0e-0000-4000-8000-000000000001");
        assert_ne!(uuid, from_number);
        assert_eq!(ItemId::fixed("0"), ItemId::new("0"));
    }
}
