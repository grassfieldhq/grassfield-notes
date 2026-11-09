//! The Grassfield Notes window: sidebar, note list, editor and info panel.

mod editor;
mod files;
mod list;
mod attachments;
mod imageedit;
mod overlay;
mod panels;
mod settings;
mod sidebar;
mod sketcher;

use crate::markdown::{self, MdStyle};
use crate::model::{
    self, format_date, format_full_date, now, tag_spans, wiki_links, word_count, EditorFont, Filter,
    Library, SortMode,
};
use crate::export::{self, Format};
use crate::filepicker::{FilePicker, Purpose};
use crate::theme::{self, Theme};
use crate::{attach, icons, sync};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontFamily, FontId, Id, Key, KeyboardShortcut,
    Layout, Margin, Modifiers, Pos2, Rect, Sense, Stroke, Vec2,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Pages of the full-window settings screen.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsPage {
    Language,
    Appearance,
    Icon,
    Editor,
    Sync,
    Export,
    Data,
    Shortcuts,
}

/// A keyring call running in the background.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SecretOp {
    Store,
    Forget,
}

enum Action {
    Select(u64),
    TogglePin(u64),
    ToggleArchive(u64),
    Trash(u64),
    Restore(u64),
    DeleteForever(u64),
    Duplicate(u64),
    Export(u64, Format),
    CopyText(u64),
    SetFilter(Filter),
    NewNote,
    OpenTitle(String),
    EmptyTrash,
    ToggleTagExpanded(String),
    StartRenameTag(String),
    DeleteTag(String),
    PickFiles(Purpose),
    RemoveAttachment(u64, String),
    InsertAttachmentText(u64, String),
    OpenAttachment(u64, String),
    SyncNow,
    NewSketch(u64),
    EditSketch(u64, String),
    EditImage(u64, String),
}

pub struct GrassApp {
    lib: Library,
    filter: Filter,
    query: String,
    selected: Option<u64>,
    dirty: bool,
    last_change: f64,
    persist: bool,
    focus_mode: bool,
    show_info: bool,
    show_settings: bool,
    toast: Option<(String, f64)>,
    cursor: Option<(usize, usize)>,
    cursor_note: Option<u64>,
    layout_cache: Option<(u64, egui::text::LayoutJob)>,
    pending_cursor: Option<(usize, usize)>,
    focus_editor: bool,
    focus_search: bool,
    rename_tag: Option<(String, String)>,
    confirm_empty_trash: bool,
    applied_theme: Option<usize>,
    frame_no: u64,
    screenshot: Option<PathBuf>,
    shot_done: Arc<AtomicBool>,
    shot_requested: bool,
    actions: Vec<Action>,
    settings_page: SettingsPage,
    export_all_format: Format,
    picker: FilePicker,
    reader: attach::Reader,
    thumbs: attach::Thumbs,
    sync_job: Option<sync::Job>,
    sync_status: Option<(bool, String)>,
    /// token typed in the settings, not yet handed to the keyring
    token_input: String,
    secret_job: Option<(SecretOp, std::sync::mpsc::Receiver<Result<(), String>>)>,
    secret_status: Option<(bool, String)>,
    last_auto_sync: f64,
    applied_icon: Option<usize>,
    icon_textures: Vec<egui::TextureHandle>,
    sketcher: Option<sketcher::SketchEditor>,
    image_editor: Option<imageedit::ImageEditor>,
    /// Markdown files opened on their own
    files: Vec<files::OpenFile>,
    active_file: Option<usize>,
    /// files open in reading mode (symbols hidden, not editable)
    file_reading: bool,
    file_thumbs: std::collections::HashMap<PathBuf, (Option<std::time::SystemTime>, Option<egui::TextureHandle>)>,
    file_seq: u64,
    /// another window owns the library: this one doesn't save notes
    other_instance: bool,
    /// a file in conflict waits for the user's choice
    conflict_prompt: Option<files::ConflictPrompt>,
    /// every conflict is settled: the window may close
    quit_confirmed: bool,
    last_inbox_check: f64,
    last_lock_beat: f64,
    started: std::time::Instant,
}

impl GrassApp {
    pub fn new(_ctx: &egui::Context, screenshot: Option<PathBuf>, open: Vec<PathBuf>) -> Self {
        // Screenshots use the built-in sample notes and never touch the saved library.
        // When another window already has the library open, this one only reads it.
        let other_instance = screenshot.is_none() && instance_lock::held_by_other();
        let persist = screenshot.is_none() && !other_instance;
        if persist {
            instance_lock::take();
        }
        // pick the language before example notes are made
        crate::i18n::set(crate::i18n::system());
        if screenshot.is_some() {
            // the check picture stays in French unless a language is asked for
            crate::i18n::set(crate::i18n::Lang::Fr);
            if let Ok(states) = std::env::var("GRASSFIELD_SHOT_STATE") {
                for st in states.split(',') {
                    if let Some(l) = crate::i18n::ALL.iter().find(|l| l.code() == st.trim()) {
                        crate::i18n::set(*l);
                    }
                }
            }
        }
        let mut lib = if persist { Library::load_or_sample() } else { Library::sample() };
        if !persist {
            lib.settings.language = Some(crate::i18n::lang());
        }
        if let Some(l) = lib.settings.language {
            crate::i18n::set(l);
        }
        let first = lib.visible(&Filter::All, "").first().copied();
        let mut app = Self {
            lib,
            filter: Filter::All,
            query: String::new(),
            selected: first,
            dirty: false,
            last_change: 0.0,
            persist,
            focus_mode: false,
            show_info: false,
            show_settings: false,
            toast: None,
            cursor: None,
            cursor_note: None,
            layout_cache: None,
            pending_cursor: None,
            focus_editor: false,
            focus_search: false,
            rename_tag: None,
            confirm_empty_trash: false,
            applied_theme: None,
            frame_no: 0,
            screenshot,
            shot_done: Arc::new(AtomicBool::new(false)),
            shot_requested: false,
            actions: Vec::new(),
            settings_page: SettingsPage::Appearance,
            export_all_format: Format::Markdown,
            picker: FilePicker::default(),
            reader: attach::Reader::default(),
            thumbs: attach::Thumbs::default(),
            sync_job: None,
            sync_status: None,
            token_input: String::new(),
            secret_job: None,
            secret_status: None,
            last_auto_sync: f64::NEG_INFINITY,
            applied_icon: None,
            icon_textures: Vec::new(),
            sketcher: None,
            image_editor: None,
            files: Vec::new(),
            active_file: None,
            file_reading: true,
            file_thumbs: Default::default(),
            file_seq: 0,
            other_instance,
            conflict_prompt: None,
            quit_confirmed: false,
            last_inbox_check: 0.0,
            last_lock_beat: 0.0,
            started: std::time::Instant::now(),
        };
        if !open.is_empty() {
            app.open_files(_ctx, open);
        }
        // a token saved in plain text by an older version moves to the system keyring,
        // and the library is rewritten at once without it
        let legacy = app.lib.settings.sync.token.trim().to_string();
        if app.persist && !legacy.is_empty() {
            app.lib.settings.sync.token_in_keyring = false;
            app.start_secret(_ctx, SecretOp::Store, legacy);
            app.save_now();
        }
        // For checking the look in screenshots only: GRASSFIELD_SHOT_STATE=dark,info,settings,focus,trash,tag
        if app.screenshot.is_some() {
            if let Ok(states) = std::env::var("GRASSFIELD_SHOT_STATE") {
                for s in states.split(',') {
                    match s.trim() {
                        "dark" => app.lib.settings.theme = 2,
                        "night" => app.lib.settings.theme = 3,
                        "info" => app.show_info = true,
                        "settings" => app.show_settings = true,
                        "lang" => {
                            app.show_settings = true;
                            app.settings_page = SettingsPage::Language;
                        }
                        "en" => app.lib.settings.language = Some(crate::i18n::Lang::En),
                        "es" => app.lib.settings.language = Some(crate::i18n::Lang::Es),
                        "de" => app.lib.settings.language = Some(crate::i18n::Lang::De),
                        "it" => app.lib.settings.language = Some(crate::i18n::Lang::It),
                        "pt" => app.lib.settings.language = Some(crate::i18n::Lang::Pt),
                        "fr" => app.lib.settings.language = Some(crate::i18n::Lang::Fr),
                        "sync" => {
                            app.show_settings = true;
                            app.settings_page = SettingsPage::Sync;
                        }
                        "icons" => {
                            app.show_settings = true;
                            app.settings_page = SettingsPage::Icon;
                        }
                        "export" => {
                            app.show_settings = true;
                            app.settings_page = SettingsPage::Export;
                        }
                        "edit" => app.file_reading = false,
                        "picker" => app.picker.start(Purpose::Scan),
                        "sketch" => {
                            if let Some(id) = app.selected {
                                app.open_sketch(id, None);
                                if let Some(ed) = &mut app.sketcher {
                                    ed.demo();
                                }
                            }
                        }
                        "attach" => {
                            // attach a rendered page so the documents strip shows up
                            if let Some(id) = app.selected {
                                let tmp = std::env::temp_dir().join("grassfield-shot-docs");
                                let src = app.lib.notes.iter().find(|n| n.id != id).cloned();
                                if let Some(src) = src {
                                    if let Ok(p) = export::export_note(&src, Format::Jpg, &tmp) {
                                        if let Ok(mut a) = attach::store(id, &p) {
                                            a.text = crate::i18n::tr("Ce printemps, essayer une prairie fleurie").into();
                                            a.read = true;
                                            let name = a.name.clone();
                                            app.lib.get_mut(id).unwrap().attachments.push(a);
                                            // show it right after the first paragraph
                                            app.cursor_note = Some(id);
                                            app.cursor = Some((70, 70));
                                            app.insert_image_ref(id, &name);
                                            if states.contains("crop") {
                                                app.open_image_editor(_ctx, id, name);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        "focus" => app.focus_mode = true,
                        "trash" => {
                            app.filter = Filter::Trash;
                            app.selected = None;
                        }
                        "tag" => app.filter = Filter::Tag(if crate::i18n::lang() == crate::i18n::Lang::Fr { "travail".into() } else { "work".into() }),
                        _ => {}
                    }
                }
            }
        }
        app
    }

    fn theme(&self) -> Theme {
        theme::get(self.lib.settings.theme)
    }

    fn mark_dirty(&mut self, ctx: &egui::Context) {
        self.dirty = true;
        self.last_change = ctx.input(|i| i.time);
    }

    fn save_now(&mut self) {
        if !self.persist {
            self.dirty = false;
            return;
        }
        if let Err(e) = self.lib.save() {
            self.toast = Some((crate::i18n::trf("Échec de l'enregistrement : {e}", &[("e", &e as &dyn std::fmt::Display)]), 0.0));
        }
        self.dirty = false;
    }

    fn notify(&mut self, ctx: &egui::Context, msg: impl Into<String>) {
        self.toast = Some((msg.into(), ctx.input(|i| i.time)));
    }

    fn apply_theme(&mut self, ctx: &egui::Context) {
        if self.applied_theme == Some(self.lib.settings.theme) {
            return;
        }
        self.applied_theme = Some(self.lib.settings.theme);
        let t = self.theme();
        let mut v = if t.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.panel_fill = t.editor_bg;
        v.window_fill = t.editor_bg;
        v.extreme_bg_color = t.list_bg;
        v.faint_bg_color = t.code_bg;
        v.hyperlink_color = t.accent_text;
        v.selection.bg_fill = t.selection;
        v.selection.stroke = Stroke::new(1.0, t.accent);
        v.window_stroke = Stroke::new(1.0, t.border);
        v.text_cursor.stroke = Stroke::new(2.0, t.accent);
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, t.border);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, t.text);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, t.text);
        // plain buttons get a soft fill and border so they read as buttons, not text
        v.widgets.inactive.weak_bg_fill = t.code_bg;
        v.widgets.inactive.bg_fill = t.code_bg;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, t.border);
        v.widgets.hovered.weak_bg_fill = t.list_selected;
        v.widgets.hovered.bg_fill = t.list_selected;
        v.widgets.hovered.bg_stroke = Stroke::NONE;
        v.widgets.hovered.fg_stroke = Stroke::new(1.0, t.text);
        v.widgets.active.weak_bg_fill = t.selection;
        v.widgets.active.bg_fill = t.selection;
        v.widgets.active.fg_stroke = Stroke::new(1.0, t.text);
        v.widgets.open.weak_bg_fill = t.list_selected;
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = CornerRadius::same(6);
        }
        ctx.all_styles_mut(|s| {
            s.visuals = v.clone();
            s.spacing.item_spacing = Vec2::new(8.0, 6.0);
            s.spacing.button_padding = Vec2::new(8.0, 4.0);
            s.text_styles.insert(egui::TextStyle::Body, FontId::proportional(14.0));
            s.text_styles.insert(egui::TextStyle::Button, FontId::proportional(14.0));
            s.text_styles.insert(egui::TextStyle::Small, FontId::proportional(11.5));
            s.text_styles.insert(egui::TextStyle::Heading, FontId::new(19.0, FontFamily::Name("bold".into())));
        });
    }

    // ------------------------------------------------------------------ actions

    fn run_actions(&mut self, ctx: &egui::Context) {
        let actions = std::mem::take(&mut self.actions);
        for a in actions {
            match a {
                Action::Select(id) => {
                    self.selected = Some(id);
                    self.cursor = None;
                }
                Action::TogglePin(id) => {
                    if let Some(n) = self.lib.get_mut(id) {
                        n.pinned = !n.pinned;
                        let msg = if n.pinned { crate::i18n::tr("Note épinglée") } else { crate::i18n::tr("Note désépinglée") };
                        self.notify(ctx, msg);
                    }
                    self.mark_dirty(ctx);
                }
                Action::ToggleArchive(id) => {
                    if let Some(n) = self.lib.get_mut(id) {
                        n.archived = !n.archived;
                        let msg = if n.archived { crate::i18n::tr("Note archivée") } else { crate::i18n::tr("Note sortie des archives") };
                        self.notify(ctx, msg);
                    }
                    self.reselect_if_hidden(id);
                    self.mark_dirty(ctx);
                }
                Action::Trash(id) => {
                    if let Some(n) = self.lib.get_mut(id) {
                        n.trashed = true;
                        n.pinned = false;
                    }
                    self.notify(ctx, crate::i18n::tr("Note déplacée dans la corbeille"));
                    self.reselect_if_hidden(id);
                    self.mark_dirty(ctx);
                }
                Action::Restore(id) => {
                    if let Some(n) = self.lib.get_mut(id) {
                        n.trashed = false;
                    }
                    self.notify(ctx, crate::i18n::tr("Note restaurée"));
                    self.reselect_if_hidden(id);
                    self.mark_dirty(ctx);
                }
                Action::DeleteForever(id) => {
                    self.lib.notes.retain(|n| n.id != id);
                    self.notify(ctx, crate::i18n::tr("Note supprimée définitivement"));
                    self.reselect_if_hidden(id);
                    self.mark_dirty(ctx);
                }
                Action::Duplicate(id) => {
                    if let Some(text) = self.lib.get(id).map(|n| n.text.clone()) {
                        let nid = self.lib.new_note(text);
                        self.selected = Some(nid);
                        if matches!(self.filter, Filter::Trash | Filter::Archive) {
                            self.filter = Filter::All;
                        }
                        self.notify(ctx, crate::i18n::tr("Note dupliquée"));
                        self.mark_dirty(ctx);
                    }
                }
                Action::Export(id, kind) => self.export(ctx, &[id], kind),
                Action::CopyText(id) => {
                    if let Some(n) = self.lib.get(id) {
                        ctx.copy_text(n.text.clone());
                        self.notify(ctx, crate::i18n::tr("Texte copié dans le presse-papiers"));
                    }
                }
                Action::SetFilter(f) => {
                    self.filter = f;
                    let vis = self.lib.visible(&self.filter, &self.query);
                    if !self.selected.map(|s| vis.contains(&s)).unwrap_or(false) {
                        self.selected = vis.first().copied();
                    }
                }
                Action::NewNote => self.new_note(ctx),
                Action::OpenTitle(title) => {
                    let id = match self.lib.find_by_title(&title) {
                        Some(id) => id,
                        None => {
                            let id = self.lib.new_note(format!("# {title}\n\n"));
                            self.notify(ctx, crate::i18n::trf("Nouvelle note « {title} » créée", &[("title", &title as &dyn std::fmt::Display)]));
                            self.mark_dirty(ctx);
                            id
                        }
                    };
                    let note = self.lib.get(id).unwrap();
                    if !self.lib.matches(note, &self.filter) {
                        self.filter = if note.archived { Filter::Archive } else { Filter::All };
                    }
                    self.query.clear();
                    self.selected = Some(id);
                }
                Action::EmptyTrash => {
                    let n = self.lib.count(&Filter::Trash);
                    self.lib.notes.retain(|n| !n.trashed);
                    if self.filter == Filter::Trash {
                        self.selected = None;
                    }
                    self.notify(ctx, crate::i18n::trf("{n} note(s) supprimée(s)", &[("n", &n as &dyn std::fmt::Display)]));
                    self.mark_dirty(ctx);
                }
                Action::ToggleTagExpanded(t) => {
                    if !self.lib.expanded_tags.remove(&t) {
                        self.lib.expanded_tags.insert(t);
                    }
                    self.mark_dirty(ctx);
                }
                Action::StartRenameTag(t) => self.rename_tag = Some((t.clone(), t)),
                Action::DeleteTag(t) => {
                    self.lib.delete_tag(&t);
                    if self.filter == Filter::Tag(t.clone()) {
                        self.filter = Filter::All;
                    }
                    self.notify(ctx, crate::i18n::trf("Tag #{t} retiré des notes", &[("t", &t as &dyn std::fmt::Display)]));
                    self.mark_dirty(ctx);
                }
                Action::PickFiles(purpose) => self.picker.start(purpose),
                Action::RemoveAttachment(id, name) => {
                    if let Some(n) = self.lib.get_mut(id) {
                        n.attachments.retain(|a| a.name != name);
                        n.modified = now();
                    }
                    let _ = std::fs::remove_file(model::Library::attachment_dir(id).join(&name));
                    self.notify(ctx, crate::i18n::trf("« {name} » retiré de la note", &[("name", &name as &dyn std::fmt::Display)]));
                    self.mark_dirty(ctx);
                }
                Action::InsertAttachmentText(id, name) => {
                    if let Some(n) = self.lib.get_mut(id) {
                        if let Some(text) = n.attachments.iter().find(|a| a.name == name).map(|a| a.text.clone()) {
                            if !n.text.ends_with('\n') {
                                n.text.push('\n');
                            }
                            n.text.push_str(&crate::i18n::trf("\n> Texte de « {name} »\n\n{text}\n", &[("name", &name as &dyn std::fmt::Display), ("text", &text as &dyn std::fmt::Display)]));
                            n.modified = now();
                        }
                    }
                    self.layout_cache = None;
                    self.mark_dirty(ctx);
                }
                Action::OpenAttachment(id, name) => {
                    let p = model::Library::attachment_dir(id).join(&name);
                    let _ = crate::platform::open_path(&p);
                }
                Action::SyncNow => self.start_sync(ctx),
                Action::NewSketch(id) => self.open_sketch(id, None),
                Action::EditSketch(id, name) => self.open_sketch(id, Some(name)),
                Action::EditImage(id, name) => self.open_image_editor(ctx, id, name),
            }
        }
    }

    fn reselect_if_hidden(&mut self, id: u64) {
        if self.selected != Some(id) {
            return;
        }
        let visible = self.lib.visible(&self.filter, &self.query);
        if visible.contains(&id) {
            return;
        }
        self.selected = visible.first().copied();
    }

    fn new_note(&mut self, ctx: &egui::Context) {
        let mut text = String::from("# ");
        if let Filter::Tag(t) = &self.filter {
            text = format!("# \n\n#{t}");
        }
        let id = self.lib.new_note(text);
        if matches!(
            self.filter,
            Filter::Trash | Filter::Archive | Filter::Untagged | Filter::Pinned | Filter::Todo
        ) {
            self.filter = Filter::All;
        }
        self.query.clear();
        self.selected = Some(id);
        self.pending_cursor = Some((2, 2));
        self.focus_editor = true;
        self.mark_dirty(ctx);
    }

    fn export_dir() -> PathBuf {
        dirs::document_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Grassfield Notes")
    }

    fn export(&mut self, ctx: &egui::Context, ids: &[u64], format: Format) {
        let dir = Self::export_dir();
        let mut count = 0;
        let mut last_err = None;
        for id in ids {
            let Some(n) = self.lib.get(*id) else { continue };
            match export::export_note(n, format, &dir) {
                Ok(_) => count += 1,
                Err(e) => last_err = Some(e.to_string()),
            }
        }
        match last_err {
            Some(e) if count == 0 => self.notify(ctx, crate::i18n::trf("Export impossible : {e}", &[("e", &e as &dyn std::fmt::Display)])),
            _ => self.notify(ctx, crate::i18n::trf("{count} note(s) exportée(s) en {0} dans {1}", &[("0", &format.ext().to_uppercase() as &dyn std::fmt::Display), ("1", &dir.display() as &dyn std::fmt::Display), ("count", &count as &dyn std::fmt::Display)])),
        }
    }

    /// Add files: Markdown/text become notes, images and PDFs are attached (and their text read).
    fn add_files(&mut self, ctx: &egui::Context, files: Vec<PathBuf>, purpose: Purpose) {
        if purpose == Purpose::OpenFile {
            self.open_files(ctx, files);
            return;
        }
        let mut notes_made = 0;
        let mut attached = 0;
        let mut last = None;
        let (docs, texts): (Vec<PathBuf>, Vec<PathBuf>) = files.into_iter().partition(|p| attach::is_attachable(p));
        for p in texts {
            let ok = p
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| matches!(e.to_lowercase().as_str(), "md" | "txt" | "markdown" | "text"))
                .unwrap_or(false);
            if ok {
                if let Ok(t) = std::fs::read_to_string(&p) {
                    last = Some(self.lib.new_note(t));
                    notes_made += 1;
                }
            }
        }
        if !docs.is_empty() {
            // scanning makes a new note; attaching uses the open note (or a new one)
            let target = match (purpose, self.selected.and_then(|id| self.lib.get(id)).filter(|n| !n.trashed)) {
                (Purpose::Attach, Some(n)) => n.id,
                _ => {
                    let date = chrono::Local::now().format("%d/%m/%Y %H:%M");
                    self.lib.new_note(crate::i18n::trf("# Document numérisé du {date}\n\n#documents\n", &[("date", &date as &dyn std::fmt::Display)]))
                }
            };
            for p in docs {
                match attach::store(target, &p) {
                    Ok(a) => {
                        let inline = a.is_image().then(|| a.name.clone());
                        if let Some(n) = self.lib.get_mut(target) {
                            n.attachments.push(a);
                            n.modified = now();
                        }
                        if let Some(name) = inline {
                            self.insert_image_ref(target, &name);
                        }
                        attached += 1;
                    }
                    Err(e) => self.notify(ctx, crate::i18n::trf("Impossible de joindre {0} : {e}", &[("0", &p.display() as &dyn std::fmt::Display), ("e", &e as &dyn std::fmt::Display)])),
                }
            }
            last = Some(target);
        }
        if notes_made + attached > 0 {
            if let Some(id) = last {
                let note = self.lib.get(id).unwrap();
                if !self.lib.matches(note, &self.filter) {
                    self.filter = Filter::All;
                }
                self.selected = Some(id);
            }
            self.mark_dirty(ctx);
            let tools = attach::tools();
            let mut msg = match (notes_made, attached) {
                (n, 0) => crate::i18n::trf("{n} note(s) importée(s)", &[("n", &n as &dyn std::fmt::Display)]),
                (0, a) => crate::i18n::trf("{a} document(s) ajouté(s)", &[("a", &a as &dyn std::fmt::Display)]),
                (n, a) => crate::i18n::trf("{n} note(s) importée(s), {a} document(s) ajouté(s)", &[("n", &n as &dyn std::fmt::Display), ("a", &a as &dyn std::fmt::Display)]),
            };
            if attached > 0 {
                if tools.ocr || tools.pdf {
                    msg.push_str(" — lecture du texte en cours");
                } else {
                    msg.push_str(crate::i18n::tr(" — installez tesseract et pdftotext pour chercher dans leur texte"));
                }
            }
            self.notify(ctx, msg);
        } else {
            self.notify(ctx, crate::i18n::tr("Formats acceptés : .md, .txt, images ou PDF"));
        }
    }

    fn import_dropped(&mut self, ctx: &egui::Context) {
        let files = ctx.input(|i| i.raw.dropped_files.clone());
        if files.is_empty() {
            return;
        }
        let paths: Vec<PathBuf> = files.iter().map(|f| f.path().to_path_buf()).collect();
        // Markdown files open on their own; pictures go into the open file or note
        let (md, rest): (Vec<PathBuf>, Vec<PathBuf>) = paths.into_iter().partition(|p| files::is_markdown(p));
        if !md.is_empty() {
            self.open_files(ctx, md);
        }
        if rest.is_empty() {
            return;
        }
        match self.active_file.filter(|_| !self.show_settings) {
            Some(i) => {
                let (pics, other): (Vec<PathBuf>, Vec<PathBuf>) = rest.into_iter().partition(|p| attach::is_attachable(p) && !p.to_string_lossy().to_lowercase().ends_with(".pdf"));
                self.add_pictures_to_file(ctx, i, pics);
                if !other.is_empty() {
                    self.add_files(ctx, other, Purpose::Attach);
                }
            }
            None => self.add_files(ctx, rest, Purpose::Attach),
        }
    }

    fn start_sync(&mut self, ctx: &egui::Context) {
        if self.sync_job.is_some() {
            return;
        }
        if self.lib.settings.sync.repo_url.trim().is_empty() {
            self.sync_status = Some((false, crate::i18n::tr("Indiquez d'abord l'adresse du dépôt GitHub.").into()));
            return;
        }
        self.save_now();
        self.sync_status = Some((true, crate::i18n::tr("Synchronisation en cours…").into()));
        self.sync_job = Some(sync::start(
            self.lib.settings.sync.clone(),
            self.lib.notes.clone(),
            self.lib.synced_ids.clone(),
            ctx.clone(),
        ));
        self.last_auto_sync = ctx.input(|i| i.time);
    }

    /// Store or forget the sync token in the system keyring, in the background.
    fn start_secret(&mut self, ctx: &egui::Context, op: SecretOp, token: String) {
        if !self.persist {
            return;
        }
        // usable right away, whatever the keyring answers
        let t = token.clone();
        self.lib.settings.sync.token = token;
        let rx = match op {
            SecretOp::Store => crate::secrets::spawn(ctx, move || crate::secrets::store(&t)),
            SecretOp::Forget => crate::secrets::spawn(ctx, crate::secrets::delete),
        };
        self.secret_job = Some((op, rx));
    }

    fn poll_secret(&mut self, ctx: &egui::Context) {
        let Some((op, rx)) = &self.secret_job else { return };
        let Ok(res) = rx.try_recv() else { return };
        let op = *op;
        self.secret_job = None;
        let s = &mut self.lib.settings.sync;
        self.secret_status = Some(match (op, res) {
            (SecretOp::Store, Ok(())) => {
                s.token_in_keyring = true;
                (true, crate::i18n::tr("Jeton enregistré dans le trousseau du système").to_string())
            }
            (SecretOp::Store, Err(e)) => {
                s.token_in_keyring = false;
                (false, crate::i18n::trf("Trousseau du système indisponible ({e}) : le jeton n'est gardé que jusqu'à la fermeture de Grassfield Notes.", &[("e", &e)]))
            }
            (SecretOp::Forget, Ok(())) => {
                s.token_in_keyring = false;
                (true, crate::i18n::tr("Jeton retiré du trousseau du système").to_string())
            }
            (SecretOp::Forget, Err(e)) => (false, crate::i18n::trf("Le jeton n'a pas pu être retiré du trousseau du système : {e}", &[("e", &e)])),
        });
        self.mark_dirty(ctx);
    }

    fn poll_background(&mut self, ctx: &egui::Context) {
        self.poll_secret(ctx);
        if let Some(job) = &self.sync_job {
            if let Some(out) = job.poll() {
                self.sync_job = None;
                if let Some(t) = out.token.clone() {
                    self.lib.settings.sync.token = t;
                }
                if let Some(m) = out.merge {
                    sync::apply(&mut self.lib, m);
                    // pictures may have changed on the other computer
                    self.thumbs = attach::Thumbs::default();
                    self.layout_cache = None;
                    if let Some(id) = self.selected {
                        if self.lib.get(id).is_none() {
                            self.selected = self.lib.visible(&self.filter, &self.query).first().copied();
                        }
                    }
                    self.mark_dirty(ctx);
                }
                self.notify(ctx, out.message.clone());
                self.sync_status = Some((out.ok, out.message));
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(300));
            }
        }
        if self.persist {
            self.reader.queue_unread(&self.lib, ctx);
        }
        if self.reader.poll(&mut self.lib) {
            self.mark_dirty(ctx);
        }
        // automatic sync: shortly after start, then every 10 minutes
        let s = &self.lib.settings.sync;
        if self.persist && s.auto && !s.repo_url.trim().is_empty() && self.sync_job.is_none() {
            let t = ctx.input(|i| i.time);
            let due = if self.last_auto_sync.is_infinite() { t > 2.0 } else { t - self.last_auto_sync > 600.0 };
            if due {
                self.start_sync(ctx);
            }
            ctx.request_repaint_after(std::time::Duration::from_secs(30));
        }
    }

    fn apply_icon(&mut self, ctx: &egui::Context) {
        if self.icon_textures.is_empty() {
            self.icon_textures = (0..icons::ICONS.len())
                .map(|i| ctx.load_texture(format!("app-icon-{i}"), icons::color_image(i, 96), egui::TextureOptions::LINEAR))
                .collect();
        }
        let i = self.lib.settings.icon % icons::ICONS.len();
        if self.applied_icon != Some(i) {
            self.applied_icon = Some(i);
            ctx.send_viewport_cmd(egui::ViewportCommand::Icon(Some(Arc::new(icons::icon_data(i)))));
        }
    }

    // ------------------------------------------------------------------ shortcuts

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let mut new = false;
        let mut search = false;
        let mut focus = false;
        let mut info = false;
        let mut pin = false;
        let mut trash = false;
        let mut settings = false;
        let mut export = false;
        let typing = ctx.memory(|m| m.focused().is_some());
        ctx.input_mut(|i| {
            new = i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::N));
            focus = i.consume_shortcut(&KeyboardShortcut::new(cmd_shift, Key::F))
                || i.consume_shortcut(&KeyboardShortcut::new(Modifiers::NONE, Key::F11));
            search = i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::F));
            info = i.consume_shortcut(&KeyboardShortcut::new(cmd_shift, Key::I));
            pin = i.consume_shortcut(&KeyboardShortcut::new(cmd_shift, Key::P));
            // Ctrl+Delete deletes a word while typing, so it only trashes the note when no text field is focused.
            trash = !typing && i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::Delete));
            settings = i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::Comma));
            if i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::O)) {
                self.actions.push(Action::PickFiles(Purpose::OpenFile));
            }
            export = i.consume_shortcut(&KeyboardShortcut::new(cmd_shift, Key::E));
        });
        if focus {
            self.focus_mode = !self.focus_mode;
        }
        if new {
            self.actions.push(Action::NewNote);
        }
        if search {
            self.focus_search = true;
            self.focus_mode = false;
        }
        if info {
            self.show_info = !self.show_info;
        }
        if settings {
            self.show_settings = !self.show_settings;
        }
        if let Some(id) = self.selected {
            if pin {
                self.actions.push(Action::TogglePin(id));
            }
            if trash {
                self.actions.push(Action::Trash(id));
            }
            if export {
                self.actions.push(Action::Export(id, Format::Markdown));
            }
        }
        // Up / down in the note list when no text field has focus.
        if ctx.memory(|m| m.focused().is_none()) {
            let (up, down) = ctx.input(|i| (i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown)));
            if up || down {
                let vis = self.lib.visible(&self.filter, &self.query);
                if !vis.is_empty() {
                    let pos = self.selected.and_then(|s| vis.iter().position(|v| *v == s));
                    let next = match (pos, up) {
                        (None, _) => 0,
                        (Some(p), true) => p.saturating_sub(1),
                        (Some(p), false) => (p + 1).min(vis.len() - 1),
                    };
                    self.selected = Some(vis[next]);
                }
            }
        }
    }
}

impl eframe::App for GrassApp {
    /// Runs even while the window is hidden (eframe then skips `ui`).
    /// In screenshot mode, keep painting so the picture is always taken, and never hang.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.screenshot.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
            ctx.request_repaint();
            if self.started.elapsed() > std::time::Duration::from_secs(75) {
                eprintln!("screenshot: timed out");
                std::process::exit(1);
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.frame_no += 1;
        self.lib.refresh_index();
        crate::i18n::set(self.lib.settings.language.unwrap_or_else(crate::i18n::system));
        self.apply_theme(&ctx);
        self.shortcuts(&ctx);
        self.import_dropped(&ctx);
        self.apply_icon(&ctx);
        self.poll_background(&ctx);
        self.tick_files(&ctx);
        self.poll_inbox(&ctx);
        if self.persist {
            let t_now = ctx.input(|i| i.time);
            if t_now - self.last_lock_beat > 20.0 {
                self.last_lock_beat = t_now;
                instance_lock::take();
            }
            ctx.request_repaint_after(std::time::Duration::from_secs(20));
        }
        let t = self.theme();

        if self.show_settings {
            // Settings take the whole window, so it's always clear where you are.
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(t.editor_bg))
                .show(ui, |ui| self.settings_screen(ui));
        } else if self.active_file.is_some() {
            // a Markdown file opened on its own takes the whole window
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(t.editor_bg))
                .show(ui, |ui| self.file_view(ui));
        } else {
            if self.other_instance {
                egui::Panel::top("readonly-banner")
                    .frame(egui::Frame::new().fill(t.highlight_bg.gamma_multiply(0.6)).inner_margin(Margin::symmetric(16, 6)))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(crate::i18n::tr("Grassfield Notes est déjà ouvert dans une autre fenêtre : celle-ci affiche vos notes sans les modifier.")).color(t.text).size(12.5));
                    });
            }
            // On narrow windows the tag sidebar steps aside so the editor keeps room to write.
            let narrow = ctx.content_rect().width() < 900.0;
            if !self.focus_mode && !narrow {
                egui::Panel::left("sidebar")
                    .frame(egui::Frame::new().fill(t.sidebar_bg).inner_margin(Margin::symmetric(8, 0)))
                    .default_size(self.lib.settings.sidebar_width)
                    .size_range(180.0..=340.0)
                    .show_separator_line(false)
                    .show(ui, |ui| self.sidebar(ui));
            }
            if !self.focus_mode {
                egui::Panel::left("notes")
                    .frame(egui::Frame::new().fill(t.list_bg))
                    .default_size(320.0)
                    .size_range(240.0..=520.0)
                    .show(ui, |ui| self.note_list(ui));
            }
            if self.show_info {
                egui::Panel::right("info")
                    .frame(egui::Frame::new().fill(t.list_bg).inner_margin(Margin::symmetric(16, 0)))
                    .default_size(260.0)
                    .size_range(220.0..=380.0)
                    .show(ui, |ui| self.info_panel(ui));
            }
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(t.editor_bg))
                .show(ui, |ui| self.editor(ui));
        }

        self.sketch_window(&ctx);
        self.image_editor_window(&ctx);
        if let Some(files) = self.picker.show(&ctx, &t) {
            let purpose = self.picker.purpose;
            self.add_files(&ctx, files, purpose);
        }
        self.rename_window(&ctx);
        self.confirm_window(&ctx);
        self.check_quit(&ctx);
        self.conflict_window(&ctx);
        self.toast(&ctx);
        self.run_actions(&ctx);

        // autosave a moment after the last change
        if self.dirty {
            let now_t = ctx.input(|i| i.time);
            if now_t - self.last_change > 1.0 {
                self.save_now();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(1100));
            }
        }
        self.handle_screenshot(&ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.dirty {
            self.save_now();
        }
        self.save_files_on_exit();
        if self.persist {
            instance_lock::release();
        }
    }
}

/// One window owns the library at a time; others only read it.
mod instance_lock {
    use crate::model::Library;
    use std::time::{Duration, SystemTime};

    fn path() -> std::path::PathBuf {
        Library::data_dir().join("instance.lock")
    }

    /// Another running window refreshed the lock within the last minute.
    pub fn held_by_other() -> bool {
        let p = path();
        let Ok(meta) = std::fs::metadata(&p) else { return false };
        let fresh = meta
            .modified()
            .ok()
            .and_then(|m| SystemTime::now().duration_since(m).ok())
            .map(|age| age < Duration::from_secs(60))
            .unwrap_or(false);
        let mine = std::fs::read_to_string(&p).map(|s| s.trim() == std::process::id().to_string()).unwrap_or(false);
        fresh && !mine
    }

    pub fn take() {
        let _ = std::fs::create_dir_all(Library::data_dir());
        let _ = std::fs::write(path(), std::process::id().to_string());
    }

    pub fn release() {
        let mine = std::fs::read_to_string(path()).map(|s| s.trim() == std::process::id().to_string()).unwrap_or(false);
        if mine {
            let _ = std::fs::remove_file(path());
        }
    }
}

