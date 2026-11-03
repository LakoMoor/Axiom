//! The models hub screen: models worth running on this computer (with whether they fit its
//! memory), a Hugging Face search, and what's downloaded and running. Everything goes through the
//! `models_*` tools, so the CLI and the AI agent do exactly the same; this file only draws.

use crate::i18n::tr;
use crate::theme::{self, Tokens};
use crate::{CraftApp, icons, widgets};
use egui::{RichText, Ui, Vec2};
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Recommended,
    Search,
    Downloaded,
}

/// What the screen shows; filled from tool results, never from disk while drawing.
#[derive(Debug, Default)]
pub struct View {
    pub tab: Tab,
    pub asked: bool,
    pub memory_gb: Option<u64>,
    pub recommended: Vec<Value>,
    pub installed: Vec<Value>,
    pub running: BTreeSet<String>,
    pub query: String,
    pub hits: Vec<Value>,
    pub searching: bool,
    pub repo: Option<(String, Vec<Value>)>,
    /// Files being started (loading takes a while).
    pub starting: BTreeSet<String>,
}

impl View {
    /// Take the answer of a `models_*` tool. Returns true when the lists should be asked for again.
    pub fn took(&mut self, tool: &str, v: &Value) -> bool {
        match tool {
            "models_recommended_list" => {
                self.memory_gb = v["memory_gb"].as_u64();
                self.recommended = v["models"].as_array().cloned().unwrap_or_default();
            }
            "models_installed_list" => {
                self.installed = v["models"].as_array().cloned().unwrap_or_default();
                self.running = self.installed.iter().filter(|m| m["running"] == true).filter_map(|m| m["file"].as_str().map(str::to_string)).collect();
            }
            "models_hub_search" => {
                self.searching = false;
                self.hits = v["results"].as_array().cloned().unwrap_or_default();
            }
            "models_repo_files_list" => {
                let repo = v["repo"].as_str().unwrap_or_default().to_string();
                self.repo = Some((repo, v["files"].as_array().cloned().unwrap_or_default()));
            }
            "models_file_download" | "models_file_remove" | "models_server_start" | "models_server_stop" => return true,
            _ => {}
        }
        false
    }
}

fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / (1u64 << 30) as f64)
}

/// Ask for the lists again (after a download, a removal, a start or a stop).
pub fn refresh(app: &mut CraftApp) {
    app.act("models_recommended_list", json!({}), "Looking at the models".into());
    app.act("models_installed_list", json!({}), "Looking at your models".into());
}

pub fn show(app: &mut CraftApp, ui: &mut Ui) {
    if !app.models.asked {
        app.models.asked = true;
        refresh(app);
    }
    let t = Tokens::get(ui.ctx());
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let full = ui.available_width();
        let w = full.min(1000.0) - 48.0;
        ui.add_space(24.0);
        ui.horizontal(|ui| {
            ui.add_space(((full - w) / 2.0).max(24.0));
            ui.vertical(|ui| {
                ui.set_width(w);
                ui.label(theme::heading(tr("Models"), theme::TITLE).color(t.text));
                let memory = match app.models.memory_gb {
                    Some(g) => {
                        crate::i18n::trf("AI that runs on this computer, so nothing you write leaves it. Memory here: {gb} GB.", &[("gb", &g.to_string())])
                    }
                    None => tr("AI that runs on this computer, so nothing you write leaves it."),
                };
                ui.label(RichText::new(memory).color(t.text_dim).size(theme::BODY));
                ui.add_space(14.0);
                let tabs = [tr("Recommended"), tr("Search"), tr("On this computer")];
                let names: Vec<&str> = tabs.iter().map(String::as_str).collect();
                let now = match app.models.tab {
                    Tab::Recommended => 0,
                    Tab::Search => 1,
                    Tab::Downloaded => 2,
                };
                if let Some(i) = widgets::segmented(ui, &names, now, 32.0) {
                    app.models.tab = [Tab::Recommended, Tab::Search, Tab::Downloaded][i.min(2)];
                }
                ui.add_space(16.0);
                downloads_bar(app, ui);
                match app.models.tab {
                    Tab::Recommended => recommended(app, ui, w),
                    Tab::Search => search(app, ui),
                    Tab::Downloaded => downloaded(app, ui),
                }
                ui.add_space(24.0);
            });
        });
    });
}

/// Downloads in progress, with their bars.
fn downloads_bar(app: &CraftApp, ui: &mut Ui) {
    for j in app.jobs.running().filter(|j| j.title.starts_with("Downloading ")) {
        let p = j.progress();
        crate::engine_ui::progress_bar(ui, p.as_ref().and_then(|p| p.fraction), &j.title);
        ui.add_space(8.0);
    }
}

fn fit_pill(ui: &mut Ui, fit: &str) {
    let t = Tokens::get(ui.ctx());
    let (text, c) = match fit {
        "comfortable" => ("Fits well", t.success),
        "tight" => ("Tight fit", t.warning),
        "too_big" => ("Too big for this computer", t.danger),
        _ => ("Memory unknown", t.text_faint),
    };
    widgets::pill(ui, text, c, c.gamma_multiply(0.16));
}

fn recommended(app: &mut CraftApp, ui: &mut Ui, w: f32) {
    let t = Tokens::get(ui.ctx());
    if app.models.recommended.is_empty() {
        ui.label(RichText::new(tr("Looking…")).color(t.text_faint));
        return;
    }
    let cols = if w > 700.0 { 2 } else { 1 };
    let cw = (w - 12.0 * (cols as f32 - 1.0)) / cols as f32;
    let list = app.models.recommended.clone();
    for row in list.chunks(cols) {
        ui.horizontal_top(|ui| {
            for m in row {
                ui.allocate_ui_with_layout(Vec2::new(cw, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| model_card(app, ui, m, cw));
            }
        });
        ui.add_space(12.0);
    }
}

fn model_card(app: &mut CraftApp, ui: &mut Ui, m: &Value, w: f32) {
    let t = Tokens::get(ui.ctx());
    let file = m["file"].as_str().unwrap_or_default().to_string();
    let title = m["title"].as_str().unwrap_or_default().to_string();
    let downloading = app.jobs.running().any(|j| j.title == format!("Downloading {title}"));
    widgets::card(&t).show(ui, |ui| {
        ui.set_width(w - 34.0);
        ui.set_min_height(140.0);
        ui.horizontal(|ui| {
            icons::show(ui, "cpu", 18.0, t.icon);
            ui.label(RichText::new(&title).font(theme::semibold(theme::CARD)).color(t.text));
        });
        ui.label(
            RichText::new(format!(
                "{} · {} · {}",
                m["maker"].as_str().unwrap_or_default(),
                tr(m["focus"].as_str().map(focus_name).unwrap_or("Chat")),
                gb(m["size_bytes"].as_u64().unwrap_or(0))
            ))
            .color(t.text_faint)
            .size(theme::CAPTION),
        );
        ui.add_space(4.0);
        ui.label(RichText::new(m["about"].as_str().unwrap_or_default()).color(t.text_dim).size(theme::SMALL));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            fit_pill(ui, m["fit"].as_str().unwrap_or("unknown"));
            if m["downloaded"] == true {
                widgets::pill(ui, "Downloaded", t.accent, t.accent_soft);
            }
        });
        ui.add_space(10.0);
        let id = m["id"].as_str().unwrap_or_default().to_string();
        if downloading {
            ui.label(RichText::new(tr("Downloading…")).color(t.text_faint).size(theme::SMALL));
        } else if m["downloaded"] == true {
            run_controls(app, ui, &file);
        } else if widgets::primary_icon(ui, Some("download"), "Download", true).clicked() {
            app.act("models_file_download", json!({"id": id}), format!("Downloading {title}"));
        }
    });
}

fn focus_name(f: &str) -> &'static str {
    match f {
        "code" => "Code",
        "tiny" => "Tiny",
        _ => "Chat",
    }
}

/// Run, stop and delete for a downloaded file.
fn run_controls(app: &mut CraftApp, ui: &mut Ui, file: &str) {
    let running = app.models.running.contains(file);
    let starting = app.models.starting.contains(file);
    ui.horizontal(|ui| {
        if starting {
            ui.label(RichText::new(tr("Loading the model… this can take a minute.")).size(theme::SMALL));
        } else if running {
            widgets::pill(ui, "Running", Tokens::get(ui.ctx()).success, Tokens::get(ui.ctx()).success.gamma_multiply(0.16));
            if widgets::secondary(ui, Some("circle-stop"), "Stop").clicked() {
                app.models.running.remove(file);
                app.act("models_server_stop", json!({"file": file}), "Stopping the model".into());
            }
        } else if widgets::primary_icon(ui, Some("play"), "Run", true).clicked() {
            app.models.starting.insert(file.to_string());
            app.act("models_server_start", json!({"file": file}), format!("Starting {file}"));
        }
        if !starting && icons::button(ui, "trash-2", 28.0, false, "Delete this model from the computer").clicked() {
            app.act("models_file_remove", json!({"file": file}), "Deleting the model".into());
        }
    });
}

fn search(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        let r = widgets::text_field_frame(&t)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut app.models.query)
                        .hint_text(tr("Search Hugging Face, e.g. qwen or coder"))
                        .desired_width(420.0)
                        .frame(egui::Frame::NONE),
                )
            })
            .inner;
        let go = (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            | widgets::primary_icon(ui, Some("search"), "Search", !app.models.query.trim().is_empty()).clicked();
        if go && !app.models.query.trim().is_empty() {
            app.models.searching = true;
            app.models.repo = None;
            app.act("models_hub_search", json!({"query": app.models.query.trim()}), "Searching Hugging Face".into());
        }
    });
    ui.add_space(12.0);
    if app.models.searching {
        ui.label(RichText::new(tr("Searching…")).color(t.text_faint));
    }
    if let Some((repo, files)) = app.models.repo.clone() {
        if widgets::link(ui, &tr("Back to results")).clicked() {
            app.models.repo = None;
        }
        ui.label(RichText::new(&repo).font(theme::semibold(theme::CARD)).color(t.text));
        ui.add_space(6.0);
        if files.is_empty() {
            ui.label(RichText::new(tr("No single-file models here.")).color(t.text_faint));
        }
        for f in files {
            let name = f["name"].as_str().unwrap_or_default().to_string();
            let have = app.models.installed.iter().any(|i| i["file"] == name.as_str());
            widgets::card(&t).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&name).color(t.text).size(theme::SMALL));
                        ui.label(
                            RichText::new(format!("{} · {}", f["quant"].as_str().unwrap_or("?"), gb(f["size_bytes"].as_u64().unwrap_or(0))))
                                .color(t.text_faint)
                                .size(theme::CAPTION),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if have {
                            widgets::pill(ui, "Downloaded", t.accent, t.accent_soft);
                        } else if widgets::primary_icon(ui, Some("download"), "Download", true).clicked() {
                            app.act("models_file_download", json!({"repo": repo, "file": name}), format!("Downloading {name}"));
                        }
                    });
                });
            });
            ui.add_space(6.0);
        }
        return;
    }
    for h in app.models.hits.clone() {
        let repo = h["repo"].as_str().unwrap_or_default().to_string();
        let r = widgets::card(&t).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&repo).color(t.text).size(theme::SMALL));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} {} · {} {}", h["downloads"], tr("downloads"), h["likes"], tr("likes")))
                            .color(t.text_faint)
                            .size(theme::CAPTION),
                    );
                });
            });
        });
        if r.response.interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            app.act("models_repo_files_list", json!({"repo": repo}), "Looking at the files".into());
        }
        ui.add_space(6.0);
    }
}

fn downloaded(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let list = app.models.installed.clone();
    if list.is_empty() {
        widgets::empty_state(ui, "cpu", "No models yet", "Download one from Recommended or Search.");
        return;
    }
    for m in list {
        let file = m["file"].as_str().unwrap_or_default().to_string();
        widgets::card(&t).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(&file).color(t.text).size(theme::SMALL));
                    let from = m["repo"].as_str().unwrap_or("added by hand");
                    ui.label(RichText::new(format!("{} · {}", gb(m["size_bytes"].as_u64().unwrap_or(0)), from)).color(t.text_faint).size(theme::CAPTION));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| run_controls(app, ui, &file));
            });
        });
        ui.add_space(6.0);
    }
}
