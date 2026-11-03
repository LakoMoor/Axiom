//! The phone layout: a narrow screen has no title bar, side panel or icon rail, and goes
//! between the main places with a row of buttons along the bottom.

use crate::state::Screen;
use crate::{CraftApp, icons};
use egui::Ui;

/// Narrower than this, the screen is laid out as a phone's.
const PHONE_WIDTH: f32 = 640.0;
/// The height of the navigation row and of what it holds.
pub const NAV_HEIGHT: f32 = 64.0;

/// Whether to lay the window out as a phone's: always on Android, otherwise when it is narrow
/// (a browser tab on a phone, or a desktop window made small).
pub fn is_phone(ctx: &egui::Context) -> bool {
    cfg!(target_os = "android") || ctx.content_rect().width() < PHONE_WIDTH
}

/// The buttons along the bottom: the same places as the icon rail.
pub fn nav_bar(app: &mut CraftApp, ui: &mut Ui) {
    let places: [(&str, &str, Screen); 3] = [("plus", "New project", Screen::Home), ("bot", "Agent", Screen::Agent), ("cpu", "Models", Screen::Models)];
    let count = places.len() + 1;
    let width = ui.available_width() / count as f32;
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (icon, tip, screen) in places {
            ui.allocate_ui_with_layout(egui::vec2(width, NAV_HEIGHT), egui::Layout::top_down(egui::Align::Center), |ui| {
                ui.add_space((NAV_HEIGHT - 44.0) / 2.0);
                let on = app.state.screen == screen || (screen == Screen::Home && matches!(app.state.screen, Screen::Workbench));
                if icons::button(ui, icon, 44.0, on, tip).clicked() {
                    app.state.screen = screen;
                }
            });
        }
        ui.allocate_ui_with_layout(egui::vec2(width, NAV_HEIGHT), egui::Layout::top_down(egui::Align::Center), |ui| {
            ui.add_space((NAV_HEIGHT - 44.0) / 2.0);
            if icons::button(ui, "settings", 44.0, app.state.settings_open, "Settings").clicked() {
                app.state.settings_open = !app.state.settings_open;
            }
        });
    });
}
