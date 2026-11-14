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
    pub fn init_network_pages(&mut self) {
        if let Some(s) = &self.sharing {
            self.clients.refresh(s, true);
            if let Err(e) = self.clients.reload_interfaces(&s.config()) {
                diagnostics::failure("network", "interface_discovery", &e);
                self.clients
                    .status
                    .set_label(&crate::tr_format!("Could not read network adapters: {e}"));
            }
        }
    }
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
                            "Sync failed · Edit or check the connection again".into()
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
                                crate::tr_format!(
                                    "Sync paused · Changes ready locally in {remaining} s"
                                )
                            } else if remaining > 0 {
                                crate::tr_format!("Sync in {remaining} s · since the last edit")
                            } else {
                                "Preparing sync…".into()
                            }
                        } else if !progress.enabled {
                            "Sync paused · Send/Sync is disabled".into()
                        } else if progress.conflict {
                            "Sync conflict · Select a revision".into()
                        } else if progress.clients == 0 {
                            "Sync · No active clients".into()
                        } else if progress.offers > 0 {
                            crate::tr_format!(
                                "Sync awaiting approval · {} client{}",
                                progress.offers,
                                if progress.offers == 1 { "" } else { "s" }
                            )
                        } else if progress.confirmed == progress.clients {
                            if progress.online == progress.clients {
                                crate::tr_format!(
                                    "Synced · {} client{} confirmed",
                                    progress.clients,
                                    if progress.clients == 1 { "" } else { "s" }
                                )
                            } else {
                                crate::tr_format!(
                                    "Last confirmed · {} client{} currently offline",
                                    progress.clients - progress.online,
                                    if progress.clients - progress.online == 1 {
                                        ""
                                    } else {
                                        "s"
                                    }
                                )
                            }
                        } else if progress.online == 0 {
                            crate::tr_format!(
                                "Sync pending · Waiting for {} client{}",
                                progress.clients,
                                if progress.clients == 1 { "" } else { "s" }
                            )
                        } else {
                            crate::tr_format!(
                                "Sync pending · {} of {} clients confirmed",
                                progress.confirmed,
                                progress.clients
                            )
                        }
                    } else {
                        "Sync unavailable".into()
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
            .ok_or("Send/Sync is unavailable.")?
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
            return Err("Autosave paused: save to a local file first.".into());
        };
        let target = security::target(path).map_err(|e| e.to_string())?;
        if (security::risky_autosave(path) || security::risky_autosave(&target))
            && b.approved_target.as_ref() != Some(&target)
            || b.approved_target
                .as_ref()
                .is_some_and(|approved| approved != &target)
        {
            b.autosave = false;
            return Err("Autosave paused: this code/startup target needs explicit approval in the document's Sync dialog.".into());
        }
        let disk = match crate::limits::read(path, crate::limits::DOCUMENT) {
            Ok(data) => Some(peer::hash(&data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                diagnostics::io_failure("storage", "autosave_read_current", &e, path);
                b.autosave = false;
                return Err(crate::tr_format!("Autosave paused (reading file): {e}"));
            }
        };
        if disk != b.disk_hash {
            b.autosave = false;
            return Err(crate::tr_format!("File conflict: {} was changed outside the editor. Autosave is paused. Both the file and editor version are preserved. Compare them and save manually.",path.display()));
        }
        if d.left.has_images() && path.extension().is_none_or(|e| e != "etdoc") {
            b.autosave = false;
            return Err(
                "Autosave paused: use Save as to store documents with images as .etdoc.".into(),
            );
        }
        let data = if path.extension().is_some_and(|e| e == "etdoc") {
            d.left.document_text()
        } else {
            d.left.plain_text()
        };
        let root = settings_path().parent().unwrap().to_owned();
        if let Err(e) = storage::preserve(&root, path) {
            diagnostics::io_failure("storage", "autosave_history", &e, path);
            b.autosave = false;
            return Err(crate::tr_format!(
                "Autosave paused (saving file history): {e}"
            ));
        }
        if let Err(e) = storage::write_document(path, data.as_bytes()) {
            diagnostics::io_failure("storage", "autosave_write_document", &e, path);
            b.autosave = false;
            return Err(crate::tr_format!("Autosave paused (writing file): {e}"));
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
                        d.report = "Sync stopped for this document".into();
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
                .ok_or("Sharing unavailable".to_string())
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
                        preview.push_str("\n… Preview truncated");
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
                .ok_or("Sharing service is missing")?
                .delivered(&offer.id)?;
            return Ok(());
        }
        let data = self
            .sharing
            .as_ref()
            .ok_or("Sharing service is missing")?
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
                approved_target: a.approved_target.clone(),
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
            return Err("Received document is open. Its acknowledgement remains pending until the session is saved successfully.".into());
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
                | Action::EditClient
                | Action::SelectClient
                | Action::RefreshInterfaces
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
                        self.clients.reload_interfaces(&s.config())?;
                        self.open_clients_tab(wid);
                    } else {
                        return Err("Could not start Send/Sync.".into());
                    }
                }
                Action::ApplySharing => {
                    let s = self.sharing.as_mut().ok_or("Sharing unavailable")?;
                    let mut c = s.config();
                    c.enabled = self.clients.enabled.value();
                    c.name = self.clients.name.value().trim().into();
                    c.automatic_interfaces = self.clients.automatic.value();
                    c.interfaces = self
                        .clients
                        .interface_choices
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| self.clients.interfaces.checked(*i as i32 + 1))
                        .map(|(_, b)| b.clone())
                        .collect();
                    c.port = self
                        .clients
                        .port
                        .value()
                        .parse()
                        .map_err(|_| "Enter a valid port.")?;
                    let result = s.configure(c);
                    self.clients.refresh(s, true);
                    result?;
                }
                Action::SelectClient => {
                    if let Some(s) = &self.sharing {
                        self.clients.select_client(s);
                    }
                }
                Action::RefreshInterfaces => {
                    if let Some(s) = &self.sharing {
                        self.clients.reload_interfaces(&s.config())?;
                    }
                }
                Action::EditClient => {
                    let index = self.clients.clients.value();
                    if index <= 0 {
                        return Err("Select a client in the list.".into());
                    }
                    let port = self
                        .clients
                        .remote_port
                        .value()
                        .parse()
                        .map_err(|_| "Invalid client port")?;
                    self.sharing
                        .as_mut()
                        .ok_or("Sharing service is missing")?
                        .edit(
                            index as usize - 1,
                            self.clients.alias.value(),
                            self.clients.host.value().trim().into(),
                            port,
                        )?;
                }
                Action::AddClient => {
                    self.sharing
                        .as_mut()
                        .ok_or("Sharing service is missing")?
                        .add(
                            self.clients.alias.value(),
                            self.clients.host.value().trim().into(),
                            self.clients
                                .remote_port
                                .value()
                                .parse()
                                .map_err(|_| "Invalid client port")?,
                        )?;
                    self.clients.host.set_value("");
                    self.clients.alias.set_value("");
                }
                Action::RemoveClient => {
                    let i = self.clients.clients.value();
                    if i > 0 {
                        self.sharing
                            .as_mut()
                            .ok_or("Sharing service is missing")?
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
                                return Err(crate::tr_format!("Could not start the tray: {e}"));
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
                    return Err("No accessible system tray. EditorTool stays open as a window. GNOME needs a StatusNotifier/AppIndicator extension.".into());
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
                            self.sharing.as_ref().ok_or("Sharing service is missing")?,
                            wid,
                            d.id,
                            sync,
                            d.sharing.as_ref(),
                        );
                    }
                }
                Action::SubmitTransfer => {
                    let (wi, id, sync) = self.transfer.context.ok_or("No document selected")?;
                    let peers = self.transfer.selected();
                    let w = self
                        .windows
                        .get_mut(wi)
                        .and_then(Option::as_mut)
                        .ok_or("Window was closed")?;
                    let di = w
                        .docs
                        .iter()
                        .position(|d| d.id == id)
                        .ok_or("Tab was closed")?;
                    let mut autosave = sync && self.transfer.autosave.value();
                    if autosave && w.docs[di].path.is_none() && !w.save(di, true) {
                        return Ok(());
                    }
                    let mut approved_target = None;
                    if autosave {
                        if let Some(path) = &w.docs[di].path {
                            let target = security::target(path).map_err(|e| e.to_string())?;
                            let previously_approved = w.docs[di]
                                .sharing
                                .as_ref()
                                .and_then(|b| b.approved_target.as_ref())
                                == Some(&target);
                            if !previously_approved
                                && (security::risky_autosave(path)
                                    || security::risky_autosave(&target))
                            {
                                autosave=chrome::decision(Some(&w.win),"Allow remote writes to executable code?","A trusted or compromised peer can change this code. Another program may execute it later.",&target.to_string_lossy(),&["Keep changes in editor only","Allow autosave for this target"],0)==Some(1);
                            }
                            if autosave {
                                approved_target = Some(target);
                            }
                        }
                    }
                    let d = &mut w.docs[di];
                    let s = self.sharing.as_ref().ok_or("Sharing service is missing")?;
                    let old = d.sharing.as_ref().and_then(|b| s.document(&b.doc));
                    let same = sync
                        && old.is_some_and(|doc| {
                            doc.active
                                && !doc.head.signature.is_empty()
                                && doc
                                    .members
                                    .iter()
                                    .filter(|p| {
                                        p.id != s.identity() && !doc.departed.contains(&p.id)
                                    })
                                    .map(|p| p.id.clone())
                                    .collect::<BTreeSet<_>>()
                                    == peers.iter().map(|p| p.id.clone()).collect()
                        });
                    if same {
                        let b = d.sharing.as_mut().unwrap();
                        b.autosave = autosave;
                        b.approved_target = approved_target.clone();
                        b.disk_hash = d.path.as_ref().and_then(|p| {
                            crate::limits::read(p, crate::limits::DOCUMENT)
                                .ok()
                                .map(|v| peer::hash(&v))
                        });
                    } else {
                        if sync&&d.sharing.is_some()&&chrome::decision(Some(&w.win),"New sync group","Changing participants requires new approvals. Existing copies are preserved.","",&["Cancel","New group"],1)!=Some(1){return Ok(());}
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
                                approved_target,
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
                        "Sync requests sent · 5-second writing pause"
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
                                        .ok_or("Sharing service is missing")?
                                        .leave(&binding.doc)?;
                                }
                                d.sharing = None;
                                d.report = "Sync stopped · Notifying other clients".into();
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
                        return Err("This document is not synchronised.".into());
                    }
                }
                Action::PreviewConflict => self.preview_conflict(),
                Action::ResolveConflict => {
                    let id = self.conflicts.doc.clone().ok_or("No conflict selected")?;
                    let i = self.conflicts.list.value();
                    if i < 1 {
                        return Ok(());
                    }
                    let key = self
                        .conflicts
                        .versions
                        .get(i as usize - 1)
                        .ok_or("Select a revision")?
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
                        return Err("Recent local changes were saved as an additional revision. Select the revision to use now.".into());
                    }
                    self.sharing
                        .as_ref()
                        .ok_or("Sharing service is missing")?
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
                    peer::Event::Disconnected(id) => {
                        let peer = self
                            .sharing
                            .as_ref()
                            .and_then(|s| s.config().clients.into_iter().find(|p| p.id == id));
                        if let Some(peer) = peer {
                            for w in self.windows.iter_mut().flatten() {
                                w.notice.push(format!("Not connected: {}\n{}:{} — last exchange failed. See the log for details.",peer.name,peer.host,peer.port));
                            }
                        }
                    }
                    peer::Event::Changed | peer::Event::Checked => {
                        if let Some(s) = &self.sharing {
                            self.clients.refresh(s, false);
                            if self.transfer.win.shown() {
                                self.transfer.clients.statuses(&self.transfer.choices, s);
                            }
                        }
                    }
                    peer::Event::Trust(p) => {
                        let s = self.sharing.as_ref().ok_or("Sharing service is missing")?;
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
                        let accept=chrome::decision(owner.as_ref(),"Connect device","Compare the fingerprint on the other device. Device approval does not grant document access.",&crate::tr_format!("Device: {}:{}\nFingerprint:\n{}\n\nAlso approve the connection on the other device.",p.host,p.port,p.id),&["Reject","Approve device"],1)==Some(1);
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
                                "Synchronise document"
                            } else {
                                "Receive document"
                            },
                            if o.sync {
                                "After acceptance, changes are exchanged with these devices. Choose whether changes should also be saved to a local file."
                            } else {
                                "After acceptance, the document opens as a new tab."
                            },
                            &format!("Document: {}\n\nParticipants:\n{}", o.title, participants),
                            &["Reject", "Editor only", "With local file"],
                            1,
                        );
                        let accepted = matches!(choice, Some(1 | 2));
                        let mut path = None;
                        if choice == Some(2) {
                            let mut chooser = crate::file_dialog::Chooser::new(
                                dialog::NativeFileChooserType::BrowseSaveFile,
                            );
                            chooser.set_title(crate::i18n::tr("Local file for received document"));
                            chooser.set_option(dialog::NativeFileChooserOptions::SaveAsConfirm);
                            chooser.show();
                            let p = chooser.filename();
                            if !p.as_os_str().is_empty() {
                                path = Some(p);
                            }
                        }
                        let mut approved_target = None;
                        if let Some(selected) = &path {
                            let target = security::target(selected).map_err(|e| e.to_string())?;
                            if (security::risky_autosave(selected)||security::risky_autosave(&target)) && chrome::decision(owner.as_ref(),"Save received executable code?","This file may be executed by another program. Sync can later replace its contents.",&target.to_string_lossy(),&["Keep in editor only","Allow this target"],0)!=Some(1) {path=None;}
                            else {approved_target=Some(target);}
                        }
                        self.sharing
                            .as_ref()
                            .ok_or("Sharing service is missing")?
                            .approve(
                                &o.id,
                                accepted,
                                path.clone(),
                                o.sync && path.is_some(),
                                approved_target,
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
                                    d.report =
                                        crate::tr_format!("{} stopped sync for this file", name);
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
                        chrome::decision(owner.as_ref(),"Sync stopped for client",&crate::tr_format!("{} no longer synchronises this document. Sync for this client and document has also stopped here. Other documents remain connected.",name),&format!("Document: {}\nClient: {}",title,client),&["Understood"],0);
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
                        return Err(crate::tr_format!(
                            "Send/Sync stopped because backup failed: {e}"
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
                "Send and Sync notice",
                &e,
                "",
                &["Close"],
                0,
            );
        }
        None
    }
}
