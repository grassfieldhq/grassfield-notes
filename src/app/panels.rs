//! Info panel, tag renaming, confirmations, toasts and the screenshot option.

use super::*;

impl GrassApp {
    pub(super) fn info_panel(&mut self, ui: &mut egui::Ui) {
        let t = self.theme();
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(crate::i18n::tr("Infos"))
                    .font(FontId::new(16.0, FontFamily::Name("bold".into())))
                    .color(t.text),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(egui::Button::new(egui::RichText::new("✖").size(12.0).color(t.muted)).frame(false))
                    .clicked()
                {
                    self.show_info = false;
                }
            });
        });
        ui.add_space(8.0);
        let Some(n) = self.selected.and_then(|id| self.lib.get(id)).cloned() else {
            ui.label(egui::RichText::new(crate::i18n::tr("Aucune note")).color(t.muted));
            return;
        };
        let words = word_count(&n.text);
        let chars = n.text.chars().count();
        let paragraphs = n.text.split("\n\n").filter(|p| !p.trim().is_empty()).count();
        let mins = ((words as f32) / 200.0).ceil().max(1.0) as usize;
        let (done, total) = n.todo_counts();
        let row = |ui: &mut egui::Ui, k: &str, v: String| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(k).size(12.5).color(t.muted));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(egui::RichText::new(v).size(12.5).color(t.text));
                });
            });
        };
        egui::ScrollArea::vertical()
            .id_salt("info-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                row(ui, crate::i18n::tr("Modifiée"), format_full_date(n.modified));
                row(ui, crate::i18n::tr("Créée"), format_full_date(n.created));
                ui.separator();
                row(ui, crate::i18n::tr("Mots"), words.to_string());
                row(ui, crate::i18n::tr("Caractères"), chars.to_string());
                row(ui, crate::i18n::tr("Paragraphes"), paragraphs.to_string());
                row(ui, crate::i18n::tr("Temps de lecture"), format!("{mins} min"));
                if total > 0 {
                    row(ui, crate::i18n::tr("Tâches"), format!("{done} / {total}"));
                    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 6.0), Sense::hover());
                    ui.painter().rect_filled(r, 3.0, t.code_bg);
                    let mut fr = r;
                    fr.set_width(r.width() * done as f32 / total as f32);
                    ui.painter().rect_filled(fr, 3.0, t.accent);
                }
                ui.separator();
                ui.label(egui::RichText::new(crate::i18n::tr("Tags")).size(12.5).color(t.muted));
                let tags = n.tags();
                if tags.is_empty() {
                    ui.label(egui::RichText::new(crate::i18n::tr("Aucun tag")).size(12.5).color(t.faint));
                }
                ui.horizontal_wrapped(|ui| {
                    for tag in tags {
                        let b = ui.add(
                            egui::Button::new(egui::RichText::new(format!("#{tag}")).size(12.5).color(t.accent_text))
                                .fill(t.tag_bg)
                                .corner_radius(6.0),
                        );
                        if b.clicked() {
                            self.actions.push(Action::SetFilter(Filter::Tag(tag)));
                        }
                    }
                });
                ui.separator();
                ui.label(egui::RichText::new(crate::i18n::tr("Liens vers d'autres notes")).size(12.5).color(t.muted));
                let links = wiki_links(&n.text);
                if links.is_empty() {
                    ui.label(egui::RichText::new(crate::i18n::tr("Aucun lien")).size(12.5).color(t.faint));
                }
                for l in links {
                    let exists = self.lib.find_by_title(&l).is_some();
                    let txt = if exists { format!("› {l}") } else { format!("+ {l}") };
                    if ui
                        .add(egui::Button::new(egui::RichText::new(txt).size(12.5).color(t.accent_text)).frame(false))
                        .on_hover_text(if exists { crate::i18n::tr("Ouvrir") } else { crate::i18n::tr("Créer cette note") })
                        .clicked()
                    {
                        self.actions.push(Action::OpenTitle(l));
                    }
                }
                ui.separator();
                ui.label(egui::RichText::new(crate::i18n::tr("Notes qui mentionnent celle-ci")).size(12.5).color(t.muted));
                let back = self.lib.backlinks(n.id);
                if back.is_empty() {
                    ui.label(egui::RichText::new(crate::i18n::tr("Aucune")).size(12.5).color(t.faint));
                }
                for b in back {
                    let title = self.lib.get(b).map(|x| x.title()).unwrap_or_default();
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new(format!("‹ {title}")).size(12.5).color(t.accent_text))
                                .frame(false),
                        )
                        .clicked()
                    {
                        self.actions.push(Action::Select(b));
                    }
                }
            });
    }

    pub(super) fn rename_window(&mut self, ctx: &egui::Context) {
        let Some((old, mut new)) = self.rename_tag.clone() else { return };
        let mut close = false;
        let mut apply = false;
        egui::Window::new(crate::i18n::tr("Renommer le tag"))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(crate::i18n::trf("Nouveau nom pour #{old} :", &[("old", &old as &dyn std::fmt::Display)]));
                let r = ui.text_edit_singleline(&mut new);
                r.request_focus();
                if ui.input(|i| i.key_pressed(Key::Enter)) {
                    apply = true;
                }
                if ui.input(|i| i.key_pressed(Key::Escape)) {
                    close = true;
                }
                ui.horizontal(|ui| {
                    if ui.button(crate::i18n::tr("Renommer")).clicked() {
                        apply = true;
                    }
                    if ui.button(crate::i18n::tr("Annuler")).clicked() {
                        close = true;
                    }
                });
            });
        if apply {
            let clean = new.trim().trim_start_matches('#').replace(' ', "-").to_lowercase();
            if !clean.is_empty() && clean != old {
                self.lib.rename_tag(&old, &clean);
                if self.filter == Filter::Tag(old.clone()) {
                    self.filter = Filter::Tag(clean.clone());
                }
                self.notify(ctx, crate::i18n::trf("#{old} renommé en #{clean}", &[("old", &old as &dyn std::fmt::Display), ("clean", &clean as &dyn std::fmt::Display)]));
                self.mark_dirty(ctx);
            }
            self.rename_tag = None;
        } else if close {
            self.rename_tag = None;
        } else {
            self.rename_tag = Some((old, new));
        }
    }

    pub(super) fn confirm_window(&mut self, ctx: &egui::Context) {
        if !self.confirm_empty_trash {
            return;
        }
        let n = self.lib.count(&Filter::Trash);
        egui::Window::new(crate::i18n::tr("Vider la corbeille ?"))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(crate::i18n::trf("{n} note(s) seront effacées pour de bon.", &[("n", &n as &dyn std::fmt::Display)]));
                ui.horizontal(|ui| {
                    if ui.button(crate::i18n::tr("Vider")).clicked() {
                        self.actions.push(Action::EmptyTrash);
                        self.confirm_empty_trash = false;
                    }
                    if ui.button(crate::i18n::tr("Annuler")).clicked() {
                        self.confirm_empty_trash = false;
                    }
                });
            });
    }

    /// Links and files are shown in full before anything opens them.
    pub(super) fn open_window(&mut self, ctx: &egui::Context) {
        let Some(req) = self.pending_open.clone() else { return };
        let t = self.theme();
        let (title, target, note) = match &req {
            OpenRequest::Link(url) => (crate::i18n::tr("Ouvrir ce lien ?"), url.clone(), crate::i18n::tr("Il s'ouvrira dans votre navigateur ou votre messagerie.")),
            OpenRequest::File(p) => {
                let note = if p.is_dir() || files::is_markdown(p) || attach::opens_safely(p, crate::safefs::Links::Follow) {
                    crate::i18n::tr("Ce fichier est en dehors du dossier de la note.")
                } else {
                    crate::i18n::tr("Ce type de fichier ne s'ouvre jamais directement : son dossier sera affiché.")
                };
                (crate::i18n::tr("Ouvrir ce fichier ?"), p.display().to_string(), note)
            }
        };
        let mut go = None;
        egui::Modal::new(Id::unique("confirm-open")).show(ctx, |ui| {
            ui.set_max_width(520.0);
            ui.label(egui::RichText::new(title).font(FontId::new(16.0, FontFamily::Name("bold".into()))).color(t.text));
            ui.add_space(6.0);
            // the whole target, never shortened, so a look-alike address can be spotted
            egui::Frame::new().fill(t.code_bg).corner_radius(6.0).inner_margin(Margin::symmetric(10, 8)).show(ui, |ui| {
                ui.add(egui::Label::new(egui::RichText::new(&target).monospace().color(t.text)).wrap());
            });
            ui.add_space(4.0);
            ui.label(egui::RichText::new(note).size(12.5).color(t.muted));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(crate::i18n::tr("Ouvrir")).clicked() {
                    go = Some(true);
                }
                if ui.button(crate::i18n::tr("Annuler")).clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    go = Some(false);
                }
            });
        });
        let Some(go) = go else { return };
        self.pending_open = None;
        if !go {
            return;
        }
        match req {
            OpenRequest::Link(url) => {
                let _ = crate::platform::open(&url);
            }
            OpenRequest::File(p) => {
                if files::is_markdown(&p) && p.is_file() {
                    self.open_files(ctx, vec![p]);
                } else if !p.is_dir() && attach::opens_safely(&p, crate::safefs::Links::Follow) {
                    let _ = crate::platform::open_path(&p);
                } else {
                    let _ = crate::platform::reveal(&p);
                }
            }
        }
    }

    pub(super) fn toast(&mut self, ctx: &egui::Context) {
        let Some((msg, start)) = self.toast.clone() else { return };
        let now_t = ctx.input(|i| i.time);
        if start == 0.0 {
            self.toast = Some((msg.clone(), now_t));
        } else if now_t - start > 3.5 {
            self.toast = None;
            return;
        }
        let t = self.theme();
        egui::Area::new(Id::unique("toast"))
            .anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -40.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(t.sidebar_bg)
                    .corner_radius(10.0)
                    .inner_margin(Margin::symmetric(16, 10))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(msg).color(t.sidebar_text));
                    });
            });
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }

    pub(super) fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot.clone() else { return };
        ctx.request_repaint();
        if self.shot_done.load(Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if self.frame_no >= 120 && !self.shot_requested {
            self.shot_requested = true;
            let done = self.shot_done.clone();
            ctx.request_screenshot(move |img| {
                let [w, h] = img.size;
                let mut bytes = Vec::with_capacity(w * h * 4);
                for p in &img.pixels {
                    bytes.extend_from_slice(&[p.r(), p.g(), p.b(), 255]);
                }
                if let Some(dir) = path.parent() {
                    if !dir.as_os_str().is_empty() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                }
                match image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                    Some(buf) => {
                        if let Err(e) = buf.save(&path) {
                            eprintln!("screenshot: {e}");
                        }
                    }
                    None => eprintln!("screenshot: bad image size"),
                }
                done.store(true, Ordering::SeqCst);
            });
        }
        if self.frame_no > 900 {
            eprintln!("screenshot: timed out");
            std::process::exit(1);
        }
    }
}
