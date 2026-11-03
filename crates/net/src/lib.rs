//! HTTP, one API for every system. On computers this is `ureq` (the types are re-exported as they
//! are). In a browser page there are no sockets, so the same small API runs on a synchronous
//! `XMLHttpRequest`: a call waits for the whole answer (no streaming yet), and CORS applies, so
//! only servers that allow the page's origin can be reached.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(not(target_arch = "wasm32"))]
pub use ureq::{Agent, Body, Error, http};

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::*;
