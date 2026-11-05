//! Tells the user when something failed: a translated sentence in a toast
//! at the bottom of the window, the technical detail in the terminal.

use std::cell::RefCell;
use std::fmt::Display;
use std::time::{Duration, Instant};

/// The same message is not repeated within this time, e.g. when one outage
/// makes several requests fail.
const REPEAT_AFTER: Duration = Duration::from_secs(5);

thread_local! {
    static OVERLAY: RefCell<Option<adw::ToastOverlay>> = const { RefCell::new(None) };
    static LAST: RefCell<Option<(String, Instant)>> = const { RefCell::new(None) };
    static ON_FAILURE: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
}

/// Sets where toasts are shown.
pub fn set_overlay(overlay: &adw::ToastOverlay) {
    OVERLAY.set(Some(overlay.clone()));
}

/// Registers what is called after every failure; the window uses it to check
/// whether the server is still reachable.
pub fn connect_failure(callback: impl Fn() + 'static) {
    ON_FAILURE.set(Some(Box::new(callback)));
}

/// Shows `text` (already translated) to the user.
pub fn message(text: &str) {
    let repeated = LAST.with_borrow(|last| {
        last.as_ref()
            .is_some_and(|(shown, at)| shown == text && at.elapsed() < REPEAT_AFTER)
    });
    if repeated {
        return;
    }
    OVERLAY.with_borrow(|overlay| {
        if let Some(overlay) = overlay {
            // Long enough to read a sentence.
            let toast = adw::Toast::new(text);
            toast.set_timeout(8);
            overlay.add_toast(toast);
        }
    });
    LAST.set(Some((text.to_owned(), Instant::now())));
}

/// Like [`failure`], with a button that calls `retry`.
pub fn failure_with_retry(text: &str, detail: &dyn Display, retry: impl Fn() + 'static) {
    eprintln!("{text}: {detail}");
    OVERLAY.with_borrow(|overlay| {
        if let Some(overlay) = overlay {
            let toast = adw::Toast::new(text);
            toast.set_timeout(8);
            toast.set_button_label(Some(&crate::i18n::gettext("Retry")));
            toast.connect_button_clicked(move |_| retry());
            overlay.add_toast(toast);
        }
    });
    notify_failure();
}

/// Reports a failure: `text` (already translated) for the user, `detail` for
/// the terminal.
pub fn failure(text: &str, detail: &dyn Display) {
    eprintln!("{text}: {detail}");
    message(text);
    notify_failure();
}

fn notify_failure() {
    // Taken out while it runs, so that a failure inside it cannot recurse.
    if let Some(callback) = ON_FAILURE.take() {
        callback();
        ON_FAILURE.set(Some(callback));
    }
}
