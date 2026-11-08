//! Finds system fonts at runtime (used by the window and by PDF / JPG export).

use std::path::PathBuf;
use std::sync::OnceLock;

pub const REGULAR: &[&str] = &[
    "NotoSans-Regular.ttf",
    "LiberationSans-Regular.ttf",
    "DejaVuSans.ttf",
    "Arial.ttf",
    "arial.ttf",
    "segoeui.ttf",
];
pub const BOLD: &[&str] = &[
    "NotoSans-Bold.ttf",
    "LiberationSans-Bold.ttf",
    "DejaVuSans-Bold.ttf",
    "Arial Bold.ttf",
    "arialbd.ttf",
    "segoeuib.ttf",
];
pub const ITALIC: &[&str] = &[
    "NotoSans-Italic.ttf",
    "LiberationSans-Italic.ttf",
    "DejaVuSans-Oblique.ttf",
    "Arial Italic.ttf",
    "ariali.ttf",
    "segoeuii.ttf",
];
pub const SERIF: &[&str] = &[
    "NotoSerif-Regular.ttf",
    "LiberationSerif-Regular.ttf",
    "DejaVuSerif.ttf",
    "Georgia.ttf",
    "georgia.ttf",
];
pub const MONO: &[&str] = &[
    "NotoSansMono-Regular.ttf",
    "LiberationMono-Regular.ttf",
    "DejaVuSansMono.ttf",
    "consola.ttf",
    "Menlo.ttc",
    "Courier New.ttf",
];

/// Font folders on Linux, macOS and Windows.
fn font_dirs() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = [
        "/usr/share/fonts/noto",
        "/usr/share/fonts/liberation",
        "/usr/share/fonts/TTF",
        "/usr/share/fonts/truetype/noto",
        "/usr/share/fonts/truetype/liberation",
        "/usr/share/fonts/truetype/dejavu",
        "/usr/share/fonts/dejavu",
        "/usr/share/fonts/google-noto",
        "/usr/share/fonts/liberation-sans",
        "/usr/share/fonts/liberation-serif",
        "/usr/share/fonts/liberation-mono",
        "/System/Library/Fonts/Supplemental",
        "/System/Library/Fonts",
        "/Library/Fonts",
        "C:\\Windows\\Fonts",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    if let Ok(w) = std::env::var("WINDIR") {
        v.push(PathBuf::from(w).join("Fonts"));
    }
    if let Ok(l) = std::env::var("LOCALAPPDATA") {
        v.push(PathBuf::from(l).join("Microsoft").join("Windows").join("Fonts"));
    }
    if let Some(h) = dirs::home_dir() {
        v.push(h.join("Library").join("Fonts"));
        v.push(h.join(".local").join("share").join("fonts"));
    }
    v
}

pub fn find_font(names: &[&str]) -> Option<Vec<u8>> {
    let dirs = font_dirs();
    for name in names {
        for d in &dirs {
            let p = d.join(name);
            if let Ok(bytes) = std::fs::read(&p) {
                // only accept fonts the rasteriser can read
                if ab_glyph::FontRef::try_from_slice(&bytes).is_ok() {
                    return Some(bytes);
                }
            }
        }
    }
    None
}

/// Regular and bold fonts for documents; falls back to the font bundled with the window toolkit.
pub fn doc_fonts() -> &'static (Vec<u8>, Vec<u8>) {
    static FONTS: OnceLock<(Vec<u8>, Vec<u8>)> = OnceLock::new();
    FONTS.get_or_init(|| {
        let fallback = || epaint_fallback().to_vec();
        let regular = find_font(REGULAR).unwrap_or_else(fallback);
        let bold = find_font(BOLD).unwrap_or_else(|| regular.clone());
        (regular, bold)
    })
}

fn epaint_fallback() -> &'static [u8] {
    // Ubuntu-Light ships inside egui's default fonts.
    static BYTES: OnceLock<Vec<u8>> = OnceLock::new();
    BYTES.get_or_init(|| {
        let defs = eframe::egui::FontDefinitions::default();
        defs.font_data
            .get("Ubuntu-Light")
            .map(|d| (*d.font).as_ref().to_vec())
            .unwrap_or_default()
    })
}
