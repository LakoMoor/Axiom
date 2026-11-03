//! Axiom on Android: the same `CraftApp` the desktop window runs, in a native activity.
//!
//! Everything is `cfg(target_os = "android")`, so on a computer this crate is an empty library.

#[cfg(target_os = "android")]
mod app;
