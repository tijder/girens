//! Tells the user when something failed. Messages go to the terminal and,
//! once the window exists, into a toast at the bottom of it.

use std::cell::RefCell;
use std::time::{Duration, Instant};

/// The same message is not repeated within this time, e.g. when one outage
/// makes several requests fail.
const REPEAT_AFTER: Duration = Duration::from_secs(5);

thread_local! {
    static OVERLAY: RefCell<Option<adw::ToastOverlay>> = const { RefCell::new(None) };
    static LAST: RefCell<Option<(String, Instant)>> = const { RefCell::new(None) };
}

/// Sets where toasts are shown.
pub fn set_overlay(overlay: &adw::ToastOverlay) {
    OVERLAY.set(Some(overlay.clone()));
}

/// Reports a failure. Use through the `report!` macro.
pub fn failure(message: String) {
    eprintln!("{message}");
    let repeated = LAST.with_borrow(|last| {
        last.as_ref()
            .is_some_and(|(text, at)| *text == message && at.elapsed() < REPEAT_AFTER)
    });
    if repeated {
        return;
    }
    OVERLAY.with_borrow(|overlay| {
        if let Some(overlay) = overlay {
            // Long enough to read a sentence.
            let toast = adw::Toast::new(&message);
            toast.set_timeout(8);
            overlay.add_toast(toast);
        }
    });
    LAST.set(Some((message, Instant::now())));
}
