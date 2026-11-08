//! Interface languages. Texts are written in French in the code; `tr` looks up the
//! translation for the chosen language (falling back to French when one is missing).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

#[path = "i18n_a.rs"]
mod table_a;
#[path = "i18n_b.rs"]
mod table_b;
#[path = "i18n_c.rs"]
mod table_c;
#[path = "i18n_d.rs"]
mod table_d;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Fr,
    En,
    Es,
    De,
    It,
    Pt,
}

pub const ALL: [Lang; 6] = [Lang::Fr, Lang::En, Lang::Es, Lang::De, Lang::It, Lang::Pt];

impl Lang {
    /// The language's own name, shown in the language picker.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::Fr => "Français",
            Lang::En => "English",
            Lang::Es => "Español",
            Lang::De => "Deutsch",
            Lang::It => "Italiano",
            Lang::Pt => "Português",
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            Lang::Fr => "fr",
            Lang::En => "en",
            Lang::Es => "es",
            Lang::De => "de",
            Lang::It => "it",
            Lang::Pt => "pt",
        }
    }
    /// Tesseract language pack for reading text in images.
    pub fn ocr_code(self) -> &'static str {
        match self {
            Lang::Fr => "fra",
            Lang::En => "eng",
            Lang::Es => "spa",
            Lang::De => "deu",
            Lang::It => "ita",
            Lang::Pt => "por",
        }
    }
    fn index(self) -> usize {
        ALL.iter().position(|l| *l == self).unwrap_or(0)
    }
    fn from_index(i: u8) -> Lang {
        ALL.get(i as usize).copied().unwrap_or(Lang::Fr)
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set(lang: Lang) {
    CURRENT.store(lang.index() as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    Lang::from_index(CURRENT.load(Ordering::Relaxed))
}

/// The computer's language (from LC_ALL / LC_MESSAGES / LANG), English when unsupported.
pub fn system() -> Lang {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
        if let Ok(v) = std::env::var(var) {
            let v = v.to_lowercase();
            if v.is_empty() || v == "c" || v.starts_with("c.") || v == "posix" {
                continue;
            }
            for l in ALL {
                if v.starts_with(l.code()) {
                    return l;
                }
            }
            return Lang::En;
        }
    }
    Lang::En
}

type Row = [&'static str; 6];

fn table() -> &'static HashMap<&'static str, Row> {
    static T: OnceLock<HashMap<&'static str, Row>> = OnceLock::new();
    T.get_or_init(|| {
        let mut m = HashMap::new();
        for row in table_a::ROWS.iter().chain(table_b::ROWS.iter()).chain(table_c::ROWS.iter()).chain(table_d::ROWS.iter()) {
            m.insert(row[0], *row);
        }
        m
    })
}

/// Translate a French interface text into the current language.
pub fn tr(fr: &'static str) -> &'static str {
    tr_in(lang(), fr)
}

/// Translate into a given language.
pub fn tr_in(l: Lang, fr: &'static str) -> &'static str {
    let s = if l == Lang::Fr {
        fr
    } else {
        match table().get(fr) {
            Some(row) if !row[l.index()].is_empty() => row[l.index()],
            _ => fr,
        }
    };
    for_platform(s)
}

/// Shortcut names follow the system: "Ctrl+B" becomes "⌘B" on macOS.
fn for_platform(s: &'static str) -> &'static str {
    if !cfg!(target_os = "macos") || !(s.contains("Ctrl") || s.contains("Strg")) {
        return s;
    }
    static CACHE: OnceLock<std::sync::Mutex<HashMap<&'static str, &'static str>>> = OnceLock::new();
    let mut map = CACHE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    // the set of interface texts is small and fixed, so each adapted text is made once and kept
    map.entry(s).or_insert_with(|| Box::leak(crate::platform::shortcut_label(s).into_owned().into_boxed_str()))
}

/// Translate a text with `{name}` placeholders, then fill them in.
pub fn trf(fr: &'static str, args: &[(&str, &dyn std::fmt::Display)]) -> String {
    trf_in(lang(), fr, args)
}

pub fn trf_in(l: Lang, fr: &'static str, args: &[(&str, &dyn std::fmt::Display)]) -> String {
    let mut s = tr_in(l, fr).to_string();
    for (name, value) in args {
        s = s.replace(&format!("{{{name}}}"), &value.to_string());
    }
    s
}

#[cfg(test)]
pub fn missing() -> Vec<(&'static str, Lang)> {
    let mut out = Vec::new();
    for row in table().values() {
        for l in &ALL[1..] {
            if row[l.index()].is_empty() {
                out.push((row[0], *l));
            }
        }
    }
    out
}
