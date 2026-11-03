//! The browser shell for the whole app: the eframe web runner around `CraftApp`.

use axiom_ai::AiHub;
use axiom_engine::{Engine, Session, Settings};
use axiom_ui_egui::CraftApp;
use std::sync::Arc;
use wasm_bindgen::JsCast as _;

const CANVAS_ID: &str = "axiom_canvas";
const LOADING_ID: &str = "axiom_loading";

pub fn start() {
    eframe::WebLogger::init(log::LevelFilter::Info).ok();
    wasm_bindgen_futures::spawn_local(async {
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            log::error!("no document");
            return;
        };
        let Some(canvas) = document.get_element_by_id(CANVAS_ID).and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok()) else {
            log::error!("missing <canvas id=\"{CANVAS_ID}\">");
            return;
        };
        let mut options = eframe::WebOptions::default();
        if query().contains("webgl")
            && let eframe::egui_wgpu::WgpuSetup::CreateNew(create) = &mut options.wgpu_options.wgpu_setup
        {
            create.instance_descriptor.backends = eframe::wgpu::Backends::GL;
        }
        let result = eframe::WebRunner::new()
            .start(
                canvas,
                options,
                Box::new(|cc| {
                    CraftApp::setup_context(&cc.egui_ctx);
                    log::info!("axiom-app: starting the engine");
                    let mut session = Session::new(Settings::load());
                    let plugins = axiom_engine::plugins::install(&mut session);
                    let mut layer_tools = axiom_ai::tools();
                    layer_tools.extend(axiom_setup::tools::specs());
                    let engine = Engine::shared_with(session, layer_tools, plugins);
                    let hub = Arc::new(AiHub::new(engine.clone()));
                    log::info!("axiom-app: engine ready");
                    let mut app = CraftApp::new(engine, hub);
                    app.load_prefs(cc.storage);
                    Ok(Box::new(app))
                }),
            )
            .await;
        if let Some(el) = document.get_element_by_id(LOADING_ID) {
            match result {
                Ok(()) => el.remove(),
                Err(e) => el.set_inner_html(&format!("<p>Axiom failed to start: {e:?}</p><p>A browser with WebGPU or WebGL2 is required.</p>")),
            }
        }
    });
}

fn query() -> String {
    web_sys::window().and_then(|w| w.location().search().ok()).unwrap_or_default()
}
