//! What the note editor and the Markdown file view add to egui's text field so text editing behaves as in a native
//! editor: the right-click menu (Cut, Copy, Paste, Select All), a right-click that keeps the selection it lands on, and
//! pictures pasted from the clipboard.

use super::*;
use egui::text::{CCursor, CCursorRange};
use egui::text_edit::TextEditState;

/// The commands of the text context menu, in the order a native editor lists them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum EditCommand {
    Cut,
    Copy,
    Paste,
    SelectAll,
}

impl EditCommand {
    pub(super) fn label(self) -> &'static str {
        match self {
            EditCommand::Cut => crate::i18n::tr("Couper"),
            EditCommand::Copy => crate::i18n::tr("Copier"),
            EditCommand::Paste => crate::i18n::tr("Coller"),
            EditCommand::SelectAll => crate::i18n::tr("Tout sélectionner"),
        }
    }

    /// The shortcut shown next to the command (Ctrl here; Grassfield's Super+C/V/X/A send the same chords).
    pub(super) fn shortcut(self) -> KeyboardShortcut {
        let key = match self {
            EditCommand::Cut => Key::X,
            EditCommand::Copy => Key::C,
            EditCommand::Paste => Key::V,
            EditCommand::SelectAll => Key::A,
        };
        KeyboardShortcut::new(Modifiers::COMMAND, key)
    }

    /// The window command that makes eframe run the clipboard action on the focused text field, exactly as the
    /// keyboard shortcut does (the system clipboard is read and written by egui-winit). Select All has none.
    pub(super) fn viewport_command(self) -> Option<egui::ViewportCommand> {
        match self {
            EditCommand::Cut => Some(egui::ViewportCommand::RequestCut),
            EditCommand::Copy => Some(egui::ViewportCommand::RequestCopy),
            EditCommand::Paste => Some(egui::ViewportCommand::RequestPaste),
            EditCommand::SelectAll => None,
        }
    }
}

/// The menu's commands and whether each one can run: Cut needs an editable field and a selection, Copy a selection,
/// Paste an editable field, Select All some text. Disabled commands stay listed, as in a native menu.
pub(super) fn menu_items(editable: bool, has_selection: bool, has_text: bool) -> [(EditCommand, bool); 4] {
    [
        (EditCommand::Cut, editable && has_selection),
        (EditCommand::Copy, has_selection),
        (EditCommand::Paste, editable),
        (EditCommand::SelectAll, has_text),
    ]
}

/// A right-click inside the selection keeps it (the menu then acts on it); anywhere else the caret moves there.
/// `selection` is the sorted char range before the click, `at` the char index under the pointer.
pub(super) fn right_click_keeps(selection: Option<(usize, usize)>, at: usize) -> bool {
    matches!(selection, Some((a, b)) if a < b && (a..=b).contains(&at))
}

/// The sorted char range the field's saved state holds, if any.
pub(super) fn saved_selection(ctx: &egui::Context, id: Id) -> Option<(usize, usize)> {
    let range = TextEditState::load(ctx, id)?.cursor.char_range()?;
    let sorted = range.as_sorted_char_range();
    Some((sorted.start.0, sorted.end.0))
}

/// egui moves the caret on any pointer press, the secondary one too, which would drop the selection a right-click is
/// meant to act on. Put it back when the press landed inside it. Returns the selection now in force.
pub(super) fn keep_selection_on_right_click(
    ctx: &egui::Context,
    id: Id,
    before: Option<(usize, usize)>,
    out: &egui::text_edit::TextEditOutput,
) -> Option<(usize, usize)> {
    let pressed = ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary));
    if !pressed || !out.response.response.hovered() {
        return None;
    }
    let pos = ctx.input(|i| i.pointer.interact_pos())?;
    let at = out.galley.cursor_from_pos(pos - out.galley_pos).index.0;
    if !right_click_keeps(before, at) {
        return None;
    }
    let (a, b) = before?;
    let mut state = TextEditState::load(ctx, id).unwrap_or_default();
    state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(a), CCursor::new(b))));
    state.store(ctx, id);
    Some((a, b))
}

/// The right-click menu of a text field showing `text`; `selection` is the sorted char range.
pub(super) fn context_menu(
    resp: &egui::Response,
    id: Id,
    editable: bool,
    selection: Option<(usize, usize)>,
    text: &str,
) {
    let menu_selection = id.with("gf-menu-selection");
    if resp.secondary_clicked() {
        // the selection the menu acts on, as it was when the menu opened
        resp.ctx.data_mut(|d| d.insert_temp(menu_selection, selection));
    }
    resp.context_menu(|ui| {
        ui.set_min_width(190.0);
        let selection = ui.data(|d| d.get_temp::<Option<(usize, usize)>>(menu_selection)).flatten().or(selection);
        // egui collapses the selection of a text field that is not focused; the field keeps the focus while its
        // menu is open, so its selection stays visible and the command finds it.
        if !ui.memory(|m| m.has_focus(id)) {
            restore_selection(ui.ctx(), id, selection);
            ui.memory_mut(|m| m.request_focus(id));
        }
        let len = text.chars().count();
        let has_selection = matches!(selection, Some((a, b)) if a < b);
        for (i, (cmd, enabled)) in menu_items(editable, has_selection, len > 0).into_iter().enumerate() {
            if i == 3 {
                ui.separator();
            }
            let shortcut = ui.ctx().format_shortcut(&cmd.shortcut());
            let button = egui::Button::new(cmd.label()).shortcut_text(shortcut);
            if ui.add_enabled(enabled, button).clicked() {
                restore_selection(ui.ctx(), id, selection);
                run(ui.ctx(), id, cmd, len);
                ui.close();
            }
        }
    });
}

/// Put `selection` (sorted char range) back into the field's saved state.
fn restore_selection(ctx: &egui::Context, id: Id, selection: Option<(usize, usize)>) {
    if let Some((a, b)) = selection {
        let mut state = TextEditState::load(ctx, id).unwrap_or_default();
        state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(a), CCursor::new(b))));
        state.store(ctx, id);
    }
}

/// Run a menu command on the field `id`. Clicking the menu took the keyboard focus from the field; it gets it back
/// first, so the cut, copy or paste that eframe delivers next frame reaches it, at its kept selection.
pub(super) fn run(ctx: &egui::Context, id: Id, cmd: EditCommand, len: usize) {
    ctx.memory_mut(|m| m.request_focus(id));
    match cmd.viewport_command() {
        Some(command) => ctx.send_viewport_cmd(command),
        None => restore_selection(ctx, id, Some((0, len))),
    }
    ctx.request_repaint();
}

/// The X11-era clipboard keys that GTK and Qt honour on Linux: Shift+Insert pastes, Ctrl+Insert copies, Shift+Delete
/// cuts. egui-winit maps them on Windows only, and Grassfield's Emoji & Symbols panel (through Omarchy's
/// omarchy-menu-emoji-insert) inserts its glyph with Shift+Insert, so without this nothing arrived in Notes.
pub(super) fn legacy_clipboard_command(key: Key, m: Modifiers) -> Option<EditCommand> {
    if cfg!(target_os = "windows") || m.alt {
        return None;
    }
    match key {
        Key::Insert if m.shift && !m.command => Some(EditCommand::Paste),
        Key::Insert if m.command && !m.shift => Some(EditCommand::Copy),
        Key::Delete if m.shift && !m.command => Some(EditCommand::Cut),
        _ => None,
    }
}

/// Turn this frame's legacy clipboard keys into the clipboard actions of the focused text field (run once per frame,
/// before any widget). The key is taken out, so Shift+Delete never also deletes a character.
pub(super) fn legacy_clipboard_keys(ctx: &egui::Context) {
    if ctx.memory(|m| m.focused().is_none()) {
        return;
    }
    let mut commands = Vec::new();
    ctx.input_mut(|i| {
        i.events.retain(|e| match e {
            egui::Event::Key { key, pressed, modifiers, .. } => match legacy_clipboard_command(*key, *modifiers) {
                Some(cmd) => {
                    if *pressed {
                        commands.push(cmd);
                    }
                    false
                }
                None => true,
            },
            _ => true,
        })
    });
    for cmd in commands {
        if let Some(command) = cmd.viewport_command() {
            ctx.send_viewport_cmd(command);
        }
    }
}

/// What Return does on a list line, as in Apple Notes or Bear: the new line gets the same marker and indent ("- ",
/// "* ", "+ ", a to-do as "- [ ] " even after a checked one, "3. " after "2. ", "> "); Return on an item with nothing
/// in it ends the list (the marker goes). `caret` is a char index with no selection. None: an ordinary Return.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ListReturn {
    /// byte range of `text` replaced by `insert`
    pub range: std::ops::Range<usize>,
    pub insert: String,
    /// the caret afterwards, as a char index
    pub caret: usize,
}

/// (indent, marker as written, marker for the next line) of a list line.
fn list_marker(line: &str) -> Option<(&str, &str, String)> {
    let indent_len = line.len() - line.trim_start_matches([' ', '\t']).len();
    let (indent, rest) = line.split_at(indent_len);
    for todo in ["- [ ]", "- [x]", "- [X]", "* [ ]", "* [x]", "* [X]"] {
        // "- [ ] text", or a bare "- [ ]" at the end of the line
        if rest.starts_with(&format!("{todo} ")) || rest == todo {
            let len = (todo.len() + 1).min(rest.len());
            return Some((indent, &rest[..len], format!("{}[ ] ", &todo[..2])));
        }
    }
    for bullet in ["- ", "* ", "+ ", "> "] {
        if rest.starts_with(bullet) {
            return Some((indent, &rest[..bullet.len()], bullet.to_string()));
        }
    }
    let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if (1..=9).contains(&digits) {
        let after = &rest[digits..];
        for delim in [". ", ") "] {
            if after.starts_with(delim) {
                let n: u64 = rest[..digits].parse().ok()?;
                return Some((indent, &rest[..digits + 2], format!("{}{}", n + 1, delim)));
            }
        }
    }
    None
}

/// Whether the end of `before` (whole lines) is inside a fenced code block: a line of three or more ` or ~ (at most
/// three spaces of indent) opens one, a backtick fence's info string has no backtick (so ```code``` on one line is
/// inline code, not a fence), and a line of the same character, at least as long and with nothing after it, closes it.
pub(super) fn in_code_block(before: &str) -> bool {
    let mut open: Option<(char, usize)> = None;
    for line in before.lines() {
        let indent = line.len() - line.trim_start_matches(' ').len();
        if indent > 3 {
            continue;
        }
        let rest = &line[indent..];
        let Some(c) = rest.chars().next().filter(|c| *c == '`' || *c == '~') else { continue };
        let run = rest.len() - rest.trim_start_matches(c).len();
        if run < 3 {
            continue;
        }
        let after = &rest[run..];
        match open {
            None if !(c == '`' && after.contains('`')) => open = Some((c, run)),
            Some((oc, olen)) if oc == c && run >= olen && after.trim().is_empty() => open = None,
            _ => {}
        }
    }
    open.is_some()
}

/// The indent of the closest list line above `start` that is less indented than `indent`: where an empty nested item
/// goes on Return. None when there is no such parent (the item is at the top level of its list).
fn parent_indent<'a>(text: &'a str, start: usize, indent: &str) -> Option<&'a str> {
    for line in text[..start].lines().rev() {
        if line.trim().is_empty() {
            return None;
        }
        let (i, _, _) = list_marker(line)?;
        if i.len() < indent.len() {
            return Some(i);
        }
    }
    None
}

pub(super) fn list_return(text: &str, caret: usize) -> Option<ListReturn> {
    let at = text.char_indices().nth(caret).map(|(b, _)| b).unwrap_or(text.len());
    let start = text[..at].rfind('\n').map(|x| x + 1).unwrap_or(0);
    let end = text[at..].find('\n').map(|x| at + x).unwrap_or(text.len());
    // inside a code block Return is just Return
    if in_code_block(&text[..start]) {
        return None;
    }
    let line = &text[start..end];
    let (indent, marker, next) = list_marker(line)?;
    let body_at = start + indent.len() + marker.len();
    if at < body_at {
        return None;
    }
    if text[body_at..end].trim().is_empty() {
        if !indent.is_empty() {
            // an empty nested item moves out one level (to its parent's indent), as in Apple Notes
            let to = parent_indent(text, start, indent).unwrap_or("");
            let insert = format!("{to}{marker}");
            let caret = text[..start].chars().count() + insert.chars().count();
            return Some(ListReturn { range: start..body_at, insert, caret });
        }
        // an empty item at the top level: Return ends the list
        let caret = text[..start].chars().count();
        return Some(ListReturn { range: start..end, insert: String::new(), caret });
    }
    let insert = format!("\n{indent}{next}");
    let caret = caret + insert.chars().count();
    Some(ListReturn { range: at..at, insert, caret })
}

/// A picture pasted from the clipboard (Ctrl+V with an image and no text on it), taken out of this frame's input so
/// no other field sees it.
pub(super) fn take_pasted_image(ctx: &egui::Context) -> Option<Arc<egui::ColorImage>> {
    ctx.input_mut(|i| {
        let at = i.events.iter().position(|e| matches!(e, egui::Event::PasteImage(_)))?;
        match i.events.remove(at) {
            egui::Event::PasteImage(image) => Some(image),
            _ => None,
        }
    })
}

/// The pasted picture as PNG bytes.
pub(super) fn encode_png(image: &egui::ColorImage) -> Result<Vec<u8>, String> {
    let [w, h] = image.size;
    let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
    let buf = image::RgbaImage::from_raw(w as u32, h as u32, rgba).ok_or_else(|| "bad picture size".to_string())?;
    let mut out = std::io::Cursor::new(Vec::new());
    buf.write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

/// Why a pasted picture was not stored.
#[derive(Debug)]
pub(super) enum PasteError {
    /// wider or taller than the attached-picture limit, or heavier once encoded
    TooLarge,
    Io(std::io::Error),
}

impl std::fmt::Display for PasteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PasteError::TooLarge => f.write_str("too large"),
            PasteError::Io(e) => e.fmt(f),
        }
    }
}

static PASTE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Write `bytes` to `dir/name` as a new file, mode 0600, never replacing anything: a hidden temporary file in the
/// same folder is written and flushed (created exclusively, links refused), then hard-linked to `name` (which fails if
/// the name exists), then removed. On any failure no partial file stays behind.
pub(super) fn write_new_complete(dir: &std::path::Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    let seq = PASTE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = dir.join(format!(".{name}.gf-paste-{}-{seq}.tmp", std::process::id()));
    if let Err(e) = crate::safefs::write_new(&tmp, bytes, 0o600) {
        if e.kind() != std::io::ErrorKind::AlreadyExists {
            let _ = std::fs::remove_file(&tmp);
        }
        return Err(e);
    }
    let linked = std::fs::hard_link(&tmp, dir.join(name));
    let _ = std::fs::remove_file(&tmp);
    linked
}

/// Store a pasted picture in `dir` as `stem.png` (or `stem 2.png`…): refused above `max_side` pixels per side or
/// `max_bytes` once encoded, checked before anything is written. Returns the file name.
pub(super) fn store_pasted_png_with(
    dir: &std::path::Path,
    stem: &str,
    image: &egui::ColorImage,
    max_side: u32,
    max_bytes: u64,
) -> Result<String, PasteError> {
    let [w, h] = image.size;
    if w as u64 > max_side as u64 || h as u64 > max_side as u64 {
        return Err(PasteError::TooLarge);
    }
    let png = encode_png(image).map_err(|e| PasteError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
    if png.len() as u64 > max_bytes {
        return Err(PasteError::TooLarge);
    }
    std::fs::create_dir_all(dir).map_err(PasteError::Io)?;
    for _ in 0..100 {
        let name = free_name(dir, stem);
        match write_new_complete(dir, &name, &png) {
            Ok(()) => return Ok(name),
            // taken between the look and the write: try the next free name
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(PasteError::Io(e)),
        }
    }
    Err(PasteError::Io(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "no free name")))
}

/// `store_pasted_png_with` at the limits of an attached picture (attach.rs).
pub(super) fn store_pasted_png(
    dir: &std::path::Path,
    stem: &str,
    image: &egui::ColorImage,
) -> Result<String, PasteError> {
    store_pasted_png_with(dir, stem, image, crate::attach::MAX_PICTURE_SIDE, crate::attach::MAX_PICTURE_BYTES)
}

/// A file name for a pasted picture that nothing in `dir` uses yet: `image-20261122-103000.png`, then `… 2.png`.
pub(super) fn free_name(dir: &std::path::Path, stem: &str) -> String {
    let mut name = format!("{stem}.png");
    let mut k = 2;
    while std::fs::symlink_metadata(dir.join(&name)).is_ok() {
        name = format!("{stem} {k}.png");
        k += 1;
    }
    name
}

impl GrassApp {
    /// Store a pasted picture as an attachment of the note and show it inline at the cursor, as a dropped one.
    pub(super) fn paste_image(&mut self, ctx: &egui::Context, id: u64, image: &egui::ColorImage) {
        let dir = Library::attachment_dir(id);
        let stem = format!("{}-{}", crate::i18n::tr("image"), chrono::Local::now().format("%Y%m%d-%H%M%S"));
        let saved = store_pasted_png(&dir, &stem, image);
        match saved {
            Ok(name) => {
                if let Some(n) = self.lib.get_mut(id) {
                    // read = false: the text in the picture is read in the background, so search finds it
                    n.attachments.push(model::Attachment { name: name.clone(), text: String::new(), read: false });
                    n.modified = now();
                }
                self.insert_image_ref(id, &name);
                self.layout_cache = None;
                self.mark_dirty(ctx);
                // the picture's texture loads on the next frame: ask for it now, not at the next pointer move
                ctx.request_repaint();
            }
            Err(PasteError::TooLarge) => self.notify(
                ctx,
                crate::i18n::tr("Image trop grande pour être collée (20 000 px de côté et 64 Mo au plus)"),
            ),
            Err(e) => self
                .notify(ctx, crate::i18n::trf("Image non enregistrée : {e}", &[("e", &e as &dyn std::fmt::Display)])),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_lists_the_four_commands_in_native_order() {
        let cmds: Vec<EditCommand> = menu_items(true, true, true).iter().map(|(c, _)| *c).collect();
        assert_eq!(cmds, vec![EditCommand::Cut, EditCommand::Copy, EditCommand::Paste, EditCommand::SelectAll]);
        assert!(menu_items(true, true, true).iter().all(|(_, on)| *on));
    }

    #[test]
    fn menu_enables_only_what_can_run() {
        // no selection: nothing to cut or copy, paste and select all still work
        assert_eq!(
            menu_items(true, false, true),
            [
                (EditCommand::Cut, false),
                (EditCommand::Copy, false),
                (EditCommand::Paste, true),
                (EditCommand::SelectAll, true)
            ]
        );
        // read-only (reading mode, a note in the trash): copy and select all only
        assert_eq!(
            menu_items(false, true, true),
            [
                (EditCommand::Cut, false),
                (EditCommand::Copy, true),
                (EditCommand::Paste, false),
                (EditCommand::SelectAll, true)
            ]
        );
        // empty note: paste only
        assert_eq!(
            menu_items(true, false, false),
            [
                (EditCommand::Cut, false),
                (EditCommand::Copy, false),
                (EditCommand::Paste, true),
                (EditCommand::SelectAll, false)
            ]
        );
    }

    #[test]
    fn clipboard_commands_go_through_eframe_and_use_the_platform_shortcuts() {
        assert_eq!(EditCommand::Cut.viewport_command(), Some(egui::ViewportCommand::RequestCut));
        assert_eq!(EditCommand::Copy.viewport_command(), Some(egui::ViewportCommand::RequestCopy));
        assert_eq!(EditCommand::Paste.viewport_command(), Some(egui::ViewportCommand::RequestPaste));
        assert_eq!(EditCommand::SelectAll.viewport_command(), None);
        let keys: Vec<Key> = [EditCommand::Cut, EditCommand::Copy, EditCommand::Paste, EditCommand::SelectAll]
            .iter()
            .map(|c| c.shortcut().logical_key)
            .collect();
        assert_eq!(keys, vec![Key::X, Key::C, Key::V, Key::A]);
        assert!([EditCommand::Cut, EditCommand::Copy, EditCommand::Paste, EditCommand::SelectAll]
            .iter()
            .all(|c| c.shortcut().modifiers == Modifiers::COMMAND));
    }

    #[test]
    fn right_click_keeps_the_selection_it_lands_on() {
        assert!(right_click_keeps(Some((4, 9)), 4));
        assert!(right_click_keeps(Some((4, 9)), 6));
        assert!(right_click_keeps(Some((4, 9)), 9));
        assert!(!right_click_keeps(Some((4, 9)), 10));
        assert!(!right_click_keeps(Some((4, 9)), 3));
        // a bare caret is not a selection: the caret moves to the click
        assert!(!right_click_keeps(Some((5, 5)), 5));
        assert!(!right_click_keeps(None, 0));
    }

    #[test]
    fn the_app_never_takes_the_clipboard_or_editing_chords() {
        // Ctrl+C/X/V become clipboard events in egui-winit only while nothing consumes them, and Ctrl+A/Z/Y reach
        // the text field as keys: no shortcut of the app may use them. The one exception is the sketch editor's
        // Ctrl+Z (sketcher.rs), which runs while the sketch window, not a text field, has the keys.
        let files = [
            ("app.rs", include_str!("../app.rs")),
            ("attachments.rs", include_str!("attachments.rs")),
            ("editor.rs", include_str!("editor.rs")),
            ("files.rs", include_str!("files.rs")),
            ("imageedit.rs", include_str!("imageedit.rs")),
            ("list.rs", include_str!("list.rs")),
            ("overlay.rs", include_str!("overlay.rs")),
            ("panels.rs", include_str!("panels.rs")),
            ("settings.rs", include_str!("settings.rs")),
            ("sidebar.rs", include_str!("sidebar.rs")),
            ("sketcher.rs", include_str!("sketcher.rs")),
        ];
        let mut sketch_undo = 0;
        for (file, src) in files {
            for line in src.lines().filter(|l| l.contains("consume_shortcut") || l.contains("consume_key")) {
                let plain_cmd = line.contains("KeyboardShortcut::new(Modifiers::COMMAND, ")
                    || line.contains("KeyboardShortcut::new(cmd, ")
                    || line.contains("consume_key(Modifiers::COMMAND, ");
                for key in ["Key::C)", "Key::V)", "Key::X)", "Key::A)", "Key::Z)", "Key::Y)"] {
                    if plain_cmd && line.contains(key) {
                        if file == "sketcher.rs" && key == "Key::Z)" {
                            sketch_undo += 1;
                            continue;
                        }
                        panic!("{key} is taken by {file}: {line}");
                    }
                }
            }
        }
        assert_eq!(sketch_undo, 1, "the sketch editor's own undo is expected once");
    }

    #[test]
    fn legacy_clipboard_keys_map_as_in_gtk_and_qt() {
        let shift = Modifiers::SHIFT;
        let ctrl = Modifiers::COMMAND;
        // on Windows egui-winit maps these keys itself (is_paste_command & co.), so this mapping stays off there
        let on = |cmd| if cfg!(target_os = "windows") { None } else { Some(cmd) };
        assert_eq!(legacy_clipboard_command(Key::Insert, shift), on(EditCommand::Paste));
        assert_eq!(legacy_clipboard_command(Key::Insert, ctrl), on(EditCommand::Copy));
        assert_eq!(legacy_clipboard_command(Key::Delete, shift), on(EditCommand::Cut));
        // plain Delete and Insert, and other chords, stay keys
        assert_eq!(legacy_clipboard_command(Key::Delete, Modifiers::NONE), None);
        assert_eq!(legacy_clipboard_command(Key::Delete, ctrl), None);
        assert_eq!(legacy_clipboard_command(Key::Insert, Modifiers::NONE), None);
        assert_eq!(legacy_clipboard_command(Key::Insert, ctrl | shift), None);
        assert_eq!(legacy_clipboard_command(Key::Insert, shift | Modifiers::ALT), None);
        assert_eq!(legacy_clipboard_command(Key::V, ctrl), None);
    }

    fn apply(text: &str, caret: usize) -> Option<(String, usize)> {
        let e = list_return(text, caret)?;
        let mut t = text.to_string();
        t.replace_range(e.range, &e.insert);
        Some((t, e.caret))
    }

    #[test]
    fn return_continues_bullets_todos_numbers_and_quotes() {
        assert_eq!(apply("- un", 4), Some(("- un\n- ".into(), 7)));
        assert_eq!(apply("* un", 4), Some(("* un\n* ".into(), 7)));
        assert_eq!(apply("  - deux", 8), Some(("  - deux\n  - ".into(), 13)));
        assert_eq!(apply("- [x] fait", 10), Some(("- [x] fait\n- [ ] ".into(), 17)));
        assert_eq!(apply("- [ ] à faire", 13), Some(("- [ ] à faire\n- [ ] ".into(), 20)));
        assert_eq!(apply("9. neuf", 7), Some(("9. neuf\n10. ".into(), 12)));
        assert_eq!(apply("1) un", 5), Some(("1) un\n2) ".into(), 9)));
        assert_eq!(apply("> cité", 6), Some(("> cité\n> ".into(), 9)));
        // in the middle of an item, the rest moves to the new item
        assert_eq!(apply("- abcd", 4), Some(("- ab\n- cd".into(), 7)));
    }

    #[test]
    fn return_on_an_empty_item_ends_the_list() {
        assert_eq!(apply("- un\n- ", 7), Some(("- un\n".into(), 5)));
        assert_eq!(apply("- un\n- [ ] ", 11), Some(("- un\n".into(), 5)));
        assert_eq!(apply("1. un\n2. ", 9), Some(("1. un\n".into(), 6)));
    }

    #[test]
    fn return_is_plain_elsewhere() {
        assert_eq!(list_return("du texte", 8), None);
        assert_eq!(list_return("-pas une liste", 14), None);
        assert_eq!(list_return("1234567890. trop long", 21), None);
        // caret inside the marker
        assert_eq!(list_return("- un", 1), None);
        // inside a code block
        assert_eq!(list_return("```\n- pas une liste", 18), None);
        assert!(list_return("```\ncode\n```\n- liste", 21).is_some());
    }

    #[test]
    fn return_handles_bare_todos_nested_items_and_every_fence() {
        // a bare to-do at the end of the line is a to-do, not a bullet
        assert_eq!(apply("- [ ] un\n- [ ]", 14), Some(("- [ ] un\n".into(), 9)));
        assert_eq!(apply("- [x]", 5), Some(("".into(), 0)));
        // an empty nested item moves out one level, to its parent's indent
        assert_eq!(apply("- un\n  - deux\n  - ", 18), Some(("- un\n  - deux\n- ".into(), 16)));
        assert_eq!(apply("- a\n    - b\n        - ", 22), Some(("- a\n    - b\n    - ".into(), 18)));
        assert_eq!(apply("- [ ] a\n\t- [ ] ", 15), Some(("- [ ] a\n- [ ] ".into(), 14)));
        // ~~~ fences count, a one-line ```code``` is not a fence
        assert_eq!(list_return("~~~\n- dans le bloc", 18), None);
        assert!(list_return("~~~\ncode\n~~~\n- liste", 20).is_some());
        assert!(list_return("```du code```\n- liste", 21).is_some());
        // a ``` line does not close a ~~~ block, a shorter fence does not close a longer one
        assert_eq!(list_return("~~~\n```\n- dedans", 15), None);
        assert_eq!(list_return("````\n```\n- dedans", 16), None);
        assert!(in_code_block("```rust\nfn main() {}\n"));
        assert!(!in_code_block("```rust\nfn main() {}\n```\n"));
        assert!(!in_code_block("    ```\n"));
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("gf-notes-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entries(dir: &std::path::Path) -> Vec<String> {
        let mut v: Vec<String> =
            std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    }

    #[test]
    fn pasted_pictures_are_stored_within_the_limits_and_never_replace_a_file() {
        let dir = scratch("store");
        let small = egui::ColorImage::new([4, 3], vec![egui::Color32::from_rgb(1, 2, 3); 12]);
        assert_eq!(store_pasted_png(&dir, "img", &small).unwrap(), "img.png");
        assert_eq!(store_pasted_png(&dir, "img", &small).unwrap(), "img 2.png");
        assert_eq!(entries(&dir), vec!["img 2.png".to_string(), "img.png".to_string()]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join("img.png")).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        // above the side limit or the size limit: refused, nothing written
        let wide = egui::ColorImage::new([21, 1], vec![egui::Color32::BLACK; 21]);
        assert!(matches!(store_pasted_png_with(&dir, "wide", &wide, 20, u64::MAX), Err(PasteError::TooLarge)));
        assert!(matches!(store_pasted_png_with(&dir, "heavy", &small, 20, 10), Err(PasteError::TooLarge)));
        let tall = egui::ColorImage::new([1, 20_001], vec![egui::Color32::BLACK; 20_001]);
        assert!(matches!(store_pasted_png(&dir, "tall", &tall), Err(PasteError::TooLarge)));
        assert_eq!(entries(&dir).len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_write_leaves_no_partial_file() {
        let dir = scratch("cleanup");
        // the final name cannot be made (its folder does not exist): the temporary file goes too
        assert!(write_new_complete(&dir, "missing/x.png", b"data").is_err());
        assert!(entries(&dir).is_empty(), "{:?}", entries(&dir));
        // an existing name is never replaced
        std::fs::write(dir.join("x.png"), b"old").unwrap();
        let e = write_new_complete(&dir, "x.png", b"new").unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(dir.join("x.png")).unwrap(), b"old");
        assert_eq!(entries(&dir), vec!["x.png".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_note_in_the_trash_can_be_selected_and_copied_but_not_changed() {
        // the editor shows a trashed note through an immutable buffer (editor.rs): typing, pasting, cutting and
        // deleting change nothing, Select All and Copy still work
        let ctx = egui::Context::default();
        let id = Id::unique("trashed-note");
        let text = String::from("dans la corbeille");
        let input = |events: Vec<egui::Event>| egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
            events,
            ..Default::default()
        };
        let frame = |events: Vec<egui::Event>| {
            let mut out = ctx.run_ui(input(events), |ui| {
                let mut ro: &str = text.as_str();
                egui::TextEdit::multiline(&mut ro).id(id).show(ui);
            });
            // no renderer here: the font atlas upload is dropped on purpose
            out.textures_delta.clear();
            out
        };
        frame(vec![]);
        ctx.memory_mut(|m| m.request_focus(id));
        run(&ctx, id, EditCommand::SelectAll, text.chars().count());
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        frame(vec![
            egui::Event::Text("x".into()),
            egui::Event::Paste("collé".into()),
            key(Key::Backspace),
            key(Key::Delete),
        ]);
        frame(vec![egui::Event::Cut]);
        assert_eq!(text, "dans la corbeille");
        // the edits above collapse the selection: select again, then copy
        run(&ctx, id, EditCommand::SelectAll, text.chars().count());
        let out = frame(vec![egui::Event::Copy]);
        assert!(
            out.platform_output
                .commands
                .iter()
                .any(|c| matches!(c, egui::OutputCommand::CopyText(t) if t == "dans la corbeille")),
            "{:?}",
            out.platform_output.commands
        );
    }

    #[test]
    fn a_pasted_picture_becomes_a_png() {
        let image = egui::ColorImage::new([3, 2], vec![egui::Color32::from_rgb(200, 10, 30); 6]);
        let png = encode_png(&image).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let back = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!((back.width(), back.height()), (3, 2));
        assert_eq!(back.get_pixel(1, 1).0, [200, 10, 30, 255]);
    }

    #[test]
    fn pasted_pictures_never_overwrite_one_another() {
        let dir = std::env::temp_dir().join(format!("gf-notes-paste-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(free_name(&dir, "image-1"), "image-1.png");
        std::fs::write(dir.join("image-1.png"), b"x").unwrap();
        assert_eq!(free_name(&dir, "image-1"), "image-1 2.png");
        #[cfg(unix)]
        {
            // a dangling link at the name counts as taken
            std::os::unix::fs::symlink(dir.join("nowhere"), dir.join("image-1 2.png")).unwrap();
            assert_eq!(free_name(&dir, "image-1"), "image-1 3.png");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
