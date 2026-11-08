//! Middle column: title, search and the list of notes.

use super::*;

impl GrassApp {
    pub(super) fn note_list(&mut self, ui: &mut egui::Ui) {
        let t = self.theme();
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            let title = egui::RichText::new(self.filter.label())
                .font(FontId::new(18.0, FontFamily::Name("bold".into())))
                .color(t.text);
            if ui.ctx().content_rect().width() < 900.0 && !self.focus_mode {
                // the sidebar is hidden on narrow windows: the title opens the sections and tags
                let title = egui::RichText::new(format!("{} ›", self.filter.label()))
                    .font(FontId::new(18.0, FontFamily::Name("bold".into())))
                    .color(t.text);
                ui.menu_button(title, |ui| {
                    ui.set_min_width(200.0);
                    for f in [
                        Filter::All,
                        Filter::Pinned,
                        Filter::Todo,
                        Filter::Today,
                        Filter::Untagged,
                        Filter::Archive,
                        Filter::Trash,
                    ] {
                        let label = format!("{}  ({})", f.label(), self.lib.count(&f));
                        if ui.selectable_label(self.filter == f, label).clicked() {
                            self.actions.push(Action::SetFilter(f));
                            ui.close();
                        }
                    }
                    ui.separator();
                    for tag in self.lib.all_tags() {
                        let f = Filter::Tag(tag.clone());
                        if ui.selectable_label(self.filter == f, format!("#{tag}")).clicked() {
                            self.actions.push(Action::SetFilter(f));
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui.button(crate::i18n::tr("⚙  Préférences")).clicked() {
                        self.show_settings = true;
                        ui.close();
                    }
                });
            } else {
                ui.label(title);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(10.0);
                if self.filter == Filter::Trash {
                    if ui
                        .add_enabled(self.lib.count(&Filter::Trash) > 0, egui::Button::new(crate::i18n::tr("Vider")))
                        .on_hover_text(crate::i18n::tr("Effacer pour de bon toutes les notes de la corbeille"))
                        .clicked()
                    {
                        self.confirm_empty_trash = true;
                    }
                } else {
                    let b = ui
                        .add(egui::Button::new(egui::RichText::new("✏").size(16.0).color(t.accent_text)).frame(false))
                        .on_hover_text(crate::i18n::tr("Nouvelle note (Ctrl+N)"));
                    if b.clicked() {
                        self.actions.push(Action::NewNote);
                    }
                    let scan = ui
                        .add(egui::Button::new(egui::RichText::new("📷").size(15.0).color(t.accent_text)).frame(false))
                        .on_hover_text(crate::i18n::tr("Numériser un document (photo, scan ou PDF) dans une nouvelle note"));
                    if scan.clicked() {
                        self.actions.push(Action::PickFiles(Purpose::Scan));
                    }
                }
                ui.menu_button(egui::RichText::new("↕").size(15.0).color(t.muted), |ui| {
                    ui.label(egui::RichText::new(crate::i18n::tr("Trier par")).small().color(t.muted));
                    let before = self.lib.settings.sort;
                    let s = &mut self.lib.settings.sort;
                    ui.radio_value(s, SortMode::Modified, crate::i18n::tr("Date de modification"));
                    ui.radio_value(s, SortMode::Created, crate::i18n::tr("Date de création"));
                    ui.radio_value(s, SortMode::Title, crate::i18n::tr("Titre"));
                    if *s != before {
                        self.dirty = true;
                    }
                    ui.separator();
                    if ui.checkbox(&mut self.lib.settings.show_previews, crate::i18n::tr("Afficher les aperçus")).changed() {
                        self.dirty = true;
                    }
                    ui.separator();
                    if ui.button(crate::i18n::tr("📄  Ouvrir un fichier .md… (Ctrl+O)")).clicked() {
                        self.actions.push(Action::PickFiles(Purpose::OpenFile));
                        ui.close();
                    }
                    if ui.button(crate::i18n::tr("📥  Importer des notes (.md, .txt)…")).clicked() {
                        self.actions.push(Action::PickFiles(Purpose::Import));
                        ui.close();
                    }
                    ui.menu_button(crate::i18n::tr("📤  Exporter cette liste"), |ui| {
                        for f in export::ALL {
                            if ui.button(f.label()).clicked() {
                                let ids = self.lib.visible(&self.filter, &self.query);
                                self.export(ui.ctx(), &ids, f);
                                ui.close();
                            }
                        }
                    });
                });
            });
        });
        ui.add_space(6.0);
        // search field
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            let w = ui.available_width() - 12.0;
            egui::Frame::new()
                .fill(t.code_bg)
                .corner_radius(8.0)
                .inner_margin(Margin::symmetric(8, 5))
                .show(ui, |ui| {
                    ui.set_width(w - 16.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("🔍").size(12.0).color(t.muted));
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.query)
                                .hint_text(crate::i18n::tr("Rechercher (mots, #tag)"))
                                .frame(egui::Frame::NONE)
                                .desired_width(ui.available_width() - 22.0),
                        );
                        if self.focus_search {
                            resp.request_focus();
                            self.focus_search = false;
                        }
                        if resp.changed() {
                            let vis = self.lib.visible(&self.filter, &self.query);
                            if !self.selected.map(|s| vis.contains(&s)).unwrap_or(false) {
                                self.selected = vis.first().copied();
                            }
                        }
                        // Escape empties the search
                        if (resp.has_focus() || resp.lost_focus()) && ui.input(|i| i.key_pressed(Key::Escape)) {
                            self.query.clear();
                        }
                        if !self.query.is_empty()
                            && ui
                                .add(egui::Button::new(egui::RichText::new("✖").size(11.0).color(t.muted)).frame(false))
                                .clicked()
                        {
                            self.query.clear();
                        }
                    });
                });
        });
        ui.add_space(8.0);
        let line_rect = ui.available_rect_before_wrap();
        ui.painter().hline(line_rect.x_range(), line_rect.top(), Stroke::new(1.0, t.border));

        let visible = self.lib.visible(&self.filter, &self.query);
        if visible.is_empty() {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                let msg = if !self.query.is_empty() {
                    crate::i18n::tr("Aucun résultat")
                } else if self.filter == Filter::Trash {
                    crate::i18n::tr("La corbeille est vide")
                } else {
                    crate::i18n::tr("Aucune note")
                };
                ui.label(egui::RichText::new(msg).size(15.0).color(t.muted));
                if self.query.is_empty() && self.filter != Filter::Trash {
                    ui.add_space(6.0);
                    if ui.button(crate::i18n::tr("Créer une note")).clicked() {
                        self.actions.push(Action::NewNote);
                    }
                }
            });
            return;
        }

        let previews = self.lib.settings.show_previews;
        let row_h = if previews { 96.0 } else { 46.0 };
        let sort = self.lib.settings.sort;
        egui::ScrollArea::vertical()
            .id_salt("list-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.add_space(4.0);
                for id in visible {
                    let Some(n) = self.lib.get(id) else { continue };
                    let w = ui.available_width();
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, row_h), Sense::click());
                    if resp.clicked() {
                        self.actions.push(Action::Select(id));
                    }
                    let (trashed, pinned, archived) = (n.trashed, n.pinned, n.archived);
                    if ui.is_rect_visible(rect) {
                        let selected = self.selected == Some(id);
                        let p = ui.painter();
                        let inner = rect.shrink2(Vec2::new(6.0, 2.0));
                        if selected {
                            p.rect_filled(inner, 8.0, t.list_selected);
                            p.rect_filled(
                                Rect::from_min_size(
                                    Pos2::new(inner.left(), inner.top() + 12.0),
                                    Vec2::new(3.0, inner.height() - 24.0),
                                ),
                                2.0,
                                t.accent,
                            );
                        } else if resp.hovered() {
                            p.rect_filled(inner, 8.0, t.list_selected.gamma_multiply(0.45));
                        } else {
                            p.hline(
                                (rect.left() + 16.0)..=(rect.right() - 12.0),
                                rect.bottom() - 0.5,
                                Stroke::new(1.0, t.border),
                            );
                        }
                        let left = rect.left() + 18.0;
                        let right = rect.right() - 16.0;
                        let mut y = rect.top() + 12.0;
                        // date
                        let date = format_date(match sort {
                            SortMode::Created => n.created,
                            _ => n.modified,
                        });
                        let date_g = p.layout_no_wrap(date, FontId::proportional(11.5), t.muted);
                        let date_w = date_g.size().x;
                        p.galley(Pos2::new(right - date_w, y + 3.0), date_g, t.muted);
                        // title
                        let mut title = n.title();
                        if title.is_empty() {
                            title = crate::i18n::tr("Nouvelle note").into();
                        }
                        let mut tx = left;
                        if pinned {
                            p.text(Pos2::new(tx, y + 2.0), Align2::LEFT_TOP, "📌", FontId::proportional(12.0), t.accent);
                            tx += 18.0;
                        }
                        let mut job = egui::text::LayoutJob::single_section(
                            title,
                            egui::TextFormat::simple(FontId::new(14.5, FontFamily::Name("bold".into())), t.text),
                        );
                        job.wrap.max_width = (right - date_w - 10.0 - tx).max(20.0);
                        job.wrap.max_rows = 1;
                        job.wrap.break_anywhere = true;
                        let g = p.layout_job(job);
                        let title_h = g.size().y;
                        p.galley(Pos2::new(tx, y), g, t.text);
                        y += title_h + 4.0;
                        if previews {
                            let preview = n.preview();
                            let preview = if preview.is_empty() {
                                crate::i18n::tr("Pas de texte supplémentaire").to_string()
                            } else {
                                preview
                            };
                            let mut job = egui::text::LayoutJob::single_section(
                                preview,
                                egui::TextFormat::simple(FontId::proportional(12.5), t.muted),
                            );
                            job.wrap.max_width = right - left;
                            job.wrap.max_rows = 2;
                            let g = p.layout_job(job);
                            p.galley(Pos2::new(left, y), g, t.muted);
                            // footer: todo progress and tags
                            let fy = rect.bottom() - 22.0;
                            let mut fx = left;
                            let (done, total) = n.todo_counts();
                            if total > 0 {
                                let col = if done == total { t.accent_text } else { t.muted };
                                let g = p.layout_no_wrap(format!("☑ {done}/{total}"), FontId::proportional(11.0), col);
                                let gw = g.size().x;
                                p.galley(Pos2::new(fx, fy), g, col);
                                fx += gw + 12.0;
                            }
                            for tag in self.lib.tags_of(n).iter().take(3) {
                                let g = p.layout_no_wrap(format!("#{tag}"), FontId::proportional(11.0), t.accent_text);
                                let gw = g.size().x;
                                if fx + gw + 4.0 > right {
                                    break;
                                }
                                p.rect_filled(
                                    Rect::from_min_size(Pos2::new(fx - 4.0, fy - 1.5), Vec2::new(gw + 8.0, g.size().y + 3.0)),
                                    5.0,
                                    t.tag_bg,
                                );
                                p.galley(Pos2::new(fx, fy), g, t.accent_text);
                                fx += gw + 14.0;
                            }
                        }
                    }
                    resp.context_menu(|ui| {
                        Self::note_menu(ui, &mut self.actions, id, trashed, pinned, archived);
                    });
                }
                ui.add_space(20.0);
            });
    }

    pub(super) fn note_menu(
        ui: &mut egui::Ui,
        actions: &mut Vec<Action>,
        id: u64,
        trashed: bool,
        pinned: bool,
        archived: bool,
    ) {
        ui.set_min_width(210.0);
        if trashed {
            if ui.button(crate::i18n::tr("↩  Restaurer")).clicked() {
                actions.push(Action::Restore(id));
                ui.close();
            }
            if ui.button(crate::i18n::tr("✖  Effacer pour de bon")).clicked() {
                actions.push(Action::DeleteForever(id));
                ui.close();
            }
            return;
        }
        if ui.button(if pinned { crate::i18n::tr("📌  Désépingler") } else { crate::i18n::tr("📌  Épingler") }).clicked() {
            actions.push(Action::TogglePin(id));
            ui.close();
        }
        if ui.button(crate::i18n::tr("⧉  Dupliquer")).clicked() {
            actions.push(Action::Duplicate(id));
            ui.close();
        }
        if ui.button(crate::i18n::tr("📋  Copier le texte")).clicked() {
            actions.push(Action::CopyText(id));
            ui.close();
        }
        if ui.button(crate::i18n::tr("📎  Joindre un document…")).clicked() {
            actions.push(Action::PickFiles(Purpose::Attach));
            ui.close();
        }
        ui.menu_button(crate::i18n::tr("📤  Exporter"), |ui| {
            for f in export::ALL {
                if ui.button(f.label()).clicked() {
                    actions.push(Action::Export(id, f));
                    ui.close();
                }
            }
        });
        ui.separator();
        if ui.button(if archived { crate::i18n::tr("📦  Désarchiver") } else { crate::i18n::tr("📦  Archiver") }).clicked() {
            actions.push(Action::ToggleArchive(id));
            ui.close();
        }
        if ui.button(crate::i18n::tr("🗑  Mettre à la corbeille")).clicked() {
            actions.push(Action::Trash(id));
            ui.close();
        }
    }
}
