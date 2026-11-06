//! A small Plex client with exactly the functionality Girens needs.
//!
//! Everything is async and independent of GTK.

pub use girens_model::model;
pub mod myplex;
pub mod playback;
pub mod playqueue;
mod server;

pub use girens_model::{Error, ItemId, Page, Result};
pub use server::{ClientInfo, PlexServer};

pub use girens_model::net::{download, download_to_file, download_to_file_with_progress};
