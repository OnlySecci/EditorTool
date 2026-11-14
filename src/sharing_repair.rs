//! User-facing recovery paths preserve content and never invent a common parent.
use crate::*;
pub fn status(code: &str) -> &str {
    match code {
    "new_history_required"=>"Sync needs repair · Start a new approved history; local contents are preserved",
    "waiting_base"=>"Restoring the missing base · Waiting for devices; Repair sync can keep this text as a branch",
    "recover_branch_available"=>"Original base unavailable · Recovering this text as an independent branch",
    "waiting_link"=>"Restoring the document link · Asking approved devices",
    "root_approval"=>"Signed history found · Approval required",
    "link_unavailable"=>"Original history unavailable · Start a new approved history",
    "history_pending"=>"Sync current · Restoring older snapshots in the background",
    "waiting_proofs"|"waiting_history"=>"Restoring signed history and contents from other devices",
    "quota"=>"Sync paused · Storage/history quota reached; no history was deleted",
    "integrity"=>"Sync paused · Stored data needs integrity recovery",
    "network_wait"=>"Sync waiting · Transfer interrupted; local contents are preserved",
    _=>"Sync pending · Check document history or Repair sync",
}
}
impl Runtime {
    fn sync_position(&self, id: &str, preferred: Option<usize>) -> Option<(usize, usize)> {
        if let Some(wi) = preferred {
            if let Some(w) = self.windows.get(wi).and_then(Option::as_ref) {
                if w.docs
                    .get(w.active)
                    .is_some_and(|d| d.sharing.as_ref().is_some_and(|b| b.doc == id))
                {
                    return Some((wi, w.active));
                }
            }
        }
        let positions = || {
            self.windows.iter().enumerate().flat_map(|(wi, w)| {
                w.iter()
                    .flat_map(move |w| w.docs.iter().enumerate().map(move |(di, d)| (wi, di, d)))
            })
        };
        positions()
            .find(|(_, _, d)| {
                self.conflicts.editor_key.as_ref() == Some(&d.cache_key)
                    && d.sharing.as_ref().is_some_and(|b| b.doc == id)
            })
            .or_else(|| {
                positions().find(|(_, _, d)| d.sharing.as_ref().is_some_and(|b| b.doc == id))
            })
            .map(|(wi, di, _)| (wi, di))
    }
    pub(super) fn capture_local_branches(&mut self, id: &str) -> Result<bool, String> {
        let Some(service) = &self.sharing else {
            return Ok(false);
        };
        if service.needs_new_history(id) {
            return Ok(false);
        }
        let before = service
            .document(id)
            .map(|d| {
                std::iter::once(d.head)
                    .chain(d.conflicts)
                    .map(|r| r.key())
                    .collect::<std::collections::BTreeSet<_>>()
            })
            .unwrap_or_default();
        let pending = self
            .windows
            .iter()
            .enumerate()
            .flat_map(|(wi, w)| {
                w.iter().flat_map(move |w| {
                    w.docs
                        .iter()
                        .enumerate()
                        .filter(move |(_, d)| {
                            d.sharing.as_ref().is_some_and(|b| b.doc == id)
                                && d.left_meta.borrow().revision != d.share_revision
                        })
                        .map(move |(di, _)| (wi, di))
                })
            })
            .collect::<Vec<_>>();
        if pending.is_empty() {
            return Ok(false);
        }
        if !self.cache_enabled {
            return Err(
                "Save the local drafts first; protected recovery storage is unavailable".into(),
            );
        }
        self.persist();
        self.store.flush()?;
        let draft_keys = pending
            .iter()
            .map(|(wi, di)| {
                self.windows[*wi].as_ref().unwrap().docs[*di]
                    .cache_key
                    .clone()
            })
            .collect::<Vec<_>>();
        for (wi, di) in pending {
            let d = &self.windows[wi].as_ref().unwrap().docs[di];
            let binding = d.sharing.clone().unwrap();
            let data = d.left.cache_text();
            let key = d.cache_key.clone();
            let service = self.sharing.as_ref().unwrap();
            let revision = match service.tree_publish(&binding, &data, &key)? {
                Some(r) => r,
                None => service.recover_draft(&binding, &data)?,
            };
            let d = &mut self.windows[wi].as_mut().unwrap().docs[di];
            d.sharing.as_mut().unwrap().point_to(&revision);
            d.share_revision = d.left_meta.borrow().revision;
            d.sync_error = None;
        }
        self.persist();
        self.store.flush()?;
        self.sharing.as_ref().unwrap().ack_drafts(&draft_keys)?;
        let after = self
            .sharing
            .as_ref()
            .unwrap()
            .document(id)
            .map(|d| {
                std::iter::once(d.head)
                    .chain(d.conflicts)
                    .map(|r| r.key())
                    .collect::<std::collections::BTreeSet<_>>()
            })
            .unwrap_or_default();
        Ok(before != after)
    }
    pub(super) fn restore_pending_drafts(&mut self) -> Result<(), String> {
        if !self.cache_enabled {
            return Ok(());
        }
        let Some(service) = &self.sharing else {
            return Ok(());
        };
        let drafts = service.drafts();
        for draft in drafts {
            if draft.key.len() > 160
                || !draft
                    .key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            {
                continue;
            }
            let data = match self.sharing.as_ref().unwrap().body(&draft.hash) {
                Ok(data)
                    if peer::hash(data.as_bytes()) == draft.hash
                        && data.len() as u64 == draft.bytes =>
                {
                    data
                }
                _ => {
                    // Do not let an older cached tab overwrite the retained draft
                    // record when its newer snapshot cannot currently be read.
                    for w in self.windows.iter_mut().flatten() {
                        for d in &mut w.docs {
                            if d.cache_key == draft.key {
                                d.cache_key = session::new_key();
                                d.sharing = None;
                                d.display_name =
                                    Some(format!("Cached recovery copy - {}", d.title()));
                                d.report = "Newer pending draft needs integrity recovery; its original record was preserved. This cached copy is unsynchronised.".into();
                            }
                        }
                    }
                    diagnostics::event(
                        "sharing",
                        "pending_draft_unavailable_record_preserved_cached_copy_detached",
                    );
                    continue;
                }
            };
            let existing = self.windows.iter().enumerate().find_map(|(wi, w)| {
                w.as_ref().and_then(|w| {
                    w.docs
                        .iter()
                        .position(|d| d.cache_key == draft.key)
                        .map(|di| (wi, di))
                })
            });
            let mut original_path = None;
            if let Some((wi, di)) = existing {
                let d = &mut self.windows[wi].as_mut().unwrap().docs[di];
                if peer::hash(d.left.cache_text().as_bytes()) == draft.hash {
                    continue;
                }
                original_path = d.path.take();
                d.cache_key = session::new_key();
                d.sharing = None;
                d.display_name = Some(format!("Older cached copy — {}", d.title()));
                d.report =
                    "Older session contents preserved alongside a newer pending draft".into();
            }
            if self.windows.iter().all(Option::is_none) {
                self.handle(0, Action::NewWindow);
            }
            let wi = self.windows.iter().position(Option::is_some).unwrap();
            let w = self.windows[wi].as_mut().unwrap();
            w.add_tab(wi, self.tx);
            let d = &mut w.docs[w.active];
            d.left.set_cache_text(&data)?;
            d.path = original_path;
            d.cache_key = draft.key;
            let mut binding = draft.base;
            binding.autosave = false;
            binding.approved_target = None;
            d.sharing = Some(binding);
            d.share_revision = d.left_meta.borrow().revision.wrapping_sub(1);
            d.sync_error = None;
            d.display_name = Some("Recovered pending sync draft".into());
            d.report = "Draft restored from independent encrypted recovery storage".into();
        }
        self.persist();
        self.store.flush()?;
        Ok(())
    }
    pub(super) fn sync_status(&mut self, doc: &str, code: &str) {
        for w in self.windows.iter_mut().flatten() {
            for d in &mut w.docs {
                if d.sharing.as_ref().is_some_and(|b| b.doc == doc) {
                    d.report = if code.is_empty() {
                        "Sync ready".into()
                    } else {
                        status(code).into()
                    };
                }
            }
        }
    }
    pub(super) fn repaired_offer(
        &mut self,
        offer: &peer::Offer,
        _data: &str,
    ) -> Result<bool, String> {
        let Some(previous) = offer
            .revision
            .commit
            .as_ref()
            .and_then(|c| c.replaces.as_deref())
        else {
            return Ok(false);
        };
        let positions = self
            .windows
            .iter()
            .enumerate()
            .flat_map(|(wi, w)| {
                w.iter().flat_map(move |w| {
                    w.docs
                        .iter()
                        .enumerate()
                        .filter(move |(_, d)| d.sharing.as_ref().is_some_and(|b| b.doc == previous))
                        .map(move |(di, _)| (wi, di))
                })
            })
            .collect::<Vec<_>>();
        if positions.is_empty() {
            return Ok(false);
        }
        if !self.cache_enabled {
            return Err(
                "Protected session backup is unavailable; existing tabs were not replaced".into(),
            );
        }
        self.persist();
        self.store.flush()?;
        let draft_keys = positions
            .iter()
            .map(|(wi, di)| {
                self.windows[*wi].as_ref().unwrap().docs[*di]
                    .cache_key
                    .clone()
            })
            .collect::<Vec<_>>();
        for (wi, di) in positions {
            let local = self.windows[wi].as_ref().unwrap().docs[di]
                .left
                .cache_text();
            let mut binding = peer::Binding {
                revision: Some(offer.revision.key()),
                force: None,
                doc: offer.doc.clone(),
                clock: offer.revision.clock.clone(),
                hash: offer.revision.hash.clone(),
                autosave: false,
                approved_target: None,
                disk_hash: None,
            };
            if peer::hash(local.as_bytes()) != offer.revision.hash {
                let r = self
                    .sharing
                    .as_ref()
                    .ok_or("Sharing unavailable")?
                    .recover_draft(&binding, &local)?;
                binding.point_to(&r);
            }
            let d = &mut self.windows[wi].as_mut().unwrap().docs[di];
            // Keep the local text, path, images and cursor. Reuse no autosave grant.
            d.sharing = Some(binding);
            d.received_offer = Some(offer.id.clone());
            d.share_revision = d.left_meta.borrow().revision;
            d.sync_error = None;
            d.report = "Sync link repaired · Local differences are preserved as branches".into();
        }
        self.persist();
        self.store.flush()?;
        self.sharing.as_ref().unwrap().ack_drafts(&draft_keys)?;
        self.sharing.as_ref().unwrap().delivered(&offer.id)?;
        self.apply_shared(&offer.doc)?;
        Ok(true)
    }
    pub(super) fn recovery_action(&mut self, wid: usize, action: Action) -> Result<(), String> {
        if matches!(action, Action::RepairShownSync) {
            let id = self.conflicts.doc.clone().ok_or("No history selected")?;
            let (wi, di) = self
                .sync_position(&id, None)
                .ok_or("Open this document first")?;
            self.windows[wi]
                .as_mut()
                .unwrap()
                .action(Action::Select(di), wi, self.tx);
            return self.recovery_action(wi, Action::RepairSync);
        }
        let selected_doc = if matches!(action, Action::RepairSync | Action::RecoverLocalBranch) {
            self.windows
                .get(wid)
                .and_then(Option::as_ref)
                .and_then(|w| w.docs.get(w.active))
                .and_then(|d| d.sharing.as_ref())
                .map(|b| b.doc.clone())
                .or_else(|| self.conflicts.doc.clone())
        } else {
            self.conflicts.doc.clone()
        };
        let id = selected_doc.ok_or("This document has no sync history")?;
        match action {
            Action::RepairSync => {
                let position = self
                    .sync_position(&id, Some(wid))
                    .ok_or("Open the document to repair its link")?;
                let (wi, di) = position;
                self.windows[wi]
                    .as_mut()
                    .unwrap()
                    .action(Action::Select(di), wi, self.tx);
                if self
                    .sharing
                    .as_ref()
                    .ok_or("Sharing unavailable")?
                    .needs_new_history(&id)
                {
                    let decision=chrome::decision(None,"Repair shared history","The local link has no verifiable signed root. Existing contents and archived versions remain preserved.","First ask paired devices for the original signed history. If it is unavailable, one device can start a new history and invite the others; recipients keep their local differences as branches.",&["Cancel","Ask paired devices","New shared history…"],1);
                    if decision == Some(1) {
                        self.sharing.as_ref().unwrap().request_root(&id)?;
                    } else if decision == Some(2) {
                        self.sharing_action(wi, Action::Transfer(true));
                        self.transfer.rebuild = true;
                    }
                } else {
                    let decision=chrome::decision(None,"Repair sync","Restore the missing signed base, or preserve this text as an independent recovery branch.","A recovery branch does not overwrite other devices. Different contents require a main-version decision.",&["Cancel","Retry restoration","Keep as branch","New shared history…"],1);
                    if decision == Some(3) {
                        self.sharing_action(wi, Action::Transfer(true));
                        self.transfer.rebuild = true;
                    } else if decision == Some(2) {
                        self.recovery_action(wi, Action::RecoverLocalBranch)?;
                    } else if decision == Some(1) {
                        let d = &mut self.windows[wi].as_mut().unwrap().docs[di];
                        d.sync_error = None;
                        self.sharing
                            .as_ref()
                            .unwrap()
                            .queue_base(d.sharing.as_ref().unwrap())?;
                        self.sharing.as_ref().unwrap().retry();
                    }
                }
            }
            Action::RecoverLocalBranch => {
                let (wi, di) = self
                    .sync_position(&id, Some(wid))
                    .ok_or("Open the local document first")?;
                if !self.cache_enabled {
                    return Err(
                        "Save the local text first; protected session recovery is unavailable"
                            .into(),
                    );
                }
                self.persist();
                self.store.flush()?;
                let d = &self.windows[wi].as_ref().unwrap().docs[di];
                let revision = self
                    .sharing
                    .as_ref()
                    .ok_or("Sharing unavailable")?
                    .recover_draft(d.sharing.as_ref().unwrap(), &d.left.cache_text())?;
                let d = &mut self.windows[wi].as_mut().unwrap().docs[di];
                d.sharing.as_mut().unwrap().point_to(&revision);
                d.share_revision = d.left_meta.borrow().revision;
                d.sync_error = None;
                self.persist();
                self.store.flush()?;
                self.sharing
                    .as_ref()
                    .unwrap()
                    .ack_draft(&self.windows[wi].as_ref().unwrap().docs[di].cache_key)?;
                self.open_conflict(&id);
            }
            Action::ResolveMain | Action::ResolveEditedMain => {
                if !self.conflicts.win.shown() {
                    return Ok(());
                }
                if self.capture_local_branches(&id)? {
                    self.open_conflict(&id);
                    self.conflicts.note("Local edits were added as branches. Review the refreshed versions and choose the main version again.");
                    return Ok(());
                }
                let edited = matches!(action, Action::ResolveEditedMain);
                if !edited
                    && self
                        .conflicts
                        .versions
                        .get(self.conflicts.list.value().saturating_sub(1) as usize)
                        .is_some_and(|r| {
                            r.doc != id || !self.sharing.as_ref().unwrap().is_verified(r)
                        })
                {
                    self.conflicts.note("This archived version cannot be a signed parent of the current group. Open a recovery copy, copy or merge its contents into the shared editor, then choose Use editor text.");
                    return Ok(());
                }
                let chosen = if edited {
                    None
                } else {
                    self.conflicts
                        .versions
                        .get(self.conflicts.list.value().saturating_sub(1) as usize)
                        .map(peer::Revision::key)
                };
                if !edited {
                    if let Some(r) = self
                        .conflicts
                        .versions
                        .get(self.conflicts.list.value().saturating_sub(1) as usize)
                    {
                        let service = self.sharing.as_ref().unwrap();
                        if service.revision_body(r).is_err() {
                            service.require_body(r)?;
                            self.conflicts.note("The selected snapshot is being restored. Retry after its preview is available, or open another preserved version.");
                            return Ok(());
                        }
                    }
                }
                let text = if edited {
                    let d = self
                        .windows
                        .iter()
                        .flatten()
                        .flat_map(|w| &w.docs)
                        .find(|d| self.conflicts.editor_key.as_ref() == Some(&d.cache_key))
                        .ok_or("Open the shared document selected for merging first")?;
                    Some(d.left.cache_text())
                } else {
                    None
                };
                if chrome::decision(None,"Choose the common main version","Create a new commit joining the currently displayed branches?","Unseen or later independent edits remain separate branches. Other devices keep their history. Unpublished local edits are not discarded.",&["Cancel","Commit main version"],0)!=Some(1){return Ok(());}
                let result = self
                    .sharing
                    .as_ref()
                    .ok_or("Sharing unavailable")?
                    .resolve_main(
                        &id,
                        chosen.as_deref(),
                        &self.conflicts.tips,
                        text.as_deref(),
                    )?;
                if result.is_none() {
                    self.open_conflict(&id);
                    self.conflicts.note("Branches changed while this view was open. Review the refreshed versions before deciding.");
                    self.sync_status(&id, "branches_changed");
                    return Ok(());
                }
                self.conflict_notified.remove(&id);
                self.conflicts.win.hide();
                // A local merged draft was explicitly chosen. Other dirty copies
                // remain intact and will publish as independent branches.
                if edited {
                    if let Some(r) = result {
                        if let Some(d) = self
                            .windows
                            .iter_mut()
                            .flatten()
                            .flat_map(|w| &mut w.docs)
                            .find(|d| self.conflicts.editor_key.as_ref() == Some(&d.cache_key))
                        {
                            d.sharing.as_mut().unwrap().point_to(&r);
                            d.share_revision = d.left_meta.borrow().revision;
                            d.sync_error = None;
                        }
                    }
                }
                self.apply_shared(&id)?;
                self.persist();
            }
            Action::OpenSyncRevision | Action::CompareSyncRevision => {
                let compare = matches!(action, Action::CompareSyncRevision);
                let r = self
                    .conflicts
                    .versions
                    .get(self.conflicts.list.value().saturating_sub(1) as usize)
                    .ok_or("Select a version")?
                    .clone();
                let service = self.sharing.as_ref().ok_or("Sharing unavailable")?;
                let body = match service.revision_body(&r) {
                    Ok(body) => body,
                    Err(_) => {
                        service.require_body(&r)?;
                        self.sync_status(&id, "waiting_history");
                        return Ok(());
                    }
                };
                let current = if compare {
                    self.sync_position(&id, None).map(|(wi, di)| {
                        self.windows[wi].as_ref().unwrap().docs[di]
                            .left
                            .cache_text()
                    })
                } else {
                    None
                };
                let wi = self
                    .windows
                    .iter()
                    .position(Option::is_some)
                    .ok_or("Open a window first")?;
                let w = self.windows[wi].as_mut().unwrap();
                w.add_tab(wi, self.tx);
                let d = &mut w.docs[w.active];
                d.display_name = Some(format!("Recovered version {}", &r.hash[..12]));
                if let Some(current) = current {
                    d.left.set_cache_text(&current)?;
                    d.right.set_cache_text(&body)?;
                    w.action(Action::ToggleCompare, wi, self.tx);
                    self.tx.send((wi, Action::Compare));
                } else {
                    d.left.set_cache_text(&body)?;
                }
                self.persist();
            }
            Action::RefreshVersions => self.open_conflict(&id),
            _ => (),
        }
        Ok(())
    }
}
