//! Drawing on top of the styled text: check boxes, quote bars, rules and inline pictures.
//! Shared by the note editor and the standalone Markdown file view.

use super::*;

pub(super) struct Input<'a> {
    pub text: &'a str,
    pub galley: &'a egui::Galley,
    pub gpos: Pos2,
    pub clip: Rect,
    pub resp: &'a egui::Response,
    pub width: f32,
    pub font_size: f32,
    pub theme: &'a Theme,
    /// (file name as written in the text, texture, is an editable sketch)
    pub pics: &'a [(String, egui::TextureHandle, bool)],
    pub editable: bool,
}

#[derive(Default)]
pub(super) struct Output {
    /// byte offset of the line whose check box was clicked
    pub clicked_box: Option<usize>,
    /// picture double-clicked: (name, is sketch)
    pub open_pic: Option<(String, bool)>,
}

/// Display size of a picture: as wide as the text at most, never taller than 420.
pub(super) fn pic_size(tex: &egui::TextureHandle, w: f32) -> Vec2 {
    let s = tex.size_vec2();
    let mut dw = s.x.min(w);
    let mut dh = dw * s.y / s.x;
    if dh > 420.0 {
        dh = 420.0;
        dw = dh * s.x / s.y;
    }
    Vec2::new(dw, dh)
}

pub(super) fn paint(ui: &egui::Ui, inp: &Input) -> Output {
    let mut out = Output::default();
    let t = inp.theme;
    let (galley, gpos, resp, w) = (inp.galley, inp.gpos, inp.resp, inp.width);
    let pointer = resp.interact_pointer_pos();
    let painter = ui.painter_at(inp.clip);
    let mut ci = 0usize;
    let mut bi = 0usize;
    let mut in_fence = false;
    for line in inp.text.split_inclusive('\n') {
        let trimmed = line.trim_start_matches([' ', '\t']);
        let fence_line = trimmed.starts_with("```");
        if fence_line {
            in_fence = !in_fence;
        }
        let indent = line.len() - trimmed.len();
        let state = if in_fence || fence_line {
            None
        } else if trimmed.starts_with("- [ ] ") || trimmed.starts_with("* [ ] ") {
            Some(false)
        } else if ["- [x] ", "- [X] ", "* [x] ", "* [X] "].iter().any(|p| trimmed.starts_with(p)) {
            Some(true)
        } else {
            None
        };
        if let Some(done) = state {
            let s = ci + indent; // indent is ASCII, so chars == bytes
            let r1 = galley.pos_from_cursor(egui::text::CCursor::new(s));
            let r2 = galley.pos_from_cursor(egui::text::CCursor::new(s + 5));
            let area = Rect::from_min_max(r1.min, Pos2::new(r2.max.x, r1.max.y)).translate(gpos.to_vec2());
            painter.rect_filled(area.expand2(Vec2::new(0.5, 0.0)), 0.0, t.editor_bg);
            let size = (inp.font_size * 0.95).round();
            let bx = Rect::from_center_size(Pos2::new(area.right() - size / 2.0 - 2.0, area.center().y), Vec2::splat(size));
            if done {
                painter.rect_filled(bx, 4.0, t.accent);
                let a = bx.left_top();
                let pts = [
                    Pos2::new(a.x + size * 0.24, a.y + size * 0.52),
                    Pos2::new(a.x + size * 0.43, a.y + size * 0.70),
                    Pos2::new(a.x + size * 0.77, a.y + size * 0.32),
                ];
                painter.line_segment([pts[0], pts[1]], Stroke::new(2.0, Color32::WHITE));
                painter.line_segment([pts[1], pts[2]], Stroke::new(2.0, Color32::WHITE));
            } else {
                painter.rect_stroke(bx, 4.0, Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
            }
            if resp.clicked() && inp.editable {
                if let Some(p) = pointer {
                    if area.expand(3.0).contains(p) {
                        out.clicked_box = Some(bi);
                    }
                }
            }
        }
        if !in_fence && !fence_line {
            // horizontal rules
            let tr = line.trim();
            if tr == "---" || tr == "***" || tr == "___" {
                let r1 = galley.pos_from_cursor(egui::text::CCursor::new(ci));
                let y = r1.center().y + gpos.y;
                painter.hline((gpos.x + 36.0)..=(gpos.x + w), y, Stroke::new(1.0, t.border));
            }
            // quotes: replace the ">" with a green bar spanning the wrapped lines
            if trimmed.starts_with('>') {
                let s = ci + indent;
                let r1 = galley.pos_from_cursor(egui::text::CCursor::new(s));
                let r2 = galley.pos_from_cursor(egui::text::CCursor::new(s + 1));
                let end_c = ci + line.trim_end_matches('\n').chars().count();
                let r3 = galley.pos_from_cursor(egui::text::CCursor::new(end_c));
                let mask = Rect::from_min_max(r1.min, Pos2::new(r2.max.x, r1.max.y)).translate(gpos.to_vec2());
                painter.rect_filled(mask, 0.0, t.editor_bg);
                let bar = Rect::from_min_max(
                    Pos2::new(mask.left() + 1.0, mask.top() + 2.0),
                    Pos2::new(mask.left() + 4.0, r3.max.y + gpos.y - 2.0),
                );
                painter.rect_filled(bar, 1.5, t.accent);
            }
            // inline pictures and sketches
            if let Some(name) = crate::model::image_ref(line) {
                if let Some((_, tex, is_sketch)) = inp.pics.iter().find(|p| p.0 == name) {
                    let row = galley.pos_from_cursor(egui::text::CCursor::new(ci)).translate(gpos.to_vec2());
                    let mask = Rect::from_min_max(Pos2::new(gpos.x - 2.0, row.top()), Pos2::new(gpos.x + w + 2.0, row.bottom()));
                    painter.rect_filled(mask, 0.0, t.editor_bg);
                    let sz = pic_size(tex, w);
                    let r = Rect::from_min_size(Pos2::new(gpos.x, row.top() + 6.0), sz);
                    painter.image(tex.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    let hovered = resp.hover_pos().map(|p| r.contains(p)).unwrap_or(false);
                    painter.rect_stroke(
                        r,
                        4.0,
                        Stroke::new(if hovered { 2.0 } else { 1.0 }, if hovered { t.accent } else { t.border }),
                        egui::StrokeKind::Outside,
                    );
                    if hovered && inp.editable {
                        let hint = if *is_sketch {
                            crate::i18n::tr("Double-clic : modifier le croquis")
                        } else {
                            crate::i18n::tr("Double-clic : recadrer / redimensionner")
                        };
                        let g = painter.layout_no_wrap(hint.to_string(), FontId::proportional(11.5), Color32::WHITE);
                        let br = Rect::from_min_size(r.left_bottom() + Vec2::new(8.0, -g.size().y - 14.0), g.size() + Vec2::new(12.0, 6.0));
                        painter.rect_filled(br, 6.0, Color32::from_black_alpha(170));
                        painter.galley(br.min + Vec2::new(6.0, 3.0), g, Color32::WHITE);
                    }
                    if resp.double_clicked() && hovered {
                        out.open_pic = Some((name.to_string(), *is_sketch));
                    }
                }
            }
        }
        ci += line.chars().count();
        bi += line.len();
    }
    out
}
