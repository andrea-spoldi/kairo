//! Global tokio runtime for kube operations.
//!
//! GPUI on macOS dispatches background tasks via Grand Central Dispatch (GCD),
//! not tokio. kube-rs (via tower/hyper) requires a tokio reactor for async I/O.
//! We therefore spin up a dedicated multi-thread tokio runtime and expose its
//! handle so the UI crate can submit kube futures without touching GPUI's executor.

use std::sync::OnceLock;

static HANDLE: OnceLock<tokio::runtime::Handle> = OnceLock::new();

/// Initialise the kube tokio runtime.  Must be called once before any kube work.
pub fn init() {
    if HANDLE.get().is_some() {
        return;
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("kubescope-kube")
        .build()
        .expect("failed to build kube tokio runtime");

    // Keep the runtime alive for the lifetime of the process.
    std::thread::spawn(move || {
        let handle = rt.handle().clone();
        HANDLE.set(handle).ok();
        rt.block_on(std::future::pending::<()>());
    });

    // Spin-wait until the handle is stored (almost instant).
    while HANDLE.get().is_none() {
        std::thread::yield_now();
    }
}

/// Return the global kube tokio runtime handle.
///
/// # Panics
/// Panics if [`init`] has not been called first.
pub fn handle() -> &'static tokio::runtime::Handle {
    HANDLE.get().expect("kube runtime not initialised; call kube_runtime::init()")
}
