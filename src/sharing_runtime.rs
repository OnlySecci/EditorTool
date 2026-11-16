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
        if let Err(error) = self.restore_pending_drafts() {
            diagnostics::failure("sharing", "pending_draft_restore", &error);
        }
        let replacements = self
            .sharing
            .as_ref()
            .map(|s| s.own_replacements())
            .unwrap_or_default();
        for offer in replacements {
            if let Err(error) = self.repaired_offer(&offer, "") {
                diagnostics::failure("sharing", "interrupted_repair_relink", &error);
            }
        }
        if let Some(s) = &self.sharing {
            self.clients.refresh(s, true);
            if let Err(e) = self.clients.reload_interfaces(&s.config()) {
                diagnostics::failure("network", "interface_discovery", &e);
                self.clients
                    .status
                    .set_label(&crate::tr_format!("Could not read network adapters: {e}"));
            }
        }
        for proposal in self
            .sharing
            .as_ref()
            .map(|s| s.pending_roots())
            .unwrap_or_default()
        {
            self.tx
                .send((0, Action::PeerEvent(peer::Event::RestoreRoot(proposal))));
        }
        let pending: BTreeSet<_> = self
            .windows
            .iter()
            .flatten()
            .flat_map(|w| &w.docs)
            .filter_map(|d| d.sharing.as_ref().map(|b| b.doc.clone()))
            .collect();
        for id in pending {
            self.tx
                .send((0, Action::PeerEvent(peer::Event::Updated(id))));
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
                        if let Some(issue) = &progress.issue {
                            sharing_repair::status(issue).into()
                        } else if d.sync_error == Some(revision) {
                            "Sync paused · Open document history or Repair sync".into()
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
                        } else if progress.same_contents {
                            "Contents match · History joins automatically on the next edit or when the merge signer is online".into()
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
                if let Some(binding) = &d.sharing {
                    if self
                        .sharing
                        .as_ref()
                        .is_some_and(|s| s.progress(&binding.doc).enabled)
                    {
                        d.sync_now.activate();
                    } else {
                        d.sync_now.deactivate();
                    }
                }
                if note != d.sync_text {
                    let show = !note.is_empty();
                    d.sync_text = note;
                    d.sync_status.set_label(&d.sync_text);
                    if show != d.sync_row.visible() {
                        if show {
                            d.sync_row.show();
                        } else {
                            d.sync_row.hide();
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
    pub(super) fn publish_shared(&mut self, key: &str, expected: u64) -> Result<(), String> {
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
        let published = self
            .sharing
            .as_ref()
            .ok_or("Send/Sync is unavailable")?
            .tree_publish(&binding, &data, key)?;
        let d = &mut self.windows[wi].as_mut().unwrap().docs[di];
        let Some(revision) = published else {
            d.sync_error = Some(expected);
            d.report="Sync link is being restored; local text is preserved. Use Repair sync for recovery options.".into();
            self.persist();
            return Ok(());
        };
        if let Some(b) = &mut d.sharing {
            b.point_to(&revision);
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
    pub(super) fn apply_shared(&mut self, id: &str) -> Result<(), String> {
        let Some(document) = self.sharing.as_ref().and_then(|s| s.document(id)) else {
            self.sync_status(id, "new_history_required");
            return Ok(());
        };
        if self.sharing.as_ref().unwrap().needs_new_history(id) {
            self.sync_status(id, "new_history_required");
            return Ok(());
        }
        let own = self.sharing.as_ref().unwrap().identity();
        if document.departed.contains(&own) {
            for w in self.windows.iter_mut().flatten() {
                for d in &mut w.docs {
                    if d.sharing.as_ref().is_some_and(|b| b.doc == id) {
                        d.sharing = None;
                        d.report =
                            "This device left the document; local contents were retained".into();
                    }
                }
            }
            return Ok(());
        }
        let conflict = !document.conflicts.is_empty()
            && document
                .conflicts
                .iter()
                .any(|r| r.hash != document.head.hash);
        if conflict {
            if self.conflict_notified.insert(id.into()) {
                self.open_conflict(id);
            }
            return Ok(());
        }
        self.conflict_notified.remove(id);
        let positions = self
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
            .collect::<Vec<_>>();
        for (wi, di) in positions {
            let binding = self.windows[wi].as_ref().unwrap().docs[di]
                .sharing
                .clone()
                .unwrap();
            let Some(base) = self.sharing.as_ref().unwrap().base_revision(&binding) else {
                self.sharing.as_ref().unwrap().queue_base(&binding)?;
                let d = &mut self.windows[wi].as_mut().unwrap().docs[di];
                d.sync_error = Some(d.left_meta.borrow().revision);
                d.report = sharing_repair::status("waiting_base").into();
                continue;
            };
            let d = &mut self.windows[wi].as_mut().unwrap().docs[di];
            d.sharing.as_mut().unwrap().point_to(&base);
            // Unpublished edits are a potential independent branch. Keep them
            // in place until the writing pause publishes their actual parent.
            if d.left_meta.borrow().revision != d.share_revision {
                continue;
            }
            if !self
                .sharing
                .as_ref()
                .unwrap()
                .advances(&document.head, &base)
            {
                let data = d.left.cache_text();
                self.sharing
                    .as_ref()
                    .unwrap()
                    .register_base(&binding, &data)?;
                continue;
            }
            if base.key() == document.head.key() {
                if !self.sharing.as_ref().unwrap().has_body(&document.head) {
                    self.sharing
                        .as_ref()
                        .unwrap()
                        .require_body(&document.head)?;
                    if peer::hash(d.left.cache_text().as_bytes()) == document.head.hash {
                        self.tx.send((
                            0,
                            Action::PublishDoc(d.cache_key.clone(), d.left_meta.borrow().revision),
                        ));
                    }
                }
                continue;
            }
            let body = match self.sharing.as_ref().unwrap().revision_body(&document.head) {
                Ok(body) => body,
                Err(_) => {
                    self.sharing
                        .as_ref()
                        .unwrap()
                        .require_body(&document.head)?;
                    self.sync_status(id, "waiting_history");
                    continue;
                }
            };
            let d = &mut self.windows[wi].as_mut().unwrap().docs[di];
            if binding.hash != document.head.hash {
                d.clear_styles();
                d.left.apply_sync_text(&body)?;
            }
            d.sharing.as_mut().unwrap().point_to(&document.head);
            d.share_revision = d.left_meta.borrow().revision;
            d.sync_error = None;
            d.report = format!(
                "Sync · commit {} · {}",
                &document.head.key()[..12],
                document.head.name
            );
            if let Err(error) = Self::autosave_shared(d) {
                d.report = error;
                diagnostics::event("sharing", "autosave_paused_local_file_preserved");
            }
        }
        self.persist();
        Ok(())
    }
    pub(super) fn open_conflict(&mut self, id: &str) {
        if let Some(document) = self.sharing.as_ref().and_then(|s| s.document(id)) {
            let versions = self.sharing.as_ref().unwrap().versions(id);
            let preferred = self
                .windows
                .iter()
                .flatten()
                .flat_map(|w| &w.docs)
                .any(|d| {
                    self.conflicts.editor_key.as_ref() == Some(&d.cache_key)
                        && d.sharing.as_ref().is_some_and(|b| b.doc == id)
                });
            if !preferred {
                self.conflicts.editor_key = self
                    .windows
                    .iter()
                    .flatten()
                    .flat_map(|w| &w.docs)
                    .find(|d| d.sharing.as_ref().is_some_and(|b| b.doc == id))
                    .map(|d| d.cache_key.clone());
            }
            self.conflicts.show(document, versions);
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
                .and_then(|s| {
                    s.revision_body(r).inspect_err(|_| {
                        let _ = s.require_body(r);
                    })
                });
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
            .revision_body(&offer.revision)?;
        if offer.sync && self.repaired_offer(&offer, &data)? {
            return Ok(());
        }
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
                revision: Some(offer.revision.key()),
                force: offer.revision.force.clone(),
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
                | Action::ResolveMain
                | Action::ResolveEditedMain
                | Action::RepairSync
                | Action::RepairShownSync
                | Action::RecoverLocalBranch
                | Action::OpenSyncRevision
                | Action::CompareSyncRevision
                | Action::RefreshVersions
                | Action::PeerEvent(_)
                | Action::PublishDoc(..)
                | Action::SyncCountdown
                | Action::InitTray
                | Action::TrayUnavailable
                | Action::ShowWindows
                | Action::SyncNow
                | Action::QuitAll
        ) {
            return Some(action);
        }
        let background = matches!(
            &action,
            Action::PublishDoc(..)
                | Action::SyncCountdown
                | Action::PeerEvent(
                    peer::Event::Updated(_)
                        | peer::Event::Conflict(_)
                        | peer::Event::DocumentStatus(..)
                        | peer::Event::BaseReady(_)
                        | peer::Event::Received(..)
                )
        );
        let background_doc = match &action {
            Action::PeerEvent(
                peer::Event::Updated(id)
                | peer::Event::Conflict(id)
                | peer::Event::DocumentStatus(id, _)
                | peer::Event::BaseReady(id),
            ) => Some(id.clone()),
            Action::PeerEvent(peer::Event::Received(offer, _)) => Some(offer.doc.clone()),
            _ => None,
        };
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
                Action::SyncNow => {
                    let d = self
                        .windows
                        .get(wid)
                        .and_then(Option::as_ref)
                        .and_then(|w| w.docs.get(w.active))
                        .ok_or("No document selected")?;
                    let binding = d.sharing.as_ref().ok_or("This document is not shared")?;
                    let doc = binding.doc.clone();
                    let key = d.cache_key.clone();
                    let revision = d.left_meta.borrow().revision;
                    self.publish_shared(&key, revision)?;
                    if let Some(service) = &self.sharing {
                        service.retry();
                    }
                    traffic::sync(
                        &doc,
                        "User requested immediate sync; conflict rules remain enforced",
                        "",
                        "",
                    );
                    self.apply_shared(&doc)?;
                }
                Action::QuitAll => {
                    if !self.cache_enabled {
                        for w in self.windows.iter_mut().flatten() {
                            for index in 0..w.docs.len() {
                                if !w.confirm_close(index) {
                                    #[cfg(target_os = "macos")]
                                    mac_lifecycle::complete(false);
                                    return Ok(());
                                }
                            }
                        }
                    }
                    self.persist();
                    if let Err(error) = self.store.flush() {
                        #[cfg(target_os = "macos")]
                        mac_lifecycle::complete(false);
                        return Err(error);
                    }
                    self.quit_requested = true;
                    if let Some(s) = &mut self.sharing {
                        s.stop_listener();
                    }
                    app::quit();
                    #[cfg(target_os = "macos")]
                    mac_lifecycle::complete(true);
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
                        && !self.transfer.rebuild
                        && !s.needs_new_history(d.sharing.as_ref().map_or("", |b| b.doc.as_str()))
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
                            if sync {
                                d.sharing.as_ref().map(|b| b.doc.clone())
                            } else {
                                None
                            },
                        )?;
                        if sync {
                            if let Some(binding) = &d.sharing {
                                s.leave(&binding.doc)?;
                            }
                            d.sharing = Some(peer::Binding {
                                revision: Some(r.key()),
                                force: r.force,
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
                    self.store.flush()?;
                    if sync && !same {
                        let replacement = self.windows[wi]
                            .as_ref()
                            .and_then(|w| w.docs.iter().find(|d| d.id == id))
                            .and_then(|d| d.sharing.as_ref())
                            .and_then(|b| {
                                self.sharing
                                    .as_ref()
                                    .unwrap()
                                    .own_replacements()
                                    .into_iter()
                                    .find(|o| o.doc == b.doc)
                            });
                        if let Some(offer) = replacement {
                            self.repaired_offer(&offer, "")?;
                        }
                    }
                    if sync {
                        if let Some(d) = self.windows[wi]
                            .as_ref()
                            .and_then(|w| w.docs.iter().find(|d| d.id == id))
                        {
                            self.sharing.as_ref().unwrap().ack_draft(&d.cache_key)?;
                        }
                    }
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
                        self.conflicts.editor_key = self.windows[wid]
                            .as_ref()
                            .map(|w| w.docs[w.active].cache_key.clone());
                        self.open_conflict(&id);
                    } else {
                        return Err("This document is not synchronised.".into());
                    }
                }
                Action::PreviewConflict => self.preview_conflict(),
                action @ (Action::ResolveMain
                | Action::ResolveEditedMain
                | Action::RepairSync
                | Action::RepairShownSync
                | Action::RecoverLocalBranch
                | Action::OpenSyncRevision
                | Action::CompareSyncRevision
                | Action::RefreshVersions) => self.recovery_action(wid, action)?,
                Action::SyncCountdown => {
                    self.countdown_timer = None;
                }
                Action::PublishDoc(key, revision) => {
                    if let Err(e) = self.publish_shared(&key, revision) {
                        if let Some((wi, di)) = self.doc_position(&key) {
                            self.windows[wi].as_mut().unwrap().docs[di].sync_error = Some(revision);
                        }
                        diagnostics::failure("sharing", "local_publish_paused", &e);
                        if let Some((wi, di)) = self.doc_position(&key) {
                            if let Some(binding) =
                                &self.windows[wi].as_ref().unwrap().docs[di].sharing
                            {
                                self.sharing.as_ref().unwrap().note_error(&binding.doc, &e);
                            }
                        }
                        if let Some((wi, di)) = self.doc_position(&key) {
                            self.windows[wi].as_mut().unwrap().docs[di].report = e;
                        }
                    }
                }
                Action::PeerEvent(event) => match event {
                    peer::Event::RestoreRoot(proposal) => {
                        let members = proposal
                            .members
                            .iter()
                            .map(|p| format!("{} · {}:{}", p.name, p.host, p.port))
                            .collect::<Vec<_>>()
                            .join("\n");
                        if chrome::decision(None,"Restore original shared history?","A paired device found a signed root for this document. Local contents will be preserved and missing commits restored.",&format!("Document: {}\n\nParticipants:\n{}",proposal.title,members),&["Reject","Restore link"],0)==Some(1){
                            if !self.cache_enabled{return Err("Save local contents first; protected session recovery is unavailable".into());}
                            self.persist();self.store.flush()?;let offer=peer::Offer{id:format!("restore-{}",proposal.root.key()),doc:proposal.doc.clone(),title:proposal.title.clone(),sync:true,revision:proposal.root.clone(),members:proposal.members.clone(),len:proposal.root.bytes};
                            self.sharing.as_ref().unwrap().install_root(proposal)?;
                            if offer.revision.commit.as_ref().is_some_and(|c|c.replaces.is_some()){self.repaired_offer(&offer,"")?;}
                        }else{self.sharing.as_ref().unwrap().decline_root(&proposal.doc)?;}
                    }
                    peer::Event::DocumentStatus(id, code) => {
                        self.sync_status(&id, &code);
                        if code == "integrity" {
                            let jobs = self
                                .windows
                                .iter()
                                .flatten()
                                .flat_map(|w| &w.docs)
                                .filter(|d| {
                                    d.sharing.as_ref().is_some_and(|b| {
                                        b.doc == id
                                            && self.sharing.as_ref().unwrap().needs_cached_body(b)
                                            && peer::hash(d.left.cache_text().as_bytes()) == b.hash
                                    })
                                })
                                .map(|d| (d.cache_key.clone(), d.left_meta.borrow().revision))
                                .collect::<Vec<_>>();
                            for (key, revision) in jobs {
                                self.tx.send((0, Action::PublishDoc(key, revision)));
                            }
                        }
                    }
                    peer::Event::BaseReady(id) => {
                        let jobs = self
                            .windows
                            .iter_mut()
                            .flatten()
                            .flat_map(|w| &mut w.docs)
                            .filter(|d| d.sharing.as_ref().is_some_and(|b| b.doc == id))
                            .map(|d| {
                                d.sync_error = None;
                                (d.cache_key.clone(), d.left_meta.borrow().revision)
                            })
                            .collect::<Vec<_>>();
                        for (key, revision) in jobs {
                            self.tx.send((0, Action::PublishDoc(key, revision)));
                        }
                        self.apply_shared(&id)?;
                    }
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
                            if o.sync
                                && o.revision
                                    .commit
                                    .as_ref()
                                    .is_some_and(|c| c.replaces.is_some())
                            {
                                "Repair document sync"
                            } else if o.sync {
                                "Synchronise document"
                            } else {
                                "Receive document"
                            },
                            if o.sync
                                && o.revision
                                    .commit
                                    .as_ref()
                                    .is_some_and(|c| c.replaces.is_some())
                            {
                                "This creates a new verified history. Matching old tabs keep their local contents as independent branches. Autosave must be enabled again explicitly. Choose the common main version after devices have rejoined."
                            } else if o.sync {
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
                    peer::Event::Updated(id) => {
                        self.apply_shared(&id)?;
                        if self.conflicts.win.shown() && self.conflicts.doc.as_deref() == Some(&id)
                        {
                            self.preview_conflict();
                        }
                    }
                    peer::Event::Conflict(id) => {
                        let current = self
                            .sharing
                            .as_ref()
                            .and_then(|service| service.document(&id));
                        if current.as_ref().is_none_or(|d| {
                            d.conflicts.is_empty()
                                || d.conflicts.iter().all(|r| r.hash == d.head.hash)
                        }) {
                            return Ok(());
                        }
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
            if background {
                diagnostics::failure("sharing", "background_sync_paused", &e);
                if let Some(id) = background_doc {
                    if let Some(service) = &self.sharing {
                        service.note_error(&id, &e);
                    }
                    self.sync_status(&id, "network_wait");
                }
                return None;
            }
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
