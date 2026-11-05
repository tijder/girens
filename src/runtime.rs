//! Network requests run on a small tokio runtime; the UI awaits their
//! results on the GLib main loop.

use std::future::Future;
use std::sync::OnceLock;

use tokio::runtime::Runtime;

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("girens-net")
            .build()
            .expect("could not start the network runtime")
    })
}

/// Runs `future` on the network runtime and returns its output to the caller,
/// which is typically a future spawned with `glib::spawn_future_local`.
/// Dropping the returned future cancels the work.
pub async fn run<F>(future: F) -> F::Output
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    struct AbortOnDrop<T>(tokio::task::JoinHandle<T>);
    impl<T> Drop for AbortOnDrop<T> {
        fn drop(&mut self) {
            self.0.abort();
        }
    }

    let mut task = AbortOnDrop(runtime().spawn(future));
    match (&mut task.0).await {
        Ok(output) => output,
        Err(error) => std::panic::resume_unwind(error.into_panic()),
    }
}
