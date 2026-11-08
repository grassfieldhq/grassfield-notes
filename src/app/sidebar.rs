//! Left sidebar: sections and the tag tree.

use super::*;

impl GrassApp {
    pub(super) fn sidebar_row(
        ui: &mut egui::Ui,
        t: &Theme,
        icon: &str,
        label: &str,
        count: Option<usize>,
        selected: bool,
        indent: f32,
    ) -> egui::Response {
        let w = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 30.0), Sense::click());
        let p = ui.painter().clone();
        if selected {
            p.rect_filled(rect, 7.0, t.sidebar_selected);
        } else if resp.hovered() {
            p.rect_filled(rect, 7.0, t.sidebar_selected.gamma_multiply(0.5));
        }
        let icon_color = if selected { t.accent } else { t.sidebar_muted };
        p.text(
            Pos2::new(rect.left() + 12.0 + indent, rect.center().y),
            Align2::LEFT_CENTER,
            icon,
            FontId::proportional(14.0),
            icon_color,
        );
        let label_font = if selected {
            FontId::new(14.0, FontFamily::Name("bold".into()))
        } else {
            FontId::proportional(14.0)
        };
        let galley = p.layout_no_wrap(label.to_string(), label_font, t.sidebar_text);
        let gpos = Pos2::new(rect.left() + 34.0 + indent, rect.center().y - galley.size().y / 2.0);
        let clipped = p.with_clip_rect(Rect::from_min_max(rect.min, Pos2::new(rect.right() - 34.0, rect.max.y)));
        clipped.galley(gpos, galley, t.sidebar_text);
        if let Some(c) = count {
            if c > 0 {
                p.text(
                    Pos2::new(rect.right() - 12.0, rect.center().y),
                    Align2::RIGHT_CENTER,
                    c.to_string(),
                    FontId::proportional(12.0),
                    t.sidebar_muted,
                );
            }
        }
        resp
    }

    pub(super) fn draw_logo(p: &egui::Painter, center: Pos2, t: &Theme) {
        // A small original mark: a round meadow with three grass blades.
        p.circle_filled(center, 11.0, t.accent);
        let base_y = center.y + 6.0;
        let blade = Stroke::new(2.0, Color32::WHITE);
        p.line_segment([Pos2::new(center.x - 4.0, base_y), Pos2::new(center.x - 6.0, center.y - 2.0)], blade);
        p.line_segment([Pos2::new(center.x, base_y), Pos2::new(center.x, center.y - 6.0)], blade);
        p.line_segment([Pos2::new(center.x + 4.0, base_y), Pos2::new(center.x + 6.5, center.y - 1.0)], blade);
    }

    pub(super) fn sidebar(&mut self, ui: &mut egui::Ui) {
        let t = self.theme();
        let full = ui.max_rect();
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            let (r, _) = ui.allocate_exact_size(Vec2::new(26.0, 26.0), Sense::hover());
            match self.icon_textures.get(self.lib.settings.icon % icons::ICONS.len()) {
                Some(tex) => {
                    ui.painter().image(tex.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                }
                None => Self::draw_logo(ui.painter(), r.center(), &t),
            }
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new("Grassfield Notes")
                    .font(FontId::new(15.5, FontFamily::Name("bold".into())))
                    .color(t.sidebar_text),
            );
        });
        ui.add_space(14.0);

        let entries: [(&str, Filter); 7] = [
            ("📝", Filter::All),
            ("📌", Filter::Pinned),
            ("☑", Filter::Todo),
            ("📅", Filter::Today),
            ("🏷", Filter::Untagged),
            ("📦", Filter::Archive),
            ("🗑", Filter::Trash),
        ];
        egui::ScrollArea::vertical()
            .id_salt("sidebar-scroll")
            .auto_shrink([false, false])
            .max_height((ui.available_height() - 44.0).max(60.0))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                for (icon, f) in entries.iter() {
                    let count = self.lib.count(f);
                    let resp = Self::sidebar_row(ui, &t, icon, &f.label(), Some(count), self.filter == *f, 0.0);
                    if resp.clicked() {
                        self.actions.push(Action::SetFilter(f.clone()));
                    }
                    if *f == Filter::Trash && count > 0 {
                        resp.context_menu(|ui| {
                            if ui.button(crate::i18n::tr("Vider la corbeille")).clicked() {
                                self.confirm_empty_trash = true;
                                ui.close();
                            }
                        });
                    }
                }
                // Markdown files opened on their own
                if !self.files.is_empty() {
                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.label(
                            egui::RichText::new(crate::i18n::tr("FICHIERS OUVERTS"))
                                .font(FontId::new(11.0, FontFamily::Name("bold".into())))
                                .color(t.sidebar_muted),
                        );
                    });
                    ui.add_space(4.0);
                    let names: Vec<(usize, String)> = self.files.iter().enumerate().map(|(k, f)| (k, f.name())).collect();
                    for (k, name) in names {
                        let resp = Self::sidebar_row(ui, &t, "📄", &name, None, false, 0.0);
                        if resp.clicked() {
                            self.active_file = Some(k);
                        }
                    }
                }
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new(crate::i18n::tr("TAGS"))
                            .font(FontId::new(11.0, FontFamily::Name("bold".into())))
                            .color(t.sidebar_muted),
                    );
                });
                ui.add_space(4.0);
                let tags = self.lib.all_tags();
                if tags.is_empty() {
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.label(egui::RichText::new(crate::i18n::tr("Tapez #tag dans une note")).size(12.5).color(t.sidebar_muted));
                    });
                }
                for tag in tags.iter() {
                    let parts: Vec<&str> = tag.split('/').collect();
                    // only show if all parents are expanded
                    let mut visible = true;
                    let mut acc = String::new();
                    for part in &parts[..parts.len() - 1] {
                        if !acc.is_empty() {
                            acc.push('/');
                        }
                        acc.push_str(part);
                        if !self.lib.expanded_tags.contains(&acc) {
                            visible = false;
                        }
                    }
                    if !visible {
                        continue;
                    }
                    let depth = parts.len() - 1;
                    let has_children = tags
                        .iter()
                        .any(|o| o.starts_with(tag.as_str()) && o[tag.len()..].starts_with('/'));
                    let name = parts.last().unwrap();
                    let f = Filter::Tag(tag.clone());
                    let count = self.lib.count(&f);
                    let indent = depth as f32 * 14.0;
                    let resp = Self::sidebar_row(ui, &t, "#", name, Some(count), self.filter == f, indent);
                    if has_children {
                        let expanded = self.lib.expanded_tags.contains(tag);
                        let tri = Rect::from_center_size(
                            Pos2::new(resp.rect.left() + 4.0 + indent, resp.rect.center().y),
                            Vec2::splat(12.0),
                        );
                        let tri_resp = ui.interact(tri, Id::unique(("tri", tag)), Sense::click());
                        let c = tri.center();
                        let pts = if expanded {
                            vec![Pos2::new(c.x - 4.0, c.y - 2.0), Pos2::new(c.x + 4.0, c.y - 2.0), Pos2::new(c.x, c.y + 3.0)]
                        } else {
                            vec![Pos2::new(c.x - 2.0, c.y - 4.0), Pos2::new(c.x - 2.0, c.y + 4.0), Pos2::new(c.x + 3.0, c.y)]
                        };
                        ui.painter().add(egui::Shape::convex_polygon(pts, t.sidebar_muted, Stroke::NONE));
                        if tri_resp.clicked() || resp.double_clicked() {
                            self.actions.push(Action::ToggleTagExpanded(tag.clone()));
                        }
                    }
                    if resp.clicked() {
                        self.actions.push(Action::SetFilter(f.clone()));
                    }
                    resp.context_menu(|ui| {
                        if ui.button(crate::i18n::tr("Renommer le tag…")).clicked() {
                            self.actions.push(Action::StartRenameTag(tag.clone()));
                            ui.close();
                        }
                        if ui.button(crate::i18n::tr("Retirer le tag des notes")).clicked() {
                            self.actions.push(Action::DeleteTag(tag.clone()));
                            ui.close();
                        }
                    });
                }
                ui.add_space(12.0);
            });

        // bottom bar
        let bottom = Rect::from_min_max(Pos2::new(full.left(), full.bottom() - 40.0), full.max);
        ui.scope_builder(egui::UiBuilder::new().max_rect(bottom), |ui| {
            ui.horizontal_centered(|ui| {
                ui.add_space(4.0);
                let b = ui.add(
                    egui::Button::new(egui::RichText::new(crate::i18n::tr("⚙  Préférences")).color(t.sidebar_text)).frame(false),
                );
                if b.clicked() {
                    self.show_settings = !self.show_settings;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(6.0);
                    let icon = if t.dark { "☀" } else { "🌙" };
                    let b = ui
                        .add(egui::Button::new(egui::RichText::new(icon).color(t.sidebar_text)).frame(false))
                        .on_hover_text(crate::i18n::tr("Basculer clair / sombre"));
                    if b.clicked() {
                        let next = theme::all().iter().position(|th| th.dark != t.dark).unwrap_or(0);
                        self.lib.settings.theme = next;
                        self.dirty = true;
                    }
                });
            });
        });
    }
}
