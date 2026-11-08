//! Live Markdown styling for the editor: turns note text into a styled layout job.

use crate::model::{image_ref, tag_spans, EditorFont};
use std::collections::HashMap;
use crate::theme::Theme;
use eframe::egui::text::{ByteIndex, LayoutJob, LayoutSection, TextFormat};
use eframe::egui::{Color32, FontFamily, FontId, Stroke};

#[derive(Clone, Copy, Default, PartialEq)]
struct Flags {
    bold: bool,
    italic: bool,
    strike: bool,
    highlight: bool,
    code: bool,
    link: bool,
    tag: bool,
    marker: bool,
    accent_marker: bool,
    heading: u8,
    quote: bool,
    done: bool,
    fence: bool,
}

pub struct MdStyle {
    /// Height reserved for each inline picture (by file name).
    pub images: HashMap<String, f32>,
    /// Reading mode: hide the Markdown symbols (**, #, ~~, `…).
    pub hide_markup: bool,
    pub theme: Theme,
    pub size: f32,
    pub font: EditorFont,
    pub dim_markup: bool,
    pub spacing: f32,
}

pub fn body_family(font: EditorFont) -> FontFamily {
    match font {
        EditorFont::Sans => FontFamily::Proportional,
        EditorFont::Serif => FontFamily::Name("serif".into()),
        EditorFont::Mono => FontFamily::Monospace,
    }
}

fn heading_scale(level: u8) -> f32 {
    match level {
        1 => 1.75,
        2 => 1.4,
        3 => 1.2,
        4 => 1.08,
        _ => 1.0,
    }
}

impl MdStyle {
    fn format(&self, f: Flags) -> TextFormat {
        let t = &self.theme;
        let scale = if f.heading > 0 { heading_scale(f.heading) } else { 1.0 };
        let size = if f.code || f.fence { self.size * 0.92 * scale } else { self.size * scale };
        let family = if f.code || f.fence {
            FontFamily::Monospace
        } else if f.bold || f.heading > 0 {
            FontFamily::Name("bold".into())
        } else if f.italic || f.quote {
            if self.font == EditorFont::Mono {
                FontFamily::Monospace
            } else {
                FontFamily::Name("italic".into())
            }
        } else {
            body_family(self.font)
        };
        let mut color = if f.accent_marker {
            t.accent
        } else if f.marker {
            if self.dim_markup {
                t.faint
            } else {
                t.muted
            }
        } else if f.link || f.tag {
            t.accent_text
        } else if f.done || f.quote {
            t.muted
        } else {
            t.text
        };
        if f.heading > 0 && !f.marker {
            color = t.text;
        }
        let mut background = Color32::TRANSPARENT;
        if f.highlight {
            background = t.highlight_bg;
        } else if f.code || f.fence {
            background = t.code_bg;
        } else if f.tag {
            background = t.tag_bg;
        }
        let line_height = Some((self.size * scale * 1.55 * self.spacing).round());
        if self.hide_markup && f.marker && !f.accent_marker {
            // practically invisible and zero-width, but still part of the text
            return TextFormat {
                font_id: FontId::new(0.5, family),
                color: Color32::TRANSPARENT,
                line_height,
                ..Default::default()
            };
        }
        TextFormat {
            font_id: FontId::new(size, family),
            color,
            background,
            line_height,
            italics: false,
            underline: if f.link && !f.marker {
                Stroke::new(1.0, t.accent.gamma_multiply(0.6))
            } else {
                Stroke::NONE
            },
            strikethrough: if (f.strike || f.done) && !f.marker {
                Stroke::new(1.0, color)
            } else {
                Stroke::NONE
            },
            ..Default::default()
        }
    }
}

/// Byte length of a block prefix and the flags for the line content.
fn block_prefix(line: &str) -> (usize, Flags, Vec<(usize, usize, Flags)>) {
    let mut f = Flags::default();
    let mut marks: Vec<(usize, usize, Flags)> = Vec::new();
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let rest = &line[indent..];
    let marker = Flags { marker: true, ..Default::default() };
    let accent = Flags { accent_marker: true, ..Default::default() };

    // headings
    let hashes = rest.chars().take_while(|c| *c == '#').count();
    if indent == 0 && (1..=6).contains(&hashes) && rest[hashes..].starts_with(' ') {
        f.heading = hashes as u8;
        let mut m = marker;
        m.heading = hashes as u8;
        marks.push((0, hashes + 1, m));
        return (hashes + 1, f, marks);
    }
    // todos
    for (p, done) in [
        ("- [ ] ", false),
        ("* [ ] ", false),
        ("- [x] ", true),
        ("- [X] ", true),
        ("* [x] ", true),
        ("* [X] ", true),
    ] {
        if rest.starts_with(p) {
            f.done = done;
            // "- [ ]" is drawn over by a check box in the editor; keep it plain and same-coloured.
            marks.push((indent, indent + 5, accent));
            marks.push((indent + 5, indent + 6, Flags::default()));
            return (indent + 6, f, marks);
        }
    }
    if let Some(r) = rest.strip_prefix('>') {
        f.quote = true;
        let n = if r.starts_with(' ') { 2 } else { 1 };
        marks.push((indent, indent + n, accent));
        return (indent + n, f, marks);
    }
    for p in ["- ", "* ", "+ "] {
        if rest.starts_with(p) {
            marks.push((indent, indent + 1, accent));
            return (indent + 1, f, marks);
        }
    }
    // numbered lists
    let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && digits < 4 && rest[digits..].starts_with(". ") {
        marks.push((indent, indent + digits + 1, accent));
        return (indent + digits + 1, f, marks);
    }
    let tr = rest.trim_end();
    if tr == "---" || tr == "***" || tr == "___" {
        marks.push((indent, line.len(), marker));
        return (line.len(), f, marks);
    }
    (indent, f, marks)
}

/// Mark inline styling for the chars of `content`. Returns per-char flags.
fn inline_flags(content: &str, base: Flags) -> Vec<(usize, Flags)> {
    let chars: Vec<(usize, char)> = content.char_indices().collect();
    let n = chars.len();
    let mut flags = vec![base; n];
    let mut protected = vec![false; n];
    let byte_to_idx = |b: usize| chars.iter().position(|(bi, _)| *bi >= b).unwrap_or(n);

    // code spans
    let mut i = 0;
    while i < n {
        if chars[i].1 == '`' {
            if let Some(j) = (i + 1..n).find(|&j| chars[j].1 == '`') {
                for k in i..=j {
                    flags[k].code = true;
                    protected[k] = true;
                }
                flags[i].marker = true;
                flags[j].marker = true;
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    // wiki links [[...]]
    i = 0;
    while i + 1 < n {
        if !protected[i] && chars[i].1 == '[' && chars[i + 1].1 == '[' {
            if let Some(j) = (i + 2..n.saturating_sub(1))
                .find(|&j| chars[j].1 == ']' && chars[j + 1].1 == ']')
            {
                for k in i..=j + 1 {
                    flags[k].link = true;
                    protected[k] = true;
                }
                for k in [i, i + 1, j, j + 1] {
                    flags[k].marker = true;
                }
                i = j + 2;
                continue;
            }
        }
        i += 1;
    }
    // markdown links [text](url)
    i = 0;
    while i < n {
        if !protected[i] && chars[i].1 == '[' {
            if let Some(j) = (i + 1..n).find(|&j| chars[j].1 == ']') {
                if j + 1 < n && chars[j + 1].1 == '(' {
                    if let Some(k) = (j + 2..n).find(|&k| chars[k].1 == ')') {
                        for m in i..=k {
                            protected[m] = true;
                            flags[m].link = true;
                        }
                        flags[i].marker = true;
                        for m in j..=k {
                            flags[m].marker = true;
                        }
                        i = k + 1;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    // bare urls
    for scheme in ["https://", "http://"] {
        let mut start = 0;
        while let Some(p) = content[start..].find(scheme) {
            let b = start + p;
            let e = content[b..]
                .find(char::is_whitespace)
                .map(|x| b + x)
                .unwrap_or(content.len());
            let (si, ei) = (byte_to_idx(b), byte_to_idx(e));
            if si < n && !protected[si] {
                for k in si..ei {
                    flags[k].link = true;
                    protected[k] = true;
                }
            }
            start = e.max(b + 1);
            if start >= content.len() {
                break;
            }
        }
    }
    // tags
    for (s, e) in tag_spans(content) {
        let (si, ei) = (byte_to_idx(s), byte_to_idx(e));
        if si < n && !protected[si] {
            for k in si..ei {
                flags[k].tag = true;
                protected[k] = true;
            }
        }
    }
    // paired delimiters
    let pairs: [(&str, u8); 5] = [("**", 0), ("__", 0), ("~~", 1), ("==", 2), ("*", 3)];
    for (delim, kind) in pairs {
        let d: Vec<char> = delim.chars().collect();
        let dl = d.len();
        let at = |i: usize, protected: &Vec<bool>, flags: &Vec<Flags>| -> bool {
            i + dl <= n
                && (0..dl).all(|k| chars[i + k].1 == d[k] && !protected[i + k] && !flags[i + k].marker)
        };
        let mut i = 0;
        while i < n {
            if at(i, &protected, &flags) && i + dl < n && !chars[i + dl].1.is_whitespace() {
                // for single '*', skip if part of a longer run
                if dl == 1 && ((i + 1 < n && chars[i + 1].1 == '*') || (i > 0 && chars[i - 1].1 == '*')) {
                    i += 1;
                    continue;
                }
                let mut found = None;
                let mut j = i + dl + 1;
                while j + dl <= n {
                    if at(j, &protected, &flags) && !chars[j - 1].1.is_whitespace() {
                        if dl == 1 && j + 1 < n && chars[j + 1].1 == '*' {
                            j += 1;
                            continue;
                        }
                        found = Some(j);
                        break;
                    }
                    j += 1;
                }
                if let Some(j) = found {
                    for k in i..j + dl {
                        match kind {
                            0 => flags[k].bold = true,
                            1 => flags[k].strike = true,
                            2 => flags[k].highlight = true,
                            _ => flags[k].italic = true,
                        }
                    }
                    for k in 0..dl {
                        flags[i + k].marker = true;
                        flags[j + k].marker = true;
                        flags[i + k].highlight = false;
                        flags[j + k].highlight = false;
                    }
                    i = j + dl;
                    continue;
                }
            }
            i += 1;
        }
    }
    chars.iter().zip(flags).map(|((b, _), f)| (*b, f)).collect()
}

pub fn layout_job(text: &str, style: &MdStyle, wrap_width: f32) -> LayoutJob {
    let mut job = LayoutJob {
        text: text.to_string(),
        ..Default::default()
    };
    job.wrap.max_width = wrap_width;

    let push = |job: &mut LayoutJob, start: usize, end: usize, f: Flags| {
        if start >= end {
            return;
        }
        let format = style.format(f);
        if let Some(last) = job.sections.last_mut() {
            if last.byte_range.end.0 == start && last.format == format {
                last.byte_range.end = ByteIndex(end);
                return;
            }
        }
        job.sections.push(LayoutSection {
            leading_space: 0.0,
            byte_range: ByteIndex(start)..ByteIndex(end),
            format,
        });
    };

    let mut offset = 0;
    let mut in_fence = false;
    for raw in text.split_inclusive('\n') {
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        let nl = raw.len() - line.len();
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            push(
                &mut job,
                offset,
                offset + raw.len(),
                Flags { fence: true, marker: true, ..Default::default() },
            );
            offset += raw.len();
            continue;
        }
        if in_fence {
            push(&mut job, offset, offset + raw.len(), Flags { fence: true, ..Default::default() });
            offset += raw.len();
            continue;
        }
        // a picture line keeps its text but gets a tall, empty-looking row the editor paints over
        if let Some(h) = image_ref(line).and_then(|n| style.images.get(n)) {
            let format = TextFormat {
                font_id: FontId::proportional(6.0),
                color: Color32::TRANSPARENT,
                line_height: Some(*h),
                ..Default::default()
            };
            job.sections.push(LayoutSection { leading_space: 0.0, byte_range: ByteIndex(offset)..ByteIndex(offset + raw.len()), format });
            offset += raw.len();
            continue;
        }
        let (plen, base, marks) = block_prefix(line);
        let mut pos = 0;
        for (s, e, f) in marks {
            if s > pos {
                push(&mut job, offset + pos, offset + s, Flags::default());
            }
            push(&mut job, offset + s, offset + e, f);
            pos = e;
        }
        if plen > pos {
            push(&mut job, offset + pos, offset + plen, Flags::default());
        }
        let content = &line[plen..];
        let fl = inline_flags(content, base);
        for (idx, (b, f)) in fl.iter().enumerate() {
            let e = fl.get(idx + 1).map(|x| x.0).unwrap_or(content.len());
            push(&mut job, offset + plen + b, offset + plen + e, *f);
        }
        if nl > 0 {
            push(&mut job, offset + line.len(), offset + raw.len(), base);
        }
        offset += raw.len();
    }
    if job.sections.is_empty() {
        // empty text still needs a format for the cursor height
        job.sections.push(LayoutSection {
            leading_space: 0.0,
            byte_range: ByteIndex(0)..ByteIndex(0),
            format: style.format(Flags::default()),
        });
    }
    job
}

/// Convert a markdown note to a simple standalone HTML page.
fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len() * 4 / 3 + 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

/// `dir` is the note's attachment folder: pictures are embedded in the page.
pub fn to_html(title: &str, text: &str, dir: Option<&std::path::Path>) -> String {
    fn esc(s: &str) -> String {
        s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
    }
    fn inline(s: &str) -> String {
        let s = esc(s);
        let mut out = String::new();
        let mut rest = s.as_str();
        let rules: [(&str, &str, &str); 5] = [
            ("**", "<strong>", "</strong>"),
            ("~~", "<del>", "</del>"),
            ("==", "<mark>", "</mark>"),
            ("`", "<code>", "</code>"),
            ("*", "<em>", "</em>"),
        ];
        'outer: while !rest.is_empty() {
            for (d, o, c) in rules {
                if let Some(r) = rest.strip_prefix(d) {
                    if let Some(e) = r.find(d) {
                        if e > 0 {
                            out.push_str(o);
                            out.push_str(&r[..e]);
                            out.push_str(c);
                            rest = &r[e + d.len()..];
                            continue 'outer;
                        }
                    }
                }
            }
            let ch = rest.chars().next().unwrap();
            out.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
        out
    }
    let mut body = String::new();
    let mut in_list = false;
    let mut in_code = false;
    for line in text.lines() {
        let t = line.trim_start();
        if t.starts_with("```") {
            if in_code {
                body.push_str("</code></pre>\n");
            } else {
                body.push_str("<pre><code>");
            }
            in_code = !in_code;
            continue;
        }
        if in_code {
            body.push_str(&esc(line));
            body.push('\n');
            continue;
        }
        let is_item = t.starts_with("- ") || t.starts_with("* ");
        if is_item && !in_list {
            body.push_str("<ul>\n");
            in_list = true;
        } else if !is_item && in_list {
            body.push_str("</ul>\n");
            in_list = false;
        }
        if let Some(name) = image_ref(t) {
            let bytes = dir.and_then(|d| std::fs::read(d.join(name)).ok());
            match bytes {
                Some(b) => {
                    let lower = name.to_lowercase();
                    let mime = if lower.ends_with(".jpg") || lower.ends_with(".jpeg") { "image/jpeg" } else { "image/png" };
                    body.push_str(&format!("<p><img src=\"data:{mime};base64,{}\" alt=\"{}\" style=\"max-width:100%\"></p>\n", base64(&b), esc(name)));
                }
                None => body.push_str(&format!("<p><em>{}</em></p>\n", esc(&crate::export::image_placeholder(name)))),
            }
            continue;
        }
        let hashes = t.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && t[hashes..].starts_with(' ') {
            body.push_str(&format!("<h{hashes}>{}</h{hashes}>\n", inline(t[hashes + 1..].trim())));
        } else if let Some(r) = t.strip_prefix("- [ ] ") {
            body.push_str(&format!("<li><input type=\"checkbox\" disabled> {}</li>\n", inline(r)));
        } else if let Some(r) = t.strip_prefix("- [x] ").or_else(|| t.strip_prefix("- [X] ")) {
            body.push_str(&format!("<li><input type=\"checkbox\" checked disabled> {}</li>\n", inline(r)));
        } else if is_item {
            body.push_str(&format!("<li>{}</li>\n", inline(&t[2..])));
        } else if let Some(r) = t.strip_prefix("> ") {
            body.push_str(&format!("<blockquote>{}</blockquote>\n", inline(r)));
        } else if t == "---" {
            body.push_str("<hr>\n");
        } else if t.is_empty() {
            continue;
        } else {
            body.push_str(&format!("<p>{}</p>\n", inline(t)));
        }
    }
    if in_list {
        body.push_str("</ul>\n");
    }
    if in_code {
        body.push_str("</code></pre>\n");
    }
    format!(
        "<!DOCTYPE html>\n<html lang=\"{}\"><head><meta charset=\"utf-8\"><title>{}</title>\n<style>body{{font-family:sans-serif;max-width:720px;margin:40px auto;line-height:1.6;color:#2b2f2a}}blockquote{{border-left:3px solid #4c9a52;margin:0;padding-left:12px;color:#666}}code{{background:#f0f2ec;padding:1px 4px;border-radius:3px}}pre{{background:#f0f2ec;padding:12px;border-radius:6px}}mark{{background:#f4eba5}}</style></head>\n<body>\n{}</body></html>\n",
        crate::i18n::lang().code(),
        esc(title),
        body
    )
}
