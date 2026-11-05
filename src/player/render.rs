//! Renders mpv video into a `GtkGLArea` through libmpv's render API.
//!
//! This replaces the python-mpv + PyOpenGL code in `legacy/player.py`
//! (`__on_realize`, `on_realize`, `do_render`, `GetProcAddressGetter`).

use std::cell::{Cell, RefCell};
use std::ffi::{c_char, c_int, c_uint, c_void, CString};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gtk::glib;
use gtk::prelude::*;
use libloading::Library;
use libmpv2::render::{OpenGLInitParams, RenderContext, RenderParam, RenderParamApiType};
use libmpv2::Mpv;

const GL_VIEWPORT: c_uint = 0x0BA2;
const GL_DRAW_FRAMEBUFFER_BINDING: c_uint = 0x8CA6;

type GetProcAddress = unsafe extern "C" fn(*const c_char) -> *mut c_void;
type GetCurrentContext = unsafe extern "C" fn() -> *mut c_void;
type GlGetIntegerv = unsafe extern "C" fn(c_uint, *mut c_int);

/// Looks up OpenGL functions for the GL context GDK created.
///
/// GDK uses EGL on Wayland and normally on X11 too, but can fall back to GLX
/// on X11. The loader that owns the current context is the one to ask.
struct GlResolver {
    get_proc_address: GetProcAddress,
    // Keeps the library that `get_proc_address` points into loaded.
    _library: Library,
}

impl GlResolver {
    /// Must be called with the GL context current.
    fn for_current_context() -> Result<Self, String> {
        if let Some(resolver) = Self::try_loader(
            &["libEGL.so.1", "libEGL.so"],
            b"eglGetCurrentContext\0",
            b"eglGetProcAddress\0",
        ) {
            return Ok(resolver);
        }
        if let Some(resolver) = Self::try_loader(
            &["libGLX.so.0", "libGL.so.1"],
            b"glXGetCurrentContext\0",
            b"glXGetProcAddressARB\0",
        ) {
            return Ok(resolver);
        }
        Err("neither EGL nor GLX has a current OpenGL context".into())
    }

    fn try_loader(names: &[&str], current_context: &[u8], get_proc: &[u8]) -> Option<Self> {
        for name in names {
            // SAFETY: these are the system GL loader libraries, already loaded
            // into the process by GDK; loading them runs no unexpected code.
            let Ok(library) = (unsafe { Library::new(*name) }) else {
                continue;
            };
            // SAFETY: the symbol types match the EGL/GLX prototypes.
            let found = unsafe {
                let current = library.get::<GetCurrentContext>(current_context).ok()?;
                if current().is_null() {
                    return None;
                }
                *library.get::<GetProcAddress>(get_proc).ok()?
            };
            return Some(Self {
                get_proc_address: found,
                _library: library,
            });
        }
        None
    }

    fn resolve(&self, name: &str) -> *mut c_void {
        let Ok(name) = CString::new(name) else {
            return std::ptr::null_mut();
        };
        // SAFETY: `name` is a valid NUL-terminated string.
        unsafe { (self.get_proc_address)(name.as_ptr()) }
    }
}

fn resolve_for_mpv(resolver: &Rc<GlResolver>, name: &str) -> *mut c_void {
    resolver.resolve(name)
}

struct GlState {
    // Declared first so it is dropped before the resolver it was created with.
    context: RenderContext<'static>,
    gl_get_integerv: GlGetIntegerv,
    _resolver: Rc<GlResolver>,
}

struct Inner {
    area: gtk::GLArea,
    // `gl` borrows from `mpv` (see `realize`), so it is declared first and
    // therefore dropped first.
    gl: RefCell<Option<GlState>>,
    mpv: Arc<Mpv>,
    frames_rendered: Cell<u64>,
}

/// Draws the video of one mpv instance into one `GtkGLArea`.
#[derive(Clone)]
pub struct VideoRenderer {
    inner: Rc<Inner>,
}

impl VideoRenderer {
    /// Connects `mpv` (which must have been created with `vo=libmpv`) to
    /// `area`. Rendering starts when the area is realized and stops when it is
    /// unrealized; the returned value only needs to be kept for inspection.
    pub fn attach(area: &gtk::GLArea, mpv: Arc<Mpv>) -> Self {
        let inner = Rc::new(Inner {
            area: area.clone(),
            gl: RefCell::new(None),
            mpv,
            frames_rendered: Cell::new(0),
        });

        // mpv draws every frame itself; GTK must not redraw on its own or
        // clear buffers it does not need.
        area.set_auto_render(false);
        area.set_has_depth_buffer(false);
        area.set_has_stencil_buffer(false);

        let weak = Rc::downgrade(&inner);
        area.connect_realize(move |_| {
            if let Some(inner) = weak.upgrade() {
                if let Err(err) = inner.realize() {
                    eprintln!("Could not set up video rendering: {err}");
                }
            }
        });

        let weak = Rc::downgrade(&inner);
        area.connect_unrealize(move |area| {
            if let Some(inner) = weak.upgrade() {
                // The render context has to be freed with its GL context current.
                area.make_current();
                inner.gl.borrow_mut().take();
            }
        });

        let weak = Rc::downgrade(&inner);
        area.connect_render(move |_, _| {
            if let Some(inner) = weak.upgrade() {
                inner.render();
            }
            glib::Propagation::Stop
        });

        // The renderer lives as long as the area it is attached to.
        let keep_alive = inner.clone();
        area.connect_destroy(move |_| {
            let _ = &keep_alive;
        });

        if area.is_realized() {
            if let Err(err) = inner.realize() {
                eprintln!("Could not set up video rendering: {err}");
            }
        }

        Self { inner }
    }

    /// Number of video frames drawn so far.
    pub fn frames_rendered(&self) -> u64 {
        self.inner.frames_rendered.get()
    }
}

impl Inner {
    fn realize(&self) -> Result<(), String> {
        if self.gl.borrow().is_some() {
            return Ok(());
        }
        self.area.make_current();
        if let Some(err) = self.area.error() {
            return Err(err.to_string());
        }

        let resolver = Rc::new(GlResolver::for_current_context()?);
        let gl_get_integerv = resolver.resolve("glGetIntegerv");
        if gl_get_integerv.is_null() {
            return Err("glGetIntegerv is not available".into());
        }
        // SAFETY: a non-null pointer returned for "glGetIntegerv" is that function.
        let gl_get_integerv: GlGetIntegerv = unsafe { std::mem::transmute(gl_get_integerv) };

        let context = self
            .mpv
            .create_render_context([
                RenderParam::ApiType(RenderParamApiType::OpenGl),
                RenderParam::InitParams(OpenGLInitParams {
                    get_proc_address: resolve_for_mpv,
                    ctx: resolver.clone(),
                }),
            ])
            .map_err(|err| format!("mpv render context: {err}"))?;
        // SAFETY: the lifetime ties the context to `self.mpv`. Both live in
        // `Inner`, the `Arc` keeps the mpv core at a stable address, and
        // `Inner` drops `gl` before `mpv`.
        let mut context: RenderContext<'static> = unsafe { std::mem::transmute(context) };

        // mpv calls this from its own threads whenever a new frame is ready.
        // Requests are coalesced so that at most one redraw is pending.
        let area = glib::SendWeakRef::from(self.area.downgrade());
        let pending = Arc::new(AtomicBool::new(false));
        context.set_update_callback(move || {
            if pending.swap(true, Ordering::AcqRel) {
                return;
            }
            let area = area.clone();
            let pending = pending.clone();
            glib::MainContext::default().invoke(move || {
                pending.store(false, Ordering::Release);
                if let Some(area) = area.upgrade() {
                    area.queue_render();
                }
            });
        });

        *self.gl.borrow_mut() = Some(GlState {
            context,
            gl_get_integerv,
            _resolver: resolver,
        });
        Ok(())
    }

    fn render(&self) {
        let gl = self.gl.borrow();
        let Some(gl) = gl.as_ref() else {
            return;
        };

        // GTK has bound its framebuffer and set the viewport to the area's
        // size in device pixels, which is also correct with fractional scaling.
        let mut framebuffer: c_int = 0;
        let mut viewport: [c_int; 4] = [0; 4];
        // SAFETY: the area's GL context is current inside the render signal
        // and both out-pointers are large enough for the queried values.
        unsafe {
            (gl.gl_get_integerv)(GL_DRAW_FRAMEBUFFER_BINDING, &mut framebuffer);
            (gl.gl_get_integerv)(GL_VIEWPORT, viewport.as_mut_ptr());
        }
        let (width, height) = (viewport[2], viewport[3]);
        if width <= 0 || height <= 0 {
            return;
        }

        match gl
            .context
            .render::<Rc<GlResolver>>(framebuffer, width, height, true)
        {
            Ok(()) => self.frames_rendered.set(self.frames_rendered.get() + 1),
            Err(err) => eprintln!("mpv could not render a frame: {err}"),
        }
    }
}
