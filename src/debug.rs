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
        // An open dialog keeps the window from closing, so then the process
        // simply ends; otherwise closing exercises the normal shutdown.
        if std::env::var_os("GIRENS_DEBUG_OPEN").is_some() {
            std::process::exit(0);
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

/// Seconds between the actions of `GIRENS_DEBUG_ACTIVATE`.
const ACTIVATE_INTERVAL: u64 = 3;

/// With `GIRENS_DEBUG_ACTIVATE="show-show-by-id 123"` that window action is
/// activated once the server is connected; several actions separated by `;`
/// follow each other, to visit one page after another. With `GIRENS_DEBUG_SIDEBAR_ROW=3`
/// that sidebar row is selected; with `GIRENS_DEBUG_SEARCH=text` that is
/// searched for; with `GIRENS_DEBUG_OPEN=preferences|shortcuts|about|sync|accounts|pin|wrong-pin` that
/// dialog is opened; with `GIRENS_DEBUG_ACCOUNT="switch 1"` (or `sign-out 1`,
/// `add-server <url> <token>`) that is done with the accounts, once. Lets
/// scripts open a page for a screenshot.
pub fn after_connect(window: &crate::window::PlexWindow) {
    if let Ok(spec) = std::env::var("GIRENS_DEBUG_ACTIVATE") {
        for (index, step) in spec.split(';').enumerate() {
            let mut parts = step.split_whitespace();
            let (Some(action), Some(Ok(key))) = (
                parts.next().map(str::to_owned),
                parts.next().map(str::parse::<i64>),
            ) else {
                continue;
            };
            let window = window.downgrade();
            let delay = Duration::from_secs(ACTIVATE_INTERVAL * index as u64);
            glib::timeout_add_local_once(delay, move || {
                if let Some(window) = window.upgrade() {
                    gtk::prelude::ActionGroupExt::activate_action(
                        &window,
                        &action,
                        Some(&key.to_variant()),
                    );
                }
            });
        }
    }
    // Switching connects again, which must not repeat the step.
    static ACCOUNT_STEP: std::sync::Once = std::sync::Once::new();
    if let Ok(step) = std::env::var("GIRENS_DEBUG_ACCOUNT") {
        ACCOUNT_STEP.call_once(|| window.debug_account(&step));
    }
    if let Ok(dialog) = std::env::var("GIRENS_DEBUG_OPEN") {
        window.debug_open(&dialog);
    }
    if let Ok(query) = std::env::var("GIRENS_DEBUG_SEARCH") {
        window.search(&query);
    }
    if let Some(row) = std::env::var("GIRENS_DEBUG_SIDEBAR_ROW")
        .ok()
        .and_then(|value| value.parse().ok())
    {
        window.debug_select_sidebar_row(row);
    }
}
