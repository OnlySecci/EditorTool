use crate::*;
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};
pub struct Pending {
    pub revision: u64,
    pub timer: app::TimeoutHandle,
    pub deadline: Instant,
}

impl Runtime {
    fn doc_position(&self, key: &str) -> Option<(usize, usize)> {
        self.windows.iter().enumerate().find_map(|(wi, w)| {
            w.as_ref().and_then(|w| {
                w.docs
                    .iter()
                    .position(|d| d.cache_key == key)
                    .map(|di| (wi, di))
            })
        })
    }
    pub fn sharing_tick(&mut self) {
        let active: BTreeSet<_> = self
            .windows
            .iter()
            .flatten()
            .flat_map(|w| {
                w.docs
                    .iter()
                    .filter_map(|d| d.sharing.as_ref().map(|b| b.doc.clone()))
            })
            .collect();
        if let Some(s) = &self.sharing {
            s.set_active(&active);
        }
        let live: BTreeSet<_> = self
            .windows
            .iter()
            .flatten()
            .flat_map(|w| {
                w.docs
                    .iter()
                    .filter(|d| d.sharing.is_some())
                    .map(|d| d.cache_key.clone())
            })
            .collect();
        self.share_timers.retain(|key, pending| {
            if !live.contains(key) {
                app::remove_timeout3(pending.timer);
                false
            } else {
                true
            }
        });
        let now = Instant::now();
        let mut visible_countdown = false;
        for w in self.windows.iter_mut().flatten() {
            for d in &mut w.docs {
                let revision = d.left_meta.borrow().revision;
                if d.sharing.is_some()
                    && revision != d.share_revision
                    && d.sync_error != Some(revision)
                {
                    if !self
                        .share_timers
                        .get(&d.cache_key)
                        .is_some_and(|p| p.revision == revision)
                    {
                        if let Some(p) = self.share_timers.remove(&d.cache_key) {
                            app::remove_timeout3(p.timer);
                        }
                        d.sync_error = None;
                        let key = d.cache_key.clone();
                        let tx = self.tx;
                        let timer = app::add_timeout3(5.0, move |_| {
                            tx.send((0, Action::PublishDoc(key.clone(), revision)))
                        });
                        self.share_timers.insert(
                            d.cache_key.clone(),
                            Pending {
                                revision,
                                timer,
                                deadline: now + Duration::from_secs(5),
                            },
                        );
                    }
                }
                let note = if let Some(binding) = &d.sharing {
                    if let Some(service) = &self.sharing {
                        let progress = service.progress(&binding.doc);
                        if d.sync_error == Some(revision) {
                            "Sync fehlgeschlagen · Bearbeiten oder Verbindung erneut prüfen".into()
                        } else if let Some(p) = self.share_timers.get(&d.cache_key) {
                            if d.group.visible_r() {
                                visible_countdown = true;
                            }
                            let remaining = p
                                .deadline
                                .saturating_duration_since(now)
                                .as_secs_f64()
                                .ceil() as u64;
                            if !progress.enabled {
                                format!("Sync pausiert · Änderungen in {remaining} s lokal bereit")
                            } else if remaining > 0 {
                                format!("Sync in {remaining} s · nach der letzten Eingabe")
                            } else {
                                "Sync wird vorbereitet…".into()
                            }
                        } else if !progress.enabled {
                            "Sync pausiert · Senden/Sync ist ausgeschaltet".into()
                        } else if progress.conflict {
                            "Sync-Konflikt · Bitte eine Version auswählen".into()
                        } else if progress.clients == 0 {
                            "Sync · Keine aktiven Clients".into()
                        } else if progress.offers > 0 {
                            format!(
                                "Sync wartet auf Freigabe · {} Client{}",
                                progress.offers,
                                if progress.offers == 1 { "" } else { "s" }
                            )
                        } else if progress.confirmed == progress.clients {
                            if progress.online == progress.clients {
                                format!(
                                    "Synchronisiert · {} Client{} bestätigt",
                                    progress.clients,
                                    if progress.clients == 1 { "" } else { "s" }
                                )
                            } else {
                                format!(
                                    "Zuletzt bestätigt · {} Client{} derzeit offline",
                                    progress.clients - progress.online,
                                    if progress.clients - progress.online == 1 {
                                        ""
                                    } else {
                                        "s"
                                    }
                                )
                            }
                        } else if progress.online == 0 {
                            format!(
                                "Sync ausstehend · Warte auf {} Client{}",
                                progress.clients,
                                if progress.clients == 1 { "" } else { "s" }
                            )
                        } else {
                            format!(
                                "Sync ausstehend · {} von {} Clients bestätigt",
                                progress.confirmed, progress.clients
                            )
                        }
                    } else {
                        "Sync nicht verfügbar".into()
                    }
                } else {
                    String::new()
                };
                if note != d.sync_text {
                    let show = !note.is_empty();
                    d.sync_text = note;
                    d.sync_status.set_label(&d.sync_text);
                    if show != d.sync_status.visible() {
                        if show {
                            d.sync_status.show();
                        } else {
                            d.sync_status.hide();
                        }
                        d.group.recalc();
                    }
                }
            }
        }
        if visible_countdown && self.countdown_timer.is_none() {
            let tx = self.tx;
            self.countdown_timer = Some(app::add_timeout3(1.0, move |_| {
                tx.send((0, Action::SyncCountdown))
            }));
        } else if !visible_countdown {
            if let Some(timer) = self.countdown_timer.take() {
                app::remove_timeout3(timer);
            }
        }
    }
    fn publish_shared(&mut self, key: &str, expected: u64) -> Result<(), String> {
        let Some((wi, di)) = self.doc_position(key) else {
            return Ok(());
        };
        let d = &self.windows[wi].as_ref().unwrap().docs[di];
        if d.left_meta.borrow().revision != expected {
            return Ok(());
        }
        let Some(binding) = d.sharing.clone() else {
            return Ok(());
        };
        if self
            .share_timers
            .get(key)
            .is_some_and(|p| p.revision == expected)
        {
            if let Some(p) = self.share_timers.remove(key) {
                app::remove_timeout3(p.timer);
            }
        }
        let data = d.left.cache_text();
        let revision = self
            .sharing
            .as_ref()
            .ok_or("Senden/Sync ist nicht verfügbar.")?
            .publish(&binding.doc, &binding.clock, &data)?;
        let d = &mut self.windows[wi].as_mut().unwrap().docs[di];
        if let Some(b) = &mut d.sharing {
            b.clock = revision.clock;
            b.hash = revision.hash;
        }
        d.share_revision = expected;
        d.sync_error = None;
        // A divergent local edit remains in the editor until a version is chosen.
        if self
            .sharing
            .as_ref()
            .and_then(|s| s.document(&binding.doc))
            .is_some_and(|d| d.conflicts.is_empty())
        {
            Self::autosave_shared(d)?;
        }
        Ok(())
    }
    fn autosave_shared(d: &mut Document) -> Result<(), String> {
        let Some(b) = &mut d.sharing else {
            return Ok(());
        };
        if !b.autosave {
            return Ok(());
        }
        let Some(path) = &d.path else {
            b.autosave = false;
            return Err(
                "Automatisches Speichern pausiert: Bitte zuerst eine lokale Datei speichern."
                    .into(),
            );
        };
        let disk = match crate::limits::read(path, crate::limits::DOCUMENT) {
            Ok(data) => Some(peer::hash(&data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                b.autosave = false;
                return Err(format!("Automatisches Speichern pausiert: {e}"));
            }
        };
        if disk != b.disk_hash {
            b.autosave = false;
            return Err(format!("Dateikonflikt: {} wurde ausserhalb des Editors geändert. Automatisches Speichern ist pausiert. Die Datei und die Editor-Version bleiben erhalten. Bitte vergleichen und anschliessend selbst speichern.",path.display()));
        }
        if d.left.has_images() && path.extension().is_none_or(|e| e != "etdoc") {
            b.autosave = false;
            return Err("Automatisches Speichern pausiert: Dokument mit Bildern bitte über Speichern unter als .etdoc sichern.".into());
        }
        let data = if path.extension().is_some_and(|e| e == "etdoc") {
            d.left.document_text()
        } else {
            d.left.plain_text()
        };
        let root = settings_path().parent().unwrap().to_owned();
        if let Err(e) = storage::preserve(&root, path)
            .and_then(|_| session::atomic_write(path, data.as_bytes()))
        {
            b.autosave = false;
            return Err(format!("Automatisches Speichern pausiert: {e}"));
        }
        b.disk_hash = Some(peer::hash(data.as_bytes()));
        d.saved_revision = d.left_meta.borrow().revision;
        Ok(())
    }
    fn apply_shared(&mut self, id: &str) -> Result<(), String> {
        let Some(document) = self.sharing.as_ref().and_then(|s| s.document(id)) else {
            return Ok(());
        };
        let own = self.sharing.as_ref().unwrap().identity();
        if document.departed.contains(&own)
            || !document
                .members
                .iter()
                .any(|p| p.id != own && !document.departed.contains(&p.id))
        {
            for w in self.windows.iter_mut().flatten() {
                for d in &mut w.docs {
                    if d.sharing.as_ref().is_some_and(|b| b.doc == id) {
                        d.sharing = None;
                        d.report = "Sync für dieses Dokument beendet".into();
                    }
                }
            }
            return Ok(());
        }
        if !document.conflicts.is_empty() {
            self.open_conflict(id);
            return Ok(());
        }
        self.conflict_notified.remove(id);
        if self.conflicts.doc.as_deref() == Some(id) && self.conflicts.versions.len() > 1 {
            self.conflicts.win.hide();
        }
        let positions: Vec<_> = self
            .windows
            .iter()
            .enumerate()
            .flat_map(|(wi, w)| {
                w.iter().flat_map(move |w| {
                    w.docs
                        .iter()
                        .enumerate()
                        .filter(move |(_, d)| d.sharing.as_ref().is_some_and(|b| b.doc == id))
                        .map(move |(di, _)| (wi, di))
                })
            })
            .collect();
        let mut data = None;
        for (wi, di) in positions {
            let d = &mut self.windows[wi].as_mut().unwrap().docs[di];
            if d.left_meta.borrow().revision != d.share_revision {
                continue;
            }
            let b = d.sharing.as_ref().unwrap();
            if b.hash == document.head.hash && b.clock == document.head.clock {
                continue;
            }
            if b.hash != document.head.hash {
                if data.is_none() {
                    data = Some(self.sharing.as_ref().unwrap().body(&document.head.hash)?);
                }
                d.clear_styles();
                d.left.apply_sync_text(data.as_ref().unwrap())?;
            }
            let b = d.sharing.as_mut().unwrap();
            b.hash = document.head.hash.clone();
            b.clock = document.head.clock.clone();
            d.share_revision = d.left_meta.borrow().revision;
            d.report = format!("Sync · {}", document.head.name);
            Self::autosave_shared(d)?;
        }
        Ok(())
    }
    fn open_conflict(&mut self, id: &str) {
        if let Some(document) = self.sharing.as_ref().and_then(|s| s.document(id)) {
            self.conflicts.show(document);
            self.preview_conflict();
        }
    }
    fn preview_conflict(&mut self) {
        let i = self.conflicts.list.value();
        if i < 1 {
            return;
        }
        if let Some(r) = self.conflicts.versions.get(i as usize - 1) {
            let result = self
                .sharing
                .as_ref()
                .ok_or("Freigabe nicht verfügbar".to_string())
                .and_then(|s| s.body(&r.hash));
            let text = match result {
                Ok(data) => {
                    let parsed = serde_json::from_str::<serde_json::Value>(&data).ok();
                    let text = parsed
                        .as_ref()
                        .filter(|v| v["format"] == "EditorTool.Document.v1")
                        .and_then(|v| v["text"].as_str())
                        .unwrap_or(&data);
                    let mut preview: String = text.chars().take(32_000).collect();
                    if text.chars().count() > 32_000 {
                        preview.push_str("\n… Vorschau gekürzt");
                    }
                    preview
                }
                Err(e) => e,
            };
            self.conflicts.text.set_text(&text);
        }
    }
    fn receive_shared(&mut self, offer: peer::Offer, a: peer::Acceptance) -> Result<(), String> {
        if self.windows.iter().flatten().any(|w| {
            w.docs
                .iter()
                .any(|d| d.received_offer.as_deref() == Some(&offer.id))
        }) {
            self.sharing
                .as_ref()
                .ok_or("Freigabe fehlt")?
                .delivered(&offer.id)?;
            return Ok(());
        }
        let data = self
            .sharing
            .as_ref()
            .ok_or("Freigabe fehlt")?
            .body(&offer.revision.hash)?;
        let wi = if let Some(wi) = self.windows.iter().position(Option::is_some) {
            wi
        } else {
            self.handle(0, Action::NewWindow);
            self.windows.len() - 1
        };
        let w = self.windows[wi].as_mut().unwrap();
        let d = &w.docs[w.active];
        if d.page != 0
            || d.dirty()
            || d.path.is_some()
            || d.left_buf.length() > 0
            || d.sharing.is_some()
        {
            w.add_tab(wi, self.tx);
        }
        let d = &mut w.docs[w.active];
        d.left.set_cache_text(&data)?;
        d.display_name = Some(offer.title.clone());
        d.received_offer = Some(offer.id.clone());
        d.path = a.path.clone();
        d.left_language = Some(Language::detect(Some(std::path::Path::new(&offer.title))));
        if offer.sync {
            d.sharing = Some(peer::Binding {
                doc: offer.doc.clone(),
                clock: offer.revision.clock,
                hash: offer.revision.hash,
                autosave: a.autosave,
                disk_hash: a.path.as_ref().and_then(|p| {
                    crate::limits::read(p, crate::limits::DOCUMENT)
                        .ok()
                        .map(|v| peer::hash(&v))
                }),
            });
        }
        d.share_revision = d.left_meta.borrow().revision;
        // An accepted save target is a local choice; never use a sender's path.
        if a.path.is_some() {
            let index = w.active;
            if !w.save(index, false) {
                if let Some(b) = &mut w.docs[index].sharing {
                    b.autosave = false;
                }
            }
        }
        if !w.embedded {
            w.win.show();
            chrome::dark_title(&w.win);
            shutdown::hook(&w.win, self.weak_self.clone());
        }
        w.docs[w.active].left.take_focus().ok();
        w.tab_signature.clear();
        self.persist();
        if !self.cache_enabled {
            return Err("Empfangenes Dokument ist geöffnet. Die Empfangsbestätigung bleibt bis zur erfolgreichen Sitzungssicherung ausstehend.".into());
        }
        self.store.flush()?;
        self.sharing.as_ref().unwrap().delivered(&offer.id)?;
        if offer.sync {
            self.apply_shared(&offer.doc)?;
        }
        Ok(())
    }
    fn show_windows(&mut self) {
        if self.windows.iter().all(Option::is_none) {
            self.handle(0, Action::NewWindow);
        }
        for w in self.windows.iter_mut().flatten().filter(|w| !w.embedded) {
            w.win.show();
            chrome::dark_title(&w.win);
            shutdown::hook(&w.win, self.weak_self.clone());
        }
    }
    pub fn sharing_action(&mut self, wid: usize, action: Action) -> Option<Action> {
        if !matches!(
            action,
            Action::SharingSettings
                | Action::ApplySharing
                | Action::KeepTray
                | Action::AddClient
                | Action::RemoveClient
                | Action::ProbeClients
                | Action::Transfer(_)
                | Action::SubmitTransfer
                | Action::StopSync
                | Action::ShowConflicts
                | Action::PreviewConflict
                | Action::ResolveConflict
                | Action::PeerEvent(_)
                | Action::PublishDoc(..)
                | Action::SyncCountdown
                | Action::InitTray
                | Action::TrayUnavailable
                | Action::ShowWindows
                | Action::QuitAll
        ) {
            return Some(action);
        }
        let result: Result<(), String> = (|| {
            match action {
                Action::SharingSettings => {
                    if let Some(s) = &self.sharing {
                        self.clients.refresh(s, true);
                        self.open_clients_tab(wid);
                    } else {
                        return Err("Senden/Sync konnte nicht gestartet werden.".into());
                    }
                }
                Action::ApplySharing => {
                    let s = self.sharing.as_mut().ok_or("Freigabe nicht verfügbar")?;
                    let mut c = s.config();
                    c.enabled = self.clients.enabled.value();
                    c.name = self.clients.name.value().trim().into();
                    c.port = self
                        .clients
                        .port
                        .value()
                        .parse()
                        .map_err(|_| "Bitte einen gültigen Port eingeben.")?;
                    let result = s.configure(c);
                    self.clients.refresh(s, true);
                    result?;
                }
                Action::AddClient => {
                    self.sharing.as_mut().ok_or("Freigabe fehlt")?.add(
                        self.clients.alias.value(),
                        self.clients.host.value().trim().into(),
                        self.clients
                            .remote_port
                            .value()
                            .parse()
                            .map_err(|_| "Ungültiger Client-Port")?,
                    )?;
                    self.clients.host.set_value("");
                    self.clients.alias.set_value("");
                }
                Action::RemoveClient => {
                    let i = self.clients.clients.value();
                    if i > 0 {
                        self.sharing
                            .as_mut()
                            .ok_or("Freigabe fehlt")?
                            .remove(i as usize - 1)?;
                    }
                }
                Action::ProbeClients => {
                    if let Some(s) = &self.sharing {
                        s.retry();
                        for w in self.windows.iter_mut().flatten() {
                            for d in &mut w.docs {
                                d.sync_error = None;
                            }
                        }
                    }
                }
                Action::KeepTray => {
                    let mut p = preferences::get();
                    p.keep_in_tray = self.clients.background.value();
                    preferences::set(p);
                    preferences::save(&self.config).map_err(|e| e.to_string())?;
                    self.tx.send((0, Action::InitTray));
                    if !p.keep_in_tray {
                        self.show_windows();
                        self.tray = None;
                    }
                }
                Action::InitTray => {
                    if preferences::get().keep_in_tray && self.tray.is_none() {
                        match tray::Native::new(self.tx) {
                            Ok(t) => self.tray = Some(t),
                            Err(e) => {
                                self.tx.send((0, Action::TrayUnavailable));
                                return Err(format!("Tray konnte nicht gestartet werden: {e}"));
                            }
                        }
                    }
                    let ids: BTreeSet<_> = self
                        .windows
                        .iter()
                        .flatten()
                        .flat_map(|w| {
                            w.docs
                                .iter()
                                .filter_map(|d| d.sharing.as_ref().map(|b| b.doc.clone()))
                        })
                        .collect();
                    for id in ids {
                        self.apply_shared(&id)?;
                    }
                }
                Action::TrayUnavailable => {
                    let mut p = preferences::get();
                    p.keep_in_tray = false;
                    preferences::set(p);
                    let _ = preferences::save(&self.config);
                    self.show_windows();
                    self.tray = None;
                    self.clients.background.set_value(false);
                    return Err("Kein erreichbarer System-Tray. EditorTool bleibt als Fenster geöffnet. Unter GNOME wird eine StatusNotifier-/AppIndicator-Erweiterung benötigt.".into());
                }
                Action::ShowWindows => self.show_windows(),
                Action::QuitAll => {
                    self.persist();
                    self.store.flush()?;
                    self.quit_requested = true;
                    if let Some(s) = &mut self.sharing {
                        s.stop_listener();
                    }
                    app::quit();
                }
                Action::Transfer(sync) => {
                    if let Some(Some(w)) = self.windows.get(wid) {
                        let d = &w.docs[w.active];
                        if d.page != 0 {
                            return Ok(());
                        }
                        self.transfer.show(
                            self.sharing.as_ref().ok_or("Freigabe fehlt")?,
                            wid,
                            d.id,
                            sync,
                            d.sharing.as_ref(),
                        );
                    }
                }
                Action::SubmitTransfer => {
                    let (wi, id, sync) = self.transfer.context.ok_or("Kein Dokument ausgewählt")?;
                    let peers = self.transfer.selected();
                    let w = self
                        .windows
                        .get_mut(wi)
                        .and_then(Option::as_mut)
                        .ok_or("Fenster wurde geschlossen")?;
                    let di = w
                        .docs
                        .iter()
                        .position(|d| d.id == id)
                        .ok_or("Tab wurde geschlossen")?;
                    let autosave = sync && self.transfer.autosave.value();
                    if autosave && w.docs[di].path.is_none() && !w.save(di, true) {
                        return Ok(());
                    }
                    let d = &mut w.docs[di];
                    let s = self.sharing.as_ref().ok_or("Freigabe fehlt")?;
                    let old = d.sharing.as_ref().and_then(|b| s.document(&b.doc));
                    let same = sync
                        && old.is_some_and(|doc| {
                            doc.members
                                .iter()
                                .filter(|p| p.id != s.identity() && !doc.departed.contains(&p.id))
                                .map(|p| p.id.clone())
                                .collect::<BTreeSet<_>>()
                                == peers.iter().map(|p| p.id.clone()).collect()
                        });
                    if same {
                        let b = d.sharing.as_mut().unwrap();
                        b.autosave = autosave;
                        b.disk_hash = d.path.as_ref().and_then(|p| {
                            crate::limits::read(p, crate::limits::DOCUMENT)
                                .ok()
                                .map(|v| peer::hash(&v))
                        });
                    } else {
                        if sync&&d.sharing.is_some()&&chrome::decision(Some(&w.win),"Neue Sync-Gruppe","Ein geänderter Teilnehmerkreis benötigt neue Freigaben. Die bisherigen Kopien bleiben erhalten.","",&["Abbrechen","Neue Gruppe"],1)!=Some(1){return Ok(());}
                        let (doc, r) = s.begin(
                            d.title().trim_end_matches(" *").into(),
                            &d.left.cache_text(),
                            peers,
                            sync,
                        )?;
                        if sync {
                            if let Some(binding) = &d.sharing {
                                s.leave(&binding.doc)?;
                            }
                            d.sharing = Some(peer::Binding {
                                doc,
                                clock: r.clock,
                                hash: r.hash,
                                autosave,
                                disk_hash: d.path.as_ref().and_then(|p| {
                                    crate::limits::read(p, crate::limits::DOCUMENT)
                                        .ok()
                                        .map(|v| peer::hash(&v))
                                }),
                            });
                            d.share_revision = d.left_meta.borrow().revision;
                        }
                    }
                    d.report = if sync {
                        "Sync-Anfragen gesendet · 5 Sekunden Schreibpause"
                    } else {
                        "Sendeanfrage gesendet"
                    }
                    .into();
                    self.transfer.win.hide();
                    self.persist();
                }
                Action::StopSync => {
                    if let Some((wi, id, _)) = self.transfer.context {
                        if let Some(Some(w)) = self.windows.get_mut(wi) {
                            if let Some(d) = w.docs.iter_mut().find(|d| d.id == id) {
                                if let Some(binding) = &d.sharing {
                                    self.sharing
                                        .as_ref()
                                        .ok_or("Freigabe fehlt")?
                                        .leave(&binding.doc)?;
                                }
                                d.sharing = None;
                                d.report =
                                    "Sync beendet · Andere Clients werden benachrichtigt".into();
                            }
                        }
                    }
                    self.transfer.win.hide();
                }
                Action::ShowConflicts => {
                    if let Some(id) = self
                        .windows
                        .get(wid)
                        .and_then(Option::as_ref)
                        .and_then(|w| w.docs[w.active].sharing.as_ref().map(|b| b.doc.clone()))
                    {
                        self.open_conflict(&id);
                    } else {
                        return Err("Dieses Dokument wird nicht synchronisiert.".into());
                    }
                }
                Action::PreviewConflict => self.preview_conflict(),
                Action::ResolveConflict => {
                    let id = self
                        .conflicts
                        .doc
                        .clone()
                        .ok_or("Kein Konflikt ausgewählt")?;
                    let i = self.conflicts.list.value();
                    if i < 1 {
                        return Ok(());
                    }
                    let key = self
                        .conflicts
                        .versions
                        .get(i as usize - 1)
                        .ok_or("Bitte Version auswählen")?
                        .key();
                    let pending: Vec<_> = self
                        .windows
                        .iter()
                        .flatten()
                        .flat_map(|w| {
                            w.docs
                                .iter()
                                .filter(|d| {
                                    d.sharing.as_ref().is_some_and(|b| b.doc == id)
                                        && d.left_meta.borrow().revision != d.share_revision
                                })
                                .map(|d| (d.cache_key.clone(), d.left_meta.borrow().revision))
                        })
                        .collect();
                    if !pending.is_empty() {
                        for (k, r) in pending {
                            self.publish_shared(&k, r)?;
                        }
                        self.open_conflict(&id);
                        return Err("Die letzten lokalen Änderungen wurden als zusätzliche Version gesichert. Bitte jetzt die gewünschte Version auswählen.".into());
                    }
                    self.sharing
                        .as_ref()
                        .ok_or("Freigabe fehlt")?
                        .resolve(&id, &key)?;
                    self.conflict_notified.remove(&id);
                    self.conflicts.win.hide();
                    self.apply_shared(&id)?;
                }
                Action::SyncCountdown => {
                    self.countdown_timer = None;
                }
                Action::PublishDoc(key, revision) => {
                    if let Err(e) = self.publish_shared(&key, revision) {
                        if let Some((wi, di)) = self.doc_position(&key) {
                            self.windows[wi].as_mut().unwrap().docs[di].sync_error = Some(revision);
                        }
                        return Err(e);
                    }
                }
                Action::PeerEvent(event) => match event {
                    peer::Event::Changed => {
                        if let Some(s) = &self.sharing {
                            self.clients.refresh(s, false);
                        }
                    }
                    peer::Event::Trust(p) => {
                        let s = self.sharing.as_ref().ok_or("Freigabe fehlt")?;
                        if s.config()
                            .clients
                            .iter()
                            .any(|c| c.id == p.id && c.cert == p.cert)
                        {
                            return Ok(());
                        }
                        let owner = self
                            .windows
                            .iter()
                            .flatten()
                            .find(|w| !w.embedded && w.win.shown())
                            .map(|w| w.win.clone());
                        let accept=chrome::decision(owner.as_ref(),"Gerät verbinden","Vergleiche den Fingerabdruck auf dem anderen Gerät. Die Gerätefreigabe erlaubt noch keine Dokumentübertragung.",&format!("Gerät: {}:{}\nFingerabdruck:\n{}\n\nAnschliessend auch auf dem anderen Gerät freigeben.",p.host,p.port,p.id),&["Ablehnen","Gerät freigeben"],1)==Some(1);
                        self.sharing.as_mut().unwrap().trust(p, accept)?;
                    }
                    peer::Event::Offer(o) => {
                        let participants = o
                            .members
                            .iter()
                            .map(|p| format!("{} ({}:{})", p.name, p.host, p.port))
                            .collect::<Vec<_>>()
                            .join("\n");
                        let owner = self
                            .windows
                            .iter()
                            .flatten()
                            .find(|w| !w.embedded && w.win.shown())
                            .map(|w| w.win.clone());
                        let choice = chrome::decision(
                            owner.as_ref(),
                            if o.sync {
                                "Dokument synchronisieren"
                            } else {
                                "Dokument empfangen"
                            },
                            if o.sync {
                                "Nach Annahme werden Änderungen mit diesen Geräten ausgetauscht. Wähle, ob Änderungen auch in eine lokale Datei gespeichert werden sollen."
                            } else {
                                "Das Dokument wird nach Annahme als neuer Tab geöffnet."
                            },
                            &format!("Dokument: {}\n\nTeilnehmer:\n{}", o.title, participants),
                            &["Ablehnen", "Nur im Editor", "Mit lokaler Datei"],
                            1,
                        );
                        let accepted = matches!(choice, Some(1 | 2));
                        let mut path = None;
                        if choice == Some(2) {
                            let mut chooser = crate::file_dialog::Chooser::new(
                                dialog::NativeFileChooserType::BrowseSaveFile,
                            );
                            chooser.set_title("Lokale Datei für empfangenes Dokument");
                            chooser.set_option(dialog::NativeFileChooserOptions::SaveAsConfirm);
                            chooser.show();
                            let p = chooser.filename();
                            if !p.as_os_str().is_empty() {
                                path = Some(p);
                            }
                        }
                        self.sharing.as_ref().ok_or("Freigabe fehlt")?.approve(
                            &o.id,
                            accepted,
                            path.clone(),
                            o.sync && path.is_some(),
                        )?;
                    }
                    peer::Event::Left(id, client, name) => {
                        let title = self
                            .sharing
                            .as_ref()
                            .and_then(|s| s.document(&id))
                            .map(|d| d.title)
                            .unwrap_or_default();
                        for w in self.windows.iter_mut().flatten() {
                            for d in &mut w.docs {
                                if d.sharing.as_ref().is_some_and(|b| b.doc == id) {
                                    d.report = format!("{} hat Sync für diese Datei beendet", name);
                                }
                            }
                        }
                        self.apply_shared(&id)?;
                        let owner = self
                            .windows
                            .iter()
                            .flatten()
                            .find(|w| !w.embedded && w.win.shown())
                            .map(|w| w.win.clone());
                        chrome::decision(owner.as_ref(),"Sync für Client beendet",&format!("{} synchronisiert dieses Dokument nicht mehr. Die Verbindung für diesen Client und dieses Dokument wurde auch hier beendet. Andere Dokumente bleiben verbunden.",name),&format!("Dokument: {}\nClient: {}",title,client),&["Verstanden"],0);
                    }
                    peer::Event::Received(o, a) => self.receive_shared(o, a)?,
                    peer::Event::Updated(id) => self.apply_shared(&id)?,
                    peer::Event::Conflict(id) => {
                        if self.conflict_notified.insert(id.clone()) {
                            self.open_conflict(&id);
                        }
                    }
                    peer::Event::Notice(e) => return Err(e),
                    peer::Event::Fatal(e) => {
                        if let Some(s) = &mut self.sharing {
                            s.disable_after_error();
                        }
                        return Err(format!(
                            "Senden/Sync gestoppt, weil die Sicherung fehlgeschlagen ist: {e}"
                        ));
                    }
                },
                _ => return Ok(()),
            }
            Ok(())
        })();
        if let Err(e) = result {
            let owner = self
                .windows
                .iter()
                .flatten()
                .find(|w| !w.embedded && w.win.shown())
                .map(|w| w.win.clone());
            chrome::decision(
                owner.as_ref(),
                "Hinweis zu Senden und Sync",
                &e,
                "",
                &["Schliessen"],
                0,
            );
        }
        None
    }
}
