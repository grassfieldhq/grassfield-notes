//! Original colour themes for Grassfield Notes.

use eframe::egui::Color32;

#[derive(Clone, Copy)]
pub struct Theme {
    pub name: &'static str,
    pub dark: bool,
    pub sidebar_bg: Color32,
    pub sidebar_text: Color32,
    pub sidebar_muted: Color32,
    pub sidebar_selected: Color32,
    pub list_bg: Color32,
    pub list_selected: Color32,
    pub editor_bg: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    pub border: Color32,
    pub highlight_bg: Color32,
    pub code_bg: Color32,
    pub tag_bg: Color32,
    pub selection: Color32,
}

const fn c(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

const BASE: &[Theme] = &[
    Theme {
        name: "Prairie",
        dark: false,
        sidebar_bg: c(0x26332A),
        sidebar_text: c(0xE4EBDD),
        sidebar_muted: c(0x8FA391),
        sidebar_selected: c(0x3A4C3E),
        list_bg: c(0xF6F7F2),
        list_selected: c(0xE3ECD9),
        editor_bg: c(0xFFFFFF),
        text: c(0x2B2F2A),
        muted: c(0x7A8076),
        faint: c(0xB7BCB2),
        accent: c(0x4C9A52),
        accent_text: c(0x3B7F41),
        border: c(0xE2E5DC),
        highlight_bg: c(0xF4EBA5),
        code_bg: c(0xF0F2EC),
        tag_bg: c(0xE2F0DF),
        selection: c(0xCFE6C7),
    },
    Theme {
        name: "Rosée",
        dark: false,
        sidebar_bg: c(0xEFEBE2),
        sidebar_text: c(0x3B3A34),
        sidebar_muted: c(0x8F8B80),
        sidebar_selected: c(0xDDE6D2),
        list_bg: c(0xF8F5EE),
        list_selected: c(0xE8EEDD),
        editor_bg: c(0xFDFBF6),
        text: c(0x34332E),
        muted: c(0x85827A),
        faint: c(0xC2BEB2),
        accent: c(0x5E9A5C),
        accent_text: c(0x4A8248),
        border: c(0xE6E1D5),
        highlight_bg: c(0xF6E5A8),
        code_bg: c(0xF1EDE3),
        tag_bg: c(0xE5EEDB),
        selection: c(0xD9E8CD),
    },
    Theme {
        name: "Sous-bois",
        dark: true,
        sidebar_bg: c(0x171D19),
        sidebar_text: c(0xD5DED3),
        sidebar_muted: c(0x7C8A7E),
        sidebar_selected: c(0x2A3A2E),
        list_bg: c(0x1D241F),
        list_selected: c(0x2C3B30),
        editor_bg: c(0x222A24),
        text: c(0xDDE4DA),
        muted: c(0x8E9A8E),
        faint: c(0x55625A),
        accent: c(0x7CC47F),
        accent_text: c(0x8FD493),
        border: c(0x2E3832),
        highlight_bg: c(0x5E5427),
        code_bg: c(0x2B342E),
        tag_bg: c(0x2F4434),
        selection: c(0x35523C),
    },
    Theme {
        name: "Nuit d'été",
        dark: true,
        sidebar_bg: c(0x161A22),
        sidebar_text: c(0xD3D9E4),
        sidebar_muted: c(0x7A8396),
        sidebar_selected: c(0x283244),
        list_bg: c(0x1B2029),
        list_selected: c(0x283244),
        editor_bg: c(0x20262F),
        text: c(0xDCE2EC),
        muted: c(0x8A93A5),
        faint: c(0x515A6B),
        accent: c(0x6CC08A),
        accent_text: c(0x82D09E),
        border: c(0x2C3340),
        highlight_bg: c(0x55502A),
        code_bg: c(0x29303B),
        tag_bg: c(0x283F35),
        selection: c(0x2F4A40),
    },
    Theme {
        name: "Graphite",
        dark: true,
        sidebar_bg: c(0x1A1A1A),
        sidebar_text: c(0xDADADA),
        sidebar_muted: c(0x828282),
        sidebar_selected: c(0x333333),
        list_bg: c(0x202020),
        list_selected: c(0x343834),
        editor_bg: c(0x262626),
        text: c(0xE2E2E2),
        muted: c(0x939393),
        faint: c(0x5A5A5A),
        accent: c(0x8BC34A),
        accent_text: c(0x9CCC65),
        border: c(0x333333),
        highlight_bg: c(0x5C5424),
        code_bg: c(0x303030),
        tag_bg: c(0x34402A),
        selection: c(0x3E4C33),
    },
    Theme {
        name: "Papier",
        dark: false,
        sidebar_bg: c(0xF2F2F0),
        sidebar_text: c(0x222222),
        sidebar_muted: c(0x888888),
        sidebar_selected: c(0xDCE9D5),
        list_bg: c(0xFAFAFA),
        list_selected: c(0xE7F0E2),
        editor_bg: c(0xFFFFFF),
        text: c(0x1E1E1E),
        muted: c(0x7C7C7C),
        faint: c(0xBDBDBD),
        accent: c(0x2E7D32),
        accent_text: c(0x2E7D32),
        border: c(0xE6E6E6),
        highlight_bg: c(0xFFF09A),
        code_bg: c(0xF3F3F3),
        tag_bg: c(0xE3F1E1),
        selection: c(0xCDE5C8),
    },
];

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// Build a theme from a tint (paper colour) and an accent.
fn derive(name: &'static str, dark: bool, dark_sidebar: bool, tint: u32, accent: u32) -> Theme {
    let tint = c(tint);
    let accent = c(accent);
    let white = Color32::WHITE;
    if dark {
        let editor = mix(c(0x1E1E1E), tint, 0.16);
        let list = mix(c(0x191919), tint, 0.16);
        let side = mix(c(0x131313), tint, 0.16);
        let text = mix(c(0xE2E2E2), tint, 0.08);
        Theme {
            name,
            dark: true,
            sidebar_bg: side,
            sidebar_text: text,
            sidebar_muted: mix(text, side, 0.45),
            sidebar_selected: mix(side, accent, 0.22),
            list_bg: list,
            list_selected: mix(list, accent, 0.2),
            editor_bg: editor,
            text,
            muted: mix(text, editor, 0.4),
            faint: mix(text, editor, 0.68),
            accent,
            accent_text: mix(accent, white, 0.15),
            border: mix(editor, white, 0.08),
            highlight_bg: mix(editor, c(0xE8C547), 0.35),
            code_bg: mix(editor, white, 0.06),
            tag_bg: mix(editor, accent, 0.22),
            selection: mix(editor, accent, 0.35),
        }
    } else {
        let editor = mix(white, tint, 0.04);
        let list = mix(white, tint, 0.09);
        let text = mix(c(0x232323), tint, 0.12);
        let (side, side_text) = if dark_sidebar {
            let sb = mix(c(0x1C1C1C), tint, 0.32);
            (sb, mix(c(0xEAEAEA), tint, 0.08))
        } else {
            (mix(white, tint, 0.16), text)
        };
        Theme {
            name,
            dark: false,
            sidebar_bg: side,
            sidebar_text: side_text,
            sidebar_muted: mix(side_text, side, 0.45),
            sidebar_selected: if dark_sidebar { mix(side, white, 0.12) } else { mix(side, accent, 0.2) },
            list_bg: list,
            list_selected: mix(list, accent, 0.15),
            editor_bg: editor,
            text,
            muted: mix(text, editor, 0.45),
            faint: mix(text, editor, 0.7),
            accent,
            accent_text: mix(accent, c(0x000000), 0.15),
            border: mix(editor, tint, 0.18),
            highlight_bg: c(0xF4EBA5),
            code_bg: mix(editor, tint, 0.1),
            tag_bg: mix(editor, accent, 0.15),
            selection: mix(editor, accent, 0.28),
        }
    }
}

/// Every theme: the six hand-made ones followed by 22 derived ones (28 in all).
pub fn all() -> &'static [Theme] {
    static ALL: std::sync::OnceLock<Vec<Theme>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let mut v = BASE.to_vec();
        let extra: [(&'static str, bool, bool, u32, u32); 22] = [
            ("Menthe", false, false, 0xBFE8D6, 0x2E9E77),
            ("Tournesol", false, true, 0xF3DFA0, 0xC98A0B),
            ("Lavande", false, false, 0xD9D2F0, 0x7A63C9),
            ("Corail", false, false, 0xF5CFC4, 0xD8604A),
            ("Océan", false, true, 0xC6DDEB, 0x2A7BB0),
            ("Ardoise", false, true, 0xCDD3D8, 0x51708A),
            ("Bruyère", false, false, 0xE6CCE0, 0xA4498E),
            ("Lin", false, false, 0xEADFCB, 0x8C6D3F),
            ("Sauge", false, true, 0xD3DECF, 0x5F8657),
            ("Coquelicot", false, false, 0xF6D3D0, 0xC8382E),
            ("Ciel", false, false, 0xD3E6F7, 0x3A86D4),
            ("Glacier", true, false, 0x9FC3D9, 0x6BC1E8),
            ("Minuit", true, false, 0x34407A, 0x8EA2FF),
            ("Forêt profonde", true, false, 0x1F5130, 0x6FD08C),
            ("Aubergine", true, false, 0x5A2A55, 0xE08BD3),
            ("Cacao", true, false, 0x5B3B26, 0xE0A46B),
            ("Encre", true, false, 0x23314A, 0x5FB3F0),
            ("Braise", true, false, 0x5A2318, 0xFF8A5C),
            ("Aurore boréale", true, false, 0x1C4A4A, 0x5EE3B8),
            ("Olive nocturne", true, false, 0x45491F, 0xC7D45E),
            ("Prune", true, false, 0x40233F, 0xC79BF2),
            ("Pistache", false, true, 0xDDEBC3, 0x7DA531),
        ];
        for (name, dark, ds, tint, accent) in extra {
            v.push(derive(name, dark, ds, tint, accent));
        }
        v
    })
}

pub fn get(i: usize) -> Theme {
    let a = all();
    a[i.min(a.len() - 1)]
}
