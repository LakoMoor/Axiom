//! Agent mode: a general coding agent (OpenCode or another agent app set up in Settings → AI)
//! works in a folder the person picks, on tasks they type. It is the ordinary "tell it what to do
//! in this folder" mode, beside the study-and-rebuild flow. The agent app does the work; we start
//! it through the AI hub (so it runs under the process rules), show its feed and let the person
//! stop it.

use crate::CraftApp;
use crate::chat;
use crate::i18n::tr;
use crate::prompt_box::{self, PromptBox};
use crate::state::ChatItem;
use crate::theme::{self, Tokens};
use crate::widgets;
use axiom_ai::Conversation;
use egui::{RichText, Ui};
use std::path::PathBuf;

/// What agent mode holds: the folder, the feed and the running conversation.
#[derive(Default)]
pub struct AgentMode {
    pub folder: Option<PathBuf>,
    pub items: Vec<ChatItem>,
    pub input: String,
    conversation: Option<Box<dyn Conversation>>,
}

impl AgentMode {
    /// Work in another folder: the old conversation belongs to the old one, so it ends.
    pub fn set_folder(&mut self, folder: PathBuf) {
        if self.folder.as_ref() != Some(&folder) {
            self.stop();
            self.conversation = None;
            self.items.clear();
            self.folder = Some(folder);
        }
    }

    pub fn busy(&self) -> bool {
        self.conversation.as_ref().is_some_and(|c| c.is_busy())
    }

    pub fn stop(&mut self) {
        if let Some(c) = self.conversation.as_mut() {
            c.stop();
        }
    }

    /// Fold what the agent reported since the last frame into the feed.
    pub fn poll(&mut self) {
        let Some(c) = self.conversation.as_mut() else { return };
        for e in c.poll() {
            chat::apply_event(&mut self.items, e);
        }
    }
}

impl CraftApp {
    /// Give the agent a task in the chosen folder, starting it on the first one.
    pub fn agent_send(&mut self, text: &str) {
        let text = text.trim().to_string();
        let Some(folder) = self.agent.folder.clone() else { return };
        if text.is_empty() {
            return;
        }
        self.agent.items.push(ChatItem::User(text.clone()));
        if self.agent.conversation.is_none() {
            match self.hub.start_agent(None, folder, self.beginner()) {
                Ok(c) => self.agent.conversation = Some(c),
                Err(e) => {
                    self.agent.items.push(ChatItem::Error { message: e.message, remediation: e.remediation, log: None });
                    return;
                }
            }
        }
        if let Some(c) = self.agent.conversation.as_mut() {
            c.send(&text);
        }
    }
}

pub fn show(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let width = ui.available_width();
    let max_w = width.min(760.0);
    let pad = ((width - max_w) / 2.0).max(0.0);
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(pad + 8.0);
        ui.label(theme::heading(tr("Agent"), theme::TITLE));
    });
    ui.horizontal(|ui| {
        ui.add_space(pad + 8.0);
        match app.agent.folder.clone() {
            Some(f) => {
                ui.label(RichText::new(f.display().to_string()).color(t.text_dim));
            }
            None => {
                ui.label(RichText::new(tr("Pick a folder and tell the agent what to do in it.")).color(t.text_dim));
            }
        }
        if widgets::secondary(ui, Some("folder-open"), if app.agent.folder.is_some() { "Change folder" } else { "Choose a folder…" }).clicked() {
            app.pick(crate::Pick::AgentFolder);
        }
    });
    ui.add_space(8.0);
    let busy = app.agent.busy();
    let composer_h = 110.0;
    let feed_h = (ui.available_height() - composer_h).max(60.0);
    ui.allocate_ui(egui::vec2(width, feed_h), |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(pad);
                ui.vertical(|ui| {
                    ui.set_width(max_w);
                    if app.agent.items.is_empty() {
                        ui.label(RichText::new(tr("It can read and change files in the folder, and run commands there.")).color(t.text_faint));
                    }
                    chat::feed(ui, &app.agent.items, app.beginner());
                    if busy && !matches!(app.agent.items.last(), Some(ChatItem::Assistant { streaming: true, .. })) {
                        widgets::thinking(ui);
                    }
                });
            });
        });
    });
    ui.horizontal_top(|ui| {
        ui.add_space(pad);
        ui.allocate_ui(egui::vec2(max_w, 0.0), |ui| {
            let ready = app.agent.folder.is_some();
            let hint = if ready { tr("What should the agent do?") } else { tr("Choose a folder first") };
            let out = prompt_box::show(
                ui,
                &mut app.agent.input,
                &[],
                PromptBox {
                    name: "Give the agent a task",
                    hint,
                    rows: (1, 6),
                    attach: None,
                    stop_button: busy.then_some("Stop"),
                    send: ready && !busy,
                    recall: false,
                    history: None,
                    font_size: 15.0,
                    choices: None,
                    deep: None,
                },
            );
            if out.stop {
                app.agent.stop();
            }
            if out.send && ready && !busy {
                let text = std::mem::take(&mut app.agent.input);
                app.agent_send(&text);
            }
        });
    });
}
