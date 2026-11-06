//! A small client for Ister media servers, with exactly what Girens needs.
//!
//! The server speaks GraphQL and leaves signing in to an OpenID Connect
//! provider; what it answers is turned into the items of `girens-model`,
//! under the same operations as the Plex client has.

pub mod auth;
pub mod download;
mod map;
mod queries;
mod server;
mod wire;

pub use map::Labels;
pub use server::{machine_identifier, IsterServer, Player, TokenKeeper, User, SUBTITLE_DELAY};

/// The documents sent to the server, for checking them against its schema.
#[doc(hidden)]
pub fn documents() -> &'static [&'static str] {
    queries::ALL
}
