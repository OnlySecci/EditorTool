use crate::*;
/// Persistent management pages use the same tab surface as the other settings.
pub struct Panel {
    pub win: Window,
    pub page: settings_page::Page,
    text: TextBuffer,
}
impl Panel {
    pub fn new(tx: app::Sender<Message>, permissions: bool) -> Self {
        Group::set_current(None::<&Group>);
        let mut win = Window::default().with_size(660, 620);
        win.set_color(PANEL);
        let mut root = Flex::new(20, 20, 620, 580, None).column();
        root.set_spacing(12);
        let title = theme::heading(
            if permissions {
                "Freigaben und Berechtigungen"
            } else {
                "Speicher und Dateistände"
            },
            22,
        );
        root.fixed(&title, 36);
        let mut subtitle = Frame::default().with_label(if permissions {
            "Gerätefreigaben und Dokumentzustimmungen bleiben getrennt.\nZum Beenden eines Syncs den Dokument-Tab auswählen."
        } else {
            "Grössen werden nur beim Öffnen oder Aktualisieren ermittelt.\nAktive Sitzungen und Sync-Daten werden nicht bereinigt."
        });
        subtitle.set_align(Align::Left | Align::Inside | Align::Wrap);
        subtitle.set_label_color(MUTED);
        subtitle.set_label_size(12);
        root.fixed(&subtitle, 52);
        let text = TextBuffer::default();
        let mut view = fltk::text::TextDisplay::default();
        view.set_buffer(text.clone());
        view.set_frame(FrameType::FlatBox);
        view.set_color(BG);
        view.set_text_color(FG);
        view.set_text_size(13);
        view.set_text_font(Font::Helvetica);
        view.wrap_mode(fltk::text::WrapMode::AtBounds, 0);
        view.set_scrollbar_size(14);
        let mut row = Flex::default().row();
        row.set_spacing(8);
        for (label, action) in if permissions {
            vec![
                ("Aktualisieren", Action::Permissions),
                ("Clients verwalten…", Action::SharingSettings),
            ]
        } else {
            vec![
                ("Aktualisieren", Action::Storage),
                ("Bereinigung auswählen…", Action::StorageClean),
            ]
        } {
            let mut button = Button::default().with_label(label);
            theme::button(&mut button, false, None);
            button.emit(tx, (0, action));
        }
        row.end();
        root.fixed(&row, 36);
        root.end();
        win.end();
        win.set_callback(|w| w.hide());
        let page = settings_page::Page::new(&mut win, &root);
        Self { win, page, text }
    }
    fn set(&mut self, text: &str) {
        self.text.set_text(text);
    }
}
impl Runtime {
    pub fn open_management_tab(&mut self, wid: usize, page: u8) {
        if self.windows.iter().all(Option::is_none) {
            self.handle(0, Action::NewWindow);
        }
        let existing = self.windows.iter().enumerate().find_map(|(wi, w)| {
            w.as_ref().and_then(|w| {
                w.docs
                    .iter()
                    .position(|d| d.page == page)
                    .map(|di| (wi, di))
            })
        });
        let wi = if let Some((wi, di)) = existing {
            self.windows[wi]
                .as_mut()
                .unwrap()
                .action(Action::Select(di), wi, self.tx);
            wi
        } else {
            let wi = if self.windows.get(wid).is_some_and(Option::is_some) {
                wid
            } else {
                self.windows.iter().position(Option::is_some).unwrap()
            };
            self.windows[wi]
                .as_mut()
                .unwrap()
                .open_settings_tab(page, wi, self.tx);
            wi
        };
        let details = if page == 4 {
            (|| -> Result<String,String> {
                self.persist(); self.store.flush()?;
                let root=self.config.parent().unwrap(); let mut text=String::new();
                for (dir,label) in [("session","Sitzung inkl. ungespeicherter Tabs"),("history","Ältere Dateistände"),("sharing","Freigaben und Sync-Versionen"),("updates","Update-Pakete"),("logs","Diagnoseprotokolle")] {
                    text.push_str(&format!("{label}\n{:.2} MiB\n\n",storage::size(&root.join(dir)).map_err(|e|e.to_string())? as f64/1048576.0));
                }
                text.push_str("Vor jedem Speichern wird der vorige Dateiinhalt gesichert (bis zu 10 Stände / 256 MiB je Datei).\nDatei → Ältere Dateistände öffnet einen Stand als neuen Tab.\n\nBereinigung löscht endgültig, ohne physisches Überschreiben zu versprechen. Persönliche Dateien werden nicht gelöscht."); Ok(text)
            })().unwrap_or_else(|e|format!("Speicher konnte nicht vollständig ermittelt werden:\n{e}"))
        } else {
            let mut text=String::from("Verbindungsschutz\nTLS 1.3 · AES-256-GCM · X25519 + ML-KEM-768\nGeräteidentitäten: klassische Signaturen mit Fingerabdruckfreigabe.\n\nUpdates\nEd25519-signierte Metadaten mit Ablaufdatum und Versionsschutz.\n\nLokale Daten\nSitzungen und Historie sind unverschlüsselt, durch Benutzerrechte geschützt. Keine automatischen Diagnose-Uploads.\n\n");
            if file_dialog::portal_required() {
                text.push_str("Flatpak\nDateizugriff über einzeln erteilte Portal-Freigaben. Netzwerk, X11, Tray und Host-Aufruf für Updates bleiben freigegeben.\n\n");
            }
            text.push_str(
                &self
                    .sharing
                    .as_ref()
                    .map(peer::Service::permissions)
                    .unwrap_or_else(|| "Senden/Sync nicht verfügbar.".into()),
            );
            text
        };
        if page == 4 {
            self.storage_panel.set(&details);
        } else {
            self.permissions_panel.set(&details);
        }
        let w = self.windows[wi].as_mut().unwrap();
        if !w.embedded {
            w.win.show();
            chrome::dark_title(&w.win);
            shutdown::hook(&w.win, self.weak_self.clone());
        }
    }
    pub fn storage_dialog(&mut self) {
        let result = (|| -> Result<(), String> {
            self.persist();
            self.store.flush()?;
            let root = self.config.parent().unwrap().to_owned();
            let mut details = String::new();
            for (dir, label) in [
                ("session", "Sitzung inkl. ungespeicherter Tabs"),
                ("history", "Ältere Dateistände"),
                ("sharing", "Freigaben, Geräteidentität und Sync-Versionen"),
                ("updates", "Update-Pakete"),
                ("logs", "Diagnoseprotokolle"),
            ] {
                let size = storage::size(&root.join(dir)).map_err(|e| e.to_string())?;
                details.push_str(&format!("{label}: {:.2} MiB\n", size as f64 / 1048576.0));
            }
            let choice = chrome::decision(None, "Speicher und Bereinigung", "Sitzungen, Freigaben und Geräteidentität bleiben erhalten. Bereinigen entfernt nur die gewählte Kategorie; gelöschte Dateistände können nicht wiederhergestellt werden.", &details, &["Schliessen", "Verwaiste Tabs", "Dateistände", "Update-Pakete"], 0);
            let (selected, category) = match choice {
                Some(1) => (
                    self.store
                        .orphan_files(&self.snapshot())
                        .map_err(|e| e.to_string())?,
                    "verwaiste Tab-Sicherungen",
                ),
                Some(2) => (
                    storage::files(&root.join("history")).map_err(|e| e.to_string())?,
                    "ältere Dateistände",
                ),
                Some(3) if !self.update_busy => (
                    storage::files(&root.join("updates")).map_err(|e| e.to_string())?,
                    "heruntergeladene Update-Dateien",
                ),
                Some(3) => return Err(
                    "Während einer Aktualisierung können Update-Dateien nicht bereinigt werden."
                        .into(),
                ),
                _ => return Ok(()),
            };
            if selected.is_empty() {
                dialog::message_default("Keine Dateien in dieser Kategorie vorhanden.");
                return Ok(());
            }
            let bytes: u64 = selected
                .iter()
                .map(|p| std::fs::symlink_metadata(p).map(|m| m.len()).unwrap_or(0))
                .sum();
            if chrome::decision(None, "Dateien endgültig löschen?", &format!("{} Dateien ({:.2} MiB): {category}. Benutzerdateien und aktuelle Sitzungen bleiben erhalten.", selected.len(), bytes as f64 / 1048576.0), "", &["Abbrechen", "Endgültig löschen"], 0) == Some(1) {
                let n = storage::remove_files(&root, &selected).map_err(|e| e.to_string())?;
                dialog::message_default(&format!("{n} Dateien entfernt."));
            }
            Ok(())
        })();
        if let Err(error) = result {
            dialog::alert_default(&error);
        }
    }
    pub fn history_dialog(&mut self, wid: usize) {
        let result = (|| -> Result<(), String> {
            let path = self
                .windows
                .get(wid)
                .and_then(Option::as_ref)
                .and_then(|w| w.docs[w.active].path.clone())
                .ok_or("Bitte eine gespeicherte Datei auswählen.")?;
            let root = self.config.parent().unwrap().to_owned();
            let versions = storage::versions(&root, &path).map_err(|e| e.to_string())?;
            if versions.is_empty() {
                dialog::message_default("Noch keine älteren Dateistände. Beim Speichern wird der vorherige Dateiinhalt lokal gesichert (höchstens 10 Stände / 256 MiB je Datei).");
                return Ok(());
            }
            let labels: Vec<String> = versions.iter().map(|p| storage::describe(p)).collect();
            let Some(index) = chrome::select_version(&labels) else {
                return Ok(());
            };
            let text = limits::read_text(&versions[index]).map_err(|e| e.to_string())?;
            rich::validate_snapshot(&text)?;
            let w = self.windows[wid].as_mut().unwrap();
            w.add_tab(wid, self.tx);
            let d = &mut w.docs[w.active];
            d.left.set_cache_text(&text)?;
            d.display_name = Some(format!(
                "Wiederhergestellt – {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ));
            d.saved_revision = u64::MAX;
            d.report = "Älterer Dateistand · als neue Datei speichern".into();
            Ok(())
        })();
        if let Err(error) = result {
            dialog::alert_default(&error);
        }
    }
}
