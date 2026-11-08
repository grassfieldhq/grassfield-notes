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
    let s = std::fs::read_to_string(sidecar(png)).ok()?;
    serde_json::from_str(&s).ok()
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

/// Draw the sketch on white paper.
pub fn rasterize(sk: &Sketch) -> image::RgbaImage {
    let (w, h) = (sk.width as u32, sk.height as u32);
    let mut img = image::RgbaImage::from_pixel(w, h, image::Rgba([255, 255, 255, 255]));
    for st in &sk.strokes {
        if st.tool == Tool::Eraser || st.points.is_empty() {
            continue;
        }
        let half = st.width / 2.0;
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in &st.points {
            x0 = x0.min(p[0]);
            y0 = y0.min(p[1]);
            x1 = x1.max(p[0]);
            y1 = y1.max(p[1]);
        }
        let bx0 = ((x0 - half - 1.0).floor().max(0.0)) as u32;
        let by0 = ((y0 - half - 1.0).floor().max(0.0)) as u32;
        let bx1 = ((x1 + half + 1.0).ceil().min(w as f32 - 1.0)).max(0.0) as u32;
        let by1 = ((y1 + half + 1.0).ceil().min(h as f32 - 1.0)).max(0.0) as u32;
        let alpha = st.tool.alpha();
        let segs: Vec<([f32; 2], [f32; 2])> = if st.points.len() == 1 {
            vec![(st.points[0], st.points[0])]
        } else {
            st.points.windows(2).map(|w| (w[0], w[1])).collect()
        };
        for y in by0..=by1 {
            for x in bx0..=bx1 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let mut d2 = f32::MAX;
                for (a, b) in &segs {
                    d2 = d2.min(seg_dist2(px, py, *a, *b));
                    if d2 == 0.0 {
                        break;
                    }
                }
                // one coverage value per stroke, so translucent ink doesn't pile up
                let cover = (half + 0.5 - d2.sqrt()).clamp(0.0, 1.0);
                if cover <= 0.0 {
                    continue;
                }
                let a = cover * alpha;
                let p = img.get_pixel_mut(x, y);
                for i in 0..3 {
                    p.0[i] = (p.0[i] as f32 * (1.0 - a) + st.color[i] as f32 * a).round() as u8;
                }
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
