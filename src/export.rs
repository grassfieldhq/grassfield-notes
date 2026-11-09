//! Export notes to TXT, Markdown, TextBundle, RTF, PDF, JPG, HTML, DOCX and ePub.

use crate::markdown;
use crate::model::{strip_markdown_line, Library, Note};
use std::io::Write;
use std::path::{Path, PathBuf};

mod render;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    Txt,
    Markdown,
    TextBundle,
    Rtf,
    Pdf,
    Jpg,
    Html,
    Docx,
    Epub,
}

pub const ALL: [Format; 9] = [
    Format::Txt,
    Format::Markdown,
    Format::TextBundle,
    Format::Rtf,
    Format::Pdf,
    Format::Jpg,
    Format::Html,
    Format::Docx,
    Format::Epub,
];

impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Format::Txt => crate::i18n::tr("Texte brut (.txt)"),
            Format::Markdown => "Markdown (.md)",
            Format::TextBundle => "TextBundle (.textbundle)",
            Format::Rtf => crate::i18n::tr("Texte enrichi (.rtf)"),
            Format::Pdf => "PDF (.pdf)",
            Format::Jpg => crate::i18n::tr("Image (.jpg)"),
            Format::Html => crate::i18n::tr("Page web (.html)"),
            Format::Docx => "Word (.docx)",
            Format::Epub => crate::i18n::tr("Livre numérique (.epub)"),
        }
    }
    pub fn ext(self) -> &'static str {
        match self {
            Format::Txt => "txt",
            Format::Markdown => "md",
            Format::TextBundle => "textbundle",
            Format::Rtf => "rtf",
            Format::Pdf => "pdf",
            Format::Jpg => "jpg",
            Format::Html => "html",
            Format::Docx => "docx",
            Format::Epub => "epub",
        }
    }
}

/// A simple block view of a note, shared by the rich formats.
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Heading(u8, String),
    Para(String),
    Bullet(String),
    Number(String, String),
    Todo(bool, String),
    Quote(String),
    Code(String),
    Rule,
    Blank,
    /// a picture line `![](file)`
    Image(String),
}

/// Remove inline Markdown markers, keeping the words.
pub fn strip_inline(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' | '`' => {}
            '~' if chars.peek() == Some(&'~') => {
                chars.next();
            }
            '=' if chars.peek() == Some(&'=') => {
                chars.next();
            }
            '[' if chars.peek() == Some(&'[') => {
                chars.next();
            }
            ']' if chars.peek() == Some(&']') => {
                chars.next();
            }
            _ => out.push(c),
        }
    }
    out
}

pub fn blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut in_code = false;
    for line in text.lines() {
        let t = line.trim_start();
        if t.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            out.push(Block::Code(line.to_string()));
            continue;
        }
        let hashes = t.chars().take_while(|c| *c == '#').count();
        let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
        let b = if let Some(name) = crate::model::image_ref(t) {
            Block::Image(name.to_string())
        } else if t.is_empty() {
            Block::Blank
        } else if (1..=6).contains(&hashes) && t[hashes..].starts_with(' ') {
            Block::Heading(hashes as u8, strip_inline(t[hashes + 1..].trim()))
        } else if let Some(r) = t.strip_prefix("- [ ] ").or_else(|| t.strip_prefix("* [ ] ")) {
            Block::Todo(false, strip_inline(r))
        } else if let Some(r) = ["- [x] ", "- [X] ", "* [x] ", "* [X] "].iter().find_map(|p| t.strip_prefix(p)) {
            Block::Todo(true, strip_inline(r))
        } else if let Some(r) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")).or_else(|| t.strip_prefix("+ ")) {
            Block::Bullet(strip_inline(r))
        } else if digits > 0 && t[digits..].starts_with(". ") {
            Block::Number(t[..digits].to_string(), strip_inline(&t[digits + 2..]))
        } else if let Some(r) = t.strip_prefix('>') {
            Block::Quote(strip_inline(r.trim_start()))
        } else if t == "---" || t == "***" || t == "___" {
            Block::Rule
        } else {
            Block::Para(strip_inline(t))
        };
        out.push(b);
    }
    out
}

pub fn image_placeholder(name: &str) -> String {
    crate::i18n::trf("[Image : {name}]", &[("name", &name)])
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn safe_name(title: &str, id: u64) -> String {
    let base: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
        .collect::<String>()
        .trim()
        .chars()
        .take(80)
        .collect();
    if base.is_empty() {
        format!("note-{id}")
    } else {
        base
    }
}

fn unique_path(dir: &Path, base: &str, ext: &str) -> PathBuf {
    let mut p = dir.join(format!("{base}.{ext}"));
    let mut k = 2;
    while p.exists() {
        p = dir.join(format!("{base} {k}.{ext}"));
        k += 1;
    }
    p
}

/// Write one note in the given format inside `dir`. Returns the path written.
pub fn export_note(note: &Note, format: Format, dir: &Path) -> std::io::Result<PathBuf> {
    export_note_with(note, format, dir, &Library::attachment_dir(note.id))
}

/// The file name an export gets by default ("Title.pdf").
pub fn file_name(note: &Note, format: Format) -> String {
    format!("{}.{}", safe_name(&note.title(), note.id), format.ext())
}

/// Same, with pictures looked up in `pics` (a Markdown file's own folder, for example).
pub fn export_note_with(note: &Note, format: Format, dir: &Path, pics: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let base = safe_name(&note.title(), note.id);
    let path = unique_path(dir, &base, format.ext());
    export_note_at(note, format, &path, pics)
}

/// Write one note at exactly `path` (chosen by the user in a Save panel).
pub fn export_note_at(note: &Note, format: Format, path: &Path, pics: &Path) -> std::io::Result<PathBuf> {
    let mut path = path.to_path_buf();
    if path.extension().map(|e| e.to_string_lossy().to_lowercase()) != Some(format.ext().to_string()) {
        let mut s = path.into_os_string();
        s.push(format!(".{}", format.ext()));
        path = PathBuf::from(s);
    }
    let title = note.title();
    match format {
        Format::Txt => {
            let txt: Vec<String> = note.text.lines().map(strip_markdown_line).collect();
            std::fs::write(&path, txt.join("\n"))?;
        }
        Format::Markdown => std::fs::write(&path, &note.text)?,
        Format::Html => std::fs::write(&path, markdown::to_html(&title, &note.text, Some(pics)))?,
        Format::Rtf => std::fs::write(&path, rtf(&note.text))?,
        Format::TextBundle => textbundle(note, &path)?,
        Format::Docx => std::fs::write(&path, docx(&title, &note.text)?)?,
        Format::Epub => std::fs::write(&path, epub(&title, &note.text, note.id, pics)?)?,
        Format::Pdf => std::fs::write(&path, render::pdf(&note.text, pics)?)?,
        Format::Jpg => render::jpg(&note.text, &path, pics)?,
    }
    Ok(path)
}

fn textbundle(note: &Note, path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path.join("assets"))?;
    let mut text = note.text.clone();
    for a in &note.attachments {
        let src = Library::attachment_dir(note.id).join(&a.name);
        let Some(name) = crate::sync::safe_file_name(&a.name) else { continue };
        if crate::safefs::copy_regular(&src, crate::safefs::Links::Refuse, &path.join("assets").join(&name), crate::attach::MAX_PICTURE_BYTES, 0o644).is_ok() {
            if text.contains(&format!("]({})", a.name)) {
                text = text.replace(&format!("]({})", a.name), &format!("](assets/{})", a.name));
            } else {
                text.push_str(&format!("\n\n![{}](assets/{})", a.name, a.name));
            }
        }
    }
    std::fs::write(path.join("text.markdown"), text)?;
    std::fs::write(
        path.join("info.json"),
        "{\n  \"version\": 2,\n  \"type\": \"net.daringfireball.markdown\",\n  \"transient\": false,\n  \"creatorIdentifier\": \"app.grassfield.notes\"\n}\n",
    )
}

fn rtf_text(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            c if (c as u32) < 128 => out.push(c),
            c => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{}?", *u as i16));
                }
            }
        }
    }
    out
}

pub fn rtf(text: &str) -> String {
    let mut out = String::from("{\\rtf1\\ansi\\ansicpg1252\\deff0{\\fonttbl{\\f0\\fswiss Helvetica;}{\\f1\\fmodern Courier New;}}\n\\fs24\n");
    for b in blocks(text) {
        let line = match b {
            Block::Heading(l, t) => {
                let size = match l { 1 => 40, 2 => 32, 3 => 28, _ => 26 };
                format!("{{\\b\\fs{size} {}}}\\par", rtf_text(&t))
            }
            Block::Para(t) => format!("{}\\par", rtf_text(&t)),
            Block::Bullet(t) => format!("\\li360 \\u8226? {}\\par\\li0", rtf_text(&t)),
            Block::Number(n, t) => format!("\\li360 {n}. {}\\par\\li0", rtf_text(&t)),
            Block::Todo(done, t) => {
                let mark = if done { "\\u9745?" } else { "\\u9744?" };
                if done {
                    format!("\\li360 {mark} {{\\strike {}}}\\par\\li0", rtf_text(&t))
                } else {
                    format!("\\li360 {mark} {}\\par\\li0", rtf_text(&t))
                }
            }
            Block::Quote(t) => format!("\\li360 {{\\i {}}}\\par\\li0", rtf_text(&t)),
            Block::Code(t) => format!("{{\\f1\\fs20 {}}}\\par", rtf_text(&t)),
            Block::Rule => "\\pard\\brdrb\\brdrs\\brdrw10\\brsp20 \\par\\pard".to_string(),
            Block::Blank => "\\par".to_string(),
            Block::Image(n) => format!("{{\\i {}}}\\par", rtf_text(&image_placeholder(&n))),
        };
        out.push_str(&line);
        out.push('\n');
    }
    out.push('}');
    out
}

/// Minimal ZIP writer (files stored without compression), enough for DOCX and ePub.
pub struct Zip {
    data: Vec<u8>,
    central: Vec<u8>,
    count: u16,
}

impl Zip {
    pub fn new() -> Self {
        Self { data: Vec::new(), central: Vec::new(), count: 0 }
    }

    pub fn add(&mut self, name: &str, content: &[u8]) {
        let crc = crc32fast::hash(content);
        let offset = self.data.len() as u32;
        let size = content.len() as u32;
        let n = name.as_bytes();
        // local file header
        self.data.extend_from_slice(&0x04034b50u32.to_le_bytes());
        // version needed, flags, method (0 = stored), time, date (1980-01-01)
        self.data.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0x21, 0]);
        self.data.extend_from_slice(&crc.to_le_bytes());
        self.data.extend_from_slice(&size.to_le_bytes());
        self.data.extend_from_slice(&size.to_le_bytes());
        self.data.extend_from_slice(&(n.len() as u16).to_le_bytes());
        self.data.extend_from_slice(&0u16.to_le_bytes());
        self.data.extend_from_slice(n);
        self.data.extend_from_slice(content);
        // central directory entry
        self.central.extend_from_slice(&0x02014b50u32.to_le_bytes());
        // made by, needed, flags, method, time, date
        self.central.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0x21, 0]);
        self.central.extend_from_slice(&crc.to_le_bytes());
        self.central.extend_from_slice(&size.to_le_bytes());
        self.central.extend_from_slice(&size.to_le_bytes());
        self.central.extend_from_slice(&(n.len() as u16).to_le_bytes());
        self.central.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        self.central.extend_from_slice(&offset.to_le_bytes());
        self.central.extend_from_slice(n);
        self.count += 1;
    }

    pub fn finish(mut self) -> Vec<u8> {
        let cd_offset = self.data.len() as u32;
        let cd_size = self.central.len() as u32;
        self.data.extend_from_slice(&self.central);
        self.data.extend_from_slice(&0x06054b50u32.to_le_bytes());
        self.data.extend_from_slice(&[0, 0, 0, 0]);
        self.data.extend_from_slice(&self.count.to_le_bytes());
        self.data.extend_from_slice(&self.count.to_le_bytes());
        self.data.extend_from_slice(&cd_size.to_le_bytes());
        self.data.extend_from_slice(&cd_offset.to_le_bytes());
        self.data.extend_from_slice(&[0, 0]);
        self.data
    }
}

fn docx_run(text: &str, props: &str) -> String {
    format!("<w:r><w:rPr>{props}</w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r>", xml(text))
}

pub fn docx(_title: &str, text: &str) -> std::io::Result<Vec<u8>> {
    let mut body = String::new();
    for b in blocks(text) {
        let p = match b {
            Block::Heading(l, t) => {
                let size = match l { 1 => 40, 2 => 32, 3 => 28, _ => 26 };
                format!("<w:p><w:pPr><w:spacing w:before=\"240\" w:after=\"120\"/></w:pPr>{}</w:p>", docx_run(&t, &format!("<w:b/><w:sz w:val=\"{size}\"/>")))
            }
            Block::Para(t) => format!("<w:p>{}</w:p>", docx_run(&t, "")),
            Block::Bullet(t) => format!("<w:p><w:pPr><w:ind w:left=\"360\"/></w:pPr>{}</w:p>", docx_run(&format!("• {t}"), "")),
            Block::Number(n, t) => format!("<w:p><w:pPr><w:ind w:left=\"360\"/></w:pPr>{}</w:p>", docx_run(&format!("{n}. {t}"), "")),
            Block::Todo(done, t) => {
                let mark = if done { "☑ " } else { "☐ " };
                let props = if done { "<w:strike/><w:color w:val=\"777777\"/>" } else { "" };
                format!("<w:p><w:pPr><w:ind w:left=\"360\"/></w:pPr>{}{}</w:p>", docx_run(mark, ""), docx_run(&t, props))
            }
            Block::Quote(t) => format!("<w:p><w:pPr><w:ind w:left=\"540\"/></w:pPr>{}</w:p>", docx_run(&t, "<w:i/><w:color w:val=\"666666\"/>")),
            Block::Code(t) => format!("<w:p>{}</w:p>", docx_run(&t, "<w:rFonts w:ascii=\"Courier New\" w:hAnsi=\"Courier New\"/><w:sz w:val=\"20\"/>")),
            Block::Rule => "<w:p><w:pPr><w:pBdr><w:bottom w:val=\"single\" w:sz=\"6\" w:space=\"1\" w:color=\"CCCCCC\"/></w:pBdr></w:pPr></w:p>".to_string(),
            Block::Blank => "<w:p/>".to_string(),
            Block::Image(n) => format!("<w:p>{}</w:p>", docx_run(&image_placeholder(&n), "<w:i/><w:color w:val=\"777777\"/>")),
        };
        body.push_str(&p);
    }
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{body}<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1134\" w:right=\"1134\" w:bottom=\"1134\" w:left=\"1134\"/></w:sectPr></w:body></w:document>"
    );
    let mut z = Zip::new();
    z.add("[Content_Types].xml", b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>");
    z.add("_rels/.rels", b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>");
    z.add("word/document.xml", document.as_bytes());
    Ok(z.finish())
}

pub fn epub(title: &str, text: &str, id: u64, pics: &Path) -> std::io::Result<Vec<u8>> {
    let mut body = String::new();
    let mut in_list = false;
    let mut images: Vec<(String, Vec<u8>, &str)> = Vec::new();
    for b in blocks(text) {
        let is_item = matches!(b, Block::Bullet(_) | Block::Todo(..) | Block::Number(..));
        if is_item && !in_list {
            body.push_str("<ul>\n");
            in_list = true;
        } else if !is_item && in_list {
            body.push_str("</ul>\n");
            in_list = false;
        }
        let h = match b {
            Block::Heading(l, t) => format!("<h{l}>{}</h{l}>", xml(&t)),
            Block::Para(t) => format!("<p>{}</p>", xml(&t)),
            Block::Bullet(t) => format!("<li>{}</li>", xml(&t)),
            Block::Number(n, t) => format!("<li>{n}. {}</li>", xml(&t)),
            Block::Todo(done, t) => format!("<li>{} {}</li>", if done { "☑" } else { "☐" }, xml(&t)),
            Block::Quote(t) => format!("<blockquote><p>{}</p></blockquote>", xml(&t)),
            Block::Code(t) => format!("<pre><code>{}</code></pre>", xml(&t)),
            Block::Rule => "<hr/>".to_string(),
            Block::Blank => String::new(),
            Block::Image(n) => {
                // only pictures inside the note's own folder; the stored name and type come
                // from the picture's content, never from the text
                let picture = crate::safefs::confine(pics, &n).and_then(|p| crate::attach::read_picture(&p, crate::safefs::Links::Refuse));
                match picture {
                    Some((bytes, kind)) => {
                        let k = images.len();
                        let ext = kind.ext();
                        images.push((format!("images/img{k}.{ext}"), bytes, kind.mime()));
                        format!("<p><img src=\"images/img{k}.{ext}\" alt=\"{}\" style=\"max-width:100%\"/></p>", xml(&n))
                    }
                    None => format!("<p><em>{}</em></p>", xml(&image_placeholder(&n))),
                }
            }
        };
        body.push_str(&h);
        body.push('\n');
    }
    if in_list {
        body.push_str("</ul>\n");
    }
    let t = xml(title);
    let lang = crate::i18n::lang().code();
    let chapter = format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xml:lang=\"{lang}\"><head><title>{t}</title></head><body>\n{body}</body></html>\n");
    let nav = format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>{t}</title></head><body><nav epub:type=\"toc\"><ol><li><a href=\"note.xhtml\">{t}</a></li></ol></nav></body></html>\n");
    let manifest_imgs: String = images
        .iter()
        .enumerate()
        .map(|(k, (href, _, mt))| format!("<item id=\"img{k}\" href=\"{href}\" media-type=\"{mt}\"/>"))
        .collect();
    let modified = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let opf = format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"uid\"><metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:identifier id=\"uid\">grassfield-note-{id}</dc:identifier><dc:title>{t}</dc:title><dc:language>{lang}</dc:language><meta property=\"dcterms:modified\">{modified}</meta></metadata><manifest><item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/><item id=\"note\" href=\"note.xhtml\" media-type=\"application/xhtml+xml\"/>{manifest_imgs}</manifest><spine><itemref idref=\"note\"/></spine></package>\n");
    let mut z = Zip::new();
    z.add("mimetype", b"application/epub+zip");
    z.add("META-INF/container.xml", b"<?xml version=\"1.0\"?>\n<container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\"><rootfiles><rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/></rootfiles></container>");
    z.add("OEBPS/content.opf", opf.as_bytes());
    z.add("OEBPS/nav.xhtml", nav.as_bytes());
    z.add("OEBPS/note.xhtml", chapter.as_bytes());
    for (href, bytes, _) in &images {
        z.add(&format!("OEBPS/{href}"), bytes);
    }
    Ok(z.finish())
}

#[allow(dead_code)]
pub fn write_all(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut f = std::fs::File::create(path)?;
    f.write_all(bytes)
}
