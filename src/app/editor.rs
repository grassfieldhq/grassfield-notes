//! Right side: formatting toolbar and the live-styled Markdown editor.

use super::*;

impl GrassApp {
    fn char_to_byte(s: &str, ci: usize) -> usize {
        s.char_indices().nth(ci).map(|(b, _)| b).unwrap_or(s.len())
    }

    fn byte_to_char(s: &str, b: usize) -> usize {
        s[..b.min(s.len())].chars().count()
    }

    fn current_range(&self, text: &str) -> (usize, usize) {
        let n = text.chars().count();
        let (a, b) = self.cursor.unwrap_or((n, n));
        (a.min(n), b.min(n))
    }

    /// Wrap the current selection with markers like `**`.
    pub(super) fn wrap_selection(&mut self, prefix: &str, suffix: &str, placeholder: &str) {
        let Some(id) = self.selected else { return };
        let Some(text) = self.lib.get(id).map(|n| n.text.clone()) else { return };
        let (a, b) = self.current_range(&text);
        let Some(note) = self.lib.get_mut(id) else { return };
        let ba = Self::char_to_byte(&note.text, a);
        let bb = Self::char_to_byte(&note.text, b);
        let pl = prefix.chars().count();
        if a == b {
            let ins = format!("{prefix}{placeholder}{suffix}");
            note.text.insert_str(ba, &ins);
            let ph = placeholder.chars().count();
            self.pending_cursor = Some((a + pl, a + pl + ph));
        } else {
            let sl = suffix.chars().count();
            let selected = &note.text[ba..bb];
            // "*" must not match one star of a "**" bold marker
            let lone_star_ok = |before: &str, after: &str| {
                prefix != "*" || (!before.ends_with("**") && !after.starts_with("**"))
            };
            let around = note.text[..ba].ends_with(prefix)
                && note.text[bb..].starts_with(suffix)
                && lone_star_ok(&note.text[..ba], &note.text[bb..]);
            let inside = selected.len() >= prefix.len() + suffix.len()
                && selected.starts_with(prefix)
                && selected.ends_with(suffix)
                && (prefix != "*" || (!selected.starts_with("**") && !selected.ends_with("**")));
            if around {
                // markers just outside the selection: remove them
                note.text.replace_range(bb..bb + suffix.len(), "");
                note.text.replace_range(ba - prefix.len()..ba, "");
                self.pending_cursor = Some((a - pl, b - pl));
            } else if inside && b - a >= pl + sl {
                // the selection includes the markers: remove them
                note.text.replace_range(bb - suffix.len()..bb, "");
                note.text.replace_range(ba..ba + prefix.len(), "");
                self.pending_cursor = Some((a, b - pl - sl));
            } else {
                note.text.insert_str(bb, suffix);
                note.text.insert_str(ba, prefix);
                self.pending_cursor = Some((a + pl, b + pl));
            }
        }
        note.modified = now();
        self.focus_editor = true;
        self.dirty = true;
    }

    /// Toggle a prefix (heading, list, quote, todo) on the selected lines.
    pub(super) fn line_prefix(&mut self, prefix: &str) {
        let Some(id) = self.selected else { return };
        let Some(text) = self.lib.get(id).map(|n| n.text.clone()) else { return };
        let (a, b) = self.current_range(&text);
        let Some(note) = self.lib.get_mut(id) else { return };
        let ba = Self::char_to_byte(&note.text, a);
        let bb = Self::char_to_byte(&note.text, b);
        let start = note.text[..ba].rfind('\n').map(|x| x + 1).unwrap_or(0);
        let end = note.text[bb..].find('\n').map(|x| bb + x).unwrap_or(note.text.len());
        let block = note.text[start..end].to_string();
        let known = [
            "- [ ] ", "- [x] ", "- [X] ", "> ", "- ", "* ", "###### ", "##### ", "#### ", "### ", "## ", "# ",
        ];
        let lines: Vec<&str> = block.split('\n').collect();
        let all_have = lines.iter().all(|l| l.starts_with(prefix));
        let mut out = Vec::new();
        for l in &lines {
            let mut s = l.to_string();
            if all_have {
                s = s[prefix.len()..].to_string();
            } else {
                for k in known {
                    if let Some(r) = s.strip_prefix(k) {
                        s = r.to_string();
                        break;
                    }
                }
                s = format!("{prefix}{s}");
            }
            out.push(s);
        }
        let new_block = out.join("\n");
        note.text.replace_range(start..end, &new_block);
        let start_c = Self::byte_to_char(&note.text, start);
        let end_c = start_c + new_block.chars().count();
        if lines.len() == 1 {
            self.pending_cursor = Some((end_c, end_c));
        } else {
            self.pending_cursor = Some((start_c, end_c));
        }
        note.modified = now();
        self.focus_editor = true;
        self.dirty = true;
    }

    fn insert_text(&mut self, s: &str) {
        let Some(id) = self.selected else { return };
        let Some(text) = self.lib.get(id).map(|n| n.text.clone()) else { return };
        let (a, b) = self.current_range(&text);
        let Some(note) = self.lib.get_mut(id) else { return };
        let ba = Self::char_to_byte(&note.text, a);
        let bb = Self::char_to_byte(&note.text, b);
        note.text.replace_range(ba..bb, s);
        let c = a + s.chars().count();
        self.pending_cursor = Some((c, c));
        note.modified = now();
        self.focus_editor = true;
        self.dirty = true;
    }

    pub(super) fn toggle_todo_at_line(text: &mut String, line_start: usize) -> bool {
        let rest = &text[line_start..];
        let indent = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        let p = line_start + indent;
        let r = &text[p..];
        if r.starts_with("- [ ]") || r.starts_with("* [ ]") {
            text.replace_range(p + 3..p + 4, "x");
            true
        } else if r.starts_with("- [x]") || r.starts_with("- [X]") || r.starts_with("* [x]") || r.starts_with("* [X]") {
            text.replace_range(p + 3..p + 4, " ");
            true
        } else {
            false
        }
    }

    fn toolbar_button(ui: &mut egui::Ui, t: &Theme, label: egui::RichText, tip: &str) -> bool {
        // frame only on hover, so the toolbar stays calm but buttons still react
        ui.add(
            egui::Button::new(label.color(t.muted))
                .frame(true)
                .frame_when_inactive(false)
                .min_size(Vec2::new(26.0, 26.0)),
        )
        .on_hover_text(tip)
        .clicked()
    }

    fn toolbar_sep(ui: &mut egui::Ui, t: &Theme) {
        let (r, _) = ui.allocate_exact_size(Vec2::new(13.0, 26.0), Sense::hover());
        ui.painter().vline(r.center().x, (r.top() + 6.0)..=(r.bottom() - 6.0), Stroke::new(1.0, t.border));
    }

    /// Formatting buttons: (icon, tooltip, menu name, ends a group).
    fn format_items(t: &Theme) -> Vec<(egui::RichText, &'static str, &'static str, bool)> {
        let bold = |s: &str| egui::RichText::new(s).font(FontId::new(14.0, FontFamily::Name("bold".into())));
        vec![
            (bold("H1"), crate::i18n::tr("Titre 1"), crate::i18n::tr("Titre 1"), false),
            (bold("H2"), crate::i18n::tr("Titre 2"), crate::i18n::tr("Titre 2"), false),
            (bold("H3"), crate::i18n::tr("Titre 3"), crate::i18n::tr("Titre 3"), true),
            (bold("B"), crate::i18n::tr("Gras (Ctrl+B)"), crate::i18n::tr("Gras"), false),
            (
                egui::RichText::new("I").font(FontId::new(14.0, FontFamily::Name("italic".into()))),
                crate::i18n::tr("Italique (Ctrl+I)"),
                crate::i18n::tr("Italique"),
                false,
            ),
            (egui::RichText::new("S").strikethrough().size(14.0), crate::i18n::tr("Barré (Ctrl+Maj+X)"), crate::i18n::tr("Barré"), false),
            (
                egui::RichText::new("ab").size(13.0).background_color(t.highlight_bg),
                crate::i18n::tr("Surligner (Ctrl+Maj+H)"),
                crate::i18n::tr("Surligner"),
                false,
            ),
            (egui::RichText::new("</>").monospace().size(12.0), crate::i18n::tr("Code en ligne"), crate::i18n::tr("Code en ligne"), true),
            (egui::RichText::new("☰").size(14.0), crate::i18n::tr("Liste à puces"), crate::i18n::tr("Liste à puces"), false),
            (
                egui::RichText::new("☑").size(14.0),
                crate::i18n::tr("Liste de tâches (Ctrl+Entrée pour cocher)"),
                crate::i18n::tr("Liste de tâches"),
                false,
            ),
            (egui::RichText::new("“").size(18.0), crate::i18n::tr("Citation"), crate::i18n::tr("Citation"), false),
            (egui::RichText::new("🔗").size(13.0), crate::i18n::tr("Lien vers une note [[…]] (Ctrl+K)"), crate::i18n::tr("Lien vers une note"), false),
            (egui::RichText::new("—").size(14.0), crate::i18n::tr("Séparateur"), crate::i18n::tr("Séparateur"), false),
            (egui::RichText::new("#").size(14.0), crate::i18n::tr("Ajouter un tag"), crate::i18n::tr("Ajouter un tag"), false),
        ]
    }

    fn apply_format(&mut self, i: usize) {
        match i {
            0 => self.line_prefix("# "),
            1 => self.line_prefix("## "),
            2 => self.line_prefix("### "),
            3 => self.wrap_selection("**", "**", crate::i18n::tr("gras")),
            4 => self.wrap_selection("*", "*", crate::i18n::tr("italique")),
            5 => self.wrap_selection("~~", "~~", crate::i18n::tr("barré")),
            6 => self.wrap_selection("==", "==", crate::i18n::tr("surligné")),
            7 => self.wrap_selection("`", "`", crate::i18n::tr("code")),
            8 => self.line_prefix("- "),
            9 => self.line_prefix("- [ ] "),
            10 => self.line_prefix("> "),
            11 => self.wrap_selection("[[", "]]", crate::i18n::tr("Titre de la note")),
            12 => self.insert_text("\n---\n"),
            _ => self.wrap_selection("#", "", crate::i18n::tr("tag")),
        }
    }

    fn editor_toolbar(&mut self, ui: &mut egui::Ui) {
        let t = self.theme();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.add_space(8.0);
            // "show / hide columns" icon: a window outline with a filled left column when columns are shown
            let (r, resp) = ui.allocate_exact_size(Vec2::new(30.0, 26.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, 6.0, t.list_selected);
            }
            let ib = Rect::from_center_size(r.center(), Vec2::new(17.0, 13.0));
            let col = if resp.hovered() { t.text } else { t.muted };
            if !self.focus_mode {
                ui.painter().rect_filled(
                    Rect::from_min_max(ib.min, Pos2::new(ib.left() + 6.0, ib.bottom())),
                    CornerRadius { nw: 3, sw: 3, ne: 0, se: 0 },
                    col.gamma_multiply(0.5),
                );
            }
            ui.painter().rect_stroke(ib, 3.0, Stroke::new(1.3, col), egui::StrokeKind::Inside);
            ui.painter().vline(ib.left() + 6.0, ib.y_range(), Stroke::new(1.3, col));
            let tip = if self.focus_mode { crate::i18n::tr("Afficher les colonnes (Ctrl+Maj+F)") } else { crate::i18n::tr("Mode concentration (Ctrl+Maj+F)") };
            if resp.on_hover_text(tip).clicked() {
                self.focus_mode = !self.focus_mode;
            }
            Self::toolbar_sep(ui, &t);
            let has_note = self.selected.and_then(|id| self.lib.get(id)).map(|n| !n.trashed).unwrap_or(false);
            // On narrow editors only the most used buttons stay visible; the rest go in an "Aa" menu
            // so the toolbar never pushes the note text out of view.
            let compact = ui.available_width() < 665.0;
            let mut picked: Option<usize> = None;
            ui.add_enabled_ui(has_note, |ui| {
                let items = Self::format_items(&t);
                for (i, (label, tip, _name, group_end)) in items.iter().enumerate() {
                    if compact && ![0, 3, 4, 9].contains(&i) {
                        continue;
                    }
                    if Self::toolbar_button(ui, &t, label.clone(), tip) {
                        picked = Some(i);
                    }
                    if *group_end && !compact {
                        Self::toolbar_sep(ui, &t);
                    }
                }
                if compact {
                    ui.menu_button(egui::RichText::new("Aa…").size(13.0).color(t.muted), |ui| {
                        ui.set_min_width(200.0);
                        for (i, (_, tip, name, _)) in items.iter().enumerate() {
                            if [0, 3, 4, 9].contains(&i) {
                                continue;
                            }
                            if ui.button(*name).on_hover_text(*tip).clicked() {
                                picked = Some(i);
                                ui.close();
                            }
                        }
                    });
                }
            });
            if let Some(i) = picked {
                self.apply_format(i);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(10.0);
                let info_col = if self.show_info { t.accent_text } else { t.muted };
                if ui
                    .add(egui::Button::new(egui::RichText::new("ℹ").size(16.0).color(info_col)).frame(true).frame_when_inactive(false).min_size(Vec2::new(26.0, 26.0)))
                    .on_hover_text(crate::i18n::tr("Infos sur la note (Ctrl+Maj+I)"))
                    .clicked()
                {
                    self.show_info = !self.show_info;
                }
                if let Some(id) = self.selected {
                    if let Some(n) = self.lib.get(id) {
                        let (trashed, pinned, archived) = (n.trashed, n.pinned, n.archived);
                        ui.menu_button(egui::RichText::new("•••").size(10.0).color(t.muted), |ui| {
                            Self::note_menu(ui, &mut self.actions, id, trashed, pinned, archived);
                        });
                        if !trashed {
                            if ui
                                .add(
                                    egui::Button::new(egui::RichText::new("📎").size(14.0).color(t.muted))
                                        .frame(true)
                                        .frame_when_inactive(false)
                                        .min_size(Vec2::new(26.0, 26.0)),
                                )
                                .on_hover_text(crate::i18n::tr("Joindre un document, une image ou un PDF"))
                                .clicked()
                            {
                                self.actions.push(Action::PickFiles(Purpose::Attach));
                            }
                            if ui
                                .add(
                                    egui::Button::new(egui::RichText::new("🎨").size(14.0).color(t.muted))
                                        .frame(true)
                                        .frame_when_inactive(false)
                                        .min_size(Vec2::new(26.0, 26.0)),
                                )
                                .on_hover_text(crate::i18n::tr("Nouveau croquis dans la note"))
                                .clicked()
                            {
                                self.actions.push(Action::NewSketch(id));
                            }
                            let pin_col = if pinned { t.accent } else { t.muted };
                            if ui
                                .add(egui::Button::new(egui::RichText::new("📌").size(14.0).color(pin_col)).frame(true).frame_when_inactive(false).min_size(Vec2::new(26.0, 26.0)))
                                .on_hover_text(if pinned { crate::i18n::tr("Désépingler") } else { crate::i18n::tr("Épingler en haut de la liste") })
                                .clicked()
                            {
                                self.actions.push(Action::TogglePin(id));
                            }
                        }
                    }
                }
            });
        });
    }

    pub(super) fn editor(&mut self, ui: &mut egui::Ui) {
        let t = self.theme();
        let ctx = ui.ctx().clone();
        ui.add_space(8.0);
        self.editor_toolbar(ui);
        ui.add_space(4.0);
        let r = ui.available_rect_before_wrap();
        ui.painter().hline(r.x_range(), r.top(), Stroke::new(1.0, t.border));

        let Some(id) = self.selected.filter(|id| self.lib.get(*id).is_some()) else {
            ui.add_space(ui.available_height() * 0.35);
            ui.vertical_centered(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::new(60.0, 60.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 28.0, t.tag_bg);
                Self::draw_logo(ui.painter(), r.center(), &t);
                ui.add_space(10.0);
                ui.label(egui::RichText::new(crate::i18n::tr("Aucune note sélectionnée")).size(17.0).color(t.muted));
                ui.add_space(6.0);
                if ui.button(crate::i18n::tr("✏  Nouvelle note")).clicked() {
                    self.actions.push(Action::NewNote);
                }
            });
            return;
        };
        // a cursor position only makes sense for the note it came from
        if self.cursor_note != Some(id) {
            self.cursor_note = Some(id);
            self.cursor = None;
        }

        let trashed = self.lib.get(id).map(|n| n.trashed).unwrap_or(false);
        if trashed {
            egui::Frame::new()
                .fill(t.highlight_bg.gamma_multiply(0.5))
                .inner_margin(Margin::symmetric(16, 8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(crate::i18n::tr("Cette note est dans la corbeille.")).color(t.text));
                        if ui.button(crate::i18n::tr("Restaurer")).clicked() {
                            self.actions.push(Action::Restore(id));
                        }
                        if ui.button(crate::i18n::tr("Effacer pour de bon")).clicked() {
                            self.actions.push(Action::DeleteForever(id));
                        }
                    });
                });
        }

        // status bar at the bottom
        let (words, chars, modified) = self
            .lib
            .get(id)
            .map(|n| (word_count(&n.text), n.text.chars().count(), n.modified))
            .unwrap_or((0, 0, 0));
        egui::Panel::bottom("status")
            .frame(egui::Frame::new().fill(t.editor_bg).inner_margin(Margin::symmetric(16, 6)))
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let mins = ((words as f32) / 200.0).ceil().max(1.0) as usize;
                    ui.label(
                        egui::RichText::new(crate::i18n::trf("{words} mots · {chars} caractères · {mins} min de lecture", &[("words", &words as &dyn std::fmt::Display), ("chars", &chars as &dyn std::fmt::Display), ("mins", &mins as &dyn std::fmt::Display)]))
                            .size(11.5)
                            .color(t.muted),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(crate::i18n::trf("Modifiée le {0}", &[("0", &format_full_date(modified) as &dyn std::fmt::Display)]))
                                .size(11.5)
                                .color(t.muted),
                        );
                    });
                });
            });

        self.attachments_bar(ui, id, trashed);

        let settings = self.lib.settings.clone();
        // pictures shown inside the text: (file name, texture, is it an editable sketch)
        let pics: Vec<(String, egui::TextureHandle, bool)> = {
            let atts = self.lib.get(id).map(|n| n.attachments.clone()).unwrap_or_default();
            atts.iter()
                .filter_map(|a| {
                    let tex = self.thumbs.get(&ctx, id, a)?;
                    let sk = crate::sketch::sidecar(&Library::attachment_dir(id).join(&a.name)).exists();
                    Some((a.name.clone(), tex, sk))
                })
                .collect()
        };
        let mut open_pic: Option<(String, bool)> = None;
        let mut style = MdStyle {
            images: Default::default(),
            hide_markup: false,
            theme: t,
            size: settings.font_size,
            font: settings.editor_font,
            dim_markup: settings.dim_markup,
            spacing: settings.line_spacing,
        };
        let editor_id = Id::unique(("note-editor", id));
        if let Some((a, b)) = self.pending_cursor.take() {
            let mut state = egui::text_edit::TextEditState::load(&ctx, editor_id).unwrap_or_default();
            state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(a),
                egui::text::CCursor::new(b),
            )));
            state.store(&ctx, editor_id);
            self.cursor = Some((a, b));
        }

        let mut changed = false;
        let mut clicked_box: Option<usize> = None;
        let mut ctrl_click: Option<usize> = None;
        let mut toggle_line_at: Option<usize> = None;
        // Ctrl+Enter toggles the to-do on the cursor line. It is taken before the editor
        // runs so it never also inserts a new line.
        if ctx.memory(|m| m.has_focus(editor_id)) && !trashed {
            let ce = ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Enter)));
            if ce {
                if let Some(text) = self.lib.get(id).map(|n| n.text.clone()) {
                    let (a, _) = self.current_range(&text);
                    toggle_line_at = Some(Self::char_to_byte(&text, a));
                }
            }
        }
        let focus_editor = std::mem::take(&mut self.focus_editor);
        let mut new_cursor = None;
        let style_key = (
            settings.theme,
            settings.font_size.to_bits(),
            settings.editor_font as u8,
            settings.dim_markup,
            settings.line_spacing.to_bits(),
        );
        let cache = &mut self.layout_cache;
        egui::ScrollArea::vertical()
            .id_salt(("editor-scroll", id))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // keep comfortable margins, and shrink them on narrow windows
                let avail = ui.available_width();
                let pad = if avail < 520.0 { 16.0 } else { 40.0 };
                let w = settings.line_width.min(avail - 2.0 * pad).max(120.0);
                let side = ((avail - w) / 2.0).max(pad.min(avail / 2.0));
                ui.add_space(28.0);
                // room for each inline picture: as wide as the text at most, never taller than 420
                let pic_size = |tex: &egui::TextureHandle| overlay::pic_size(tex, w);
                let mut img_key = 0u64;
                for (name, tex, _) in &pics {
                    let h = pic_size(tex).y + 16.0;
                    style.images.insert(name.clone(), h);
                    img_key = img_key.wrapping_mul(31).wrapping_add(h.to_bits() as u64 ^ name.len() as u64);
                }
                ui.horizontal(|ui| {
                    ui.add_space(side);
                    let Some(note) = self.lib.notes.iter_mut().find(|n| n.id == id) else { return };
                    // Styling the whole note is the costly part, so reuse it while nothing changed.
                    let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
                        use std::hash::{Hash, Hasher};
                        let mut h = std::collections::hash_map::DefaultHasher::new();
                        buf.as_str().hash(&mut h);
                        wrap.to_bits().hash(&mut h);
                        style_key.hash(&mut h);
                        img_key.hash(&mut h);
                        let key = h.finish();
                        let job = match cache {
                            Some((k, job)) if *k == key => job.clone(),
                            _ => {
                                let job = markdown::layout_job(buf.as_str(), &style, wrap);
                                *cache = Some((key, job.clone()));
                                job
                            }
                        };
                        ui.fonts_mut(|f| f.layout_job(job))
                    };
                    let min_h = (ui.available_height() - 40.0).max(200.0);
                    let out = egui::TextEdit::multiline(&mut note.text)
                        .id(editor_id)
                        .frame(egui::Frame::NONE)
                        .desired_width(w)
                        .min_size(Vec2::new(w, min_h))
                        .lock_focus(true)
                        .interactive(!trashed)
                        .hint_text(crate::i18n::tr("Commencez à écrire…"))
                        .layouter(&mut layouter)
                        .show(ui);
                    let resp = out.response.response.clone();
                    if focus_editor {
                        resp.request_focus();
                    }
                    if resp.changed() {
                        changed = true;
                    }
                    if let Some(r) = out.cursor_range {
                        let rr = r.as_sorted_char_range();
                        new_cursor = Some((rr.start.0, rr.end.0));
                    }

                    // Check boxes, quotes, rules and pictures drawn over the text.
                    let galley = out.galley.clone();
                    let gpos = out.galley_pos;
                    let pointer = resp.interact_pointer_pos();
                    let ov = overlay::paint(ui, &overlay::Input {
                        text: &note.text,
                        galley: &galley,
                        gpos,
                        clip: out.text_clip_rect,
                        resp: &resp,
                        width: w,
                        font_size: settings.font_size,
                        theme: &t,
                        pics: &pics,
                        editable: !trashed,
                    });
                    if ov.clicked_box.is_some() {
                        clicked_box = ov.clicked_box;
                    }
                    if ov.open_pic.is_some() {
                        open_pic = ov.open_pic;
                    }
                    // Ctrl+click on links and tags
                    let cmd_down = ui.input(|i| i.modifiers.command);
                    if resp.clicked() && cmd_down {
                        if let Some(p) = pointer {
                            let c = galley.cursor_from_pos(p - gpos);
                            ctrl_click = Some(Self::char_to_byte(&note.text, c.index.0));
                        }
                    }
                    if resp.hovered() && cmd_down {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if changed {
                        note.modified = now();
                    }
                });
                ui.add_space(40.0);
            });
        if new_cursor.is_some() {
            self.cursor = new_cursor;
        }
        if let Some((name, is_sketch)) = open_pic {
            self.actions.push(if is_sketch { Action::EditSketch(id, name) } else { Action::EditImage(id, name) });
        }

        if let Some(line_start) = clicked_box {
            if let Some(note) = self.lib.get_mut(id) {
                if Self::toggle_todo_at_line(&mut note.text, line_start) {
                    note.modified = now();
                    changed = true;
                }
            }
        }
        if let Some(b) = toggle_line_at {
            if let Some(note) = self.lib.get_mut(id) {
                let ls = note.text[..b.min(note.text.len())].rfind('\n').map(|x| x + 1).unwrap_or(0);
                if Self::toggle_todo_at_line(&mut note.text, ls) {
                    note.modified = now();
                    changed = true;
                }
            }
        }
        if let Some(b) = ctrl_click {
            if let Some(text) = self.lib.get(id).map(|n| n.text.clone()) {
                self.follow_link(&ctx, &text, b);
            }
        }
        if changed {
            self.mark_dirty(&ctx);
        }

        // formatting shortcuts while editing
        if ctx.memory(|m| m.has_focus(editor_id)) && !trashed {
            let (b, i, x, h, k) = ctx.input_mut(|inp| {
                (
                    inp.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::B)),
                    inp.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::I)),
                    inp.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::X)),
                    inp.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::H)),
                    inp.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::K)),
                )
            });
            if b {
                self.wrap_selection("**", "**", crate::i18n::tr("gras"));
            }
            if i {
                self.wrap_selection("*", "*", crate::i18n::tr("italique"));
            }
            if x {
                self.wrap_selection("~~", "~~", crate::i18n::tr("barré"));
            }
            if h {
                self.wrap_selection("==", "==", crate::i18n::tr("surligné"));
            }
            if k {
                self.wrap_selection("[[", "]]", crate::i18n::tr("Titre de la note"));
            }
        }
    }

    fn follow_link(&mut self, _ctx: &egui::Context, text: &str, byte: usize) {
        let byte = byte.min(text.len());
        let ls = text[..byte].rfind('\n').map(|x| x + 1).unwrap_or(0);
        let le = text[ls..].find('\n').map(|x| ls + x).unwrap_or(text.len());
        let line = &text[ls..le];
        let off = byte - ls;
        // wiki links
        let mut start = 0;
        while let Some(s) = line[start..].find("[[") {
            let s = start + s;
            if let Some(e) = line[s + 2..].find("]]") {
                let e = s + 2 + e + 2;
                if off >= s && off <= e {
                    let title = line[s + 2..e - 2].trim().to_string();
                    if !title.is_empty() {
                        self.actions.push(Action::OpenTitle(title));
                    }
                    return;
                }
                start = e;
            } else {
                break;
            }
        }
        // tags
        for (s, e) in tag_spans(line) {
            if off >= s && off <= e {
                self.actions.push(Action::SetFilter(Filter::Tag(line[s + 1..e].to_lowercase())));
                return;
            }
        }
        // urls
        for scheme in ["https://", "http://"] {
            let mut st = 0;
            while let Some(p) = line[st..].find(scheme) {
                let s = st + p;
                let e = line[s..]
                    .find(|c: char| c.is_whitespace() || c == ')')
                    .map(|x| s + x)
                    .unwrap_or(line.len());
                if off >= s && off <= e {
                    // notes may come from a synced repository: show the address first
                    self.pending_open = Some(OpenRequest::Link(line[s..e].to_string()));
                    return;
                }
                st = e.max(s + 1);
                if st >= line.len() {
                    break;
                }
            }
        }
    }
}
