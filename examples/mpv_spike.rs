//! Phase 0 spike: play a file or URL with mpv inside a `GtkGLArea`.
//!
//! ```sh
//! cargo run --example mpv_spike -- <file-or-url> [seconds-before-quit]
//! ```
//!
//! Prints the mpv events it sees and, on exit, how many frames were drawn.
//! Run it once on Wayland and once with `GDK_BACKEND=x11`.

#[path = "../src/player/render.rs"]
mod render;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gtk::prelude::*;
use gtk::{gio, glib};
use libmpv2::events::Event;
use libmpv2::{Format, Mpv};

fn main() -> glib::ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(source) = args.next() else {
        eprintln!("usage: mpv_spike <file-or-url> [seconds-before-quit]");
        return glib::ExitCode::FAILURE;
    };
    let quit_after = args.next().and_then(|value| value.parse::<u64>().ok());

    // SAFETY: no other thread exists yet, so nothing reads the locale concurrently.
    unsafe {
        gettextrs::setlocale(gettextrs::LocaleCategory::LcAll, "");
        // libmpv refuses to initialise unless LC_NUMERIC is "C".
        gettextrs::setlocale(gettextrs::LocaleCategory::LcNumeric, "C");
    }
    gtk::disable_setlocale();

    let app = gtk::Application::new(
        Some("nl.g4d.Girens.MpvSpike"),
        gio::ApplicationFlags::NON_UNIQUE,
    );
    app.connect_activate(move |app| activate(app, &source, quit_after));
    app.run_with_args::<&str>(&[])
}

fn activate(app: &gtk::Application, source: &str, quit_after: Option<u64>) {
    let mpv = Mpv::with_initializer(|init| {
        init.set_property("vo", "libmpv")?;
        init.set_property("keep-open", "yes")?;
        init.set_property("cache", "yes")?;
        Ok(())
    })
    .expect("could not create the mpv core");

    mpv.observe_property("time-pos", Format::Double, 1)
        .expect("could not observe time-pos");
    let mpv = Arc::new(mpv);

    let area = gtk::GLArea::new();
    let renderer = render::VideoRenderer::attach(&area, mpv.clone());

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Girens mpv spike")
        .default_width(960)
        .default_height(540)
        .child(&area)
        .build();

    // Polling is enough for the spike; the real player will use mpv's wakeup
    // callback instead.
    let last_position = Rc::new(RefCell::new(None::<f64>));
    glib::timeout_add_local(Duration::from_millis(50), {
        let mpv = mpv.clone();
        let last_position = last_position.clone();
        move || {
            while let Some(event) = mpv.wait_event(0.0) {
                match event {
                    Ok(Event::PropertyChange {
                        name: "time-pos",
                        change,
                        ..
                    }) => {
                        if let libmpv2::events::PropertyData::Double(position) = change {
                            *last_position.borrow_mut() = Some(position);
                        }
                    }
                    Ok(event) => eprintln!("mpv event: {event:?}"),
                    Err(err) => eprintln!("mpv error: {err}"),
                }
            }
            glib::ControlFlow::Continue
        }
    });

    window.connect_close_request({
        let renderer = renderer.clone();
        let last_position = last_position.clone();
        move |_| {
            eprintln!(
                "frames rendered: {}, last position: {:?} s",
                renderer.frames_rendered(),
                last_position.borrow()
            );
            glib::Propagation::Proceed
        }
    });

    if let Some(seconds) = quit_after {
        let window = window.downgrade();
        glib::timeout_add_seconds_local_once(seconds as u32, move || {
            if let Some(window) = window.upgrade() {
                window.close();
            }
        });
    }

    window.present();
    if let Err(err) = mpv.command("loadfile", &[source, "replace"]) {
        eprintln!("could not load {source}: {err}");
    }
}
