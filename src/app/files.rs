//! Standalone Markdown files: open any .md on the computer, read it, edit it in place.

use super::*;
use crate::i18n::{tr, trf};
use std::path::Path;
use std::time::SystemTime;

pub(super) struct OpenFile {
    pub path: PathBuf,
    pub text: String,
    saved: String,
    disk_mtime: Option<SystemTime>,
    last_edit: f64,
    last_check: f64,
    /// the file changed on disk while there were unsaved edits here
    conflict: bool,
    read_only: bool,
    /// too large to style: shown as plain text (and read-only)
    plain: bool,
    /// the file used Windows line endings (kept when saving)
    crlf: bool,
    error: Option<String>,
    cache: Option<(u64, egui::text::LayoutJob)>,
    /// styling of each line, so a keystroke only restyles its line
    lines: markdown::LineCache,
    uid: u64,
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// What to do once a save conflict is settled.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum AfterConflict {
    Stay,
    Close,
    Quit,
}

/// A file changed on disk while it had unsaved edits here: the user chooses what to keep.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct ConflictPrompt {
    pub uid: u64,
    pub then: AfterConflict,
}

#[cfg(unix)]
fn mode_of(m: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    m.permissions().mode() & 0o777
}

#[cfg(not(unix))]
fn mode_of(_: &std::fs::Metadata) -> u32 {
    0
}

/// Save a Markdown file.
///
/// Links: the path was resolved once, when the file was opened (`canonicalize` follows
/// links there), and saving writes to that resolved file only. If a link has appeared at
/// that path since, saving is refused: a link is never written through, nor silently
/// replaced.
///
/// The text goes to a temporary file in the same folder, is flushed to disk, then renamed
/// over the file, which keeps its permissions: a crash or a full disk never leaves a
/// half-written file. (The renamed file is a new inode: hard links keep the old text.)
fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mode = match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => return Err(tr("Le fichier a été remplacé par un lien : il n'a pas été enregistré.").to_string()),
        Ok(m) if !m.is_file() => return Err(tr("Ce n'est plus un fichier ordinaire : il n'a pas été enregistré.").to_string()),
        Ok(m) => mode_of(&m),
        // moved or deleted meanwhile: saving puts it back
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => 0o644,
        Err(e) => return Err(e.to_string()),
    };
    crate::safefs::write_atomic(path, bytes, mode).map_err(|e| e.to_string())
}

pub(super) fn is_markdown(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e.to_lowercase().as_str(), "md" | "markdown" | "mdown" | "mkd" | "txt" | "text"))
        .unwrap_or(false)
}

/// Files larger than this open read-only and without Markdown styling: editing and
/// styling them would freeze the window.
pub(super) const MAX_EDIT_BYTES: usize = 1024 * 1024;
/// Files larger than this are not opened at all.
pub(super) const MAX_OPEN_BYTES: u64 = 32 * 1024 * 1024;

/// Read a text file the user asked to open: a regular file (never a named pipe or a
/// device, which would hang the window), of a bounded size, in UTF-8.
fn read_text(path: &Path) -> Result<String, String> {
    let bytes = crate::safefs::read_regular(path, crate::safefs::Links::Follow, MAX_OPEN_BYTES)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    String::from_utf8(bytes).map_err(|_| trf("{name} n'est pas un fichier texte (UTF-8)", &[("name", &path.display())]))
}

impl OpenFile {
    pub fn open(path: &Path, uid: u64) -> Result<Self, String> {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let text = read_text(&path)?;
        // Windows line endings are edited as plain newlines and written back as they were
        let crlf = text.contains("\r\n");
        let text = text.replace("\r\n", "\n");
        let plain = text.len() > MAX_EDIT_BYTES;
        let read_only = plain || std::fs::metadata(&path).map(|m| m.permissions().readonly()).unwrap_or(false);
        Ok(Self {
            disk_mtime: mtime(&path),
            saved: text.clone(),
            text,
            path,
            last_edit: 0.0,
            last_check: 0.0,
            conflict: false,
            read_only,
            plain,
            crlf,
            error: None,
            cache: None,
            lines: Default::default(),
            uid,
        })
    }

    pub fn uid(&self) -> u64 {
        self.uid
    }

    pub fn name(&self) -> String {
        self.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
    }

    pub fn dir(&self) -> PathBuf {
        self.path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."))
    }

    pub fn dirty(&self) -> bool {
        self.text != self.saved
    }

    /// The file as a note, for exports.
    pub fn as_note(&self) -> model::Note {
        model::Note { id: self.uid, text: self.text.clone(), created: now(), modified: now(), pinned: false, archived: false, trashed: false, attachments: vec![] }
    }

    fn disk_text(&self) -> String {
        if self.crlf { self.text.replace('\n', "\r\n") } else { self.text.clone() }
    }

    /// The file changed on disk since it was read or saved here.
    pub fn changed_on_disk(&self) -> bool {
        mtime(&self.path) != self.disk_mtime
    }

    /// Unsaved edits that would overwrite changes made by another program.
    pub fn in_conflict(&mut self) -> bool {
        if self.dirty() && !self.read_only && (self.conflict || self.changed_on_disk()) {
            self.conflict = true;
        }
        self.conflict && self.dirty()
    }

    /// Keep this window's text in a new file next to the original ("name (my version).md").
    pub fn save_copy(&self) -> Result<PathBuf, String> {
        let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let ext = self.path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        let mine = tr("ma version");
        let mut last = String::new();
        for k in 1..100 {
            let name = if k == 1 { format!("{stem} ({mine}){ext}") } else { format!("{stem} ({mine} {k}){ext}") };
            match crate::safefs::write_new(&self.dir().join(&name), self.disk_text().as_bytes(), 0o644) {
                Ok(()) => return Ok(self.dir().join(name)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => last = e.to_string(),
            }
        }
        Err(last)
    }

    pub fn save(&mut self) -> Result<(), String> {
        if self.read_only {
            return Err(tr("Ce fichier est en lecture seule.").to_string());
        }
        write_file(&self.path, self.disk_text().as_bytes())?;
        self.saved = self.text.clone();
        self.disk_mtime = mtime(&self.path);
        self.conflict = false;
        self.error = None;
        Ok(())
    }

    fn reload(&mut self) {
        match read_text(&self.path) {
            Ok(t) => {
                self.crlf = t.contains("\r\n");
                let t = t.replace("\r\n", "\n");
                if t.len() > MAX_EDIT_BYTES {
                    self.plain = true;
                    self.read_only = true;
                }
                self.saved = t.clone();
                self.text = t;
                self.disk_mtime = mtime(&self.path);
                self.conflict = false;
                self.error = None;
                self.cache = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// Follow changes made by other programs.
    fn check_disk(&mut self, now_t: f64) {
        if now_t - self.last_check < 1.5 {
            return;
        }
        self.last_check = now_t;
        let m = mtime(&self.path);
        if m.is_none() {
            self.error = Some(tr("Le fichier a été déplacé ou supprimé.").to_string());
            return;
        }
        if m != self.disk_mtime {
            if self.dirty() {
                self.conflict = true;
            } else {
                self.reload();
            }
        }
    }

    /// Resolve a picture or link target written in the file: only inside the file's own
    /// folder (no absolute path, no "..", no link leading out of it).
    pub fn resolve(&self, target: &str) -> Option<PathBuf> {
        crate::safefs::confine(&self.dir(), target)
    }
}

/// Where "Ajouter à mes notes" puts a file when another window owns the library.
pub(super) fn inbox_dir() -> PathBuf {
    Library::data_dir().join("inbox")
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct InboxItem {
    pub text: String,
    pub base_dir: PathBuf,
}

impl GrassApp {
    pub(super) fn open_files(&mut self, ctx: &egui::Context, paths: Vec<PathBuf>) {
        for p in paths {
            let canon = p.canonicalize().unwrap_or_else(|_| p.clone());
            if let Some(i) = self.files.iter().position(|f| f.path == canon) {
                self.active_file = Some(i);
                continue;
            }
            self.file_seq += 1;
            match OpenFile::open(&p, self.file_seq) {
                Ok(f) => {
                    self.files.push(f);
                    self.active_file = Some(self.files.len() - 1);
                }
                Err(e) => self.notify(ctx, trf("Impossible d'ouvrir le fichier : {e}", &[("e", &e)])),
            }
        }
    }

    fn close_file(&mut self, ctx: &egui::Context, i: usize) {
        if i >= self.files.len() {
            return;
        }
        // never save silently over a file another program changed: ask first
        if self.files[i].in_conflict() {
            self.active_file = Some(i);
            self.conflict_prompt = Some(ConflictPrompt { uid: self.files[i].uid, then: AfterConflict::Close });
            return;
        }
        if self.files[i].dirty() {
            if let Err(e) = self.files[i].save() {
                self.notify(ctx, trf("Fichier non enregistré : {e}", &[("e", &e)]));
                return;
            }
        }
        self.files.remove(i);
        self.active_file = if self.files.is_empty() { None } else { Some(i.min(self.files.len() - 1)) };
    }

    /// Export an open file, at `at` (chosen in a Save panel) or in the default export folder.
    pub(super) fn write_file_export(&mut self, ctx: &egui::Context, i: usize, fmt: Format, at: Option<PathBuf>) {
        let f = &self.files[i];
        let note = f.as_note();
        let res = match at {
            Some(p) => export::export_note_at(&note, fmt, &p, &f.dir()),
            None => export::export_note_with(&note, fmt, &Self::export_dir(), &f.dir()),
        };
        let msg = match res {
            Ok(p) => trf("Exporté : {0}", &[("0", &p.display())]),
            Err(e) => trf("Export impossible : {e}", &[("e", &e)]),
        };
        self.notify(ctx, msg);
    }

    /// Before the window closes: files in conflict are settled one by one with the user.
    pub(super) fn check_quit(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) || self.quit_confirmed {
            return;
        }
        if let Some(i) = self.files.iter_mut().position(|f| f.in_conflict()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.show_settings = false;
            self.active_file = Some(i);
            self.conflict_prompt = Some(ConflictPrompt { uid: self.files[i].uid, then: AfterConflict::Quit });
        }
    }

    /// The "Keep mine / Keep theirs / Save as" question for a file in conflict.
    pub(super) fn conflict_window(&mut self, ctx: &egui::Context) {
        let Some(prompt) = self.conflict_prompt else { return };
        let Some(i) = self.files.iter().position(|f| f.uid == prompt.uid) else {
            self.conflict_prompt = None;
            return;
        };
        let t = self.theme();
        let name = self.files[i].name();
        #[derive(PartialEq)]
        enum Choice {
            Mine,
            Theirs,
            SaveAs,
            Cancel,
        }
        let mut choice = None;
        egui::Modal::new(Id::unique("file-conflict")).show(ctx, |ui| {
            ui.set_max_width(480.0);
            ui.label(egui::RichText::new(trf("« {name} » a été modifié par un autre programme pendant que vous le modifiiez ici.", &[("name", &name)])).font(FontId::new(16.0, FontFamily::Name("bold".into()))).color(t.text));
            ui.add_space(4.0);
            ui.label(egui::RichText::new(tr("Quelle version garder ? « Enregistrer sous… » garde les deux : votre version va dans un nouveau fichier.")).color(t.muted));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button(tr("Garder ma version")).clicked() {
                    choice = Some(Choice::Mine);
                }
                if ui.button(tr("Garder la leur")).clicked() {
                    choice = Some(Choice::Theirs);
                }
                if ui.button(tr("Enregistrer sous…")).clicked() {
                    choice = Some(Choice::SaveAs);
                }
                if ui.button(tr("Annuler")).clicked() {
                    choice = Some(Choice::Cancel);
                }
            });
        });
        let Some(choice) = choice else { return };
        self.conflict_prompt = None;
        let settled = match choice {
            Choice::Mine => match self.files[i].save() {
                Ok(()) => true,
                Err(e) => {
                    self.notify(ctx, trf("Fichier non enregistré : {e}", &[("e", &e)]));
                    false
                }
            },
            Choice::Theirs => {
                self.files[i].reload();
                true
            }
            Choice::SaveAs if self.screenshot.is_none() && crate::portal::available() => {
                let f = &self.files[i];
                let rx = crate::portal::ask(ctx, tr("Enregistrer sous…").to_string(), crate::portal::Ask::Save { name: f.name() }, Some(f.dir()));
                self.save_as_panel = Some((prompt, rx));
                false
            }
            Choice::SaveAs => match self.files[i].save_copy() {
                Ok(p) => {
                    self.notify(ctx, trf("Votre version est enregistrée dans {0}", &[("0", &p.display())]));
                    self.files[i].reload();
                    true
                }
                Err(e) => {
                    self.notify(ctx, trf("Fichier non enregistré : {e}", &[("e", &e)]));
                    false
                }
            },
            Choice::Cancel => false,
        };
        if settled {
            self.after_conflict(ctx, i, prompt.then);
        }
    }

    /// The answer of the Save panel opened by "Save as…" in a conflict.
    pub(super) fn poll_save_as(&mut self, ctx: &egui::Context) {
        let Some((_, rx)) = &self.save_as_panel else { return };
        let answer = match rx.try_recv() {
            Ok(a) => a,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => crate::portal::Answer::Unavailable,
        };
        let Some((prompt, _)) = self.save_as_panel.take() else { return };
        let Some(i) = self.files.iter().position(|f| f.uid == prompt.uid) else { return };
        let saved = match answer {
            crate::portal::Answer::Files(p) if !p.is_empty() => {
                let text = self.files[i].disk_text();
                crate::safefs::write_atomic(&p[0], text.as_bytes(), 0o644).map(|_| p[0].clone()).map_err(|e| e.to_string())
            }
            crate::portal::Answer::Unavailable => self.files[i].save_copy(),
            // cancelled: the question comes back
            _ => {
                self.conflict_prompt = Some(prompt);
                return;
            }
        };
        match saved {
            Ok(p) => {
                self.notify(ctx, trf("Votre version est enregistrée dans {0}", &[("0", &p.display())]));
                self.files[i].reload();
                self.after_conflict(ctx, i, prompt.then);
            }
            Err(e) => {
                self.notify(ctx, trf("Fichier non enregistré : {e}", &[("e", &e)]));
                self.conflict_prompt = Some(prompt);
            }
        }
    }

    fn after_conflict(&mut self, ctx: &egui::Context, i: usize, then: AfterConflict) {
        match then {
            AfterConflict::Stay => {}
            AfterConflict::Close => self.close_file(ctx, i),
            AfterConflict::Quit => {
                if let Some(next) = self.files.iter_mut().position(|f| f.in_conflict()) {
                    self.active_file = Some(next);
                    self.conflict_prompt = Some(ConflictPrompt { uid: self.files[next].uid, then: AfterConflict::Quit });
                } else {
                    self.quit_confirmed = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    /// Last chance when the app ends without a chance to ask (logout, a signal):
    /// edits in conflict are kept in a copy next to the file, never written over it.
    pub(super) fn save_files_on_exit(&mut self) {
        for f in &mut self.files {
            if !f.dirty() || f.read_only {
                continue;
            }
            if f.in_conflict() {
                let _ = f.save_copy();
            } else {
                let _ = f.save();
            }
        }
    }

    /// Save edited files a moment after typing stops, and follow outside changes.
    pub(super) fn tick_files(&mut self, ctx: &egui::Context) {
        let now_t = ctx.input(|i| i.time);
        let mut errors = Vec::new();
        for f in &mut self.files {
            f.check_disk(now_t);
            if f.dirty() && !f.conflict && !f.read_only && now_t - f.last_edit > 1.0 {
                // changed on disk since the last check: never write over it unasked
                if f.in_conflict() {
                    continue;
                }
                if let Err(e) = f.save() {
                    f.error = Some(e.clone());
                    errors.push(e);
                }
            }
        }
        if !self.files.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(1500));
        }
        for e in errors {
            self.notify(ctx, trf("Fichier non enregistré : {e}", &[("e", &e)]));
        }
    }

    /// Copy a file into the library as a note (pictures next to it become attachments).
    pub(super) fn add_text_to_library(&mut self, ctx: &egui::Context, text: &str, base_dir: &Path) -> Option<u64> {
        let id = self.lib.new_note(String::new());
        let mut out = String::with_capacity(text.len());
        for line in text.split_inclusive('\n') {
            if let Some(name) = crate::model::image_ref(line) {
                // only pictures inside the file's own folder come along: the library is
                // synced, so a path to any other file on the computer is never copied in
                let picture = crate::safefs::confine(base_dir, name)
                    .filter(|p| attach::kind_of(p, crate::safefs::Links::Refuse).map(|k| k.is_picture()).unwrap_or(false));
                if let Some(p) = picture {
                    if let Ok(a) = attach::store(id, &p) {
                        let nl = if line.ends_with('\n') { "\n" } else { "" };
                        out.push_str(&format!("![]({}){nl}", a.name));
                        if let Some(n) = self.lib.get_mut(id) {
                            n.attachments.push(a);
                        }
                        continue;
                    }
                }
            }
            out.push_str(line);
        }
        if let Some(n) = self.lib.get_mut(id) {
            n.text = out;
        }
        self.mark_dirty(ctx);
        Some(id)
    }

    fn add_file_to_library(&mut self, ctx: &egui::Context, i: usize) {
        let (text, dir, name) = {
            let f = &self.files[i];
            (f.text.clone(), f.dir(), f.name())
        };
        if self.persist {
            if let Some(id) = self.add_text_to_library(ctx, &text, &dir) {
                self.filter = Filter::All;
                self.selected = Some(id);
                self.notify(ctx, trf("« {name} » ajouté à vos notes", &[("name", &name)]));
            }
        } else if self.other_instance {
            // the window that owns the library picks it up within a few seconds
            let item = InboxItem { text, base_dir: dir };
            let path = inbox_dir().join(format!("{}.json", chrono::Local::now().format("%Y%m%d-%H%M%S%3f")));
            let ok = std::fs::create_dir_all(inbox_dir()).is_ok()
                && serde_json::to_string(&item).ok().map(|s| std::fs::write(&path, s).is_ok()).unwrap_or(false);
            let msg = if ok { trf("« {name} » envoyé à vos notes dans l'autre fenêtre", &[("name", &name)]) } else { tr("Impossible d'ajouter le fichier à vos notes").to_string() };
            self.notify(ctx, msg);
        }
    }

    /// Bring in files added from other windows.
    pub(super) fn poll_inbox(&mut self, ctx: &egui::Context) {
        if !self.persist {
            return;
        }
        let t = ctx.input(|i| i.time);
        if t - self.last_inbox_check < 3.0 {
            return;
        }
        self.last_inbox_check = t;
        let Ok(rd) = std::fs::read_dir(inbox_dir()) else { return };
        let mut added = 0;
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            if let Some(item) = std::fs::read_to_string(&p).ok().and_then(|s| serde_json::from_str::<InboxItem>(&s).ok()) {
                if self.add_text_to_library(ctx, &item.text, &item.base_dir).is_some() {
                    added += 1;
                }
            }
            let _ = std::fs::remove_file(&p);
        }
        if added > 0 {
            self.notify(ctx, trf("{n} fichier(s) ajouté(s) à vos notes", &[("n", &added)]));
        }
    }

    fn file_texture(&mut self, ctx: &egui::Context, p: &Path) -> Option<egui::TextureHandle> {
        let m = mtime(p);
        if let Some((when, tex)) = self.file_thumbs.get(p) {
            if *when == m {
                return tex.clone();
            }
        }
        let tex = attach::load_picture(p, crate::safefs::Links::Refuse).map(|img| {
            let small = img.thumbnail(1000, 1000).to_rgba8();
            let ci = egui::ColorImage::from_rgba_unmultiplied([small.width() as usize, small.height() as usize], small.as_raw());
            ctx.load_texture(format!("file-pic-{}", p.display()), ci, egui::TextureOptions::LINEAR)
        });
        self.file_thumbs.insert(p.to_path_buf(), (m, tex.clone()));
        tex
    }

    /// Insert pictures dropped on an open file: copied next to it, referenced relatively.
    pub(super) fn add_pictures_to_file(&mut self, ctx: &egui::Context, i: usize, pics: Vec<PathBuf>) {
        let dir = self.files[i].dir();
        for p in pics {
            let target = if p.parent() == Some(dir.as_path()) {
                p.clone()
            } else {
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let mut dst = dir.join(&name);
                let mut k = 2;
                while dst.exists() {
                    dst = dir.join(format!("{k}-{name}"));
                    k += 1;
                }
                if let Err(e) = std::fs::copy(&p, &dst) {
                    self.notify(ctx, trf("Impossible de joindre {0} : {e}", &[("0", &p.display()), ("e", &e)]));
                    continue;
                }
                dst
            };
            let rel = target.file_name().map(|n| n.to_string_lossy().replace(' ', "%20")).unwrap_or_default();
            let f = &mut self.files[i];
            if !f.text.ends_with('\n') && !f.text.is_empty() {
                f.text.push('\n');
            }
            f.text.push_str(&format!("![]({rel})\n"));
            f.last_edit = ctx.input(|x| x.time);
            f.cache = None;
        }
    }

    pub(super) fn file_view(&mut self, ui: &mut egui::Ui) {
        let t = self.theme();
        let ctx = ui.ctx().clone();
        let Some(i) = self.active_file.filter(|i| *i < self.files.len()) else {
            self.active_file = None;
            return;
        };
        // keyboard: save, close, reading/editing
        let (save_now, close_now) = ctx.input_mut(|inp| {
            (
                inp.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::S)),
                inp.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::W)),
            )
        });
        if save_now && self.files[i].in_conflict() {
            self.conflict_prompt = Some(ConflictPrompt { uid: self.files[i].uid, then: AfterConflict::Stay });
        } else if save_now {
            match self.files[i].save() {
                Ok(()) => self.notify(&ctx, tr("Fichier enregistré")),
                Err(e) => self.notify(&ctx, trf("Fichier non enregistré : {e}", &[("e", &e)])),
            }
        }
        if close_now {
            self.close_file(&ctx, i);
            return;
        }

        let mut close_tab: Option<usize> = None;
        let mut action_add = false;
        let mut export_fmt: Option<Format> = None;
        let mut toggle_reading = false;
        egui::Panel::top("file-top")
            .frame(egui::Frame::new().fill(t.list_bg).inner_margin(Margin::symmetric(12, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(tr("‹  Mes notes")).on_hover_text(tr("Revenir à la bibliothèque (les fichiers restent ouverts)")).clicked() {
                        self.active_file = None;
                    }
                    ui.add_space(8.0);
                    // tabs
                    for (k, f) in self.files.iter().enumerate() {
                        let mut label = f.name();
                        if f.dirty() {
                            label.push_str(" •");
                        }
                        let sel = k == i;
                        egui::Frame::new()
                            .fill(if sel { t.editor_bg } else { Color32::TRANSPARENT })
                            .stroke(Stroke::new(1.0, if sel { t.border } else { Color32::TRANSPARENT }))
                            .corner_radius(6.0)
                            .inner_margin(Margin::symmetric(6, 2))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let r = ui.add(egui::Label::new(egui::RichText::new(label).color(if sel { t.text } else { t.muted })).sense(Sense::click()));
                                    if r.on_hover_text(f.path.display().to_string()).clicked() {
                                        self.active_file = Some(k);
                                    }
                                    if ui.add(egui::Button::new(egui::RichText::new("✖").size(10.0).color(t.muted)).frame(false)).on_hover_text(tr("Fermer (Ctrl+W)")).clicked() {
                                        close_tab = Some(k);
                                    }
                                });
                            });
                    }
                    if ui.add(egui::Button::new(egui::RichText::new("+").size(16.0).color(t.accent_text)).frame(false)).on_hover_text(tr("Ouvrir un fichier Markdown (Ctrl+O)")).clicked() {
                        self.actions.push(Action::PickFiles(Purpose::OpenFile));
                    }
                });
                ui.add_space(4.0);
                let (path_s, dir, ro, dirty) = {
                    let f = &self.files[i];
                    (f.path.display().to_string(), f.dir(), f.read_only, f.dirty())
                };
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(path_s).size(12.0).color(t.muted));
                    let status = if ro {
                        tr("Lecture seule")
                    } else if dirty {
                        tr("Enregistrement…")
                    } else {
                        tr("Enregistré")
                    };
                    ui.label(egui::RichText::new(format!("· {status}")).size(12.0).color(t.muted));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.menu_button(tr("📤  Exporter"), |ui| {
                            for fmt in export::ALL {
                                if ui.button(fmt.label()).clicked() {
                                    export_fmt = Some(fmt);
                                    ui.close();
                                }
                            }
                        });
                        if ui.button(tr("📂  Dossier")).on_hover_text(tr("Ouvrir le dossier du fichier")).clicked() {
                            let _ = crate::platform::open_path(&dir);
                        }
                        let can_add = self.persist || self.other_instance;
                        if ui.add_enabled(can_add, egui::Button::new(tr("📥  Ajouter à mes notes"))).on_hover_text(tr("Copier ce fichier dans Grassfield Notes, avec ses images")).clicked() {
                            action_add = true;
                        }
                        let (label, tip) = if self.file_reading {
                            (tr("✏  Modifier"), tr("Passer en mode édition"))
                        } else {
                            (tr("📖  Lecture"), tr("Masquer les symboles Markdown et lire tranquillement"))
                        };
                        let b = egui::Button::new(egui::RichText::new(label).color(Color32::WHITE)).fill(t.accent).stroke(Stroke::NONE);
                        if ui.add_enabled(!ro || !self.file_reading, b).on_hover_text(tip).clicked() {
                            toggle_reading = true;
                        }
                    });
                });
            });
        if toggle_reading {
            self.file_reading = !self.file_reading;
            self.files[i].cache = None;
        }
        if let Some(k) = close_tab {
            self.close_file(&ctx, k);
            return;
        }
        if action_add {
            self.add_file_to_library(&ctx, i);
        }
        if let Some(fmt) = export_fmt {
            if self.screenshot.is_none() && crate::portal::available() && self.export_panel.is_none() {
                let name = export::file_name(&self.files[i].as_note(), fmt);
                let rx = crate::portal::ask(&ctx, tr("Exporter").to_string(), crate::portal::Ask::Save { name }, Some(Self::export_dir()));
                self.export_panel = Some((ExportTarget::File(self.files[i].uid), fmt, rx));
            } else {
                self.write_file_export(&ctx, i, fmt, None);
            }
        }

        // banners
        let (conflict, mut err, read_only) = {
            let f = &self.files[i];
            (f.conflict, f.error.clone(), f.read_only)
        };
        if err.is_none() && self.files[i].plain {
            err = Some(tr("Ce fichier est trop volumineux pour être modifié ici : il s'ouvre en lecture seule, sans mise en forme.").to_string());
        }
        if conflict || err.is_some() {
            egui::Frame::new().fill(t.highlight_bg.gamma_multiply(0.6)).inner_margin(Margin::symmetric(16, 8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    if conflict {
                        ui.label(egui::RichText::new(tr("Le fichier a été modifié par un autre programme.")).color(t.text));
                        if ui.button(tr("Recharger le fichier")).clicked() {
                            self.files[i].reload();
                        }
                        if ui.button(tr("Garder ma version")).clicked() {
                            let _ = self.files[i].save();
                        }
                    } else if let Some(e) = &err {
                        ui.label(egui::RichText::new(e).color(t.text));
                    }
                });
            });
        }

        // pictures referenced by the file
        let refs: Vec<String> = self.files[i]
            .text
            .lines()
            .filter_map(|l| crate::model::image_ref(l).map(|s| s.to_string()))
            .filter(|n| !n.starts_with("http://") && !n.starts_with("https://"))
            .collect();
        let mut pics: Vec<(String, egui::TextureHandle, bool)> = Vec::new();
        for name in refs {
            let Some(p) = self.files[i].resolve(&name) else { continue };
            if let Some(tex) = self.file_texture(&ctx, &p) {
                pics.push((name, tex, false));
            }
        }

        let settings = self.lib.settings.clone();
        let reading = self.file_reading || read_only;
        let mut style = MdStyle {
            images: Default::default(),
            hide_markup: reading,
            theme: t,
            size: settings.font_size,
            font: settings.editor_font,
            dim_markup: settings.dim_markup,
            spacing: settings.line_spacing,
        };
        let mut open_pic: Option<String> = None;
        let mut clicked_box: Option<usize> = None;
        let mut link_at: Option<usize> = None;
        let mut edited = false;
        let f = &mut self.files[i];
        let editor_id = Id::unique(("file-editor", f.uid));
        egui::ScrollArea::vertical()
            .id_salt(("file-scroll", f.uid))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let avail = ui.available_width();
                let pad = if avail < 520.0 { 16.0 } else { 40.0 };
                let w = settings.line_width.min(avail - 2.0 * pad).max(120.0);
                let side = ((avail - w) / 2.0).max(pad.min(avail / 2.0));
                ui.add_space(28.0);
                let mut img_key = 0u64;
                for (name, tex, _) in &pics {
                    let h = overlay::pic_size(tex, w).y + 16.0;
                    style.images.insert(name.clone(), h);
                    img_key = img_key.wrapping_mul(31).wrapping_add(h.to_bits() as u64 ^ name.len() as u64);
                }
                let style_key = (settings.theme, settings.font_size.to_bits(), settings.editor_font as u8, settings.dim_markup, settings.line_spacing.to_bits(), reading, img_key);
                ui.horizontal(|ui| {
                    ui.add_space(side);
                    let plain = f.plain;
                    let cache = &mut f.cache;
                    let lines = &mut f.lines;
                    let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
                        use std::hash::{Hash, Hasher};
                        let mut h = std::collections::hash_map::DefaultHasher::new();
                        buf.as_str().hash(&mut h);
                        wrap.to_bits().hash(&mut h);
                        style_key.hash(&mut h);
                        let key = h.finish();
                        let job = match cache {
                            Some((k, job)) if *k == key => job.clone(),
                            _ => {
                                let job = if plain { markdown::plain_job(buf.as_str(), &style, wrap) } else { markdown::layout_job_cached(buf.as_str(), &style, wrap, lines) };
                                *cache = Some((key, job.clone()));
                                job
                            }
                        };
                        ui.fonts_mut(|fo| fo.layout_job(job))
                    };
                    let min_h = (ui.available_height() - 40.0).max(200.0);
                    let out = if reading {
                        // selectable but not editable
                        let mut ro: &str = f.text.as_str();
                        egui::TextEdit::multiline(&mut ro)
                            .id(editor_id)
                            .frame(egui::Frame::NONE)
                            .desired_width(w)
                            .min_size(Vec2::new(w, min_h))
                            .layouter(&mut layouter)
                            .show(ui)
                    } else {
                        egui::TextEdit::multiline(&mut f.text)
                            .id(editor_id)
                            .frame(egui::Frame::NONE)
                            .desired_width(w)
                            .min_size(Vec2::new(w, min_h))
                            .lock_focus(true)
                            .hint_text(tr("Commencez à écrire…"))
                            .layouter(&mut layouter)
                            .show(ui)
                    };
                    let resp = out.response.response.clone();
                    if resp.changed() {
                        edited = true;
                    }
                    let galley = out.galley.clone();
                    let ov = overlay::paint(ui, &overlay::Input {
                        text: &f.text,
                        galley: &galley,
                        gpos: out.galley_pos,
                        clip: out.text_clip_rect,
                        resp: &resp,
                        width: w,
                        font_size: settings.font_size,
                        theme: &t,
                        pics: &pics,
                        editable: !reading,
                    });
                    clicked_box = ov.clicked_box;
                    open_pic = ov.open_pic.map(|p| p.0);
                    // links: a plain click while reading, Ctrl+click while editing
                    let cmd = ui.input(|x| x.modifiers.command);
                    if resp.clicked() && (reading || cmd) {
                        if let Some(p) = resp.interact_pointer_pos() {
                            let c = galley.cursor_from_pos(p - out.galley_pos);
                            link_at = Some(f.text.char_indices().nth(c.index.0).map(|(b, _)| b).unwrap_or(f.text.len()));
                        }
                    }
                });
                ui.add_space(40.0);
            });
        let now_t = ctx.input(|x| x.time);
        if let Some(line_start) = clicked_box {
            if Self::toggle_todo_at_line(&mut f.text, line_start) {
                edited = true;
            }
        }
        if edited {
            f.last_edit = now_t;
        }
        if let Some(p) = open_pic.and_then(|name| f.resolve(&name)) {
            if attach::opens_safely(&p, crate::safefs::Links::Refuse) {
                let _ = crate::platform::open_path(&p);
            } else {
                self.pending_open = Some(OpenRequest::File(p));
            }
        }
        if let Some(target) = link_at.and_then(|b| link_target(&f.text, b)) {
            let base = f.dir();
            match link_action(&base, &target) {
                LinkAction::OpenHere(p) => self.open_files(&ctx, vec![p]),
                LinkAction::OpenPicture(p) => {
                    let _ = crate::platform::open_path(&p);
                }
                LinkAction::Ask(req) => self.pending_open = Some(req),
                LinkAction::Unsupported => self.notify(&ctx, trf("Ce type de lien n'est pas pris en charge : {0}", &[("0", &target)])),
                LinkAction::Nothing => {}
            }
        }
    }
}

/// What a click on a link in a Markdown file does.
#[derive(Debug, PartialEq)]
enum LinkAction {
    /// a Markdown file inside the file's folder: opened in this window
    OpenHere(PathBuf),
    /// a picture inside the file's folder: opened with the default app
    OpenPicture(PathBuf),
    /// a web address, or a file elsewhere or of another type: shown to the user first
    Ask(OpenRequest),
    /// another scheme (ftp:, smb:, javascript:…)
    Unsupported,
    /// a missing file or an anchor in the same file
    Nothing,
}

fn link_action(base: &Path, target: &str) -> LinkAction {
    if crate::platform::is_web_link(target) {
        return LinkAction::Ask(OpenRequest::Link(target.to_string()));
    }
    if target.starts_with('#') {
        return LinkAction::Nothing;
    }
    if crate::platform::has_scheme(target) {
        return LinkAction::Unsupported;
    }
    let path_part = target.split('#').next().unwrap_or("");
    if let Some(p) = crate::safefs::confine(base, path_part) {
        if is_markdown(&p) && p.is_file() {
            return LinkAction::OpenHere(p);
        }
        if attach::opens_safely(&p, crate::safefs::Links::Refuse) {
            return LinkAction::OpenPicture(p);
        }
    }
    // anything else (absolute, "..", another type) is named to the user before it opens
    let decoded = path_part.replace("%20", " ");
    let p = if Path::new(&decoded).is_absolute() { PathBuf::from(&decoded) } else { base.join(&decoded) };
    if p.exists() {
        LinkAction::Ask(OpenRequest::File(p))
    } else {
        LinkAction::Nothing
    }
}

/// The target of a Markdown link `[text](target)`, `<url>`, bare URL or `[[name]]` under the byte offset.
fn link_target(text: &str, byte: usize) -> Option<String> {
    let byte = byte.min(text.len());
    let ls = text[..byte].rfind('\n').map(|x| x + 1).unwrap_or(0);
    let le = text[ls..].find('\n').map(|x| ls + x).unwrap_or(text.len());
    let line = &text[ls..le];
    let off = byte - ls;
    // [text](target)
    let mut start = 0;
    while let Some(s) = line[start..].find('[') {
        let s = start + s;
        if let Some(mid) = line[s..].find("](") {
            let mid = s + mid;
            if let Some(e) = line[mid + 2..].find(')') {
                let e = mid + 2 + e;
                if off >= s && off <= e {
                    let t = line[mid + 2..e].trim();
                    let t = t.split(" \"").next().unwrap_or(t).trim_matches(['<', '>']);
                    return Some(t.to_string());
                }
                start = e + 1;
                continue;
            }
        }
        start = s + 1;
    }
    // [[name]] -> name.md next to the file
    if let Some(s) = line.find("[[") {
        if let Some(e) = line[s..].find("]]") {
            if off >= s && off <= s + e + 2 {
                return Some(format!("{}.md", line[s + 2..s + e].trim()));
            }
        }
    }
    // bare urls
    for scheme in ["https://", "http://"] {
        if let Some(s) = line.find(scheme) {
            let e = line[s..].find(|c: char| c.is_whitespace() || c == ')').map(|x| s + x).unwrap_or(line.len());
            if off >= s && off <= e {
                return Some(line[s..e].to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_files_open_save_and_link() {
        let dir = std::env::temp_dir().join(format!("gf-mdfile-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("Lisez-moi.md");
        std::fs::write(&p, "# Titre\r\n\r\nVoir [l'autre](Autre%20page.md) et ![plan](img/plan.png \"Plan\")\r\n").unwrap();
        std::fs::write(dir.join("Autre page.md"), "# Autre\n").unwrap();
        let mut f = OpenFile::open(&p, 1).unwrap();
        assert!(!f.text.contains('\r'), "Windows line endings are normalised");
        assert_eq!(crate::model::image_ref("![plan](img/plan.png \"Plan\")"), Some("img/plan.png"));
        assert_eq!(crate::model::image_ref("![a](<mon image.png>)"), Some("mon image.png"));
        assert_eq!(f.resolve("Autre%20page.md"), Some(dir.canonicalize().unwrap().join("Autre page.md")));
        // nothing outside the file's folder
        assert_eq!(f.resolve("/etc/passwd"), None);
        assert_eq!(f.resolve("../Lisez-moi.md"), None);
        f.text.push_str("Ajout\n");
        f.save().unwrap();
        // the file came with Windows line endings and keeps them
        assert!(std::fs::read_to_string(&p).unwrap().ends_with("Ajout\r\n"));
        let text = "Voir [l'autre](Autre%20page.md) ici";
        assert_eq!(link_target(text, 8).as_deref(), Some("Autre%20page.md"));
        assert_eq!(link_target("lien https://example.org fin", 10).as_deref(), Some("https://example.org"));
        assert_eq!(link_target("voir [[Notes]]", 9).as_deref(), Some("Notes.md"));
    }

    #[cfg(unix)]
    #[test]
    fn saving_is_atomic_keeps_the_mode_and_never_follows_links() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("gf-save-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("note.md");
        std::fs::write(&p, "v1").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o640)).unwrap();
        let mut f = OpenFile::open(&p, 1).unwrap();
        f.text = "v2".into();
        f.save().unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "v2");
        assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o640, "mode kept");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no temporary file left");
        // a link appears where the file was: refused, the link's target untouched
        let target = dir.join("target.md");
        std::fs::write(&target, "cible").unwrap();
        std::fs::remove_file(&p).unwrap();
        std::os::unix::fs::symlink(&target, &p).unwrap();
        f.text = "v3".into();
        assert!(f.save().is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "cible");
        assert!(std::fs::symlink_metadata(&p).unwrap().file_type().is_symlink());
    }

    #[test]
    fn a_conflict_is_never_saved_silently() {
        let dir = std::env::temp_dir().join(format!("gf-conflict-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("note.md");
        std::fs::write(&p, "v1").unwrap();
        let mut f = OpenFile::open(&p, 1).unwrap();
        f.text = "mine".into();
        // another program writes the file (with a clearly different modification time)
        std::fs::write(&p, "theirs").unwrap();
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
        std::fs::File::options().write(true).open(&p).unwrap().set_modified(later).unwrap();
        assert!(f.in_conflict());
        let copy = f.save_copy().unwrap();
        assert_eq!(copy.file_name().unwrap().to_string_lossy(), "note (ma version).md");
        assert_eq!(std::fs::read_to_string(&copy).unwrap(), "mine");
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "theirs", "the other program's change is kept");
        // a second copy never overwrites the first
        let copy2 = f.save_copy().unwrap();
        assert_ne!(copy, copy2);
    }

    #[test]
    fn links_only_open_unasked_inside_the_folder() {
        let dir = std::env::temp_dir().join(format!("gf-links-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sous")).unwrap();
        std::fs::write(dir.join("autre.md"), "# a").unwrap();
        std::fs::write(dir.join("sous/photo.png"), b"\x89PNG\r\n\x1a\n....").unwrap();
        std::fs::write(dir.join("faux.png"), b"#!/bin/sh\necho pwned\n").unwrap();
        std::fs::write(dir.join("photo.desktop"), b"\x89PNG\r\n\x1a\n....").unwrap();
        std::fs::write(dir.join("script.sh"), b"#!/bin/sh\n").unwrap();
        let base = dir.canonicalize().unwrap();
        assert_eq!(link_action(&base, "autre.md"), LinkAction::OpenHere(base.join("autre.md")));
        assert_eq!(link_action(&base, "sous/photo.png"), LinkAction::OpenPicture(base.join("sous/photo.png")));
        assert_eq!(link_action(&base, "https://example.org/x"), LinkAction::Ask(OpenRequest::Link("https://example.org/x".into())));
        assert_eq!(link_action(&base, "mailto:a@b.c"), LinkAction::Ask(OpenRequest::Link("mailto:a@b.c".into())));
        assert_eq!(link_action(&base, "javascript:alert(1)"), LinkAction::Unsupported);
        assert_eq!(link_action(&base, "file:///etc/passwd"), LinkAction::Unsupported);
        assert_eq!(link_action(&base, "#titre"), LinkAction::Nothing);
        assert_eq!(link_action(&base, "absent.md"), LinkAction::Nothing);
        // a picture name with other content, a picture content with another name, a script
        for name in ["faux.png", "photo.desktop", "script.sh"] {
            assert_eq!(link_action(&base, name), LinkAction::Ask(OpenRequest::File(base.join(name))), "{name}");
        }
        // anything outside the folder is named first, even Markdown
        let outside = link_action(&base.join("sous"), "../autre.md");
        assert_eq!(outside, LinkAction::Ask(OpenRequest::File(base.join("sous").join("../autre.md"))));
        assert!(matches!(link_action(&base, "/etc/hostname"), LinkAction::Ask(_) | LinkAction::Nothing));
    }

    #[test]
    fn big_files_open_read_only_and_plain() {
        let dir = std::env::temp_dir().join(format!("gf-bigfile-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("gros.md");
        std::fs::write(&p, "*a ".repeat(MAX_EDIT_BYTES / 3 + 10)).unwrap();
        let f = OpenFile::open(&p, 1).unwrap();
        assert!(f.plain && f.read_only);
        let small = dir.join("petit.md");
        std::fs::write(&small, "# ok\n").unwrap();
        let f = OpenFile::open(&small, 2).unwrap();
        assert!(!f.plain && !f.read_only);
    }

    #[cfg(unix)]
    #[test]
    fn a_named_pipe_never_hangs_the_window() {
        let dir = std::env::temp_dir().join(format!("gf-fifo-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("piege.md");
        let _ = std::fs::remove_file(&p);
        assert!(std::process::Command::new("mkfifo").arg(&p).status().unwrap().success());
        // returns at once with an error instead of waiting for a writer
        assert!(OpenFile::open(&p, 1).is_err());
    }

}
