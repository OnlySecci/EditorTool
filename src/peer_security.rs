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
        let used_doc =
            crate::security::retained_bytes(&self.root, revisions.clone().filter(|r| r.doc == doc));
        let used_peer =
            crate::security::retained_bytes(&self.root, revisions.filter(|r| r.author == author));
        crate::security::reserve(&self.root, doc, author, bytes, used_doc, used_peer)
    }
    pub(super) fn accept_proofs(
        &self,
        doc: &str,
        mut nodes: Vec<Revision>,
        source: &str,
        epoch: u64,
    ) -> Result<(), String> {
        if nodes.len() > 512 {
            return Err("Too many signed proof nodes".into());
        }
        let mut s = self.state.lock().unwrap();
        if self.network_epoch.load(Ordering::Relaxed) != epoch || !self.allowed(&s, doc, source) {
            return Err("Proof permission changed".into());
        }
        let d = s.docs.get(doc).ok_or("Unknown document")?;
        let members = d.members.clone();
        let root = d.root.clone();
        for r in &nodes {
            validate_revision(r)?;
            crate::revision::verify(r, doc, &members)?;
            if r.parents.is_empty() && r.key() != root {
                return Err("Unapproved signed root".into());
            }
        }
        while !nodes.is_empty() {
            let before = nodes.len();
            let mut waiting = vec![];
            for r in nodes {
                if s.proofs.contains_key(&r.key()) {
                    continue;
                }
                if r.parents.iter().any(|p| !s.proofs.contains_key(p)) {
                    waiting.push(r);
                    continue;
                }
                crate::revision::causal(&r, &s.proofs)?;
                if s.proofs.len() >= crate::security::limits().proofs as usize {
                    return Err("Signed history quota reached; start a fresh sharing group or adjust Security limits".into());
                }
                if s.proofs.values().any(|known| {
                    known.doc == doc
                        && known.author == r.author
                        && known.clock.get(&r.author) == r.clock.get(&r.author)
                        && known.key() != r.key()
                }) {
                    return Err("Device revision fork detected; inspect versions and revoke a copied identity".into());
                }
                s.proofs.insert(r.key(), r);
            }
            if waiting.len() == before {
                return Err("Signed revision parents are missing or cyclic".into());
            }
            nodes = waiting;
        }
        self.save(&s)
    }
}
pub(super) fn fetch_proofs(
    inner: &Arc<Inner>,
    peer: &Peer,
    doc: &str,
    head: &Revision,
    started: Instant,
    epoch: u64,
) -> Result<(), String> {
    {
        let s = inner.state.lock().unwrap();
        if !inner.allowed(&s, doc, &peer.id) {
            return Err("Unexpected document in peer reply".into());
        }
        let d = &s.docs[doc];
        validate_revision(head)?;
        if s.proofs
            .get(&head.key())
            .is_some_and(|known| known.signature == head.signature)
        {
            return Ok(());
        }
        crate::revision::verify(head, doc, &d.members)?;
    }
    let mut nodes = BTreeMap::from([(head.key(), head.clone())]);
    loop {
        let missing = {
            let s = inner.state.lock().unwrap();
            nodes
                .values()
                .flat_map(|r| r.parents.iter())
                .filter(|id| !s.proofs.contains_key(*id) && !nodes.contains_key(*id))
                .cloned()
                .collect::<BTreeSet<_>>()
        };
        if missing.is_empty() {
            break;
        }
        if nodes.len() + missing.len() > 512 {
            return Err("Too many unseen signed versions; share a fresh group".into());
        }
        for keys in missing.into_iter().collect::<Vec<_>>().chunks(32) {
            let mut stream = connect(inner, peer, started)?;
            write_header(
                &mut stream,
                &Request::Proofs {
                    doc: doc.into(),
                    keys: keys.to_vec(),
                },
            )?;
            let Reply::Proofs(proofs) = read_header(&mut stream)? else {
                return Err("Missing signed provenance reply".into());
            };
            if proofs.len() != keys.len() {
                return Err("Incomplete signed provenance reply".into());
            }
            for proof in proofs {
                if !keys.contains(&proof.key()) {
                    return Err("Unexpected signed provenance".into());
                }
                {
                    let s = inner.state.lock().unwrap();
                    crate::revision::verify(
                        &proof,
                        doc,
                        &s.docs.get(doc).ok_or("Document closed")?.members,
                    )?;
                }
                nodes.insert(proof.key(), proof);
            }
        }
    }
    inner.accept_proofs(doc, nodes.into_values().collect(), &peer.id, epoch)
}
pub(super) fn push_proofs(
    inner: &Arc<Inner>,
    peer: &Peer,
    doc: &str,
    head: &Revision,
    started: Instant,
) -> Result<(), String> {
    let known = inner
        .acknowledgements
        .lock()
        .unwrap()
        .get(&(doc.into(), peer.id.clone()))
        .cloned();
    let nodes = {
        let s = inner.state.lock().unwrap();
        if !inner.allowed(&s, doc, &peer.id) {
            return Err("Document no longer shared".into());
        }
        let mut pending = vec![(head.key(), false)];
        let mut seen = BTreeSet::new();
        let mut ordered = vec![];
        while let Some((id, visited)) = pending.pop() {
            if known.as_ref() == Some(&id) {
                continue;
            }
            if visited {
                ordered.push(s.proofs.get(&id).ok_or("Missing signed parent")?.clone());
                continue;
            }
            if !seen.insert(id.clone()) {
                continue;
            }
            if seen.len() > 512 {
                return Err("Peer is too far behind; share a fresh group".into());
            }
            let r = s.proofs.get(&id).ok_or("Missing signed parent")?;
            pending.push((id, true));
            for p in r.parents.iter().rev() {
                pending.push((p.clone(), false));
            }
        }
        ordered
    };
    for nodes in nodes.chunks(32) {
        let mut stream = connect(inner, peer, started)?;
        write_header(
            &mut stream,
            &Request::ProofPush {
                doc: doc.into(),
                nodes: nodes.to_vec(),
            },
        )?;
        if !matches!(read_header(&mut stream)?, Reply::Done) {
            return Err("Signed provenance refused".into());
        }
    }
    Ok(())
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
            .collect::<BTreeSet<_>>();
        s.docs.retain(|id, d| d.active || pending.contains(id));
        let retained_groups = s
            .docs
            .keys()
            .cloned()
            .chain(pending)
            .collect::<BTreeSet<_>>();
        s.proofs.retain(|_, r| retained_groups.contains(&r.doc));
        self.inner.save(&s)?;
        let mut retained = BTreeSet::new();
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
        // Proof-only history keeps causal metadata; bodies no longer in the retained
        // document history can be removed. Open tabs/session files are separate.
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
