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
    let options = eframe::NativeOptions { android_app: Some(app), ..Default::default() };
    let run = eframe::run_native(
        "Axiom",
        options,
        Box::new(move |cc| {
            log::info!("axiom-android: window open");
            CraftApp::setup_context(&cc.egui_ctx);
            let mut app = CraftApp::new(engine, hub);
            app.load_prefs(cc.storage);
            Ok(Box::new(app))
        }),
    );
    if let Err(e) = run {
        log::error!("axiom-android: the window couldn't start: {e}");
    }
}
