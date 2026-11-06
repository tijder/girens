//! What Girens shows and plays, whichever server it comes from.

mod de;
mod error;
mod id;
pub mod model;
pub mod net;
mod queue;

pub use error::{Error, Result};
pub use id::ItemId;
pub use queue::PlayQueue;

/// A slice of a longer list.
#[derive(Debug, Clone)]
pub struct Page<T> {
    pub items: Vec<T>,
    /// Position of the first item in the whole list.
    pub offset: u32,
    /// Size of the whole list.
    pub total: u32,
}

/// What a player tells the server about the item it has open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Playing,
    Paused,
    Stopped,
}
