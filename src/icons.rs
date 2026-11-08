//! Fifteen original app icons: a meadow mark on different backgrounds and shapes.

pub struct IconStyle {
    pub name: &'static str,
    bg: [u8; 3],
    bg2: [u8; 3],
    mark: [u8; 3],
    /// 0 = circle, 1 = rounded square, 2 = soft square
    shape: u8,
}

pub const ICONS: [IconStyle; 15] = [
    IconStyle { name: "Prairie", bg: [0x5C, 0xAE, 0x62], bg2: [0x3B, 0x85, 0x41], mark: [255, 255, 255], shape: 1 },
    IconStyle { name: "Mousse", bg: [0x2F, 0x45, 0x34], bg2: [0x1D, 0x2C, 0x21], mark: [0x9F, 0xE0, 0xA0], shape: 1 },
    IconStyle { name: "Menthe", bg: [0x7D, 0xD8, 0xB4], bg2: [0x3F, 0xB0, 0x88], mark: [255, 255, 255], shape: 0 },
    IconStyle { name: "Tournesol", bg: [0xF6, 0xC8, 0x4C], bg2: [0xE0, 0x96, 0x1A], mark: [0x4A, 0x32, 0x0A], shape: 1 },
    IconStyle { name: "Ciel", bg: [0x7C, 0xB8, 0xF0], bg2: [0x3A, 0x86, 0xD4], mark: [255, 255, 255], shape: 2 },
    IconStyle { name: "Lavande", bg: [0xB5, 0xA3, 0xEC], bg2: [0x7A, 0x63, 0xC9], mark: [255, 255, 255], shape: 1 },
    IconStyle { name: "Corail", bg: [0xF5, 0x8E, 0x76], bg2: [0xD8, 0x60, 0x4A], mark: [255, 255, 255], shape: 0 },
    IconStyle { name: "Graphite", bg: [0x3A, 0x3A, 0x3A], bg2: [0x1E, 0x1E, 0x1E], mark: [0x8B, 0xC3, 0x4A], shape: 1 },
    IconStyle { name: "Sable", bg: [0xF1, 0xE6, 0xCF], bg2: [0xD9, 0xC6, 0x9F], mark: [0x5E, 0x8A, 0x3C], shape: 2 },
    IconStyle { name: "Océan", bg: [0x2A, 0x7B, 0xB0], bg2: [0x17, 0x4A, 0x75], mark: [0xC6, 0xF0, 0xFF], shape: 0 },
    IconStyle { name: "Rose", bg: [0xF4, 0xB6, 0xC9], bg2: [0xDE, 0x7B, 0x9C], mark: [255, 255, 255], shape: 1 },
    IconStyle { name: "Papier", bg: [0xFF, 0xFF, 0xFF], bg2: [0xE9, 0xEC, 0xE6], mark: [0x2E, 0x7D, 0x32], shape: 1 },
    IconStyle { name: "Nuit", bg: [0x20, 0x26, 0x3A], bg2: [0x0E, 0x12, 0x1E], mark: [0x82, 0xD0, 0x9E], shape: 2 },
    IconStyle { name: "Sarcelle", bg: [0x2F, 0xA8, 0xA0], bg2: [0x17, 0x6E, 0x69], mark: [255, 255, 255], shape: 0 },
    IconStyle { name: "Automne", bg: [0xE0, 0x7B, 0x39], bg2: [0x9E, 0x3F, 0x14], mark: [0xFF, 0xE9, 0xB0], shape: 1 },
];

fn seg_dist(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (vx, vy) = (bx - ax, by - ay);
    let t = (((px - ax) * vx + (py - ay) * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
    let (dx, dy) = (px - ax - vx * t, py - ay - vy * t);
    (dx * dx + dy * dy).sqrt()
}

/// RGBA pixels of icon `i` at `size`×`size`.
pub fn rgba(i: usize, size: u32) -> Vec<u8> {
    let st = &ICONS[i % ICONS.len()];
    let s = size as f32;
    let mut out = vec![0u8; (size * size * 4) as usize];
    let half = s / 2.0;
    let radius = match st.shape {
        0 => half - s * 0.04,
        1 => s * 0.24,
        _ => s * 0.12,
    };
    let inset = s * 0.04;
    // three grass blades, in units of the icon size
    let blades = [
        (0.40, 0.70, 0.32, 0.40),
        (0.50, 0.72, 0.50, 0.28),
        (0.60, 0.70, 0.69, 0.42),
    ];
    let bw = s * 0.045;
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            // distance to the rounded shape edge (negative inside)
            let (qx, qy) = ((fx - half).abs() - (half - inset - radius), (fy - half).abs() - (half - inset - radius));
            let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - radius;
            let cover = (0.5 - outside).clamp(0.0, 1.0);
            if cover <= 0.0 {
                continue;
            }
            let t = fy / s;
            let mut col = [0f32; 3];
            for k in 0..3 {
                col[k] = st.bg[k] as f32 * (1.0 - t) + st.bg2[k] as f32 * t;
            }
            // meadow line
            let ground = (s * 0.72 - fy).abs() < s * 0.025 && (fx - half).abs() < s * 0.2;
            let mut d = f32::MAX;
            for (ax, ay, bx, by) in blades {
                d = d.min(seg_dist(fx, fy, ax * s, ay * s, bx * s, by * s));
            }
            let mark = ((bw - d) + 0.5).clamp(0.0, 1.0).max(if ground { 1.0 } else { 0.0 });
            for k in 0..3 {
                col[k] = col[k] * (1.0 - mark) + st.mark[k] as f32 * mark;
            }
            let idx = ((y * size + x) * 4) as usize;
            out[idx] = col[0] as u8;
            out[idx + 1] = col[1] as u8;
            out[idx + 2] = col[2] as u8;
            out[idx + 3] = (cover * 255.0) as u8;
        }
    }
    out
}

pub fn color_image(i: usize, size: u32) -> eframe::egui::ColorImage {
    eframe::egui::ColorImage::from_rgba_unmultiplied([size as usize, size as usize], &rgba(i, size))
}

pub fn icon_data(i: usize) -> eframe::egui::IconData {
    eframe::egui::IconData { rgba: rgba(i, 128), width: 128, height: 128 }
}
