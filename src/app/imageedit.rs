//! Crop, resize and rotate an image attached to a note.

use super::*;
use crate::i18n::{tr, trf};

#[derive(Clone, Copy, PartialEq)]
enum Drag {
    Move,
    Corner(u8),
    New([f32; 2]),
}

pub(super) struct ImageEditor {
    note_id: u64,
    name: String,
    img: image::RgbaImage,
    tex: Option<egui::TextureHandle>,
    /// crop rectangle in 0..1 of the image
    crop: [f32; 4],
    aspect: Option<f32>,
    scale: f32,
    drag: Option<(Drag, [f32; 4], [f32; 2])>,
}

impl ImageEditor {
    fn refresh_texture(&mut self, ctx: &egui::Context) {
        let preview = image::DynamicImage::ImageRgba8(self.img.clone()).thumbnail(1400, 1400).to_rgba8();
        let ci = egui::ColorImage::from_rgba_unmultiplied([preview.width() as usize, preview.height() as usize], preview.as_raw());
        self.tex = Some(ctx.load_texture("image-editor", ci, egui::TextureOptions::LINEAR));
    }

    fn crop_px(&self) -> (u32, u32, u32, u32) {
        let (w, h) = (self.img.width() as f32, self.img.height() as f32);
        let x = (self.crop[0] * w).round().clamp(0.0, w - 1.0) as u32;
        let y = (self.crop[1] * h).round().clamp(0.0, h - 1.0) as u32;
        let cw = (((self.crop[2] - self.crop[0]) * w).round() as u32).clamp(1, self.img.width() - x);
        let ch = (((self.crop[3] - self.crop[1]) * h).round() as u32).clamp(1, self.img.height() - y);
        (x, y, cw, ch)
    }

    /// Largest centred crop with the chosen proportions.
    fn apply_aspect(&mut self) {
        let Some(a) = self.aspect else { return };
        let (w, h) = (self.img.width() as f32, self.img.height() as f32);
        let (mut cw, mut ch) = (w, w / a);
        if ch > h {
            ch = h;
            cw = h * a;
        }
        let (x, y) = ((w - cw) / 2.0 / w, (h - ch) / 2.0 / h);
        self.crop = [x, y, x + cw / w, y + ch / h];
    }
}

impl GrassApp {
    pub(super) fn open_image_editor(&mut self, ctx: &egui::Context, note_id: u64, name: String) {
        let path = Library::attachment_dir(note_id).join(&name);
        match image::open(&path) {
            Ok(img) => {
                let mut ed = ImageEditor {
                    note_id,
                    name,
                    img: img.to_rgba8(),
                    tex: None,
                    crop: [0.0, 0.0, 1.0, 1.0],
                    aspect: None,
                    scale: 1.0,
                    drag: None,
                };
                ed.refresh_texture(ctx);
                self.image_editor = Some(ed);
            }
            Err(e) => self.notify(ctx, trf("Image illisible : {e}", &[("e", &e)])),
        }
    }

    pub(super) fn image_editor_window(&mut self, ctx: &egui::Context) {
        let Some(mut ed) = self.image_editor.take() else { return };
        let t = self.theme();
        let screen = ctx.content_rect();
        let size = Vec2::new((screen.width() - 80.0).clamp(420.0, 1100.0), (screen.height() - 60.0).max(360.0));
        let mut close = false;
        let mut save = false;
        egui::Modal::new(Id::unique("image-editor"))
            .backdrop_color(Color32::from_black_alpha(120))
            .frame(egui::Frame::popup(&ctx.global_style()).fill(t.editor_bg).inner_margin(Margin::same(14)))
            .show(ctx, |ui| {
                ui.set_width(size.x - 28.0);
                ui.set_height(size.y - 28.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(tr("Recadrer et redimensionner")).font(FontId::new(18.0, FontFamily::Name("bold".into()))).color(t.text));
                    ui.add_space(12.0);
                    ui.label(egui::RichText::new(&ed.name).color(t.muted).size(12.5));
                });
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(tr("Proportions")).color(t.muted));
                    for (label, a) in [(tr("Libre"), None), ("1:1", Some(1.0)), ("4:3", Some(4.0 / 3.0)), ("3:2", Some(1.5)), ("16:9", Some(16.0 / 9.0)), ("3:4", Some(0.75))] {
                        if ui.selectable_label(ed.aspect == a, label).clicked() {
                            ed.aspect = a;
                            if a.is_some() {
                                ed.apply_aspect();
                            }
                        }
                    }
                    ui.add_space(12.0);
                    if ui.button(tr("↺ Pivoter à gauche")).clicked() {
                        ed.img = image::imageops::rotate270(&ed.img);
                        ed.crop = [0.0, 0.0, 1.0, 1.0];
                        ed.apply_aspect();
                        ed.refresh_texture(ui.ctx());
                    }
                    if ui.button(tr("↻ Pivoter à droite")).clicked() {
                        ed.img = image::imageops::rotate90(&ed.img);
                        ed.crop = [0.0, 0.0, 1.0, 1.0];
                        ed.apply_aspect();
                        ed.refresh_texture(ui.ctx());
                    }
                    if ui.button(tr("Réinitialiser")).clicked() {
                        ed.crop = [0.0, 0.0, 1.0, 1.0];
                        ed.aspect = None;
                        ed.scale = 1.0;
                    }
                });
                ui.add_space(6.0);
                // picture with the crop frame
                let bottom = 78.0;
                let avail = ui.available_size() - Vec2::new(0.0, bottom);
                let (iw, ih) = (ed.img.width() as f32, ed.img.height() as f32);
                let k = (avail.x / iw).min(avail.y / ih);
                let shown = Vec2::new(iw * k, ih * k);
                let (outer, _) = ui.allocate_exact_size(Vec2::new(avail.x, shown.y.max(40.0)), Sense::hover());
                let area = Rect::from_center_size(outer.center(), shown);
                if let Some(tex) = &ed.tex {
                    ui.painter().image(tex.id(), area, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                }
                let to_screen = |c: [f32; 4]| Rect::from_min_max(
                    Pos2::new(area.left() + c[0] * area.width(), area.top() + c[1] * area.height()),
                    Pos2::new(area.left() + c[2] * area.width(), area.top() + c[3] * area.height()),
                );
                let to_norm = |p: Pos2| [((p.x - area.left()) / area.width()).clamp(0.0, 1.0), ((p.y - area.top()) / area.height()).clamp(0.0, 1.0)];
                let resp = ui.interact(area.expand(10.0), Id::unique("crop-area"), Sense::drag());
                if resp.drag_started() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        let r = to_screen(ed.crop);
                        let corners = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()];
                        let kind = if let Some(i) = corners.iter().position(|c| c.distance(pos) < 14.0) {
                            Drag::Corner(i as u8)
                        } else if r.contains(pos) {
                            Drag::Move
                        } else {
                            Drag::New(to_norm(pos))
                        };
                        ed.drag = Some((kind, ed.crop, to_norm(pos)));
                    }
                }
                if let (Some((kind, start_crop, start)), Some(pos)) = (ed.drag, resp.interact_pointer_pos()) {
                    let q = to_norm(pos);
                    let min = 0.03;
                    let mut c = start_crop;
                    match kind {
                        Drag::Move => {
                            let (dx, dy) = (q[0] - start[0], q[1] - start[1]);
                            let (w, h) = (c[2] - c[0], c[3] - c[1]);
                            c[0] = (start_crop[0] + dx).clamp(0.0, 1.0 - w);
                            c[1] = (start_crop[1] + dy).clamp(0.0, 1.0 - h);
                            c[2] = c[0] + w;
                            c[3] = c[1] + h;
                        }
                        Drag::Corner(i) => {
                            match i {
                                0 => { c[0] = q[0].min(c[2] - min); c[1] = q[1].min(c[3] - min); }
                                1 => { c[2] = q[0].max(c[0] + min); c[1] = q[1].min(c[3] - min); }
                                2 => { c[2] = q[0].max(c[0] + min); c[3] = q[1].max(c[1] + min); }
                                _ => { c[0] = q[0].min(c[2] - min); c[3] = q[1].max(c[1] + min); }
                            }
                            if let Some(a) = ed.aspect {
                                // keep proportions: height follows width (in pixels)
                                let wpx = (c[2] - c[0]) * iw;
                                let hn = (wpx / a) / ih;
                                if i <= 1 { c[1] = (c[3] - hn).max(0.0); } else { c[3] = (c[1] + hn).min(1.0); }
                            }
                        }
                        Drag::New(s) => {
                            c = [s[0].min(q[0]), s[1].min(q[1]), s[0].max(q[0]).max(s[0].min(q[0]) + min), s[1].max(q[1]).max(s[1].min(q[1]) + min)];
                            if let Some(a) = ed.aspect {
                                let hn = ((c[2] - c[0]) * iw / a) / ih;
                                c[3] = (c[1] + hn).min(1.0);
                            }
                        }
                    }
                    ed.crop = c;
                }
                if resp.drag_stopped() {
                    ed.drag = None;
                }
                // shade outside the crop, draw frame, thirds and handles
                let r = to_screen(ed.crop);
                let shade = Color32::from_black_alpha(130);
                let p = ui.painter();
                p.rect_filled(Rect::from_min_max(area.min, Pos2::new(area.right(), r.top())), 0.0, shade);
                p.rect_filled(Rect::from_min_max(Pos2::new(area.left(), r.bottom()), area.max), 0.0, shade);
                p.rect_filled(Rect::from_min_max(Pos2::new(area.left(), r.top()), Pos2::new(r.left(), r.bottom())), 0.0, shade);
                p.rect_filled(Rect::from_min_max(Pos2::new(r.right(), r.top()), Pos2::new(area.right(), r.bottom())), 0.0, shade);
                p.rect_stroke(r, 0.0, Stroke::new(3.0, Color32::WHITE), egui::StrokeKind::Outside);
                p.rect_stroke(r, 0.0, Stroke::new(1.5, t.accent), egui::StrokeKind::Middle);
                for f in [1.0 / 3.0, 2.0 / 3.0] {
                    p.vline(r.left() + r.width() * f, r.y_range(), Stroke::new(1.0, Color32::from_white_alpha(110)));
                    p.hline(r.x_range(), r.top() + r.height() * f, Stroke::new(1.0, Color32::from_white_alpha(110)));
                }
                for c in [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()] {
                    p.rect_filled(Rect::from_center_size(c, Vec2::splat(12.0)), 2.0, Color32::WHITE);
                    p.rect_stroke(Rect::from_center_size(c, Vec2::splat(12.0)), 2.0, Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
                }
                ui.add_space(8.0);
                let (_, _, cw, ch) = ed.crop_px();
                let (fw, fh) = (((cw as f32) * ed.scale).round().max(1.0) as u32, ((ch as f32) * ed.scale).round().max(1.0) as u32);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(tr("Taille finale")).color(t.muted));
                    ui.add(egui::Slider::new(&mut ed.scale, 0.1..=1.0).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)));
                    ui.label(egui::RichText::new(format!("{fw} × {fh} px")).color(t.text));
                    ui.label(egui::RichText::new(trf("(origine : {w} × {h} px)", &[("w", &ed.img.width()), ("h", &ed.img.height())])).color(t.muted).size(12.0));
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(tr("Glissez les coins pour recadrer, ou dessinez un nouveau cadre.")).size(12.0).color(t.muted));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let b = egui::Button::new(egui::RichText::new(tr("Enregistrer")).color(Color32::WHITE)).fill(t.accent).stroke(Stroke::NONE);
                        if ui.add(b).clicked() {
                            save = true;
                        }
                        if ui.button(tr("Annuler")).clicked() {
                            close = true;
                        }
                    });
                });
            });
        if save {
            self.save_image_edit(ctx, ed);
            return;
        }
        if close || ctx.input(|i| i.key_pressed(Key::Escape)) {
            return;
        }
        self.image_editor = Some(ed);
    }

    fn save_image_edit(&mut self, ctx: &egui::Context, ed: ImageEditor) {
        let (x, y, cw, ch) = ed.crop_px();
        let cropped = image::imageops::crop_imm(&ed.img, x, y, cw, ch).to_image();
        let (fw, fh) = (((cw as f32) * ed.scale).round().max(1.0) as u32, ((ch as f32) * ed.scale).round().max(1.0) as u32);
        let out = if (fw, fh) != (cw, ch) {
            image::imageops::resize(&cropped, fw, fh, image::imageops::FilterType::Lanczos3)
        } else {
            cropped
        };
        let dir = Library::attachment_dir(ed.note_id);
        let lower = ed.name.to_lowercase();
        // keep PNG/JPEG files in place; other formats are saved as PNG next to them
        let new_name = if lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
            ed.name.clone()
        } else {
            let stem = ed.name.rsplit_once('.').map(|(s, _)| s).unwrap_or(&ed.name);
            format!("{stem}.png")
        };
        let path = dir.join(&new_name);
        let result = if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
            image::DynamicImage::ImageRgba8(out).to_rgb8().save_with_format(&path, image::ImageFormat::Jpeg)
        } else {
            out.save_with_format(&path, image::ImageFormat::Png)
        };
        if let Err(e) = result {
            self.notify(ctx, trf("Image non enregistrée : {e}", &[("e", &e)]));
            return;
        }
        if new_name != ed.name {
            let _ = std::fs::remove_file(dir.join(&ed.name));
        }
        // a cropped sketch is no longer the drawing it came from
        let _ = std::fs::remove_file(crate::sketch::sidecar(&dir.join(&ed.name)));
        self.thumbs.invalidate(ed.note_id, &ed.name);
        self.thumbs.invalidate(ed.note_id, &new_name);
        if let Some(n) = self.lib.get_mut(ed.note_id) {
            if let Some(a) = n.attachments.iter_mut().find(|a| a.name == ed.name) {
                a.name = new_name.clone();
                a.read = false; // read the text again
                a.text.clear();
            }
            if new_name != ed.name {
                n.text = n.text.replace(&format!("]({})", ed.name), &format!("]({new_name})"));
            }
            n.modified = now();
        }
        self.layout_cache = None;
        self.mark_dirty(ctx);
        self.notify(ctx, trf("Image enregistrée ({w} × {h} px)", &[("w", &fw), ("h", &fh)]));
    }
}
