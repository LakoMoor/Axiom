//! A file browser inside the app, for a computer with no file dialog the app can open: a phone
//! (Android's own picker needs Java, which the app doesn't carry). It lists folders and files from
//! the phone's storage, and what is chosen comes back as the same `Done::Picked` a dialog gives.

use crate::i18n::tr;
use crate::theme::{self, Tokens};
use crate::{CraftApp, Done, Pick, icons, widgets};
use egui::{RichText, Vec2};
use std::path::{Path, PathBuf};

/// One line of a listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
}

/// What is open: the folder shown, its lines, and (for pictures) what is ticked so far.
#[derive(Debug, Clone, PartialEq)]
pub struct FileBrowser {
    pub pick: Pick,
    pub dir: PathBuf,
    pub entries: Vec<Entry>,
    /// Why the folder couldn't be read, in plain words.
    pub error: Option<String>,
    pub ticked: Vec<PathBuf>,
}

/// What a dialog of this kind asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wants {
    File,
    Folder,
    Files,
}

pub fn wants(pick: Pick) -> Wants {
    match pick {
        Pick::OpenFile | Pick::TargetFile | Pick::AgentProgram => Wants::File,
        Pick::Pictures | Pick::Attach => Wants::Files,
        Pick::OpenFolder | Pick::GhidraDir | Pick::TargetFolder | Pick::ProjectsDir | Pick::ProjectFolder | Pick::ExtraProjectsDir | Pick::AgentFolder => {
            Wants::Folder
        }
    }
}

fn title(pick: Pick) -> String {
    tr(match pick {
        Pick::OpenFile => "Open an app or file",
        Pick::OpenFolder => "Open a folder or app bundle",
        Pick::GhidraDir => "Where is Ghidra installed?",
        Pick::TargetFile => "Pick the app, game or file to rebuild",
        Pick::TargetFolder => "Pick the app or game folder",
        Pick::ProjectsDir => "Where should new projects go?",
        Pick::ProjectFolder => "Pick the folder for this project",
        Pick::ExtraProjectsDir => "Pick a folder with projects in it",
        Pick::AgentProgram => "Where is the program?",
        Pick::AgentFolder => "Pick the folder the agent works in",
        Pick::Pictures => "Pictures of the original",
        Pick::Attach => "Attach pictures",
    })
}

/// Where a browser starts: the phone's shared storage when there is some, else the app's own folder.
fn start_dir() -> PathBuf {
    ["/storage/emulated/0", "/sdcard"].iter().map(PathBuf::from).find(|p| p.is_dir()).unwrap_or_else(|| PathBuf::from("."))
}

/// A folder's lines: folders first, then files, each in name order, hidden ones left out. An
/// unreadable folder says so rather than showing nothing.
pub fn list(dir: &Path) -> Result<Vec<Entry>, String> {
    let read = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    let mut entries: Vec<Entry> = read
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            (!name.starts_with('.')).then(|| Entry { name, is_dir: e.path().is_dir() })
        })
        .collect();
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(entries)
}

fn load(pick: Pick, dir: PathBuf, ticked: Vec<PathBuf>) -> FileBrowser {
    let (entries, error) = match list(&dir) {
        Ok(entries) => (entries, None),
        Err(e) => (Vec::new(), Some(e)),
    };
    FileBrowser { pick, dir, entries, error, ticked }
}

/// Open the browser for `pick`.
pub fn open(app: &mut CraftApp, pick: Pick) {
    app.state.browser = Some(load(pick, start_dir(), Vec::new()));
}

/// Draw the browser when one is open, and hand a choice back like a file dialog does.
pub fn show(app: &mut CraftApp, ctx: &egui::Context) {
    let Some(mut b) = app.state.browser.take() else { return };
    let t = Tokens::get(ctx);
    let wants = wants(b.pick);
    let mut cancel = false;
    let mut go: Option<PathBuf> = None;
    let mut finish: Option<Vec<PathBuf>> = None;
    let resp = egui::Modal::new(egui::Id::new("file-browser"))
        .frame(egui::Frame::new().fill(t.card).corner_radius(t.r_lg()).inner_margin(egui::Margin::same(16)).stroke(egui::Stroke::new(1.0, t.card_border)))
        .show(ctx, |ui| {
            let width = if crate::phone::is_phone(ctx) { (ctx.content_rect().width() - 2.0 * 16.0 - 24.0).max(240.0) } else { 560.0 };
            ui.set_width(width);
            ui.label(theme::heading(title(b.pick), theme::SECTION));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if b.dir.parent().is_some() && icons::button(ui, "chevron-up", 36.0, false, "Up").clicked() {
                    go = b.dir.parent().map(Path::to_path_buf);
                }
                ui.add(egui::Label::new(RichText::new(b.dir.display().to_string()).color(t.text_dim).size(theme::SMALL)).wrap());
            });
            ui.add_space(6.0);
            let height = (ctx.content_rect().height() - 300.0).clamp(200.0, 520.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(height).min_scrolled_height(height).show(ui, |ui| {
                if let Some(e) = &b.error {
                    ui.add(egui::Label::new(RichText::new(tr("Axiom can't read this folder. On a phone, allow \"All files access\" for Axiom in the system Settings (Apps, Axiom, Permissions).")).color(t.danger)).wrap());
                    ui.add(egui::Label::new(RichText::new(e).color(t.text_faint).size(theme::CAPTION)).wrap());
                } else if b.entries.is_empty() {
                    ui.label(RichText::new(tr("This folder is empty.")).color(t.text_faint));
                    // Without "All files access" Android lists only the person's own media, so a
                    // folder that looks empty may just be hidden.
                    #[cfg(target_os = "android")]
                    ui.add(
                        egui::Label::new(
                            RichText::new(tr("Axiom can't read this folder. On a phone, allow \"All files access\" for Axiom in the system Settings (Apps, Axiom, Permissions)."))
                                .color(t.text_faint)
                                .size(theme::CAPTION),
                        )
                        .wrap(),
                    );
                }
                for e in &b.entries {
                    let path = b.dir.join(&e.name);
                    let ticked = b.ticked.contains(&path);
                    let (rect, r) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), egui::Sense::click());
                    if ticked {
                        ui.painter().rect_filled(rect, t.radius, t.accent_soft);
                    } else if r.hovered() {
                        ui.painter().rect_filled(rect, t.radius, t.hover);
                    }
                    icons::paint(ui, egui::Rect::from_center_size(egui::pos2(rect.left() + 20.0, rect.center().y), Vec2::splat(18.0)), if e.is_dir { "folder" } else { "file" }, 18.0, if e.is_dir { t.accent } else { t.icon });
                    ui.painter().text(
                        egui::pos2(rect.left() + 40.0, rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        &e.name,
                        theme::medium(theme::SMALL),
                        t.text,
                    );
                    if r.clicked() {
                        if e.is_dir {
                            go = Some(path);
                        } else {
                            match wants {
                                Wants::File => finish = Some(vec![path]),
                                Wants::Files => {
                                    if ticked {
                                        b.ticked.retain(|p| *p != path);
                                    } else {
                                        b.ticked.push(path);
                                    }
                                }
                                Wants::Folder => {}
                            }
                        }
                    }
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if widgets::secondary(ui, None, "Cancel").clicked() {
                    cancel = true;
                }
                match wants {
                    Wants::Folder => {
                        if widgets::primary(ui, "Use this folder").clicked() {
                            finish = Some(vec![b.dir.clone()]);
                        }
                    }
                    Wants::Files => {
                        if widgets::primary_icon(ui, None, "Done", !b.ticked.is_empty()).clicked() {
                            finish = Some(b.ticked.clone());
                        }
                    }
                    Wants::File => {}
                }
            });
        });
    if let Some(dir) = go {
        b = load(b.pick, dir, std::mem::take(&mut b.ticked));
    }
    if let Some(paths) = finish {
        let pick = b.pick;
        app.jobs.spawn("Choose a file", false, move |_| {
            Ok(match pick {
                Pick::Pictures => Done::Pictures(paths),
                Pick::Attach => Done::ToAttach(paths),
                _ => Done::Picked { pick, path: paths.into_iter().next() },
            })
        });
        return;
    }
    if cancel || resp.should_close() {
        return;
    }
    app.state.browser = Some(b);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_come_first_and_hidden_ones_are_left_out() {
        let dir = tempfile::tempdir().unwrap();
        for f in ["b.txt", "A.txt", ".hidden"] {
            std::fs::write(dir.path().join(f), b"x").unwrap();
        }
        std::fs::create_dir(dir.path().join("zdir")).unwrap();
        let names: Vec<(String, bool)> = list(dir.path()).unwrap().into_iter().map(|e| (e.name, e.is_dir)).collect();
        assert_eq!(names, vec![("zdir".into(), true), ("A.txt".into(), false), ("b.txt".into(), false)]);
    }

    #[test]
    fn a_missing_folder_is_an_error_not_an_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list(&dir.path().join("nope")).is_err());
        let b = load(Pick::OpenFile, dir.path().join("nope"), Vec::new());
        assert!(b.error.is_some() && b.entries.is_empty());
    }

    #[test]
    fn every_dialog_kind_says_what_it_wants() {
        assert_eq!(wants(Pick::OpenFile), Wants::File);
        assert_eq!(wants(Pick::AgentFolder), Wants::Folder);
        assert_eq!(wants(Pick::Attach), Wants::Files);
    }
}
