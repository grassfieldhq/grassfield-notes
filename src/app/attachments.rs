//! Strip of attached documents (scans, images, PDFs) under the note.

use super::*;

impl GrassApp {
    pub(super) fn attachments_bar(&mut self, ui: &mut egui::Ui, id: u64, trashed: bool) {
        let t = self.theme();
        let Some(note) = self.lib.get(id) else { return };
        let list = note.attachments.clone();
        if list.is_empty() {
            return;
        }
        let ctx = ui.ctx().clone();
        egui::Panel::bottom("attachments")
            .frame(egui::Frame::new().fill(t.list_bg).inner_margin(Margin::symmetric(16, 8)))
            .show_separator_line(true)
            .resizable(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(crate::i18n::trf("📎 Documents joints ({0})", &[("0", &list.len() as &dyn std::fmt::Display)]))
                            .font(FontId::new(12.5, FontFamily::Name("bold".into())))
                            .color(t.muted),
                    );
                    ui.label(egui::RichText::new(crate::i18n::tr("clic : ouvrir · clic droit : plus d'options")).size(11.5).color(t.faint));
                });
                ui.add_space(4.0);
                egui::ScrollArea::horizontal().id_salt(("att-scroll", id)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        for a in &list {
                            let (r, resp) = ui.allocate_exact_size(Vec2::new(150.0, 112.0), Sense::click());
                            let p = ui.painter();
                            p.rect_filled(r, 8.0, t.editor_bg);
                            p.rect_stroke(r, 8.0, Stroke::new(1.0, if resp.hovered() { t.accent } else { t.border }), egui::StrokeKind::Inside);
                            let preview = Rect::from_min_size(r.min + Vec2::new(6.0, 6.0), Vec2::new(r.width() - 12.0, 64.0));
                            if let Some(tex) = self.thumbs.get(&ctx, id, a) {
                                let sz = tex.size_vec2();
                                let k = (preview.width() / sz.x).min(preview.height() / sz.y);
                                let ir = Rect::from_center_size(preview.center(), sz * k);
                                p.image(tex.id(), ir, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                            } else {
                                p.rect_filled(preview, 6.0, t.code_bg);
                                let label = if a.is_pdf() { "PDF" } else { "IMG" };
                                p.text(preview.center(), Align2::CENTER_CENTER, label, FontId::new(16.0, FontFamily::Name("bold".into())), t.accent_text);
                            }
                            let name_g = p.layout(a.name.clone(), FontId::proportional(11.5), t.text, r.width() - 12.0);
                            let clip = p.with_clip_rect(Rect::from_min_max(Pos2::new(r.left(), r.top() + 72.0), Pos2::new(r.right() - 4.0, r.top() + 88.0)));
                            clip.galley(Pos2::new(r.left() + 6.0, r.top() + 73.0), name_g, t.text);
                            let status = if self.reader.is_reading(id, &a.name) {
                                crate::i18n::tr("Lecture du texte…").to_string()
                            } else if !a.read {
                                crate::i18n::tr("En attente de lecture").to_string()
                            } else if a.text.is_empty() {
                                crate::i18n::tr("Aucun texte trouvé").to_string()
                            } else {
                                crate::i18n::trf("{0} mots reconnus", &[("0", &word_count(&a.text) as &dyn std::fmt::Display)])
                            };
                            p.text(Pos2::new(r.left() + 6.0, r.bottom() - 10.0), Align2::LEFT_CENTER, status, FontId::proportional(10.5), t.muted);
                            let name = a.name.clone();
                            if resp.clicked() {
                                self.actions.push(Action::OpenAttachment(id, name.clone()));
                            }
                            let has_text = !a.text.is_empty();
                            let is_image = a.is_image();
                            let resp = resp.on_hover_text(if has_text {
                                a.text.chars().take(300).collect::<String>()
                            } else {
                                a.name.clone()
                            });
                            resp.context_menu(|ui| {
                                if ui.button(crate::i18n::tr("Ouvrir")).clicked() {
                                    self.actions.push(Action::OpenAttachment(id, name.clone()));
                                    ui.close();
                                }
                                let path = Library::attachment_dir(id).join(&name);
                                if crate::sketch::sidecar(&path).exists()
                                    && ui.add_enabled(!trashed, egui::Button::new(crate::i18n::tr("Modifier le croquis"))).clicked() {
                                        self.actions.push(Action::EditSketch(id, name.clone()));
                                        ui.close();
                                    }
                                if is_image
                                    && ui.add_enabled(!trashed, egui::Button::new(crate::i18n::tr("Recadrer / redimensionner…"))).clicked() {
                                        self.actions.push(Action::EditImage(id, name.clone()));
                                        ui.close();
                                    }
                                if ui.add_enabled(has_text && !trashed, egui::Button::new(crate::i18n::tr("Insérer le texte reconnu dans la note"))).clicked() {
                                    self.actions.push(Action::InsertAttachmentText(id, name.clone()));
                                    ui.close();
                                }
                                if ui.add_enabled(!trashed, egui::Button::new(crate::i18n::tr("Retirer de la note"))).clicked() {
                                    self.actions.push(Action::RemoveAttachment(id, name.clone()));
                                    ui.close();
                                }
                            });
                        }
                        if !trashed {
                            let (r, resp) = ui.allocate_exact_size(Vec2::new(90.0, 112.0), Sense::click());
                            ui.painter().rect_stroke(r, 8.0, Stroke::new(1.0, if resp.hovered() { t.accent } else { t.border }), egui::StrokeKind::Inside);
                            ui.painter().text(r.center() - Vec2::new(0.0, 8.0), Align2::CENTER_CENTER, "+", FontId::proportional(24.0), t.accent_text);
                            ui.painter().text(r.center() + Vec2::new(0.0, 18.0), Align2::CENTER_CENTER, crate::i18n::tr("Joindre"), FontId::proportional(12.0), t.muted);
                            if resp.clicked() {
                                self.actions.push(Action::PickFiles(Purpose::Attach));
                            }
                        }
                    });
                });
            });
    }
}
