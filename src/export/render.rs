//! Lays a note out on pages and draws it as a PDF (embedded fonts) or a JPG image.

use super::{blocks, Block};
use crate::fonts::doc_fonts;
use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

struct Run {
    x: f32,
    /// baseline, measured from the top of the page
    y: f32,
    size: f32,
    bold: bool,
    text: String,
    color: [u8; 3],
    strike: bool,
}

enum Deco {
    Box { x: f32, y: f32, size: f32, done: bool },
    Line { x1: f32, x2: f32, y: f32 },
    Bar { x: f32, y1: f32, y2: f32 },
    Image { x: f32, y: f32, w: f32, h: f32, idx: usize },
}

#[derive(Default)]
struct Page {
    runs: Vec<Run>,
    decos: Vec<Deco>,
}

struct Fonts<'a> {
    regular: FontRef<'a>,
    bold: FontRef<'a>,
}

impl<'a> Fonts<'a> {
    fn load() -> std::io::Result<Fonts<'static>> {
        let (r, b) = doc_fonts();
        let err = |_| std::io::Error::other(crate::i18n::tr("police introuvable pour l'export"));
        Ok(Fonts {
            regular: FontRef::try_from_slice(r).map_err(err)?,
            bold: FontRef::try_from_slice(b).map_err(err)?,
        })
    }
    fn font(&self, bold: bool) -> &FontRef<'a> {
        if bold {
            &self.bold
        } else {
            &self.regular
        }
    }
    /// Width of `s` when the font's em square is `size` units (how PDF sizes text).
    fn width(&self, s: &str, bold: bool, size: f32) -> f32 {
        let f = self.font(bold);
        let upem = f.units_per_em().unwrap_or(1000.0);
        s.chars().map(|c| f.h_advance_unscaled(f.glyph_id(c))).sum::<f32>() * size / upem
    }

    /// ab_glyph scales by line height; convert an em size to that.
    fn em_scale(&self, bold: bool, size: f32) -> PxScale {
        let f = self.font(bold);
        let upem = f.units_per_em().unwrap_or(1000.0);
        PxScale::from(size * f.height_unscaled() / upem)
    }
}

/// Page geometry in the output's own units (points for PDF, pixels for JPG).
struct Geometry {
    width: f32,
    /// None = one long page
    height: Option<f32>,
    margin: f32,
    base: f32,
    /// output units per image pixel
    px: f32,
}

fn layout(text: &str, g: &Geometry, fonts: &Fonts, dir: &Path, images: &mut Vec<image::RgbImage>) -> Vec<Page> {
    let mut pages = vec![Page::default()];
    let mut y = g.margin;
    let usable = g.width - 2.0 * g.margin;
    let black = [0x22, 0x26, 0x22];
    let gray = [0x77, 0x7c, 0x75];
    let green = [0x3b, 0x7f, 0x41];

    for b in blocks(text) {
        if let Block::Image(name) = &b {
            if let Ok(img) = image::open(dir.join(name)) {
                let img = img.to_rgb8();
                let (iw, ih) = (img.width() as f32, img.height() as f32);
                let mut w = (iw * g.px).min(usable);
                let mut h = w * ih / iw;
                let max_h = g.height.map(|ph| (ph - 2.0 * g.margin) * 0.6).unwrap_or(900.0);
                if h > max_h {
                    h = max_h;
                    w = h * iw / ih;
                }
                y += g.base * 0.4;
                if let Some(ph) = g.height {
                    if y + h > ph - g.margin {
                        pages.push(Page::default());
                        y = g.margin;
                    }
                }
                let idx = images.len();
                images.push(img);
                pages.last_mut().unwrap().decos.push(Deco::Image { x: g.margin, y, w, h, idx });
                y += h + g.base * 0.6;
                continue;
            }
        }
        let (size, bold, indent, color, before, after) = match &b {
            Block::Heading(l, _) => {
                let k = match l { 1 => 1.8, 2 => 1.45, 3 => 1.2, _ => 1.05 };
                (g.base * k, true, 0.0, black, g.base * 0.9, g.base * 0.35)
            }
            Block::Bullet(_) | Block::Number(..) | Block::Todo(..) => (g.base, false, g.base * 1.7, black, 0.0, g.base * 0.15),
            Block::Quote(_) => (g.base, false, g.base * 1.3, gray, g.base * 0.2, g.base * 0.3),
            Block::Code(_) => (g.base * 0.9, false, g.base * 0.8, [0x44, 0x44, 0x44], 0.0, 0.0),
            Block::Rule => (g.base, false, 0.0, gray, g.base * 0.5, g.base * 0.5),
            Block::Blank => (g.base, false, 0.0, black, 0.0, 0.0),
            Block::Para(_) => (g.base, false, 0.0, black, 0.0, g.base * 0.35),
            Block::Image(_) => (g.base, false, 0.0, gray, 0.0, g.base * 0.35),
        };
        let line_h = size * 1.45;
        let words: String = match &b {
            Block::Heading(_, t) | Block::Para(t) | Block::Bullet(t) | Block::Quote(t) | Block::Code(t) => t.clone(),
            Block::Number(_, t) | Block::Todo(_, t) => t.clone(),
            Block::Rule | Block::Blank => String::new(),
            Block::Image(n) => super::image_placeholder(n),
        };
        // wrap
        let mut lines: Vec<String> = Vec::new();
        let mut cur = String::new();
        for w in words.split(' ') {
            let candidate = if cur.is_empty() { w.to_string() } else { format!("{cur} {w}") };
            if !cur.is_empty() && fonts.width(&candidate, bold, size) > usable - indent {
                lines.push(std::mem::take(&mut cur));
                cur = w.to_string();
            } else {
                cur = candidate;
            }
        }
        if !cur.is_empty() || lines.is_empty() {
            lines.push(cur);
        }
        let block_h = match b {
            Block::Blank => line_h * 0.5,
            Block::Rule => g.base,
            _ => line_h * lines.len() as f32,
        };
        y += before;
        if let Some(h) = g.height {
            if y + block_h.min(h - 2.0 * g.margin) > h - g.margin {
                pages.push(Page::default());
                y = g.margin;
            }
        }
        let page = pages.last_mut().unwrap();
        let top = y;
        match &b {
            Block::Rule => {
                page.decos.push(Deco::Line { x1: g.margin, x2: g.width - g.margin, y: y + g.base * 0.5 });
            }
            Block::Blank => {}
            _ => {
                for (i, l) in lines.iter().enumerate() {
                    let baseline = y + size * 1.1;
                    if i == 0 {
                        match &b {
                            Block::Bullet(_) => page.runs.push(Run { x: g.margin + g.base * 0.5, y: baseline, size, bold: false, text: "•".into(), color: green, strike: false }),
                            Block::Number(n, _) => page.runs.push(Run { x: g.margin, y: baseline, size, bold: false, text: format!("{n}."), color: green, strike: false }),
                            Block::Todo(done, _) => page.decos.push(Deco::Box { x: g.margin + g.base * 0.15, y: baseline - size * 0.8, size: size * 0.85, done: *done }),
                            _ => {}
                        }
                    }
                    let (c, strike) = match &b {
                        Block::Todo(true, _) => (gray, true),
                        _ => (color, false),
                    };
                    page.runs.push(Run { x: g.margin + indent, y: baseline, size, bold, text: l.clone(), color: c, strike });
                    y += line_h;
                }
                if let Block::Quote(_) = b {
                    page.decos.push(Deco::Bar { x: g.margin + g.base * 0.3, y1: top + size * 0.2, y2: y - size * 0.1 });
                }
            }
        }
        if matches!(b, Block::Blank | Block::Rule) {
            y += block_h;
        }
        y += after;
    }
    pages
}

fn deflate(data: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = e.write_all(data);
    e.finish().unwrap_or_default()
}

/// A4 PDF with the note's fonts embedded (so accents and symbols print correctly).
pub fn pdf(text: &str, dir: &Path) -> std::io::Result<Vec<u8>> {
    let fonts = Fonts::load()?;
    let g = Geometry { width: 595.0, height: Some(842.0), margin: 56.0, base: 11.0, px: 0.5 };
    let mut images = Vec::new();
    let pages = layout(text, &g, &fonts, dir, &mut images);
    // glyphs used per font: gid -> (char, width/1000)
    let mut used: [BTreeMap<u16, (char, i64)>; 2] = [BTreeMap::new(), BTreeMap::new()];
    let mut contents = Vec::new();
    for page in &pages {
        let mut c = String::new();
        for d in &page.decos {
            match d {
                Deco::Line { x1, x2, y } => c.push_str(&format!("0.8 0.8 0.8 RG 0.8 w {x1:.2} {:.2} m {x2:.2} {:.2} l S\n", 842.0 - y, 842.0 - y)),
                Deco::Bar { x, y1, y2 } => c.push_str(&format!("0.3 0.6 0.32 rg {x:.2} {:.2} 2.5 {:.2} re f\n", 842.0 - y2, y2 - y1)),
                Deco::Image { x, y, w, h, idx } => c.push_str(&format!("q {w:.2} 0 0 {h:.2} {x:.2} {:.2} cm /Im{idx} Do Q\n", 842.0 - y - h)),
                Deco::Box { x, y, size, done } => {
                    let by = 842.0 - y - size;
                    if *done {
                        c.push_str(&format!("0.3 0.6 0.32 rg {x:.2} {by:.2} {size:.2} {size:.2} re f\n"));
                        c.push_str(&format!(
                            "1 1 1 RG 1.4 w {:.2} {:.2} m {:.2} {:.2} l {:.2} {:.2} l S\n",
                            x + size * 0.22, by + size * 0.5, x + size * 0.43, by + size * 0.28, x + size * 0.78, by + size * 0.72
                        ));
                    } else {
                        c.push_str(&format!("0.3 0.6 0.32 RG 1 w {x:.2} {by:.2} {size:.2} {size:.2} re S\n"));
                    }
                }
            }
        }
        for r in &page.runs {
            let fi = r.bold as usize;
            let font = fonts.font(r.bold);
            let upem = font.units_per_em().unwrap_or(1000.0);
            let mut hex = String::new();
            for ch in r.text.chars() {
                let gid = font.glyph_id(ch).0;
                let w = (font.h_advance_unscaled(ab_glyph::GlyphId(gid)) * 1000.0 / upem).round() as i64;
                used[fi].entry(gid).or_insert((ch, w));
                hex.push_str(&format!("{gid:04X}"));
            }
            let [cr, cg, cb] = r.color.map(|v| v as f32 / 255.0);
            c.push_str(&format!(
                "BT /F{} {:.2} Tf {cr:.3} {cg:.3} {cb:.3} rg 1 0 0 1 {:.2} {:.2} Tm <{hex}> Tj ET\n",
                fi + 1, r.size, r.x, 842.0 - r.y
            ));
            if r.strike {
                let w = fonts.width(&r.text, r.bold, r.size);
                let sy = 842.0 - r.y + r.size * 0.3;
                c.push_str(&format!("{cr:.3} {cg:.3} {cb:.3} RG 0.7 w {:.2} {sy:.2} m {:.2} {sy:.2} l S\n", r.x, r.x + w));
            }
        }
        contents.push(c);
    }

    // objects: 1 catalog, 2 pages, then fonts, then page + content pairs
    let mut objs: Vec<Vec<u8>> = Vec::new();
    let (reg_bytes, bold_bytes) = doc_fonts();
    let mut font_refs = Vec::new();
    for (fi, bytes) in [reg_bytes, bold_bytes].iter().enumerate() {
        let font = fonts.font(fi == 1);
        let upem = font.units_per_em().unwrap_or(1000.0);
        let k = 1000.0 / upem;
        let asc = (font.ascent_unscaled() * k).round() as i64;
        let desc = (font.descent_unscaled() * k).round() as i64;
        let name = if fi == 1 { "GrassfieldSans-Bold" } else { "GrassfieldSans" };
        let base = 3 + objs.len();
        // base: Type0, base+1: CIDFont, base+2: descriptor, base+3: font file, base+4: ToUnicode
        let mut w = String::new();
        for (gid, (_, wd)) in &used[fi] {
            w.push_str(&format!("{gid} [{wd}] "));
        }
        let mut cmap = String::from("/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def /CMapName /Adobe-Identity-UCS def /CMapType 2 def 1 begincodespacerange <0000> <FFFF> endcodespacerange\n");
        let entries: Vec<_> = used[fi].iter().collect();
        for chunk in entries.chunks(100) {
            cmap.push_str(&format!("{} beginbfchar\n", chunk.len()));
            for (gid, (ch, _)) in chunk {
                let mut buf = [0u16; 2];
                let u: String = ch.encode_utf16(&mut buf).iter().map(|v| format!("{v:04X}")).collect();
                cmap.push_str(&format!("<{gid:04X}> <{u}>\n"));
            }
            cmap.push_str("endbfchar\n");
        }
        cmap.push_str("endcmap CMapName currentdict /CMap defineresource pop end end\n");
        objs.push(format!("<< /Type /Font /Subtype /Type0 /BaseFont /{name} /Encoding /Identity-H /DescendantFonts [{} 0 R] /ToUnicode {} 0 R >>", base + 1, base + 4).into_bytes());
        objs.push(format!("<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{name} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {} 0 R /DW 500 /W [{w}] /CIDToGIDMap /Identity >>", base + 2).into_bytes());
        objs.push(format!("<< /Type /FontDescriptor /FontName /{name} /Flags 32 /FontBBox [-300 {desc} 1200 {asc}] /ItalicAngle 0 /Ascent {asc} /Descent {desc} /CapHeight {} /StemV 80 /FontFile2 {} 0 R >>", (asc as f32 * 0.7) as i64, base + 3).into_bytes());
        let packed = deflate(bytes);
        let mut ff = format!("<< /Length {} /Length1 {} /Filter /FlateDecode >>\nstream\n", packed.len(), bytes.len()).into_bytes();
        ff.extend_from_slice(&packed);
        ff.extend_from_slice(b"\nendstream");
        objs.push(ff);
        let packed = deflate(cmap.as_bytes());
        let mut cm = format!("<< /Length {} /Filter /FlateDecode >>\nstream\n", packed.len()).into_bytes();
        cm.extend_from_slice(&packed);
        cm.extend_from_slice(b"\nendstream");
        objs.push(cm);
        font_refs.push(base);
    }
    // pictures as JPEG image objects
    let mut xobj = String::new();
    for (i, img) in images.iter().enumerate() {
        let mut jpg = Vec::new();
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 88);
        if image::DynamicImage::ImageRgb8(img.clone()).write_with_encoder(enc).is_err() {
            continue;
        }
        let id = 3 + objs.len();
        let mut o = format!(
            "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
            img.width(), img.height(), jpg.len()
        ).into_bytes();
        o.extend_from_slice(&jpg);
        o.extend_from_slice(b"\nendstream");
        objs.push(o);
        xobj.push_str(&format!("/Im{i} {id} 0 R "));
    }
    let mut kids = Vec::new();
    for c in &contents {
        let page_id = 3 + objs.len();
        kids.push(page_id);
        objs.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 {} 0 R /F2 {} 0 R >> /XObject << {} >> >> /Contents {} 0 R >>", font_refs[0], font_refs[1], xobj, page_id + 1).into_bytes());
        let packed = deflate(c.as_bytes());
        let mut st = format!("<< /Length {} /Filter /FlateDecode >>\nstream\n", packed.len()).into_bytes();
        st.extend_from_slice(&packed);
        st.extend_from_slice(b"\nendstream");
        objs.push(st);
    }
    let kids_s: Vec<String> = kids.iter().map(|k| format!("{k} 0 R")).collect();
    let mut all: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids_s.join(" "), kids.len()).into_bytes(),
    ];
    all.extend(objs);

    let mut out = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in all.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", all.len() + 1).as_bytes());
    for off in offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", all.len() + 1).as_bytes());
    Ok(out)
}

/// The whole note drawn as one tall JPG image.
pub fn jpg(text: &str, path: &Path, dir: &Path) -> std::io::Result<()> {
    let fonts = Fonts::load()?;
    let g = Geometry { width: 1200.0, height: None, margin: 100.0, base: 26.0, px: 1.0 };
    let mut images = Vec::new();
    let pages = layout(text, &g, &fonts, dir, &mut images);
    let page = &pages[0];
    let pic_bottom = page.decos.iter().map(|d| if let Deco::Image { y, h, .. } = d { y + h } else { 0.0 }).fold(0.0f32, f32::max);
    let bottom = page.runs.iter().map(|r| r.y + r.size).fold(pic_bottom, f32::max).max(400.0);
    let (w, h) = (g.width as u32, (bottom + g.margin) as u32);
    let mut img = image::RgbImage::from_pixel(w, h, image::Rgb([255, 255, 255]));
    let mut blend = |x: i32, y: i32, color: [u8; 3], a: f32| {
        if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 || a <= 0.0 {
            return;
        }
        let p = img.get_pixel_mut(x as u32, y as u32);
        for i in 0..3 {
            p.0[i] = (p.0[i] as f32 * (1.0 - a) + color[i] as f32 * a).round() as u8;
        }
    };
    let green = [0x4c, 0x9a, 0x52];
    let rect = |blend: &mut dyn FnMut(i32, i32, [u8; 3], f32), x: f32, y: f32, rw: f32, rh: f32, c: [u8; 3]| {
        for yy in y as i32..(y + rh) as i32 {
            for xx in x as i32..(x + rw) as i32 {
                blend(xx, yy, c, 1.0);
            }
        }
    };
    for d in &page.decos {
        match d {
            Deco::Line { x1, x2, y } => rect(&mut blend, *x1, *y, x2 - x1, 2.0, [0xdd, 0xdd, 0xdd]),
            Deco::Bar { x, y1, y2 } => rect(&mut blend, *x, *y1, 5.0, y2 - y1, green),
            Deco::Image { x, y, w, h, idx } => {
                let pic = image::imageops::resize(&images[*idx], (*w as u32).max(1), (*h as u32).max(1), image::imageops::FilterType::Triangle);
                for (px, py, c) in pic.enumerate_pixels() {
                    blend(*x as i32 + px as i32, *y as i32 + py as i32, c.0, 1.0);
                }
            }
            Deco::Box { x, y, size, done } => {
                if *done {
                    rect(&mut blend, *x, *y, *size, *size, green);
                    // check mark
                    let pts = [(0.22, 0.5), (0.43, 0.72), (0.78, 0.28)];
                    for seg in 0..2 {
                        let (ax, ay) = pts[seg];
                        let (bx, by) = pts[seg + 1];
                        for s in 0..60 {
                            let t = s as f32 / 59.0;
                            let px = x + size * (ax + (bx - ax) * t);
                            let py = y + size * (ay + (by - ay) * t);
                            rect(&mut blend, px - 1.5, py - 1.5, 3.0, 3.0, [255, 255, 255]);
                        }
                    }
                } else {
                    rect(&mut blend, *x, *y, *size, 2.5, green);
                    rect(&mut blend, *x, y + size - 2.5, *size, 2.5, green);
                    rect(&mut blend, *x, *y, 2.5, *size, green);
                    rect(&mut blend, x + size - 2.5, *y, 2.5, *size, green);
                }
            }
        }
    }
    for r in &page.runs {
        let font = fonts.font(r.bold);
        let scale = fonts.em_scale(r.bold, r.size);
        let sf = font.as_scaled(scale);
        let mut x = r.x;
        for ch in r.text.chars() {
            let id = sf.glyph_id(ch);
            let glyph = id.with_scale_and_position(scale, ab_glyph::point(x, r.y));
            if let Some(outline) = font.outline_glyph(glyph) {
                let bb = outline.px_bounds();
                outline.draw(|gx, gy, cov| blend(bb.min.x as i32 + gx as i32, bb.min.y as i32 + gy as i32, r.color, cov));
            }
            x += sf.h_advance(id);
        }
        if r.strike {
            let wdt = fonts.width(&r.text, r.bold, r.size);
            rect(&mut blend, r.x, r.y - r.size * 0.3, wdt, 2.0, r.color);
        }
    }
    img.save_with_format(path, image::ImageFormat::Jpeg).map_err(std::io::Error::other)
}
