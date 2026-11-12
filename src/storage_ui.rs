use crate::*;
impl Runtime {
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
    pub fn permissions_dialog(&mut self, wid: usize) {
        let details = self
            .sharing
            .as_ref()
            .map(peer::Service::permissions)
            .unwrap_or_else(|| "Senden und Sync nicht verfügbar.".into());
        let choice = chrome::decision(None, "Freigaben und Berechtigungen", "Gerätefreigabe erlaubt Verbindungsaufbau. Dokumente werden erst nach gesonderter Zustimmung übertragen. Sync-Teilnehmer dürfen Änderungen abgleichen; beendeter Sync löscht keine Kopien auf anderen Geräten. Lokale Versionen liegen unverschlüsselt im Benutzerprofil.", &details, &["Schliessen", "Clients verwalten", "Sync dieses Tabs beenden"], 0);
        match choice {
            Some(1) => self.open_clients_tab(wid),
            Some(2) => self.handle(wid, Action::StopSync),
            _ => (),
        }
    }
}
