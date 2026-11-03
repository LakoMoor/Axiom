//! The browser's way to give the app a file: the picker and drag-and-drop. A page has no disk, so
//! the file is read in memory (asynchronously, as the browser only allows) and kept under
//! `/web/…` (`axiom_platform::fs::stage_file`); the screens then open that path like any other.

use crate::Pick;
use axiom_platform::sync::Mutex;
use std::path::PathBuf;
use std::sync::LazyLock;

/// Files that arrived and wait for the next frame: which button asked (none for a drop) and where
/// the file was kept.
static ARRIVED: LazyLock<Mutex<Vec<(Option<Pick>, PathBuf)>>> = LazyLock::new(|| Mutex::new(Vec::new()));

fn deliver(pick: Option<Pick>, name: &str, bytes: Vec<u8>, ctx: &egui::Context) {
    let path = axiom_platform::fs::stage_file(name, bytes);
    if let Ok(mut q) = ARRIVED.lock() {
        q.push((pick, path));
    }
    ctx.request_repaint();
}

/// Show the browser's file picker for `pick`.
pub fn pick_file(pick: Pick, ctx: egui::Context) {
    wasm_bindgen_futures::spawn_local(async move {
        let Some(file) = rfd::AsyncFileDialog::new().pick_file().await else { return };
        let bytes = file.read().await;
        deliver(Some(pick), &file.file_name(), bytes, &ctx);
    });
}

/// Read a dropped file.
pub fn read_dropped(file: egui::DroppedFileHandle, ctx: egui::Context) {
    wasm_bindgen_futures::spawn_local(async move {
        let name = file.path().file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".to_string());
        match file.bytes_async().await {
            Ok(bytes) => deliver(None, &name, bytes, &ctx),
            Err(e) => log::warn!("The browser couldn't read {name}: {e}"),
        }
    });
}

/// The files that arrived since the last frame.
pub fn take_arrived() -> Vec<(Option<Pick>, PathBuf)> {
    ARRIVED.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
}
