//! Full-window settings screen with a section list on the left.

use super::*;

impl GrassApp {
    pub(super) fn settings_screen(&mut self, ui: &mut egui::Ui) {
        let t = self.theme();
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.key_pressed(Key::Escape)) && !self.picker.open {
            self.show_settings = false;
        }
        // top bar
        egui::Panel::top("settings-top")
            .frame(egui::Frame::new().fill(t.list_bg).inner_margin(Margin::symmetric(16, 10)))
            .show_separator_line(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let back = egui::Button::new(egui::RichText::new(crate::i18n::tr("‹  Retour aux notes")).color(t.accent_text).size(14.0));
                    if ui.add(back).on_hover_text(crate::i18n::tr("Échap")).clicked() {
                        self.show_settings = false;
                    }
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new(crate::i18n::tr("Préférences"))
                            .font(FontId::new(20.0, FontFamily::Name("bold".into())))
                            .color(t.text),
                    );
                });
            });
        // section list
        let pages = [
            (SettingsPage::Language, "🌐", crate::i18n::tr("Langue")),
            (SettingsPage::Appearance, "🎨", crate::i18n::tr("Apparence")),
            (SettingsPage::Icon, "🌿", crate::i18n::tr("Icône de l'app")),
            (SettingsPage::Editor, "✏", crate::i18n::tr("Éditeur")),
            (SettingsPage::Sync, "🔄", crate::i18n::tr("Synchronisation GitHub")),
            (SettingsPage::Export, "📤", crate::i18n::tr("Export")),
            (SettingsPage::Data, "💾", crate::i18n::tr("Données et documents")),
            (SettingsPage::Shortcuts, "⌨", crate::i18n::tr("Raccourcis")),
        ];
        egui::Panel::left("settings-nav")
            .frame(egui::Frame::new().fill(t.sidebar_bg).inner_margin(Margin::symmetric(10, 14)))
            .exact_size(270.0)
            .resizable(false)
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for (page, icon, label) in pages {
                    let r = Self::sidebar_row(ui, &t, icon, label, None, self.settings_page == page, 0.0);
                    if r.clicked() {
                        self.settings_page = page;
                    }
                }
            });
        let mut changed = false;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(t.editor_bg).inner_margin(Margin::symmetric(32, 20)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.set_max_width(820.0);
                    let title = pages.iter().find(|p| p.0 == self.settings_page).map(|p| p.2).unwrap_or("");
                    ui.label(egui::RichText::new(title).font(FontId::new(22.0, FontFamily::Name("bold".into()))).color(t.text));
                    ui.add_space(12.0);
                    match self.settings_page {
                        SettingsPage::Language => changed |= self.page_language(ui, &t),
                        SettingsPage::Appearance => changed |= self.page_appearance(ui, &t),
                        SettingsPage::Icon => changed |= self.page_icon(ui, &t),
                        SettingsPage::Editor => changed |= self.page_editor(ui, &t),
                        SettingsPage::Sync => changed |= self.page_sync(ui, &t),
                        SettingsPage::Export => self.page_export(ui, &t),
                        SettingsPage::Data => self.page_data(ui, &t),
                        SettingsPage::Shortcuts => Self::page_shortcuts(ui, &t),
                    }
                    ui.add_space(40.0);
                });
            });
        if changed {
            self.mark_dirty(&ctx);
        }
    }

    fn section(ui: &mut egui::Ui, t: &Theme, title: &str, hint: &str) {
        ui.add_space(10.0);
        ui.label(egui::RichText::new(title).font(FontId::new(15.0, FontFamily::Name("bold".into()))).color(t.text));
        if !hint.is_empty() {
            ui.label(egui::RichText::new(hint).size(12.5).color(t.muted));
        }
        ui.add_space(6.0);
    }

    fn page_language(&mut self, ui: &mut egui::Ui, t: &Theme) -> bool {
        let mut changed = false;
        Self::section(ui, t, crate::i18n::tr("Langue de l'interface"), crate::i18n::tr("Les notes elles-mêmes ne sont pas traduites."));
        let current = self.lib.settings.language;
        let auto_label = format!("{}  ·  {}", crate::i18n::tr("Automatique (langue de l'ordinateur)"), crate::i18n::system().native_name());
        let mut choices: Vec<(Option<crate::i18n::Lang>, String)> = vec![(None, auto_label)];
        for l in crate::i18n::ALL {
            choices.push((Some(l), l.native_name().to_string()));
        }
        for (value, label) in choices {
            let sel = current == value;
            let w = ui.available_width().min(420.0);
            let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 38.0), Sense::click());
            if sel {
                ui.painter().rect_filled(r, 8.0, t.list_selected);
                ui.painter().rect_stroke(r, 8.0, Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
            } else if resp.hovered() {
                ui.painter().rect_filled(r, 8.0, t.code_bg);
            }
            let dot = Pos2::new(r.left() + 18.0, r.center().y);
            ui.painter().circle_stroke(dot, 7.0, Stroke::new(1.5, if sel { t.accent } else { t.faint }));
            if sel {
                ui.painter().circle_filled(dot, 4.0, t.accent);
            }
            ui.painter().text(Pos2::new(r.left() + 36.0, r.center().y), Align2::LEFT_CENTER, label, FontId::proportional(14.5), t.text);
            if resp.clicked() && !sel {
                self.lib.settings.language = value;
                crate::i18n::set(value.unwrap_or_else(crate::i18n::system));
                changed = true;
            }
            ui.add_space(4.0);
        }
        changed
    }

    fn page_appearance(&mut self, ui: &mut egui::Ui, t: &Theme) -> bool {
        let mut changed = false;
        for (dark, title) in [(false, crate::i18n::tr("Thèmes clairs")), (true, crate::i18n::tr("Thèmes sombres"))] {
            Self::section(ui, t, title, "");
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(10.0, 10.0);
                for (i, th) in theme::all().iter().enumerate().filter(|(_, th)| th.dark == dark) {
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(150.0, 86.0), Sense::click());
                    let p = ui.painter();
                    let sel = self.lib.settings.theme == i;
                    p.rect_filled(r, 8.0, th.editor_bg);
                    let side = Rect::from_min_size(r.min, Vec2::new(34.0, r.height()));
                    p.rect_filled(side, CornerRadius { nw: 8, sw: 8, ne: 0, se: 0 }, th.sidebar_bg);
                    let list = Rect::from_min_size(Pos2::new(r.left() + 34.0, r.top()), Vec2::new(40.0, r.height()));
                    p.rect_filled(list, 0.0, th.list_bg);
                    for k in 0..3 {
                        let y = r.top() + 14.0 + k as f32 * 10.0;
                        p.hline((r.left() + 84.0)..=(r.right() - 12.0 - k as f32 * 12.0), y, Stroke::new(3.0, th.faint));
                    }
                    p.circle_filled(Pos2::new(r.left() + 17.0, r.top() + 16.0), 5.5, th.accent);
                    p.rect_filled(Rect::from_min_size(Pos2::new(r.left() + 84.0, r.top() + 42.0), Vec2::new(26.0, 7.0)), 3.0, th.tag_bg);
                    // name band across the bottom so long names fit
                    let band = Rect::from_min_max(Pos2::new(r.left() + 34.0, r.bottom() - 24.0), r.max);
                    p.rect_filled(band, CornerRadius { nw: 0, sw: 0, ne: 0, se: 8 }, th.editor_bg);
                    p.text(Pos2::new(r.left() + 42.0, r.bottom() - 12.0), Align2::LEFT_CENTER, th.name, FontId::proportional(12.0), th.text);
                    let stroke = if sel {
                        Stroke::new(2.5, t.accent)
                    } else if resp.hovered() {
                        Stroke::new(1.5, t.faint)
                    } else {
                        Stroke::new(1.0, t.border)
                    };
                    p.rect_stroke(r, 8.0, stroke, egui::StrokeKind::Outside);
                    if resp.on_hover_text(th.name).clicked() {
                        self.lib.settings.theme = i;
                        changed = true;
                    }
                }
            });
        }
        Self::section(ui, t, crate::i18n::tr("Liste des notes"), "");
        changed |= ui.checkbox(&mut self.lib.settings.show_previews, crate::i18n::tr("Afficher un aperçu du texte sous chaque titre")).changed();
        ui.label(egui::RichText::new(crate::i18n::trf("{0} thèmes disponibles.", &[("0", &theme::all().len() as &dyn std::fmt::Display)])).size(12.0).color(t.muted));
        changed
    }

    fn page_icon(&mut self, ui: &mut egui::Ui, t: &Theme) -> bool {
        let mut changed = false;
        Self::section(ui, t, crate::i18n::tr("Choisissez l'icône de Grassfield Notes"), crate::i18n::tr("Elle s'affiche dans la barre des tâches et en haut de la barre latérale."));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(12.0, 12.0);
            for (i, style) in icons::ICONS.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(112.0, 120.0), Sense::click());
                let sel = self.lib.settings.icon == i;
                if sel {
                    ui.painter().rect_filled(r, 10.0, t.list_selected);
                    ui.painter().rect_stroke(r, 10.0, Stroke::new(2.0, t.accent), egui::StrokeKind::Inside);
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, 10.0, t.code_bg);
                }
                if let Some(tex) = self.icon_textures.get(i) {
                    let ir = Rect::from_center_size(Pos2::new(r.center().x, r.top() + 46.0), Vec2::splat(68.0));
                    ui.painter().image(tex.id(), ir, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                }
                ui.painter().text(Pos2::new(r.center().x, r.bottom() - 14.0), Align2::CENTER_CENTER, style.name, FontId::proportional(12.5), t.text);
                if resp.clicked() {
                    self.lib.settings.icon = i;
                    changed = true;
                }
            }
        });
        changed
    }

    fn page_editor(&mut self, ui: &mut egui::Ui, t: &Theme) -> bool {
        let mut changed = false;
        let s = &mut self.lib.settings;
        Self::section(ui, t, crate::i18n::tr("Texte"), "");
        egui::Grid::new("prefs-editor").num_columns(2).spacing([24.0, 14.0]).show(ui, |ui| {
            ui.label(crate::i18n::tr("Police de l'éditeur"));
            ui.horizontal(|ui| {
                changed |= ui.selectable_value(&mut s.editor_font, EditorFont::Sans, "Sans").changed();
                changed |= ui.selectable_value(&mut s.editor_font, EditorFont::Serif, "Serif").changed();
                changed |= ui.selectable_value(&mut s.editor_font, EditorFont::Mono, "Mono").changed();
            });
            ui.end_row();
            ui.label(crate::i18n::tr("Taille du texte"));
            changed |= ui.add(egui::Slider::new(&mut s.font_size, 12.0..=26.0).step_by(1.0).fixed_decimals(0).suffix(" pt")).changed();
            ui.end_row();
            ui.label(crate::i18n::tr("Interligne"));
            changed |= ui.add(egui::Slider::new(&mut s.line_spacing, 0.85..=1.6).fixed_decimals(2)).changed();
            ui.end_row();
            ui.label(crate::i18n::tr("Largeur de ligne"));
            changed |= ui.add(egui::Slider::new(&mut s.line_width, 420.0..=1400.0).step_by(10.0).fixed_decimals(0).suffix(" px")).changed();
            ui.end_row();
            ui.label(crate::i18n::tr("Symboles Markdown"));
            changed |= ui.checkbox(&mut s.dim_markup, crate::i18n::tr("Atténuer les symboles (**, #, ~~…)")).changed();
            ui.end_row();
        });
        if changed {
            self.layout_cache = None;
        }
        changed
    }

    fn page_sync(&mut self, ui: &mut egui::Ui, t: &Theme) -> bool {
        let mut changed = false;
        ui.label(
            egui::RichText::new(
                crate::i18n::tr("Vos notes sont copiées dans un dépôt GitHub sous forme de fichiers Markdown (un fichier par note, documents joints compris). Sur un autre ordinateur, indiquez le même dépôt pour retrouver vos notes. Si une note change des deux côtés, la plus récente l'emporte."),
            )
            .size(13.0)
            .color(t.muted),
        );
        Self::section(ui, t, crate::i18n::tr("Dépôt"), crate::i18n::tr("Créez d'abord un dépôt vide et privé sur github.com."));
        let s = &mut self.lib.settings.sync;
        egui::Grid::new("prefs-sync").num_columns(2).spacing([24.0, 12.0]).show(ui, |ui| {
            ui.label(crate::i18n::tr("Adresse du dépôt"));
            changed |= ui
                .add(egui::TextEdit::singleline(&mut s.repo_url).hint_text(crate::i18n::tr("https://github.com/nom/mes-notes.git")).desired_width(420.0))
                .changed();
            ui.end_row();
            ui.label(crate::i18n::tr("Branche"));
            changed |= ui.add(egui::TextEdit::singleline(&mut s.branch).hint_text("main").desired_width(160.0)).changed();
            ui.end_row();
            ui.label(crate::i18n::tr("Jeton d'accès"));
            changed |= ui
                .add(egui::TextEdit::singleline(&mut s.token).password(true).hint_text(crate::i18n::tr("ghp_… (facultatif avec SSH)")).desired_width(420.0))
                .changed();
            ui.end_row();
            ui.label(crate::i18n::tr("Automatique"));
            changed |= ui.checkbox(&mut s.auto, crate::i18n::tr("Synchroniser au démarrage puis toutes les 10 minutes")).changed();
            ui.end_row();
        });
        ui.label(
            egui::RichText::new(
                crate::i18n::tr("Jeton : sur GitHub, Paramètres › Developer settings › Personal access tokens › Fine-grained, accès « Contents : lecture et écriture » au dépôt. Il est enregistré sur cet ordinateur avec vos notes. Avec une adresse git@github.com:…, votre clé SSH est utilisée et le jeton n'est pas nécessaire."),
            )
            .size(12.0)
            .color(t.muted),
        );
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            let busy = self.sync_job.is_some();
            let label = if busy { crate::i18n::tr("Synchronisation…") } else { crate::i18n::tr("🔄  Synchroniser maintenant") };
            let b = egui::Button::new(egui::RichText::new(label).color(Color32::WHITE).size(14.0)).fill(t.accent).stroke(Stroke::NONE);
            if ui.add_enabled(!busy && self.persist, b).clicked() {
                self.actions.push(Action::SyncNow);
            }
            if busy {
                ui.spinner();
            }
            if ui.button(crate::i18n::tr("📂  Ouvrir la copie locale")).clicked() {
                let d = sync::repo_dir();
                let _ = std::fs::create_dir_all(&d);
                let _ = crate::platform::open_path(&d);
            }
        });
        if let Some((ok, msg)) = &self.sync_status {
            ui.add_space(6.0);
            let col = if *ok { t.accent_text } else { Color32::from_rgb(0xC8, 0x38, 0x2E) };
            ui.label(egui::RichText::new(msg).color(col).size(13.0));
        }
        let last = self.lib.settings.sync.last_sync;
        ui.label(
            egui::RichText::new(if last > 0 { crate::i18n::trf("Dernière synchronisation : {0}", &[("0", &format_full_date(last) as &dyn std::fmt::Display)]) } else { crate::i18n::tr("Jamais synchronisé").into() })
                .size(12.0)
                .color(t.muted),
        );
        changed
    }

    fn page_export(&mut self, ui: &mut egui::Ui, t: &Theme) {
        Self::section(ui, t, crate::i18n::tr("Exporter toutes les notes"), crate::i18n::tr("Une note s'exporte aussi depuis son menu « ••• » ou un clic droit dans la liste."));
        ui.horizontal_wrapped(|ui| {
            for f in export::ALL {
                ui.selectable_value(&mut self.export_all_format, f, f.label());
            }
        });
        ui.add_space(10.0);
        let n = self.lib.notes.iter().filter(|n| !n.trashed).count();
        let b = egui::Button::new(egui::RichText::new(crate::i18n::trf("📤  Exporter {n} note(s) en {0}", &[("0", &self.export_all_format.ext().to_uppercase() as &dyn std::fmt::Display), ("n", &n as &dyn std::fmt::Display)])).color(Color32::WHITE))
            .fill(t.accent)
            .stroke(Stroke::NONE);
        if ui.add(b).clicked() {
            let ids: Vec<u64> = self.lib.notes.iter().filter(|n| !n.trashed).map(|n| n.id).collect();
            let f = self.export_all_format;
            self.export(ui.ctx(), &ids, f);
        }
        ui.label(egui::RichText::new(crate::i18n::trf("Dossier : {0}", &[("0", &Self::export_dir().display() as &dyn std::fmt::Display)])).size(12.0).color(t.muted));
        Self::section(ui, t, crate::i18n::tr("Formats"), "");
        for (f, what) in [
            (Format::Txt, crate::i18n::tr("le texte seul, sans mise en forme")),
            (Format::Markdown, crate::i18n::tr("le texte avec ses symboles Markdown")),
            (Format::TextBundle, crate::i18n::tr("un dossier avec le texte et les documents joints")),
            (Format::Rtf, crate::i18n::tr("ouvrable dans tous les traitements de texte")),
            (Format::Pdf, crate::i18n::tr("page A4 prête à imprimer")),
            (Format::Jpg, crate::i18n::tr("toute la note en une image")),
            (Format::Html, crate::i18n::tr("page web")),
            (Format::Docx, crate::i18n::tr("document Word")),
            (Format::Epub, crate::i18n::tr("livre numérique pour liseuse")),
        ] {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(f.label()).color(t.text).size(13.0));
                ui.label(egui::RichText::new(format!("— {what}")).color(t.muted).size(12.5));
            });
        }
    }

    fn page_data(&mut self, ui: &mut egui::Ui, t: &Theme) {
        Self::section(ui, t, crate::i18n::tr("Vos notes restent sur cet ordinateur"), crate::i18n::tr("Elles fonctionnent sans connexion ; la synchronisation GitHub est facultative."));
        ui.label(egui::RichText::new(Library::data_path().display().to_string()).monospace().size(12.0).color(t.muted));
        ui.horizontal(|ui| {
            if ui.button(crate::i18n::tr("📂  Ouvrir le dossier des notes")).clicked() {
                let d = Library::data_dir();
                let _ = std::fs::create_dir_all(&d);
                let _ = crate::platform::open_path(&d);
            }
            if ui.button(crate::i18n::tr("📥  Importer des notes (.md, .txt)…")).clicked() {
                self.actions.push(Action::PickFiles(Purpose::Import));
            }
        });
        Self::section(ui, t, crate::i18n::tr("Fichiers Markdown"), crate::i18n::tr("Grassfield Notes peut aussi lire et modifier n'importe quel fichier .md, sans l'ajouter à vos notes."));
        ui.horizontal_wrapped(|ui| {
            if ui.button(crate::i18n::tr("📄  Ouvrir un fichier .md… (Ctrl+O)")).clicked() {
                self.actions.push(Action::PickFiles(Purpose::OpenFile));
            }
            if ui.button(crate::i18n::tr("🔗  Ouvrir les fichiers .md avec Grassfield Notes")).on_hover_text(crate::i18n::tr("Double-cliquer sur un fichier .md l'ouvrira ici")).clicked() {
                let msg = desktop::associate_markdown();
                self.notify(ui.ctx(), msg);
            }
        });
        ui.label(
            egui::RichText::new(crate::i18n::tr("Vous pouvez aussi glisser un fichier .md dans la fenêtre, ou lancer : grassfield-notes fichier.md"))
                .size(12.0)
                .color(t.muted),
        );
        let docs: usize = self.lib.notes.iter().map(|n| n.attachments.len()).sum();
        ui.label(egui::RichText::new(crate::i18n::trf("{0} notes, {1} documents joints", &[("0", &self.lib.notes.len() as &dyn std::fmt::Display), ("1", &docs as &dyn std::fmt::Display)])).size(12.5).color(t.muted));
        Self::section(ui, t, crate::i18n::tr("Documents numérisés et recherche"), crate::i18n::tr("Le texte des images et des PDF est lu pour que la recherche les trouve."));
        let tools = attach::tools();
        let status = |ok: bool| if ok { crate::i18n::tr("✔ installé") } else { crate::i18n::tr("✖ absent") };
        egui::Grid::new("tools").num_columns(2).spacing([24.0, 8.0]).show(ui, |ui| {
            ui.label(crate::i18n::tr("Lecture du texte des images (tesseract)"));
            ui.label(egui::RichText::new(status(tools.ocr)).color(if tools.ocr { t.accent_text } else { t.muted }));
            ui.end_row();
            ui.label(crate::i18n::tr("Lecture du texte des PDF (pdftotext)"));
            ui.label(egui::RichText::new(status(tools.pdf)).color(if tools.pdf { t.accent_text } else { t.muted }));
            ui.end_row();
        });
        if !tools.ocr || !tools.pdf {
            ui.label(egui::RichText::new(crate::i18n::tr("Installez « tesseract » et « poppler » avec le gestionnaire de paquets pour activer la recherche dans les documents.")).size(12.0).color(t.muted));
        }
        ui.horizontal(|ui| {
            if ui.button(crate::i18n::tr("📷  Numériser un document…")).clicked() {
                self.actions.push(Action::PickFiles(Purpose::Scan));
            }
            if self.reader.busy() {
                ui.spinner();
                ui.label(egui::RichText::new(crate::i18n::tr("Lecture du texte en cours…")).size(12.5).color(t.muted));
            }
        });
    }

    fn page_shortcuts(ui: &mut egui::Ui, t: &Theme) {
        egui::Grid::new("shortcuts").num_columns(2).spacing([32.0, 10.0]).show(ui, |ui| {
            for (k, v) in [
                ("Ctrl+N", crate::i18n::tr("Nouvelle note")),
                ("Ctrl+F", crate::i18n::tr("Rechercher")),
                ("Ctrl+B / Ctrl+I", crate::i18n::tr("Gras / italique")),
                (crate::i18n::tr("Ctrl+Maj+X / Ctrl+Maj+H"), crate::i18n::tr("Barré / surligné")),
                ("Ctrl+K", crate::i18n::tr("Lien vers une note")),
                (crate::i18n::tr("Ctrl+Entrée"), crate::i18n::tr("Cocher / décocher la tâche")),
                (crate::i18n::tr("Ctrl+clic"), crate::i18n::tr("Ouvrir un lien ou un tag")),
                (crate::i18n::tr("Ctrl+Maj+F"), crate::i18n::tr("Mode concentration")),
                (crate::i18n::tr("Ctrl+Maj+I"), "Infos sur la note"),
                (crate::i18n::tr("Ctrl+Maj+P"), crate::i18n::tr("Épingler")),
                (crate::i18n::tr("Ctrl+Maj+E"), crate::i18n::tr("Exporter en Markdown")),
                ("Ctrl+,", crate::i18n::tr("Préférences")),
                (crate::i18n::tr("Échap"), crate::i18n::tr("Fermer les préférences")),
            ] {
                ui.label(egui::RichText::new(crate::platform::shortcut_label(k)).monospace().size(13.0).color(t.accent_text));
                ui.label(egui::RichText::new(v).size(13.0).color(t.text));
                ui.end_row();
            }
        });
    }
}

/// Make Grassfield Notes the app that opens .md files (Linux desktops).
mod desktop {
    // trf is not needed by every system's version below
    #[allow(unused_imports)]
    use crate::i18n::{tr, trf};

    #[cfg(target_os = "linux")]
    pub fn associate_markdown() -> String {
        let Some(apps) = dirs::data_dir().map(|d| d.join("applications")) else {
            return tr("Dossier des applications introuvable").to_string();
        };
        let exe = match std::env::current_exe() {
            Ok(e) => e,
            Err(e) => return trf("Association impossible : {e}", &[("e", &e)]),
        };
        let icon = crate::model::Library::data_dir().join("grassfield-notes.png");
        let _ = std::fs::create_dir_all(crate::model::Library::data_dir());
        let _ = image::RgbaImage::from_raw(128, 128, crate::icons::rgba(0, 128)).map(|i| i.save(&icon));
        let desktop = format!(
            "[Desktop Entry]\nType=Application\nName=Grassfield Notes\nComment={}\nExec=\"{}\" %F\nIcon={}\nTerminal=false\nCategories=Office;TextEditor;\nMimeType=text/markdown;text/x-markdown;\nStartupWMClass=grassfield-notes\n",
            tr("Notes et fichiers Markdown"),
            exe.display(),
            icon.display()
        );
        if let Err(e) = std::fs::create_dir_all(&apps).and_then(|_| std::fs::write(apps.join("grassfield-notes.desktop"), desktop)) {
            return trf("Association impossible : {e}", &[("e", &e)]);
        }
        let mut ok = true;
        for mime in ["text/markdown", "text/x-markdown"] {
            ok &= std::process::Command::new("xdg-mime")
                .args(["default", "grassfield-notes.desktop", mime])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
        }
        let _ = std::process::Command::new("update-desktop-database").arg(&apps).status();
        if ok {
            tr("Les fichiers .md s'ouvriront désormais avec Grassfield Notes").to_string()
        } else {
            tr("Raccourci créé ; choisissez Grassfield Notes dans « Ouvrir avec » si besoin").to_string()
        }
    }

    /// Windows: register Grassfield Notes for .md files for the current user (no admin rights needed).
    /// Windows then lists it in "Open with"; the user confirms the default there, as Windows requires.
    #[cfg(target_os = "windows")]
    pub fn associate_markdown() -> String {
        let exe = match std::env::current_exe() {
            Ok(e) => e.display().to_string(),
            Err(e) => return trf("Association impossible : {e}", &[("e", &e)]),
        };
        let open_cmd = format!("\"{exe}\" \"%1\"");
        let icon = format!("\"{exe}\",0");
        let prog = r"HKCU\Software\Classes\GrassfieldNotes.Markdown";
        let keys: Vec<Vec<String>> = vec![
            vec![prog.into(), "/ve".into(), "/d".into(), "Markdown".into()],
            vec![format!(r"{prog}\DefaultIcon"), "/ve".into(), "/d".into(), icon],
            vec![format!(r"{prog}\shell\open\command"), "/ve".into(), "/d".into(), open_cmd.clone()],
            vec![r"HKCU\Software\Classes\Applications\grassfield-notes.exe\shell\open\command".into(), "/ve".into(), "/d".into(), open_cmd],
            vec![r"HKCU\Software\Classes\.md\OpenWithProgids".into(), "/v".into(), "GrassfieldNotes.Markdown".into(), "/t".into(), "REG_NONE".into()],
            vec![r"HKCU\Software\Classes\.markdown\OpenWithProgids".into(), "/v".into(), "GrassfieldNotes.Markdown".into(), "/t".into(), "REG_NONE".into()],
        ];
        for k in keys {
            let ok = crate::platform::command("reg")
                .arg("add")
                .args(&k)
                .arg("/f")
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !ok {
                return trf("Association impossible : {e}", &[("e", &k[0])]);
            }
        }
        tr("Raccourci créé ; choisissez Grassfield Notes dans « Ouvrir avec » si besoin").to_string()
    }

    /// macOS opens documents through system events the app can't receive yet,
    /// so files are opened from inside the app instead.
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    pub fn associate_markdown() -> String {
        tr("Sur macOS, glissez le fichier .md dans la fenêtre ou utilisez ⌘O").to_string()
    }
}
