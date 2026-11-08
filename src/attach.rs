//! Attachments: copy scanned documents / images / PDFs next to the library, read their text
//! in the background (tesseract for images, pdftotext for PDFs) and show thumbnails.

use crate::model::{Attachment, Library};
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};

pub const IMAGE_EXT: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff", "gif"];
pub const DOC_EXT: &[&str] = &["pdf"];

pub fn is_attachable(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let e = e.to_lowercase();
            IMAGE_EXT.contains(&e.as_str()) || DOC_EXT.contains(&e.as_str())
        })
        .unwrap_or(false)
}

pub fn path_of(note_id: u64, a: &Attachment) -> PathBuf {
    Library::attachment_dir(note_id).join(&a.name)
}

/// Copy a file into the note's attachment folder; returns the stored attachment.
pub fn store(note_id: u64, src: &Path) -> std::io::Result<Attachment> {
    let dir = Library::attachment_dir(note_id);
    std::fs::create_dir_all(&dir)?;
    let file = src.file_name().and_then(|n| n.to_str()).unwrap_or("document").to_string();
    let (stem, ext) = match file.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), format!(".{e}")),
        None => (file.clone(), String::new()),
    };
    let mut name = file.clone();
    let mut k = 2;
    while dir.join(&name).exists() {
        name = format!("{stem} {k}{ext}");
        k += 1;
    }
    std::fs::copy(src, dir.join(&name))?;
    Ok(Attachment { name, text: String::new(), read: false })
}

fn has_tool(name: &str) -> bool {
    crate::platform::command(name).arg("-v").output().is_ok() || crate::platform::command(name).arg("--version").output().is_ok()
}

/// Which text readers are installed on this computer.
#[derive(Clone, Copy)]
pub struct Tools {
    pub ocr: bool,
    pub pdf: bool,
}

pub fn tools() -> Tools {
    static T: std::sync::OnceLock<Tools> = std::sync::OnceLock::new();
    *T.get_or_init(|| Tools { ocr: has_tool("tesseract"), pdf: has_tool("pdftotext") })
}

fn ocr_lang() -> String {
    let out = crate::platform::command("tesseract").arg("--list-langs").output();
    let langs = out.map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
    let have = |l: &str| langs.lines().any(|x| x.trim() == l);
    // the interface language first, then English, then French
    let mut picked: Vec<&str> = Vec::new();
    for code in [crate::i18n::lang().ocr_code(), "eng", "fra"] {
        if have(code) && !picked.contains(&code) {
            picked.push(code);
        }
    }
    if picked.is_empty() {
        "eng".into()
    } else {
        picked.join("+")
    }
}

/// Read the text of an image (OCR) or a PDF. Empty when nothing could be read.
pub fn read_text(path: &Path) -> String {
    let lower = path.to_string_lossy().to_lowercase();
    let out = if lower.ends_with(".pdf") {
        crate::platform::command("pdftotext").arg("-layout").arg(path).arg("-").output()
    } else {
        crate::platform::command("tesseract").arg(path).arg("stdout").arg("-l").arg(ocr_lang()).output()
    };
    match out {
        Ok(o) if o.status.success() => {
            let t = String::from_utf8_lossy(&o.stdout);
            // tidy: collapse runs of blank lines
            let mut lines: Vec<&str> = Vec::new();
            for l in t.lines() {
                let l = l.trim_end();
                if l.trim().is_empty() && lines.last().map(|x| x.trim().is_empty()).unwrap_or(true) {
                    continue;
                }
                lines.push(l);
            }
            lines.join("\n").trim().to_string()
        }
        _ => String::new(),
    }
}

/// Background text reading for attachments.
pub struct Reader {
    tx: Sender<(u64, String, String)>,
    rx: Receiver<(u64, String, String)>,
    pending: HashSet<(u64, String)>,
}

impl Default for Reader {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self { tx, rx, pending: HashSet::new() }
    }
}

impl Reader {
    pub fn is_reading(&self, note_id: u64, name: &str) -> bool {
        self.pending.contains(&(note_id, name.to_string()))
    }

    pub fn busy(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Start reading every attachment that hasn't been read yet.
    pub fn queue_unread(&mut self, lib: &Library, ctx: &egui::Context) {
        for n in &lib.notes {
            for a in &n.attachments {
                if !a.read && !self.pending.contains(&(n.id, a.name.clone())) {
                    self.pending.insert((n.id, a.name.clone()));
                    let tx = self.tx.clone();
                    let path = path_of(n.id, a);
                    let (id, name) = (n.id, a.name.clone());
                    let ctx = ctx.clone();
                    std::thread::spawn(move || {
                        let text = read_text(&path);
                        let _ = tx.send((id, name, text));
                        ctx.request_repaint();
                    });
                }
            }
        }
    }

    /// Store finished results; returns true when something changed.
    pub fn poll(&mut self, lib: &mut Library) -> bool {
        let mut changed = false;
        while let Ok((id, name, text)) = self.rx.try_recv() {
            self.pending.remove(&(id, name.clone()));
            if let Some(n) = lib.get_mut(id) {
                if let Some(a) = n.attachments.iter_mut().find(|a| a.name == name) {
                    a.text = text;
                    a.read = true;
                    changed = true;
                }
            }
        }
        changed
    }
}

/// Thumbnails of image attachments, loaded once.
#[derive(Default)]
pub struct Thumbs {
    map: HashMap<(u64, String), Option<egui::TextureHandle>>,
}

impl Thumbs {
    /// Forget a thumbnail after its file changed.
    pub fn invalidate(&mut self, note_id: u64, name: &str) {
        self.map.remove(&(note_id, name.to_string()));
    }

    pub fn get(&mut self, ctx: &egui::Context, note_id: u64, a: &Attachment) -> Option<egui::TextureHandle> {
        if !a.is_image() {
            return None;
        }
        let key = (note_id, a.name.clone());
        if let Some(t) = self.map.get(&key) {
            return t.clone();
        }
        let tex = image::open(path_of(note_id, a)).ok().map(|img| {
            let small = img.thumbnail(1000, 1000).to_rgba8();
            let size = [small.width() as usize, small.height() as usize];
            let ci = egui::ColorImage::from_rgba_unmultiplied(size, small.as_raw());
            ctx.load_texture(format!("thumb-{note_id}-{}-{}", a.name, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)), ci, egui::TextureOptions::LINEAR)
        });
        self.map.insert(key, tex.clone());
        tex
    }
}
