//! Choosing files: the system's Open panel (XDG desktop portal) when there is one, and a
//! small built-in chooser otherwise.

use crate::theme::Theme;
use eframe::egui::{self, Align2, Color32, FontFamily, FontId, Sense, Stroke, Vec2};
use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Purpose {
    /// Attach documents to the selected note
    Attach,
    /// Scan / import documents into a new note
    Scan,
    /// Import Markdown or text files as notes
    Import,
    /// Open a Markdown file on its own (read / edit it in place)
    OpenFile,
}

pub struct FilePicker {
    /// the built-in chooser is showing
    pub open: bool,
    pub purpose: Purpose,
    /// the system's Open panel is showing
    panel: Option<std::sync::mpsc::Receiver<crate::portal::Answer>>,
    dir: PathBuf,
    selected: BTreeSet<PathBuf>,
    exts: Vec<&'static str>,
    error: Option<String>,
}

impl Default for FilePicker {
    fn default() -> Self {
        Self {
            open: false,
            panel: None,
            purpose: Purpose::Attach,
            dir: dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")),
            selected: BTreeSet::new(),
            exts: Vec::new(),
            error: None,
        }
    }
}

impl FilePicker {
    /// Ask for files: the system's Open panel when there is one, the built-in chooser otherwise.
    pub fn start(&mut self, ctx: &egui::Context, purpose: Purpose) {
        self.start_builtin(purpose);
        if crate::portal::available() {
            self.open = false;
            let label = match purpose {
                Purpose::Import | Purpose::OpenFile => crate::i18n::tr("Fichiers Markdown").to_string(),
                _ => crate::i18n::tr("Images ou PDF").to_string(),
            };
            let ask = crate::portal::Ask::Open { multiple: purpose != Purpose::OpenFile, filter: (label, self.exts.clone()) };
            self.panel = Some(crate::portal::ask(ctx, self.title().to_string(), ask, Some(self.dir.clone())));
        }
    }

    /// The panel or the chooser is showing.
    pub fn busy(&self) -> bool {
        self.open || self.panel.is_some()
    }

    /// The built-in chooser only (screenshots, or when no system panel answers).
    pub fn start_builtin(&mut self, purpose: Purpose) {
        self.open = true;
        self.panel = None;
        self.purpose = purpose;
        self.selected.clear();
        self.error = None;
        self.exts = match purpose {
            Purpose::Import | Purpose::OpenFile => vec!["md", "markdown", "mdown", "mkd", "txt", "text"],
            _ => {
                let mut v: Vec<&'static str> = crate::attach::IMAGE_EXT.to_vec();
                v.extend_from_slice(crate::attach::DOC_EXT);
                v
            }
        };
        let start = match purpose {
            Purpose::Import | Purpose::OpenFile => dirs::document_dir(),
            _ => dirs::picture_dir().or_else(dirs::download_dir),
        };
        if let Some(d) = start.filter(|d| d.is_dir()) {
            self.dir = d;
        }
    }

    fn title(&self) -> &'static str {
        match self.purpose {
            Purpose::Attach => crate::i18n::tr("Joindre un document à la note"),
            Purpose::Scan => crate::i18n::tr("Numériser un document"),
            Purpose::Import => crate::i18n::tr("Importer des notes"),
            Purpose::OpenFile => crate::i18n::tr("Ouvrir un fichier Markdown"),
        }
    }

    fn accepts(&self, p: &std::path::Path) -> bool {
        p.extension()
            .and_then(|e| e.to_str())
            .map(|e| self.exts.contains(&e.to_lowercase().as_str()))
            .unwrap_or(false)
    }

    /// Draws the chooser over the window. Returns the chosen files once confirmed.
    pub fn show(&mut self, ctx: &egui::Context, t: &Theme) -> Option<Vec<PathBuf>> {
        if let Some(rx) = &self.panel {
            match rx.try_recv() {
                Ok(crate::portal::Answer::Files(files)) => {
                    self.panel = None;
                    let files: Vec<PathBuf> = files.into_iter().filter(|p| self.accepts(p)).collect();
                    return (!files.is_empty()).then_some(files);
                }
                Ok(crate::portal::Answer::Cancelled) => {
                    self.panel = None;
                    return None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return None,
                // no system panel answered: the built-in chooser takes over
                Ok(crate::portal::Answer::Unavailable) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.panel = None;
                    self.open = true;
                }
            }
        }
        if !self.open {
            return None;
        }
        let mut result = None;
        let screen = ctx.content_rect();
        // dim the app behind
        // (the app dims itself behind this window)
        let size = Vec2::new((screen.width() - 80.0).min(720.0), (screen.height() - 80.0).min(560.0));
        let title = self.title();
        egui::Modal::new(egui::Id::unique("file-picker"))
            .backdrop_color(Color32::from_black_alpha(120))
            .show(ctx, |ui| {
                ui.set_width(size.x);
                ui.set_height(size.y);
                ui.label(egui::RichText::new(title).font(FontId::new(18.0, FontFamily::Name("bold".into()))).color(t.text));
                ui.add_space(6.0);
                // quick places
                ui.horizontal_wrapped(|ui| {
                    let places = [
                        (crate::i18n::tr("🏠 Dossier personnel"), dirs::home_dir()),
                        (crate::i18n::tr("📄 Documents"), dirs::document_dir()),
                        (crate::i18n::tr("🖼 Images"), dirs::picture_dir()),
                        (crate::i18n::tr("⬇ Téléchargements"), dirs::download_dir()),
                        (crate::i18n::tr("🖥 Bureau"), dirs::desktop_dir()),
                    ];
                    for (label, p) in places {
                        if let Some(p) = p.filter(|p| p.is_dir()) {
                            if ui.selectable_label(self.dir == p, label).clicked() {
                                self.dir = p;
                            }
                        }
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button(crate::i18n::tr("⬆ Dossier parent")).clicked() {
                        if let Some(p) = self.dir.parent() {
                            self.dir = p.to_path_buf();
                        }
                    }
                    ui.label(egui::RichText::new(self.dir.display().to_string()).color(t.muted).size(12.5));
                });
                ui.separator();
                let mut entries: Vec<(String, PathBuf, bool)> = match std::fs::read_dir(&self.dir) {
                    Ok(rd) => rd
                        .filter_map(|e| e.ok())
                        .filter_map(|e| {
                            let name = e.file_name().to_string_lossy().to_string();
                            if name.starts_with('.') {
                                return None;
                            }
                            let p = e.path();
                            let is_dir = p.is_dir();
                            (is_dir || self.accepts(&p)).then_some((name, p, is_dir))
                        })
                        .collect(),
                    Err(e) => {
                        self.error = Some(crate::i18n::trf("Dossier illisible : {e}", &[("e", &e as &dyn std::fmt::Display)]));
                        Vec::new()
                    }
                };
                entries.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));
                let list_h = ui.available_height() - 52.0;
                egui::ScrollArea::vertical().max_height(list_h).auto_shrink([false, false]).show(ui, |ui| {
                    if entries.is_empty() {
                        ui.add_space(20.0);
                        ui.label(egui::RichText::new(crate::i18n::tr("Aucun fichier compatible ici")).color(t.muted));
                    }
                    for (name, p, is_dir) in entries {
                        let sel = self.selected.contains(&p);
                        let w = ui.available_width();
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 28.0), Sense::click());
                        if sel {
                            ui.painter().rect_filled(r, 6.0, t.selection);
                        } else if resp.hovered() {
                            ui.painter().rect_filled(r, 6.0, t.list_selected);
                        }
                        if is_dir {
                            // small painted folder
                            let c = r.left_center() + Vec2::new(17.0, 1.0);
                            let body = egui::Rect::from_center_size(c, Vec2::new(16.0, 11.0));
                            ui.painter().rect_filled(egui::Rect::from_min_size(body.min - Vec2::new(0.0, 3.0), Vec2::new(7.0, 4.0)), 1.5, t.accent);
                            ui.painter().rect_filled(body, 2.0, t.accent);
                        } else {
                            let icon = if name.to_lowercase().ends_with(".pdf") { "PDF" } else if matches!(self.purpose, Purpose::Import | Purpose::OpenFile) { "MD" } else { "IMG" };
                            ui.painter().text(r.left_center() + Vec2::new(17.0, 0.0), Align2::CENTER_CENTER, icon, FontId::new(9.0, FontFamily::Name("bold".into())), t.accent_text);
                        }
                        ui.painter().text(r.left_center() + Vec2::new(34.0, 0.0), Align2::LEFT_CENTER, &name, FontId::proportional(14.0), t.text);
                        if resp.double_clicked() && !is_dir {
                            self.selected.insert(p.clone());
                            result = Some(self.selected.iter().cloned().collect());
                        } else if resp.clicked() {
                            if is_dir {
                                self.dir = p;
                            } else if !self.selected.remove(&p) {
                                self.selected.insert(p);
                            }
                        }
                    }
                });
                if let Some(e) = &self.error {
                    ui.colored_label(Color32::from_rgb(0xC8, 0x38, 0x2E), e);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    let hint = match self.purpose {
                        Purpose::Import => crate::i18n::tr("Fichiers .md ou .txt"),
                        Purpose::OpenFile => crate::i18n::tr("Le fichier s'ouvre tel quel : vos modifications y sont enregistrées."),
                        _ => crate::i18n::tr("Images (photos, scans) ou PDF. Le texte sera lu pour la recherche."),
                    };
                    ui.label(egui::RichText::new(hint).size(12.0).color(t.muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let n = self.selected.len();
                        let label = egui::RichText::new(crate::i18n::trf("Ajouter ({n})", &[("n", &n as &dyn std::fmt::Display)]))
                            .font(FontId::new(14.0, FontFamily::Name("bold".into())))
                            .color(if n > 0 { Color32::WHITE } else { t.muted });
                        let b = egui::Button::new(label).fill(if n > 0 { t.accent } else { t.code_bg }).stroke(Stroke::NONE);
                        if ui.add_enabled(n > 0, b).clicked() {
                            result = Some(self.selected.iter().cloned().collect());
                        }
                        if ui.button(crate::i18n::tr("Annuler")).clicked() {
                            self.open = false;
                        }
                    });
                });
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.open = false;
        }
        if result.is_some() {
            self.open = false;
        }
        result
    }
}
