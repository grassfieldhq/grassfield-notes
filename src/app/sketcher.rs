//! Sketch window: draw with a pen, pencil, highlighter, eraser or ruler, then keep it in the note.

use super::*;
use crate::sketch::{self, Sketch, Tool};
use crate::i18n::tr;

pub(super) struct SketchEditor {
    note_id: u64,
    /// existing attachment being edited
    name: Option<String>,
    sketch: Sketch,
    tool: Tool,
    color: [u8; 3],
    size: f32,
    current: Option<sketch::Stroke>,
    undo: Vec<Vec<sketch::Stroke>>,
    redo: Vec<Vec<sketch::Stroke>>,
}

const PALETTE: [[u8; 3]; 9] = [
    [0x1F, 0x22, 0x1F],
    [0x7A, 0x80, 0x76],
    [0xD8, 0x3B, 0x30],
    [0xF0, 0x8A, 0x24],
    [0xF2, 0xC6, 0x2F],
    [0x3B, 0x9A, 0x4A],
    [0x2F, 0x7B, 0xC8],
    [0x8A, 0x55, 0xC9],
    [0xFF, 0xFF, 0xFF],
];

impl GrassApp {
    /// Open the sketch window for a new sketch, or to edit an existing one.
    pub(super) fn open_sketch(&mut self, note_id: u64, name: Option<String>) {
        let sk = name
            .as_ref()
            .and_then(|n| sketch::load(&Library::attachment_dir(note_id).join(n)))
            .unwrap_or_default();
        self.sketcher = Some(SketchEditor {
            note_id,
            name,
            sketch: sk,
            tool: Tool::Pen,
            color: PALETTE[0],
            size: 4.0,
            current: None,
            undo: Vec::new(),
            redo: Vec::new(),
        });
    }

    pub(super) fn sketch_window(&mut self, ctx: &egui::Context) {
        let Some(mut ed) = self.sketcher.take() else { return };
        let t = self.theme();
        let screen = ctx.content_rect();
        let size = Vec2::new((screen.width() - 60.0).max(480.0), (screen.height() - 60.0).max(360.0));
        let mut close = false;
        let mut save = false;
        egui::Modal::new(Id::unique("sketch-window"))
            .backdrop_color(Color32::from_black_alpha(120))
            .frame(egui::Frame::popup(&ctx.global_style()).fill(t.editor_bg).inner_margin(Margin::same(14)))
            .show(ctx, |ui| {
                ui.set_width(size.x - 28.0);
                ui.set_height(size.y - 28.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(tr("Croquis")).font(FontId::new(18.0, FontFamily::Name("bold".into()))).color(t.text));
                    ui.add_space(16.0);
                    for (tool, label) in [
                        (Tool::Pen, tr("Stylo")),
                        (Tool::Pencil, tr("Crayon")),
                        (Tool::Marker, tr("Surligneur")),
                        (Tool::Ruler, tr("Règle")),
                        (Tool::Eraser, tr("Gomme")),
                    ] {
                        ui.selectable_value(&mut ed.tool, tool, label);
                    }
                    ui.add_space(10.0);
                    for c in PALETTE {
                        let (r, resp) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
                        let col = Color32::from_rgb(c[0], c[1], c[2]);
                        ui.painter().circle_filled(r.center(), 9.0, col);
                        ui.painter().circle_stroke(r.center(), 9.0, Stroke::new(1.0, t.border));
                        if ed.color == c {
                            ui.painter().circle_stroke(r.center(), 11.0, Stroke::new(2.0, t.accent));
                        }
                        if resp.clicked() {
                            ed.color = c;
                            if ed.tool == Tool::Eraser {
                                ed.tool = Tool::Pen;
                            }
                        }
                    }
                    ui.color_edit_button_srgb(&mut ed.color).on_hover_text(tr("Autre couleur"));
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new(tr("Épaisseur")).color(t.muted));
                    ui.add(egui::Slider::new(&mut ed.size, 1.0..=24.0).fixed_decimals(0).show_value(false));
                });
                ui.add_space(6.0);
                // canvas, keeping the paper's proportions
                let bottom_bar = 40.0;
                let avail = ui.available_size() - Vec2::new(0.0, bottom_bar);
                let k = (avail.x / ed.sketch.width).min(avail.y / ed.sketch.height).max(0.05);
                let canvas_size = Vec2::new(ed.sketch.width * k, ed.sketch.height * k);
                let (outer, _) = ui.allocate_exact_size(Vec2::new(avail.x, canvas_size.y), Sense::hover());
                let canvas = Rect::from_center_size(outer.center(), canvas_size);
                let resp = ui.interact(canvas, Id::unique("sketch-canvas"), Sense::drag());
                let p = ui.painter_at(canvas);
                p.rect_filled(canvas, 4.0, Color32::WHITE);
                ui.painter().rect_stroke(canvas, 4.0, Stroke::new(1.0, t.border), egui::StrokeKind::Outside);
                let to_screen = |q: [f32; 2]| Pos2::new(canvas.left() + q[0] * k, canvas.top() + q[1] * k);
                let to_canvas = |s: Pos2| [((s.x - canvas.left()) / k).clamp(0.0, ed.sketch.width), ((s.y - canvas.top()) / k).clamp(0.0, ed.sketch.height)];
                // input
                if let Some(pos) = resp.interact_pointer_pos() {
                    let q = to_canvas(pos);
                    if resp.drag_started() {
                        ed.undo.push(ed.sketch.strokes.clone());
                        ed.redo.clear();
                        if ed.tool != Tool::Eraser {
                            ed.current = Some(sketch::Stroke {
                                tool: ed.tool,
                                color: ed.color,
                                width: ed.tool.width(ed.size),
                                points: vec![q, q],
                            });
                        }
                    }
                    if resp.dragged() || resp.drag_started() {
                        match ed.tool {
                            Tool::Eraser => {
                                let r = ed.tool.width(ed.size) / 2.0;
                                ed.sketch.strokes.retain(|s| !sketch::hits(s, q, r));
                            }
                            Tool::Ruler => {
                                if let Some(c) = &mut ed.current {
                                    c.points[1] = q;
                                }
                            }
                            _ => {
                                if let Some(c) = &mut ed.current {
                                    let last = *c.points.last().unwrap();
                                    if (last[0] - q[0]).abs() + (last[1] - q[1]).abs() > 1.5 / k.max(0.2) {
                                        c.points.push(q);
                                    }
                                }
                            }
                        }
                    }
                }
                if resp.drag_stopped() {
                    if let Some(c) = ed.current.take() {
                        ed.sketch.strokes.push(c);
                    }
                }
                // drawing
                let draw = |st: &sketch::Stroke| {
                    let a = (st.tool.alpha() * 255.0) as u8;
                    let col = Color32::from_rgba_unmultiplied(st.color[0], st.color[1], st.color[2], a);
                    let pts: Vec<Pos2> = st.points.iter().map(|q| to_screen(*q)).collect();
                    if pts.len() == 1 || (pts.len() == 2 && pts[0] == pts[1]) {
                        p.circle_filled(pts[0], (st.width * k / 2.0).max(0.5), col);
                    } else {
                        p.add(egui::Shape::line(pts, Stroke::new(st.width * k, col)));
                    }
                };
                for st in &ed.sketch.strokes {
                    draw(st);
                }
                if let Some(c) = &ed.current {
                    draw(c);
                }
                if let Some(h) = resp.hover_pos() {
                    let r = (ed.tool.width(ed.size) * k / 2.0).max(1.5);
                    p.circle_stroke(h, r, Stroke::new(1.0, Color32::from_black_alpha(90)));
                }
                if ed.sketch.strokes.is_empty() && ed.current.is_none() {
                    p.text(canvas.center(), Align2::CENTER_CENTER, tr("Dessinez ici avec la souris, un stylet ou le pavé tactile"), FontId::proportional(15.0), Color32::from_gray(170));
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(!ed.undo.is_empty(), egui::Button::new(tr("Annuler le trait"))).clicked() {
                        let prev = ed.undo.pop().unwrap();
                        ed.redo.push(std::mem::replace(&mut ed.sketch.strokes, prev));
                    }
                    if ui.add_enabled(!ed.redo.is_empty(), egui::Button::new(tr("Rétablir"))).clicked() {
                        let next = ed.redo.pop().unwrap();
                        ed.undo.push(std::mem::replace(&mut ed.sketch.strokes, next));
                    }
                    if ui.add_enabled(!ed.sketch.strokes.is_empty(), egui::Button::new(tr("Tout effacer"))).clicked() {
                        ed.undo.push(std::mem::take(&mut ed.sketch.strokes));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let b = egui::Button::new(egui::RichText::new(tr("Enregistrer dans la note")).color(Color32::WHITE)).fill(t.accent).stroke(Stroke::NONE);
                        if ui.add(b).clicked() {
                            save = true;
                        }
                        if ui.button(tr("Annuler")).clicked() {
                            close = true;
                        }
                    });
                });
            });
        let ctrl_z = ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Z)));
        if ctrl_z {
            if let Some(prev) = ed.undo.pop() {
                ed.redo.push(std::mem::replace(&mut ed.sketch.strokes, prev));
            }
        }
        if save {
            self.save_sketch(ctx, ed);
            return;
        }
        if close || ctx.input(|i| i.key_pressed(Key::Escape)) {
            return;
        }
        self.sketcher = Some(ed);
    }

    fn save_sketch(&mut self, ctx: &egui::Context, ed: SketchEditor) {
        let dir = Library::attachment_dir(ed.note_id);
        let name = ed
            .name
            .clone()
            .unwrap_or_else(|| format!("{}-{}.png", tr("croquis"), chrono::Local::now().format("%Y%m%d-%H%M%S")));
        if let Err(e) = sketch::save(&ed.sketch, &dir.join(&name)) {
            self.notify(ctx, crate::i18n::trf("Croquis non enregistré : {e}", &[("e", &e)]));
            return;
        }
        self.thumbs.invalidate(ed.note_id, &name);
        if let Some(n) = self.lib.get_mut(ed.note_id) {
            if ed.name.is_none() {
                n.attachments.push(model::Attachment { name: name.clone(), text: String::new(), read: true });
            }
            n.modified = now();
        }
        if ed.name.is_none() {
            self.insert_image_ref(ed.note_id, &name);
        }
        self.layout_cache = None;
        self.mark_dirty(ctx);
        self.notify(ctx, tr("Croquis enregistré dans la note"));
    }

    /// Put `![](name)` on its own line at the cursor (or at the end), so the picture shows inline.
    pub(super) fn insert_image_ref(&mut self, note_id: u64, name: &str) {
        let cursor = if self.cursor_note == Some(note_id) { self.cursor } else { None };
        let Some(n) = self.lib.get_mut(note_id) else { return };
        let at = match cursor {
            Some((_, b)) => {
                // end of the cursor's line
                let byte = n.text.char_indices().nth(b).map(|(i, _)| i).unwrap_or(n.text.len());
                n.text[byte..].find('\n').map(|x| byte + x).unwrap_or(n.text.len())
            }
            None => n.text.len(),
        };
        let mut ins = String::new();
        if at > 0 && !n.text[..at].ends_with('\n') {
            ins.push('\n');
        }
        ins.push_str(&format!("![]({name})\n"));
        n.text.insert_str(at, &ins);
        n.modified = now();
    }
}

impl SketchEditor {
    /// A small drawing, used only for the screenshot check.
    pub(super) fn demo(&mut self) {
        let mut wave = Vec::new();
        for i in 0..60 {
            let x = 150.0 + i as f32 * 15.0;
            wave.push([x, 420.0 + (i as f32 / 6.0).sin() * 80.0]);
        }
        self.sketch.strokes.push(sketch::Stroke { tool: Tool::Pen, color: [0x1F, 0x22, 0x1F], width: 5.0, points: wave });
        self.sketch.strokes.push(sketch::Stroke { tool: Tool::Marker, color: [0xF2, 0xC6, 0x2F], width: 30.0, points: vec![[200.0, 250.0], [700.0, 250.0]] });
        self.sketch.strokes.push(sketch::Stroke { tool: Tool::Ruler, color: [0x2F, 0x7B, 0xC8], width: 4.0, points: vec![[150.0, 650.0], [1050.0, 650.0]] });
        let mut circle = Vec::new();
        for i in 0..=48 {
            let a = i as f32 / 48.0 * std::f32::consts::TAU;
            circle.push([950.0 + a.cos() * 90.0, 230.0 + a.sin() * 90.0]);
        }
        self.sketch.strokes.push(sketch::Stroke { tool: Tool::Pencil, color: [0xD8, 0x3B, 0x30], width: 3.0, points: circle });
    }
}
