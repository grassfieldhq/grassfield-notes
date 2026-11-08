//! Notes, tags, filters and saving to disk.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::PathBuf;

pub fn now() -> i64 {
    chrono::Local::now().timestamp()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Note {
    pub id: u64,
    pub text: String,
    pub created: i64,
    pub modified: i64,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub trashed: bool,
    /// Scanned documents, images and PDFs attached to the note.
    #[serde(default)]
    pub attachments: Vec<Attachment>,
}

/// A file stored next to the library, with the text read from it (for search).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Attachment {
    /// File name inside the note's attachment folder.
    pub name: String,
    /// Text read from the image (OCR) or the PDF.
    #[serde(default)]
    pub text: String,
    /// True once text reading has finished (even if nothing was found).
    #[serde(default)]
    pub read: bool,
}

impl Attachment {
    pub fn is_image(&self) -> bool {
        let n = self.name.to_lowercase();
        [".png", ".jpg", ".jpeg", ".webp", ".bmp", ".tif", ".tiff", ".gif"].iter().any(|e| n.ends_with(e))
    }
    pub fn is_pdf(&self) -> bool {
        self.name.to_lowercase().ends_with(".pdf")
    }
}

impl Note {
    pub fn title(&self) -> String {
        note_title(&self.text)
    }

    pub fn tags(&self) -> BTreeSet<String> {
        parse_tags(&self.text)
    }

    pub fn has_open_todo(&self) -> bool {
        self.text.lines().any(|l| {
            let t = l.trim_start();
            t.starts_with("- [ ]") || t.starts_with("* [ ]")
        })
    }

    pub fn todo_counts(&self) -> (usize, usize) {
        let mut done = 0;
        let mut total = 0;
        for l in self.text.lines() {
            let t = l.trim_start();
            if t.starts_with("- [ ]") || t.starts_with("* [ ]") {
                total += 1;
            } else if t.starts_with("- [x]")
                || t.starts_with("- [X]")
                || t.starts_with("* [x]")
                || t.starts_with("* [X]")
            {
                total += 1;
                done += 1;
            }
        }
        (done, total)
    }

    /// Plain text preview of the body (everything after the title line).
    pub fn preview(&self) -> String {
        let mut lines = self.text.lines();
        // skip until title line found
        for l in lines.by_ref() {
            if !l.trim().is_empty() {
                break;
            }
        }
        let mut out = String::new();
        for l in lines {
            let p = strip_markdown_line(l);
            if p.is_empty() {
                continue;
            }
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&p);
            if out.len() > 400 {
                break;
            }
        }
        out
    }
}

pub fn note_title(text: &str) -> String {
    for l in text.lines() {
        let t = l.trim();
        if t.is_empty() {
            continue;
        }
        let s = strip_markdown_line(t);
        if s.is_empty() {
            continue;
        }
        return s;
    }
    String::new()
}

/// The file name of a picture line `![caption](file)`, if the line is one.
pub fn image_ref(line: &str) -> Option<&str> {
    let t = line.trim();
    let rest = t.strip_prefix("![")?;
    let close = rest.find("](")?;
    let mut name = rest[close + 2..].strip_suffix(')')?.trim();
    // ![alt](file "title") and ![alt](<file with spaces>)
    if let Some(i) = name.find(" \"") {
        name = name[..i].trim_end();
    }
    if let Some(inner) = name.strip_prefix('<').and_then(|n| n.strip_suffix('>')) {
        name = inner;
    }
    (!name.is_empty() && !name.contains('(') && !name.contains(')')).then_some(name)
}

pub fn strip_markdown_line(line: &str) -> String {
    if image_ref(line).is_some() {
        return String::new();
    }
    let mut t = line.trim();
    // headings
    let hashes = t.chars().take_while(|c| *c == '#').count();
    if hashes > 0 && hashes <= 6 && t[hashes..].starts_with(' ') {
        t = t[hashes..].trim_start();
    }
    for p in ["- [ ] ", "- [x] ", "- [X] ", "* [ ] ", "* [x] ", "> ", "- ", "* ", "+ "] {
        if let Some(rest) = t.strip_prefix(p) {
            t = rest;
            break;
        }
    }
    if t.starts_with("```") || t == "---" || t == "***" {
        return String::new();
    }
    let mut s = String::with_capacity(t.len());
    let mut chars = t.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' | '~' | '=' | '`' => {}
            '_' if chars.peek() == Some(&'_') => {}
            '[' if chars.peek() == Some(&'[') => {
                chars.next();
            }
            ']' if chars.peek() == Some(&']') => {
                chars.next();
            }
            _ => s.push(c),
        }
    }
    s.trim().to_string()
}

fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || c == '/' || c == '-' || c == '_'
}

/// Returns (byte_start, byte_end) of every `#tag` in a line of text.
pub fn tag_spans(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let bytes: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    let mut in_code = false;
    while i < bytes.len() {
        let (bi, c) = bytes[i];
        if c == '`' {
            in_code = !in_code;
        }
        if c == '\n' {
            in_code = false;
        }
        if c == '#' && !in_code {
            let prev_ok = i == 0 || bytes[i - 1].1.is_whitespace() || bytes[i - 1].1 == '(';
            let next_ok = i + 1 < bytes.len()
                && is_tag_char(bytes[i + 1].1)
                && bytes[i + 1].1 != '/'
                && bytes[i + 1].1 != '-';
            if prev_ok && next_ok {
                let mut j = i + 1;
                while j < bytes.len() && is_tag_char(bytes[j].1) {
                    j += 1;
                }
                // trim trailing slashes / dashes
                while j > i + 1 && matches!(bytes[j - 1].1, '/' | '-') {
                    j -= 1;
                }
                let end = if j < bytes.len() { bytes[j].0 } else { text.len() };
                // ignore pure numbers like #1
                let name = &text[bi + 1..end];
                if !name.chars().all(|c| c.is_ascii_digit()) {
                    out.push((bi, end));
                }
                i = j;
                continue;
            }
        }
        i += 1;
    }
    out
}

pub fn parse_tags(text: &str) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    let mut in_fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        for (s, e) in tag_spans(line) {
            set.insert(line[s + 1..e].to_lowercase());
        }
    }
    set
}

/// Wiki links `[[Title]]` found in a text.
pub fn wiki_links(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(s) = rest.find("[[") {
        let after = &rest[s + 2..];
        if let Some(e) = after.find("]]") {
            let name = &after[..e];
            if !name.contains('\n') && !name.trim().is_empty() {
                out.push(name.trim().to_string());
            }
            rest = &after[e + 2..];
        } else {
            break;
        }
    }
    out
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortMode {
    Modified,
    Created,
    Title,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditorFont {
    Sans,
    Serif,
    Mono,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Settings {
    pub theme: usize,
    pub font_size: f32,
    pub line_width: f32,
    pub line_spacing: f32,
    pub sort: SortMode,
    pub show_previews: bool,
    pub editor_font: EditorFont,
    pub dim_markup: bool,
    pub sidebar_width: f32,
    #[serde(default)]
    pub icon: usize,
    /// Interface language; None follows the computer's language.
    /// Libraries saved before languages existed were French, so they stay French.
    #[serde(default = "legacy_language")]
    pub language: Option<crate::i18n::Lang>,
    #[serde(default)]
    pub sync: SyncSettings,
}

/// Where and how notes are synced with a GitHub repository.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SyncSettings {
    /// e.g. https://github.com/name/notes.git or git@github.com:name/notes.git
    #[serde(default)]
    pub repo_url: String,
    #[serde(default)]
    pub branch: String,
    /// Optional personal access token for HTTPS repositories.
    #[serde(default)]
    pub token: String,
    /// Sync on start and every few minutes after changes.
    #[serde(default)]
    pub auto: bool,
    #[serde(default)]
    pub last_sync: i64,
}

fn legacy_language() -> Option<crate::i18n::Lang> {
    Some(crate::i18n::Lang::Fr)
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: 0,
            font_size: 16.0,
            line_width: 720.0,
            line_spacing: 1.0,
            sort: SortMode::Modified,
            show_previews: true,
            editor_font: EditorFont::Sans,
            dim_markup: true,
            sidebar_width: 230.0,
            icon: 0,
            language: None,
            sync: SyncSettings::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Library {
    pub notes: Vec<Note>,
    pub next_id: u64,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub expanded_tags: BTreeSet<String>,
    /// Notes that were present in the GitHub repository after the last sync.
    #[serde(default)]
    pub synced_ids: BTreeSet<u64>,
    /// Parsed tags per note, refreshed once per frame so filters and counts don't re-parse.
    #[serde(skip)]
    index: std::collections::HashMap<u64, (u64, std::rc::Rc<BTreeSet<String>>)>,
}

fn text_hash(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Filter {
    All,
    Pinned,
    Untagged,
    Todo,
    Today,
    Archive,
    Trash,
    Tag(String),
}

impl Filter {
    pub fn label(&self) -> String {
        match self {
            Filter::All => crate::i18n::tr("Notes").into(),
            Filter::Pinned => crate::i18n::tr("Épinglées").into(),
            Filter::Untagged => crate::i18n::tr("Sans tag").into(),
            Filter::Todo => crate::i18n::tr("À faire").into(),
            Filter::Today => crate::i18n::tr("Aujourd'hui").into(),
            Filter::Archive => crate::i18n::tr("Archives").into(),
            Filter::Trash => crate::i18n::tr("Corbeille").into(),
            Filter::Tag(t) => format!("#{t}"),
        }
    }
}

pub fn tag_matches(note_tags: &BTreeSet<String>, tag: &str) -> bool {
    note_tags
        .iter()
        .any(|t| t == tag || (t.starts_with(tag) && t[tag.len()..].starts_with('/')))
}

impl Library {
    pub fn data_path() -> PathBuf {
        if let Ok(p) = std::env::var("GRASSFIELD_DATA") {
            return PathBuf::from(p);
        }
        let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join("grassfield-notes").join("library.json")
    }

    /// Folder holding the library, attachments and the sync copy.
    pub fn data_dir() -> PathBuf {
        Self::data_path().parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."))
    }

    pub fn attachment_dir(note_id: u64) -> PathBuf {
        Self::data_dir().join("attachments").join(note_id.to_string())
    }

    pub fn load_or_sample() -> Self {
        let p = Self::data_path();
        if let Ok(s) = std::fs::read_to_string(&p) {
            if let Ok(lib) = serde_json::from_str::<Library>(&s) {
                return lib;
            }
            // keep a copy of an unreadable file instead of overwriting it
            let _ = std::fs::copy(&p, p.with_extension("json.bak"));
        }
        Self::sample()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let p = Self::data_path();
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = p.with_extension("json.tmp");
        let s = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, s)?;
        std::fs::rename(&tmp, &p)
    }

    pub fn new_note(&mut self, text: String) -> u64 {
        // Ids are based on the clock so notes made on two synced computers don't clash.
        let clock = (chrono::Local::now().timestamp_millis().max(0) as u64) * 16;
        let id = (self.next_id + 1).max(clock);
        self.next_id = id;
        let t = now();
        self.notes.push(Note {
            id,
            text,
            created: t,
            modified: t,
            pinned: false,
            archived: false,
            trashed: false,
            attachments: Vec::new(),
        });
        id
    }

    pub fn get(&self, id: u64) -> Option<&Note> {
        self.notes.iter().find(|n| n.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Note> {
        self.notes.iter_mut().find(|n| n.id == id)
    }

    pub fn find_by_title(&self, title: &str) -> Option<u64> {
        let tl = title.to_lowercase();
        self.notes
            .iter()
            .filter(|n| !n.trashed)
            .find(|n| n.title().to_lowercase() == tl)
            .map(|n| n.id)
    }

    /// Re-parse tags only for notes whose text changed since the last call.
    pub fn refresh_index(&mut self) {
        let mut next = std::collections::HashMap::with_capacity(self.notes.len());
        for n in &self.notes {
            let h = text_hash(&n.text);
            let entry = match self.index.get(&n.id) {
                Some((old, tags)) if *old == h => (h, tags.clone()),
                _ => (h, std::rc::Rc::new(n.tags())),
            };
            next.insert(n.id, entry);
        }
        self.index = next;
    }

    /// Tags of a note, from the index when it is current.
    pub fn tags_of(&self, n: &Note) -> std::rc::Rc<BTreeSet<String>> {
        match self.index.get(&n.id) {
            Some((_, tags)) => tags.clone(),
            None => std::rc::Rc::new(n.tags()),
        }
    }

    pub fn matches(&self, n: &Note, filter: &Filter) -> bool {
        match filter {
            Filter::Trash => n.trashed,
            Filter::Archive => n.archived && !n.trashed,
            _ if n.trashed || n.archived => false,
            Filter::All => true,
            Filter::Pinned => n.pinned,
            Filter::Untagged => self.tags_of(n).is_empty(),
            Filter::Todo => n.has_open_todo(),
            Filter::Today => {
                let today = chrono::Local::now().date_naive();
                let d = chrono::DateTime::from_timestamp(n.modified, 0)
                    .map(|d| d.with_timezone(&chrono::Local).date_naive());
                d == Some(today)
            }
            Filter::Tag(t) => tag_matches(&self.tags_of(n), t),
        }
    }

    pub fn count(&self, filter: &Filter) -> usize {
        self.notes.iter().filter(|n| self.matches(n, filter)).count()
    }

    /// Notes visible in the list for a filter and a search query, sorted.
    pub fn visible(&self, filter: &Filter, query: &str) -> Vec<u64> {
        let terms: Vec<String> = query
            .split_whitespace()
            .map(|s| s.to_lowercase())
            .collect();
        let mut v: Vec<&Note> = self
            .notes
            .iter()
            .filter(|n| self.matches(n, filter))
            .filter(|n| {
                if terms.is_empty() {
                    return true;
                }
                let mut lower = n.text.to_lowercase();
                for a in &n.attachments {
                    lower.push('\n');
                    lower.push_str(&a.name.to_lowercase());
                    lower.push('\n');
                    lower.push_str(&a.text.to_lowercase());
                }
                terms.iter().all(|t| {
                    if let Some(tag) = t.strip_prefix('#') {
                        !tag.is_empty() && tag_matches(&self.tags_of(n), tag)
                    } else {
                        lower.contains(t.as_str())
                    }
                })
            })
            .collect();
        let sort = self.settings.sort;
        v.sort_by(|a, b| {
            b.pinned.cmp(&a.pinned).then_with(|| match sort {
                SortMode::Modified => b.modified.cmp(&a.modified),
                SortMode::Created => b.created.cmp(&a.created),
                SortMode::Title => a.title().to_lowercase().cmp(&b.title().to_lowercase()),
            })
        });
        v.into_iter().map(|n| n.id).collect()
    }

    /// All tags used by live (not trashed) notes, including parents of nested tags.
    pub fn all_tags(&self) -> BTreeSet<String> {
        let mut set = BTreeSet::new();
        for n in self.notes.iter().filter(|n| !n.trashed) {
            for t in self.tags_of(n).iter() {
                let mut acc = String::new();
                for part in t.split('/') {
                    if part.is_empty() {
                        continue;
                    }
                    if !acc.is_empty() {
                        acc.push('/');
                    }
                    acc.push_str(part);
                    set.insert(acc.clone());
                }
            }
        }
        set
    }

    /// Rename a tag (and its children) inside every note.
    pub fn rename_tag(&mut self, old: &str, new: &str) {
        let new = new.trim().trim_start_matches('#').replace(' ', "-");
        if new.is_empty() {
            return;
        }
        for n in self.notes.iter_mut() {
            let spans = tag_spans_full(&n.text);
            if spans.is_empty() {
                continue;
            }
            let mut out = String::with_capacity(n.text.len());
            let mut last = 0;
            let mut changed = false;
            for (s, e) in spans {
                let name = &n.text[s + 1..e];
                let lname = name.to_lowercase();
                if lname == old || (lname.starts_with(old) && lname[old.len()..].starts_with('/')) {
                    out.push_str(&n.text[last..s]);
                    out.push('#');
                    out.push_str(&new);
                    out.push_str(&name[old.len()..]);
                    last = e;
                    changed = true;
                }
            }
            if changed {
                out.push_str(&n.text[last..]);
                n.text = out;
                n.modified = now();
            }
        }
    }

    /// Remove a tag (and children) from every note.
    pub fn delete_tag(&mut self, tag: &str) {
        for n in self.notes.iter_mut() {
            let spans = tag_spans_full(&n.text);
            let mut out = String::with_capacity(n.text.len());
            let mut last = 0;
            let mut changed = false;
            for (s, e) in spans {
                let lname = n.text[s + 1..e].to_lowercase();
                if lname == tag || (lname.starts_with(tag) && lname[tag.len()..].starts_with('/')) {
                    out.push_str(&n.text[last..s]);
                    last = e;
                    changed = true;
                }
            }
            if changed {
                out.push_str(&n.text[last..]);
                n.text = out;
                n.modified = now();
            }
        }
    }

    pub fn backlinks(&self, id: u64) -> Vec<u64> {
        let Some(note) = self.get(id) else { return vec![] };
        let title = note.title().to_lowercase();
        if title.is_empty() {
            return vec![];
        }
        self.notes
            .iter()
            .filter(|n| n.id != id && !n.trashed)
            .filter(|n| wiki_links(&n.text).iter().any(|l| l.to_lowercase() == title))
            .map(|n| n.id)
            .collect()
    }

    pub fn sample() -> Self {
        let mut lib = Library {
            settings: Settings::default(),
            ..Default::default()
        };
        let t = now();
        let english = crate::i18n::lang() != crate::i18n::Lang::Fr;
        let samples: [(&str, i64, bool); 6] = if english { SAMPLES_EN } else { [
            (
                "# Bienvenue dans Grassfield Notes\n\nUn carnet calme pour vos idées, écrit en **Markdown**. Tout ce que vous tapez est mis en forme en direct.\n\n## Pour commencer\n- [x] Ouvrir Grassfield Notes\n- [ ] Écrire une première note avec **Ctrl+N**\n- [ ] Ajouter un tag comme #idées ou #travail/projets\n- [ ] Cliquer sur une case pour la cocher\n\n## Mise en forme\nDu texte en **gras**, en *italique*, ~~barré~~, ==surligné== ou du `code en ligne`.\n\n> Les citations ressortent avec une barre verte.\n\nReliez vos notes entre elles avec [[Idées de jardin]] (Ctrl+clic pour l'ouvrir).\n\n#bienvenue",
                0,
                true,
            ),
            (
                "# Idées de jardin\n\nCe printemps, essayer une prairie fleurie au fond du terrain.\n\n- Semer du trèfle et de la fétuque\n- Laisser une bande non tondue pour les insectes\n- Installer un petit banc près du noyer\n\n#maison/jardin #idées",
                3600,
                false,
            ),
            (
                "# Réunion produit — lundi\n\n## Ordre du jour\n1. Retour sur la version 1.2\n2. Priorités du trimestre\n3. Questions ouvertes\n\n## Actions\n- [ ] Envoyer le compte rendu à l'équipe\n- [ ] Préparer la démo de vendredi\n- [x] Réserver la salle\n\n#travail/réunions",
                7200,
                false,
            ),
            (
                "# Liste de courses\n\n- [ ] Pain complet\n- [ ] Tomates anciennes\n- [x] Café en grains\n- [ ] Fromage de chèvre\n- [ ] Basilic frais\n\n#maison",
                86400,
                false,
            ),
            (
                "# Extrait de code utile\n\nPour compter les lignes d'un fichier :\n\n```\nwc -l notes.md\n```\n\nPenser à le noter dans le wiki de l'équipe.\n\n#travail/projets #code",
                86400 * 2,
                false,
            ),
            (
                "# Lectures de l'été\n\n- *Le Jardin des Plantes*\n- Un roman policier léger\n- Un essai sur la marche en forêt\n\nObjectif : un livre toutes les deux semaines.\n\n#lecture",
                86400 * 5,
                false,
            ),
        ]};
        for (text, age, pinned) in samples {
            let id = lib.new_note(text.to_string());
            let n = lib.get_mut(id).unwrap();
            n.created = t - age - 600;
            n.modified = t - age;
            n.pinned = pinned;
        }
        lib.expanded_tags.insert(if english { "work".into() } else { "travail".into() });
        lib
    }
}

/// Tag spans across a whole multi-line text (byte offsets into the text).
pub fn tag_spans_full(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut offset = 0;
    let mut in_fence = false;
    for line in text.split_inclusive('\n') {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
        } else if !in_fence {
            for (s, e) in tag_spans(line) {
                out.push((offset + s, offset + e));
            }
        }
        offset += line.len();
    }
    out
}

pub fn word_count(text: &str) -> usize {
    text.split_whitespace()
        .filter(|w| w.chars().any(|c| c.is_alphanumeric()))
        .count()
}

pub fn format_date(ts: i64) -> String {
    let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) else {
        return String::new();
    };
    let dt = dt.with_timezone(&chrono::Local);
    let today = chrono::Local::now().date_naive();
    let d = dt.date_naive();
    if d == today {
        dt.format("%H:%M").to_string()
    } else if Some(d) == today.pred_opt() {
        crate::i18n::tr("Hier").to_string()
    } else if (today - d).num_days() < 7 {
        let days = [crate::i18n::tr("lun."), crate::i18n::tr("mar."), crate::i18n::tr("mer."), crate::i18n::tr("jeu."), crate::i18n::tr("ven."), crate::i18n::tr("sam."), crate::i18n::tr("dim.")];
        use chrono::Datelike;
        days[d.weekday().num_days_from_monday() as usize].to_string()
    } else {
        dt.format(crate::i18n::tr("%d/%m/%Y")).to_string()
    }
}

pub fn format_full_date(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|d| d.with_timezone(&chrono::Local).format(crate::i18n::tr("%d/%m/%Y à %H:%M")).to_string())
        .unwrap_or_default()
}

/// Example notes for every language other than French.
const SAMPLES_EN: [(&str, i64, bool); 6] = [
    (
        "# Welcome to Grassfield Notes\n\nA calm notebook for your ideas, written in **Markdown**. Everything you type is formatted as you go.\n\n## Getting started\n- [x] Open Grassfield Notes\n- [ ] Write a first note with **Ctrl+N**\n- [ ] Add a tag such as #ideas or #work/projects\n- [ ] Click a box to tick it\n\n## Formatting\nText in **bold**, *italic*, ~~strikethrough~~, ==highlighted== or `inline code`.\n\n> Quotes stand out with a green bar.\n\nLink notes together with [[Garden ideas]] (Ctrl+click to open it).\n\n#welcome",
        0,
        true,
    ),
    (
        "# Garden ideas\n\nThis spring, try a wildflower meadow at the back of the garden.\n\n- Sow clover and fescue\n- Leave an unmown strip for insects\n- Put a small bench near the walnut tree\n\n#home/garden #ideas",
        3600,
        false,
    ),
    (
        "# Product meeting — Monday\n\n## Agenda\n1. Review of version 1.2\n2. Priorities for the quarter\n3. Open questions\n\n## Actions\n- [ ] Send the minutes to the team\n- [ ] Prepare Friday's demo\n- [x] Book the room\n\n#work/meetings",
        7200,
        false,
    ),
    (
        "# Shopping list\n\n- [ ] Wholemeal bread\n- [ ] Heirloom tomatoes\n- [x] Coffee beans\n- [ ] Goat cheese\n- [ ] Fresh basil\n\n#home",
        86400,
        false,
    ),
    (
        "# Handy snippet\n\nTo count the lines of a file:\n\n```\nwc -l notes.md\n```\n\nRemember to add it to the team wiki.\n\n#work/projects #code",
        86400 * 2,
        false,
    ),
    (
        "# Summer reading\n\n- *The Botanist's Garden*\n- A light detective novel\n- An essay on walking in the woods\n\nGoal: one book every two weeks.\n\n#reading",
        86400 * 5,
        false,
    ),
];
