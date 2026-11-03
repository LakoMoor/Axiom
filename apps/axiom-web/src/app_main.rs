//! Axiom in the browser, the whole app: the same `CraftApp` the desktop window runs, drawn into a
//! canvas. What needs a program on the computer (Ghidra, adb, an agent CLI) says it is desktop-only.
//!
//! Build with `packaging/web/package.sh --app`; the site lands in `dist/web-app`.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(target_arch = "wasm32")]
mod app;

#[cfg(target_arch = "wasm32")]
fn main() {
    app::start();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("axiom-app only runs in the browser: build it with packaging/web/package.sh --app");
}
