//! The entry point an Android app needs.
//!
//! The system starts a native activity by calling a C function named `android_main`, which Rust
//! can only export with `#[unsafe(no_mangle)]`. The workspace forbids `unsafe` in every crate
//! (`unsafe_code = "forbid"`), so this crate holds the one export as a macro: the app crate
//! writes `android_entry!(run)` and no unsafe code of its own, and nothing here does anything
//! but name the function the system looks for.

#[cfg(target_os = "android")]
pub use android_activity::AndroidApp;

/// Export `android_main`, which calls `$run(app)` with the app's handle to the activity.
///
/// `$run` is a `fn(AndroidApp)`.
#[macro_export]
macro_rules! android_entry {
    ($run:path) => {
        #[unsafe(no_mangle)]
        pub fn android_main(app: $crate::AndroidApp) {
            $run(app)
        }
    };
}
