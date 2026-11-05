//! Build-time configuration.
//!
//! Meson passes the real values through `GIRENS_*` environment variables (see
//! `src/meson.build`). The fallbacks make `cargo check` and `cargo test` work
//! without Meson.

pub const APP_ID: &str = "nl.g4d.Girens";
pub const GETTEXT_PACKAGE: &str = "girens";
pub const RESOURCE_PREFIX: &str = "/nl/g4d/Girens";

pub const VERSION: &str = match option_env!("GIRENS_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

pub const PKGDATADIR: &str = match option_env!("GIRENS_PKGDATADIR") {
    Some(v) => v,
    None => "/app/share/girens",
};

pub const LOCALEDIR: &str = match option_env!("GIRENS_LOCALEDIR") {
    Some(v) => v,
    None => "/app/share/locale",
};

/// Meson `buildtype`; "debug" marks the window as a development build.
pub const BUILD_TYPE: &str = match option_env!("GIRENS_BUILD_TYPE") {
    Some(v) => v,
    None => "debug",
};
