//! Security implementation inside the peer module; private state remains encapsulated.
use super::*;
impl Inner {
    pub(super) fn still_authorized(&self, doc: &str, peer: &str, epoch: u64) -> bool {
        if !self.enabled.load(Ordering::Relaxed)
            || self.network_epoch.load(Ordering::Relaxed) != epoch
        {
            return false;
        }
        let s = self.state.lock().unwrap();
        !s.revoked.contains(peer)
            && (self.allowed(&s, doc, peer)
                || s.outbox.iter().any(|o| {
                    o.target == peer
                        && o.offer.doc == doc
                        && (!o.offer.sync || s.docs.get(doc).is_some_and(|d| d.active))
                }))
    }
    pub(super) fn reserve(
        &self,
        doc: &str,
        author: &str,
        bytes: u64,
    ) -> Result<crate::security::Reservation, String> {
        let s = self.state.lock().unwrap();
        let revisions = s
            .proofs
            .values()
            .chain(s.offers.values().map(|o| &o.revision))
            .chain(s.outbox.iter().map(|o| &o.offer.revision));
        let mut used_doc =
            crate::security::retained_bytes(&self.root, revisions.clone().filter(|r| r.doc == doc));
        let mut used_peer =
            crate::security::retained_bytes(&self.root, revisions.filter(|r| r.author == author));
        let mut doc_hashes = s
            .proofs
            .values()
            .filter(|r| r.doc == doc)
            .map(|r| r.hash.clone())
            .collect::<BTreeSet<_>>();
        let mut own_hashes = s
            .proofs
            .values()
            .filter(|r| r.author == author)
            .map(|r| r.hash.clone())
            .collect::<BTreeSet<_>>();
        for d in s.drafts.values() {
            if d.base.doc == doc && doc_hashes.insert(d.hash.clone()) {
                used_doc = used_doc.saturating_add(d.bytes);
            }
            if author == self.id && own_hashes.insert(d.hash.clone()) {
                used_peer = used_peer.saturating_add(d.bytes);
            }
        }
        crate::security::reserve(&self.root, doc, author, bytes, used_doc, used_peer)
    }
}
impl Service {
    pub fn revoked(&self) -> Vec<String> {
        self.inner
            .state
            .lock()
            .unwrap()
            .revoked
            .iter()
            .cloned()
            .collect()
    }
    pub fn unblock(&mut self, id: &str) -> Result<(), String> {
        if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Enter an exact fingerprint".into());
        }
        {
            let mut s = self.inner.state.lock().unwrap();
            s.revoked.remove(id);
            self.inner.save(&s)?;
        }
        self.inner.seen.lock().unwrap().remove(id);
        crate::diagnostics::event("security", "device_unblocked_for_pairing");
        Ok(())
    }
    pub fn prepare_rotation(&mut self) -> Result<(), String> {
        let mut config = self.config();
        config.enabled = false;
        self.configure(config)?;
        let mut s = self.inner.state.lock().unwrap();
        s.revoked.insert(self.inner.id.clone());
        for d in s.docs.values_mut() {
            d.active = false;
        }
        s.outbox.clear();
        s.offers.clear();
        s.decisions.clear();
        s.leaves.clear();
        self.inner.save(&s)?;
        session::atomic_write(
            &self.inner.root.join("replace-identity.request"),
            format!("replace:{}", self.inner.id).as_bytes(),
        )
        .map_err(err)?;
        crate::diagnostics::event("security", "identity_replacement_scheduled");
        Ok(())
    }
    pub fn collect_unused(&self) -> Result<usize, String> {
        let mut s = self.inner.state.lock().unwrap();
        let _guard = crate::security::begin_cleanup()?;
        let pending = s
            .offers
            .values()
            .chain(s.outbox.iter().map(|o| &o.offer))
            .chain(s.inbox.values().map(|(o, _)| o))
            .map(|o| o.doc.clone())
            .chain(s.drafts.values().map(|d| d.base.doc.clone()))
            .collect::<BTreeSet<_>>();
        s.docs.retain(|id, d| d.active || pending.contains(id));
        let retained_groups = s
            .docs
            .keys()
            .cloned()
            .chain(pending)
            .collect::<BTreeSet<_>>();
        s.proofs.retain(|_, r| retained_groups.contains(&r.doc));
        s.unverified.retain(|_, r| retained_groups.contains(&r.doc));
        s.quarantine.retain(|_, r| retained_groups.contains(&r.doc));
        s.bases.retain(|_, r| retained_groups.contains(&r.doc));
        let existing = s.proofs.keys().cloned().collect::<BTreeSet<_>>();
        s.want_bodies.retain(|key| existing.contains(key));
        self.inner.save(&s)?;
        let mut retained = BTreeSet::new();
        for r in s.proofs.values() {
            retained.insert(r.hash.clone());
        }
        for d in s.drafts.values() {
            retained.insert(d.hash.clone());
        }
        for r in s.quarantine.values().chain(s.unverified.values()) {
            retained.insert(r.hash.clone());
        }
        for d in s.docs.values() {
            retained.insert(d.head.hash.clone());
            for r in d.conflicts.iter().chain(&d.history) {
                retained.insert(r.hash.clone());
            }
        }
        for o in s
            .offers
            .values()
            .chain(s.outbox.iter().map(|o| &o.offer))
            .chain(s.inbox.values().map(|(o, _)| o))
        {
            retained.insert(o.revision.hash.clone());
        }
        // Every retained signed branch and pending proof keeps its available body.
        // Only explicitly retired, unreferenced data is eligible for removal.
        let mut removed = 0;
        for entry in fs::read_dir(self.inner.root.join("content")).map_err(err)? {
            let entry = entry.map_err(err)?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(err)?;
            if crate::private_fs::is_link(&metadata) {
                return Err("Link in content storage rejected".into());
            }
            if path.extension().is_some_and(|e| e == "txt")
                && path
                    .file_stem()
                    .and_then(|v| v.to_str())
                    .is_some_and(|hash| !retained.contains(hash))
            {
                fs::remove_file(path).map_err(err)?;
                removed += 1;
            }
        }
        crate::diagnostics::event("security", "unused_sync_content_removed");
        Ok(removed)
    }
}
