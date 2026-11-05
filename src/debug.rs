//! Development aids, only compiled into debug builds.

use std::time::Duration;

use gtk::prelude::*;
use gtk::{gdk, glib};

/// With `GIRENS_DEBUG_TIMING=1`, prints how long after process start `what`
/// happened. Call [`mark_start`] first thing in `main`.
pub fn mark(what: &str) {
    if std::env::var_os("GIRENS_DEBUG_TIMING").is_some() {
        eprintln!("timing: {what} after {} ms", start().elapsed().as_millis());
    }
}

pub fn mark_start() {
    start();
}

fn start() -> &'static std::time::Instant {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now)
}

/// With `GIRENS_DEBUG_SCREENSHOT=<file.png>` the window is rendered to that
/// file after `GIRENS_DEBUG_SCREENSHOT_DELAY` seconds (default 5) and the
/// application quits. Used to check the UI from scripts.
pub fn schedule_screenshot(window: &gtk::Window) {
    let Ok(path) = std::env::var("GIRENS_DEBUG_SCREENSHOT") else {
        return;
    };
    let delay = std::env::var("GIRENS_DEBUG_SCREENSHOT_DELAY")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(5);
    let window = window.downgrade();
    glib::timeout_add_local_once(Duration::from_secs(delay), move || {
        let Some(window) = window.upgrade() else {
            return;
        };
        match render(&window) {
            Some(texture) => match texture.save_to_png(&path) {
                Ok(()) => eprintln!("Screenshot written to {path}"),
                Err(err) => eprintln!("Could not write {path}: {err}"),
            },
            None => eprintln!("Could not render the window"),
        }
        window.close();
    });
}

fn render(window: &gtk::Window) -> Option<gdk::Texture> {
    let (width, height) = (window.width(), window.height());
    let snapshot = gtk::Snapshot::new();
    gtk::WidgetPaintable::new(Some(window)).snapshot(&snapshot, width.into(), height.into());
    let node = snapshot.to_node()?;
    Some(window.renderer()?.render_texture(&node, None))
}

/// With `GIRENS_DEBUG_ACTIVATE="show-show-by-id 123"` that window action is
/// activated once the server is connected; with `GIRENS_DEBUG_SIDEBAR_ROW=3`
/// that sidebar row is selected; with `GIRENS_DEBUG_SEARCH=text` that is
/// searched for. Lets scripts open a page for a screenshot.
pub fn after_connect(window: &crate::window::PlexWindow) {
    if let Ok(spec) = std::env::var("GIRENS_DEBUG_ACTIVATE") {
        let mut parts = spec.split_whitespace();
        if let (Some(action), Some(Ok(key))) = (parts.next(), parts.next().map(str::parse::<i64>)) {
            gtk::prelude::ActionGroupExt::activate_action(window, action, Some(&key.to_variant()));
        }
    }
    if let Ok(query) = std::env::var("GIRENS_DEBUG_SEARCH") {
        window.debug_search(&query);
    }
    if let Some(row) = std::env::var("GIRENS_DEBUG_SIDEBAR_ROW")
        .ok()
        .and_then(|value| value.parse().ok())
    {
        window.debug_select_sidebar_row(row);
    }
}
