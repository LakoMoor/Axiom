//! "Changes" in a project's side panel: the files the latest build run added, changed or deleted,
//! with a line diff when one is opened. The data comes from the `project_changes_show` tool's
//! engine code, read on a worker; drawing only reads what was last handed back.

use crate::i18n::{tr, trf};
use crate::theme::{self, Tokens};
use crate::{CraftApp, Done, widgets};
use axiom_engine::changes::{Change, Kind, Report};
use egui::{RichText, Ui};
use std::path::PathBuf;

/// What the panel holds for the open project.
#[derive(Debug, Default)]
pub struct View {
    pub root: PathBuf,
    pub report: Report,
    /// Files whose diff is open.
    pub open: std::collections::BTreeSet<String>,
    pub loading: bool,
}

impl CraftApp {
    /// Read what the open project's latest run changed. With `detail`, that file's diff lines too.
    pub fn load_changes(&mut self, detail: Option<String>) {
        let Some(ws) = self.workspace.clone() else { return };
        self.changes.loading = true;
        self.jobs.spawn("Look at the changes", false, move |_| {
            let report = axiom_engine::changes::report(&ws, detail.as_deref());
            Ok(Done::Changes { root: ws.root.clone(), report, detail })
        });
    }

    /// A read came back: a whole report, or one file's lines for the report held.
    pub fn changes_read(&mut self, root: PathBuf, report: Report, detail: Option<String>) {
        if self.workspace.as_ref().is_none_or(|w| w.root != root) {
            return;
        }
        self.changes.loading = false;
        match detail {
            Some(path) if self.changes.root == root => {
                if let Some(lines) = report.changes.into_iter().find(|c| c.path == path)
                    && let Some(mine) = self.changes.report.changes.iter_mut().find(|c| c.path == path)
                {
                    *mine = lines;
                }
            }
            _ => {
                self.changes.root = root;
                self.changes.report = report;
            }
        }
    }
}

pub fn card(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    widgets::card(&t).inner_margin(egui::Margin::symmetric(16, 14)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            widgets::caption(ui, "Changes");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::icons::button(ui, "refresh-cw", 22.0, false, "Look at the changes again").clicked() {
                    app.load_changes(None);
                }
            });
        });
        ui.add_space(6.0);
        let r = &app.changes.report;
        if !r.has_baseline || r.changes.is_empty() {
            let text = if r.has_baseline { "Nothing changed in the last run." } else { "After a build runs, the files it changed are listed here." };
            ui.label(RichText::new(tr(text)).color(t.text_faint).size(theme::SMALL));
            return;
        }
        let (added, removed) = r.changes.iter().fold((0, 0), |(a, d), c| (a + c.added, d + c.removed));
        ui.label(
            RichText::new(trf("{n} file(s) · +{a} −{d}", &[("n", &r.changes.len().to_string()), ("a", &added.to_string()), ("d", &removed.to_string())]))
                .color(t.text_dim)
                .size(theme::SMALL),
        );
        ui.add_space(6.0);
        let rows: Vec<Change> = r.changes.iter().take(60).cloned().collect();
        for c in rows {
            row(app, ui, &c);
        }
        let total = app.changes.report.changes.len();
        if total > 60 {
            ui.label(RichText::new(trf("and {n} more", &[("n", &(total - 60).to_string())])).color(t.text_faint).size(theme::CAPTION));
        }
    });
}

fn row(app: &mut CraftApp, ui: &mut Ui, c: &Change) {
    let t = Tokens::get(ui.ctx());
    let (letter, color) = match c.kind {
        Kind::Added => ("A", t.success),
        Kind::Modified => ("M", t.warning),
        Kind::Deleted => ("D", t.danger),
    };
    let open = app.changes.open.contains(&c.path);
    let resp = ui
        .horizontal(|ui| {
            ui.label(RichText::new(letter).font(theme::mono(theme::CAPTION)).color(color));
            ui.add(egui::Label::new(RichText::new(&c.path).size(theme::SMALL).color(t.text)).truncate().sense(egui::Sense::click()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if c.binary {
                    ui.label(RichText::new(tr("binary")).color(t.text_faint).size(theme::CAPTION));
                } else {
                    ui.label(RichText::new(format!("−{}", c.removed)).color(t.danger).size(theme::CAPTION));
                    ui.label(RichText::new(format!("+{}", c.added)).color(t.success).size(theme::CAPTION));
                }
            });
        })
        .response
        .interact(egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if resp.clicked() && !c.binary {
        if open {
            app.changes.open.remove(&c.path);
        } else {
            app.changes.open.insert(c.path.clone());
            if c.lines.is_empty() {
                app.load_changes(Some(c.path.clone()));
            }
        }
    }
    if open && !c.binary {
        diff(ui, c);
    }
}

fn diff(ui: &mut Ui, c: &Change) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::new().fill(t.code_bg).corner_radius(t.radius_sm).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if c.lines.is_empty() {
            ui.label(RichText::new(tr("Looking…")).color(t.text_faint).size(theme::CAPTION));
            return;
        }
        egui::ScrollArea::both().max_height(260.0).auto_shrink([false, true]).show(ui, |ui| {
            for l in &c.lines {
                let (mark, color) = match l.op {
                    '+' => ('+', t.success),
                    '-' => ('−', t.danger),
                    '@' => ('⋯', t.text_faint),
                    _ => (' ', t.text_dim),
                };
                ui.add(egui::Label::new(RichText::new(format!("{mark} {}", l.text)).font(theme::mono(theme::CAPTION)).color(color)).extend());
            }
        });
        if c.truncated {
            ui.label(RichText::new(tr("Only the first lines are shown.")).color(t.text_faint).size(theme::CAPTION));
        }
    });
    ui.add_space(4.0);
}
