//! Root recovery transfers signed public metadata; consent precedes document access.
use super::*;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RootProposal {
    pub doc: String,
    pub title: String,
    pub root: Revision,
    pub members: Vec<Peer>,
}
#[derive(Default, Clone, Serialize, Deserialize)]
pub(super) struct LinkRecovery {
    pub checked: BTreeSet<String>,
    pub proposal: Option<RootProposal>,
}
pub(super) fn validate(inner: &Inner, p: &RootProposal) -> Result<(), String> {
    if p.doc != p.root.doc
        || p.title.chars().count() > 256
        || p.members.len() > 32
        || !p.members.iter().any(|m| m.id == inner.id)
        || p.members.iter().any(|m| !peer_valid(m))
    {
        return Err("Invalid root recovery participants".into());
    }
    validate_revision(&p.root)?;
    crate::revision::verify(&p.root, &p.doc, &p.members)?;
    if !p.root.parents.is_empty() {
        return Err("Recovery proposal is not a document root".into());
    }
    crate::revision::causal(&p.root, &BTreeMap::new())
}
pub(super) fn round(inner: &Arc<Inner>, peer: &Peer, started: Instant) -> Result<(), String> {
    let wanted = {
        let s = inner.state.lock().unwrap();
        if !s.config.clients.iter().any(|p| p.id == peer.id) {
            return Ok(());
        }
        s.links
            .iter()
            .filter(|(_, r)| r.proposal.is_none() && !r.checked.contains(&peer.id))
            .take(2)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>()
    };
    for doc in wanted {
        let mut stream = connect(inner, peer, started)?;
        write_header(&mut stream, &Request::RestoreRoot { doc: doc.clone() })?;
        let Reply::Root(proposal) = read_header(&mut stream)? else {
            return Err("Invalid root recovery reply".into());
        };
        if let Some(p) = &proposal {
            if (p.doc != doc
                && p.root.commit.as_ref().and_then(|c| c.replaces.as_ref()) != Some(&doc))
                || !p.members.iter().any(|m| m.id == peer.id)
            {
                return Err("Unrelated root recovery reply".into());
            }
            validate(inner, p)?;
        }
        let mut s = inner.state.lock().unwrap();
        if let Some(r) = s.links.get_mut(&doc) {
            r.checked.insert(peer.id.clone());
            r.proposal = proposal.clone();
        }
        inner.save(&s)?;
        drop(s);
        if let Some(p) = proposal {
            inner.event(Event::RestoreRoot(p));
        } else {
            inner.event(Event::DocumentStatus(doc, "waiting_link".into()));
        }
    }
    Ok(())
}
impl Service {
    pub fn request_root(&self, doc: &str) -> Result<(), String> {
        if doc.is_empty() || doc.len() > 128 {
            return Err("Invalid old document identifier".into());
        }
        let mut s = self.inner.state.lock().unwrap();
        if s.links.len() >= 256 && !s.links.contains_key(doc) {
            return Err("Too many pending root recovery requests".into());
        }
        s.links.insert(doc.into(), LinkRecovery::default());
        self.inner.save(&s)?;
        drop(s);
        self.wake();
        Ok(())
    }
    pub fn pending_roots(&self) -> Vec<RootProposal> {
        self.inner
            .state
            .lock()
            .unwrap()
            .links
            .values()
            .filter_map(|r| r.proposal.clone())
            .collect()
    }
    pub fn decline_root(&self, doc: &str) -> Result<(), String> {
        let mut s = self.inner.state.lock().unwrap();
        s.links.remove(doc);
        self.inner.save(&s)
    }
    pub fn install_root(&self, p: RootProposal) -> Result<(), String> {
        validate(&self.inner, &p)?;
        let mut s = self.inner.state.lock().unwrap();
        if p.members.iter().any(|m| s.revoked.contains(&m.id)) {
            return Err("This proposed history includes revoked participants; create a new history without them instead".into());
        }
        if s.docs
            .get(&p.doc)
            .is_some_and(|d| s.proofs.contains_key(&d.root) && d.root != p.root.key())
        {
            return Err(
                "A different approved root already exists; histories remain separate".into(),
            );
        }
        if let Some(old) = s.docs.get(&p.doc) {
            let archive = self.inner.root.join("recovery-v155");
            crate::private_fs::directory(&archive).map_err(err)?;
            session::atomic_write(
                &archive.join(format!("link-{}-{}.json", p.doc, session::new_key())),
                &serde_json::to_vec(old).map_err(err)?,
            )
            .map_err(err)?;
        }
        if !s.docs.contains_key(&p.doc)
            && s.docs.len() >= crate::security::limits().documents as usize
        {
            return Err("Document quota reached; old contents remain preserved".into());
        }
        let key = p.root.key();
        if !s.proofs.contains_key(&key)
            && s.proofs.len() + s.unverified.len() + s.quarantine.len()
                >= crate::security::limits().proofs as usize
        {
            return Err("Signed history quota reached".into());
        }
        let existing = s
            .proofs
            .iter()
            .filter(|(_, r)| r.doc == p.doc)
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in existing {
            let r = s.proofs.remove(&key).unwrap();
            s.unverified.insert(key, r);
        }
        s.proofs.insert(key.clone(), p.root.clone());
        s.unverified.remove(&key);
        s.docs
            .entry(p.doc.clone())
            .and_modify(|d| {
                d.root = key.clone();
                d.members = p.members.clone();
                d.active = true;
                d.issue = Some("waiting_history".into());
            })
            .or_insert(Document {
                issue: Some("waiting_history".into()),
                id: p.doc.clone(),
                title: p.title.clone(),
                head: p.root.clone(),
                conflicts: vec![],
                history: vec![],
                members: p.members.clone(),
                active: true,
                departed: BTreeSet::new(),
                root: key,
            });
        repair::promote(&mut s, &p.doc)?;
        let approved = s.proofs.keys().cloned().collect::<BTreeSet<_>>();
        let d = s.docs.get_mut(&p.doc).unwrap();
        let old = std::mem::take(&mut d.conflicts);
        for r in old {
            if approved.contains(&r.key()) {
                d.conflicts.push(r);
            } else {
                d.history.push(r);
            }
        }
        if !approved.contains(&d.head.key()) {
            d.history.push(d.head.clone());
            d.head = p.root.clone();
        }
        let replaced = p.root.commit.as_ref().and_then(|c| c.replaces.clone());
        s.links
            .retain(|id, _| id != &p.doc && Some(id) != replaced.as_ref());
        self.inner.save(&s)?;
        drop(s);
        crate::traffic::sync(
            &p.doc,
            "Approved signed root restored; original contents retained",
            &p.root.key(),
            &p.root.author,
        );
        self.inner.event(Event::BaseReady(p.doc));
        self.wake();
        Ok(())
    }
}
