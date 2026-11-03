//! The activity's shell: the engine, the AI hub and the eframe window around `CraftApp`.

use axiom_ai::AiHub;
use axiom_android_glue::AndroidApp;
use axiom_engine::{Engine, Session, Settings};
use axiom_ui_egui::CraftApp;
use std::sync::Arc;

axiom_android_glue::android_entry!(run);

fn run(app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("axiom"));
    // Settings, keys and the engine store live in the app's own private folder.
    if let Some(dir) = app.internal_data_path() {
        axiom_engine::settings::set_config_dir(dir.join("axiom"));
    }
    log::info!("axiom-android: starting the engine");
    let mut session = Session::new(Settings::load());
    let plugins = axiom_engine::plugins::install(&mut session);
    let mut layer_tools = axiom_ai::tools();
    layer_tools.extend(axiom_setup::tools::specs());
    let engine = Engine::shared_with(session, layer_tools, plugins);
    let hub = Arc::new(AiHub::new(engine.clone()));
    let android = app.clone();
    let options = eframe::NativeOptions { android_app: Some(app), ..Default::default() };
    let run = eframe::run_native(
        "Axiom",
        options,
        Box::new(move |cc| {
            log::info!("axiom-android: window open");
            CraftApp::setup_context(&cc.egui_ctx);
            let mut app = CraftApp::new(engine, hub);
            app.load_prefs(cc.storage);
            Ok(Box::new(Shell { app, android }))
        }),
    );
    if let Err(e) = run {
        log::error!("axiom-android: the window couldn't start: {e}");
    }
}

/// `CraftApp` in the activity's window. The system's bars (status, navigation, a camera cut-out)
/// draw over the window, so each frame it tells egui how much of the edges they cover and the
/// interface stays clear of them.
struct Shell {
    app: CraftApp,
    android: AndroidApp,
}

impl Shell {
    /// How much of each edge the system covers, in points: what the window's content rectangle
    /// leaves out of the whole window.
    fn insets(&self, points_per_pixel: f32) -> Option<egui::SafeAreaInsets> {
        let window = self.android.native_window()?;
        let rect = self.android.content_rect();
        if rect.right <= rect.left || rect.bottom <= rect.top {
            return None;
        }
        let gap = |px: i32| px.max(0) as f32 * points_per_pixel;
        Some(egui::SafeAreaInsets(egui::epaint::MarginF32 {
            left: gap(rect.left),
            right: gap(window.width() - rect.right),
            top: gap(rect.top),
            bottom: gap(window.height() - rect.bottom),
        }))
    }
}

impl eframe::App for Shell {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.ui(ui, frame);
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        self.app.clear_color(visuals)
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.app.save(storage);
    }

    fn on_exit(&mut self) {
        self.app.on_exit();
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        let ppp = ctx.pixels_per_point();
        if ppp > 0.0 {
            raw_input.safe_area_insets = self.insets(1.0 / ppp);
        }
    }
}
