//! Hand-drawn sketches: strokes stored as data (so they stay editable) and drawn into a PNG.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const CANVAS_W: f32 = 1200.0;
pub const CANVAS_H: f32 = 800.0;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Pen,
    Pencil,
    Marker,
    Eraser,
    Ruler,
}

impl Tool {
    /// How opaque the tool's ink is.
    pub fn alpha(self) -> f32 {
        match self {
            Tool::Pen | Tool::Ruler => 1.0,
            Tool::Pencil => 0.7,
            Tool::Marker => 0.38,
            Tool::Eraser => 0.0,
        }
    }
    /// Line thickness for a given size setting.
    pub fn width(self, size: f32) -> f32 {
        match self {
            Tool::Pen | Tool::Ruler => size,
            Tool::Pencil => (size * 0.6).max(1.0),
            Tool::Marker => size * 3.5,
            Tool::Eraser => size * 4.0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Stroke {
    pub tool: Tool,
    pub color: [u8; 3],
    /// thickness in canvas units
    pub width: f32,
    pub points: Vec<[f32; 2]>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Sketch {
    pub width: f32,
    pub height: f32,
    pub strokes: Vec<Stroke>,
}

/// Most points a sketch may hold.
pub const MAX_POINTS: usize = 100_000;
/// Widest line a sketch may draw.
pub const MAX_WIDTH: f32 = 400.0;
/// Most pixel visits drawing a sketch may take (each segment visits the pixels of its own
/// box): hand-drawn sketches use a few million, a hostile one is refused.
const MAX_WORK: f64 = 1.0e8;

impl Sketch {
    /// Sketch data may come from a synced repository: keep it within safe bounds. The size is
    /// clamped to the canvas, widths and points to finite values near it, and a sketch with
    /// too many points or too much drawing work is refused.
    pub fn sanitized(mut self) -> Option<Sketch> {
        if !self.width.is_finite() || !self.height.is_finite() {
            return None;
        }
        self.width = self.width.round().clamp(1.0, CANVAS_W);
        self.height = self.height.round().clamp(1.0, CANVAS_H);
        let (w, h) = (self.width, self.height);
        let mut points = 0;
        let mut work = 0.0f64;
        for st in &mut self.strokes {
            if !st.width.is_finite() {
                return None;
            }
            st.width = st.width.clamp(0.1, MAX_WIDTH);
            st.points.retain(|p| p[0].is_finite() && p[1].is_finite());
            for p in &mut st.points {
                p[0] = p[0].clamp(-MAX_WIDTH, w + MAX_WIDTH);
                p[1] = p[1].clamp(-MAX_WIDTH, h + MAX_WIDTH);
            }
            points += st.points.len();
            if points > MAX_POINTS {
                return None;
            }
            for (a, b) in segments(st) {
                let (x0, y0, x1, y1) = seg_box(a, b, st.width / 2.0, w as u32, h as u32);
                work += (x1.saturating_sub(x0) as f64 + 1.0) * (y1.saturating_sub(y0) as f64 + 1.0);
            }
            if work > MAX_WORK {
                return None;
            }
        }
        Some(self)
    }
}

/// The segments of a stroke (a single point is a segment of length zero).
fn segments(st: &Stroke) -> Vec<([f32; 2], [f32; 2])> {
    if st.points.len() == 1 {
        vec![(st.points[0], st.points[0])]
    } else {
        st.points.windows(2).map(|w| (w[0], w[1])).collect()
    }
}

/// The pixels a segment can touch, inside a `w` x `h` canvas (inclusive bounds).
fn seg_box(a: [f32; 2], b: [f32; 2], half: f32, w: u32, h: u32) -> (u32, u32, u32, u32) {
    let x0 = ((a[0].min(b[0]) - half - 1.0).floor().max(0.0)) as u32;
    let y0 = ((a[1].min(b[1]) - half - 1.0).floor().max(0.0)) as u32;
    let x1 = ((a[0].max(b[0]) + half + 1.0).ceil().min(w as f32 - 1.0)).max(0.0) as u32;
    let y1 = ((a[1].max(b[1]) + half + 1.0).ceil().min(h as f32 - 1.0)).max(0.0) as u32;
    (x0, y0, x1, y1)
}

impl Default for Sketch {
    fn default() -> Self {
        Self { width: CANVAS_W, height: CANVAS_H, strokes: Vec::new() }
    }
}

/// The editable data sits next to the picture: `name.png.sketch.json`.
pub fn sidecar(png: &Path) -> PathBuf {
    let mut s = png.as_os_str().to_owned();
    s.push(".sketch.json");
    PathBuf::from(s)
}

pub fn load(png: &Path) -> Option<Sketch> {
    let bytes = crate::safefs::read_regular(&sidecar(png), crate::safefs::Links::Refuse, 16 * 1024 * 1024).ok()?;
    serde_json::from_slice::<Sketch>(&bytes).ok()?.sanitized()
}

fn seg_dist2(px: f32, py: f32, a: [f32; 2], b: [f32; 2]) -> f32 {
    let (vx, vy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = vx * vx + vy * vy;
    let t = if len2 > 0.0 { (((px - a[0]) * vx + (py - a[1]) * vy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let (dx, dy) = (px - a[0] - vx * t, py - a[1] - vy * t);
    dx * dx + dy * dy
}

/// True when a point is within `radius` of the stroke's line.
pub fn hits(stroke: &Stroke, p: [f32; 2], radius: f32) -> bool {
    let r = radius + stroke.width / 2.0;
    let r2 = r * r;
    match stroke.points.len() {
        0 => false,
        1 => seg_dist2(p[0], p[1], stroke.points[0], stroke.points[0]) <= r2,
        _ => stroke.points.windows(2).any(|w| seg_dist2(p[0], p[1], w[0], w[1]) <= r2),
    }
}

/// Draw the sketch on white paper. The size is clamped to the canvas, so a sketch from
/// elsewhere can never ask for a huge picture.
pub fn rasterize(sk: &Sketch) -> image::RgbaImage {
    let w = (sk.width.round() as u32).clamp(1, CANVAS_W as u32);
    let h = (sk.height.round() as u32).clamp(1, CANVAS_H as u32);
    let mut img = image::RgbaImage::from_pixel(w, h, image::Rgba([255, 255, 255, 255]));
    let mut cover = vec![0f32; (w * h) as usize];
    for st in &sk.strokes {
        if st.tool == Tool::Eraser || st.points.is_empty() {
            continue;
        }
        let half = st.width.clamp(0.0, MAX_WIDTH) / 2.0;
        // one coverage value per pixel and stroke (the closest segment wins), so
        // translucent ink doesn't pile up; each segment only visits the pixels of its box
        let mut touched: Vec<u32> = Vec::new();
        for (a, b) in segments(st) {
            let (x0, y0, x1, y1) = seg_box(a, b, half, w, h);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let d2 = seg_dist2(x as f32 + 0.5, y as f32 + 0.5, a, b);
                    let c = (half + 0.5 - d2.sqrt()).clamp(0.0, 1.0);
                    let i = (y * w + x) as usize;
                    if c > cover[i] {
                        if cover[i] == 0.0 {
                            touched.push(i as u32);
                        }
                        cover[i] = c;
                    }
                }
            }
        }
        let alpha = st.tool.alpha();
        for i in touched {
            let i = i as usize;
            let a = cover[i] * alpha;
            cover[i] = 0.0;
            let p = img.get_pixel_mut(i as u32 % w, i as u32 / w);
            for k in 0..3 {
                p.0[k] = (p.0[k] as f32 * (1.0 - a) + st.color[k] as f32 * a).round() as u8;
            }
        }
    }
    img
}

/// Write the picture and its editable data.
pub fn save(sk: &Sketch, png: &Path) -> std::io::Result<()> {
    if let Some(d) = png.parent() {
        std::fs::create_dir_all(d)?;
    }
    rasterize(sk).save(png).map_err(std::io::Error::other)?;
    let json = serde_json::to_string(sk).map_err(std::io::Error::other)?;
    std::fs::write(sidecar(png), json)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(points: Vec<[f32; 2]>, width: f32) -> Stroke {
        Stroke { tool: Tool::Pen, color: [0, 0, 0], width, points }
    }

    #[test]
    fn sketches_from_elsewhere_stay_within_bounds() {
        // any size: clamped to the canvas, so saving never asks for a huge picture
        let huge = Sketch { width: 1.0e9, height: 3.0e8, strokes: vec![stroke(vec![[1.0e12, -5.0], [10.0, 10.0]], 1.0e6)] };
        let ok = huge.sanitized().unwrap();
        assert_eq!((ok.width, ok.height), (CANVAS_W, CANVAS_H));
        assert_eq!(ok.strokes[0].width, MAX_WIDTH);
        assert!(ok.strokes[0].points[0][0] <= CANVAS_W + MAX_WIDTH);
        let img = rasterize(&Sketch { width: 1.0e9, height: 1.0e9, strokes: vec![] });
        assert_eq!((img.width(), img.height()), (CANVAS_W as u32, CANVAS_H as u32));
        // too many points, or too much drawing work: refused
        let many = Sketch { strokes: vec![stroke(vec![[1.0, 1.0]; MAX_POINTS + 1], 2.0)], ..Default::default() };
        assert!(many.sanitized().is_none());
        let diagonals: Vec<[f32; 2]> = (0..2000).map(|k| if k % 2 == 0 { [0.0, 0.0] } else { [CANVAS_W, CANVAS_H] }).collect();
        assert!(Sketch { strokes: vec![stroke(diagonals, 2.0)], ..Default::default() }.sanitized().is_none());
        // a hand-drawn sketch is kept as it is
        let line: Vec<[f32; 2]> = (0..500).map(|k| [100.0 + k as f32, 200.0 + (k as f32 / 10.0).sin() * 30.0]).collect();
        let normal = Sketch { strokes: vec![stroke(line, 4.0); 40], ..Default::default() };
        assert_eq!(normal.clone().sanitized(), Some(normal));
    }

    #[test]
    fn a_hostile_sidecar_is_refused_on_load() {
        let dir = std::env::temp_dir().join(format!("gf-sketch-bounds-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("croquis.png");
        std::fs::write(sidecar(&png), r#"{"width":1e9,"height":1e9,"strokes":[]}"#).unwrap();
        let sk = load(&png).unwrap();
        assert_eq!((sk.width, sk.height), (CANVAS_W, CANVAS_H));
        std::fs::write(sidecar(&png), r#"{"width":1e999,"height":5,"strokes":[]}"#).unwrap();
        assert!(load(&png).is_none());
    }
}
