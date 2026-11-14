//! Bounded, resumable proof transfer. Only fully verified nodes enter the graph.
use super::*;
impl Inner {
    pub(super) fn accept_proofs(
        &self,
        doc: &str,
        nodes: Vec<Revision>,
        source: &str,
        epoch: u64,
    ) -> Result<(), String> {
        if nodes.len() > 32 {
            return Err("Too many proof nodes in one message".into());
        }
        let mut s = self.state.lock().unwrap();
        if self.network_epoch.load(Ordering::Relaxed) != epoch || !self.allowed(&s, doc, source) {
            return Err("Proof permission changed".into());
        }
        let d = s.docs.get(doc).ok_or("Unknown document")?;
        for r in &nodes {
            validate_revision(r)?;
            crate::revision::verify(r, doc, &d.members)?;
            if !tree::root_allowed(d, r) {
                return Err("Unapproved signed root".into());
            }
        }
        for r in nodes {
            if s.quarantine.contains_key(&r.key()) {
                return Err("Invalid signed ancestry remains preserved for recovery review".into());
            }
            if s.proofs.contains_key(&r.key()) {
                continue;
            }
            if !s.unverified.contains_key(&r.key())
                && s.proofs.len() + s.unverified.len() + s.quarantine.len()
                    >= crate::security::limits().proofs as usize
            {
                return Err(
                    "Signed history quota reached; stored history remains preserved".into(),
                );
            }
            s.unverified.insert(r.key(), r);
        }
        repair::promote(&mut s, doc)?;
        for r in s
            .proofs
            .values()
            .filter(|r| r.doc == doc)
            .cloned()
            .collect::<Vec<_>>()
        {
            if !self.content_path(&r.hash)?.is_file() {
                s.want_bodies.insert(r.key());
            }
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
) -> Result<bool, String> {
    {
        let s = inner.state.lock().unwrap();
        if !inner.allowed(&s, doc, &peer.id) {
            return Err("Proof access denied".into());
        }
        if s.proofs
            .get(&head.key())
            .is_some_and(|r| r.signature == head.signature)
        {
            return Ok(true);
        }
    }
    inner.accept_proofs(doc, vec![head.clone()], &peer.id, epoch)?;
    let mut budget = 512usize;
    loop {
        let missing = {
            let s = inner.state.lock().unwrap();
            if s.proofs.contains_key(&head.key()) {
                return Ok(true);
            }
            s.unverified
                .values()
                .filter(|r| r.doc == doc)
                .flat_map(crate::revision::dependencies)
                .filter(|id| !s.proofs.contains_key(id) && !s.unverified.contains_key(id))
                .collect::<BTreeSet<_>>()
        };
        if missing.is_empty() || budget == 0 {
            return Ok(false);
        }
        let mut advanced = false;
        for keys in missing
            .into_iter()
            .filter(|key| {
                !inner
                    .unavailable_proofs
                    .lock()
                    .unwrap()
                    .contains(&(peer.id.clone(), key.clone()))
            })
            .take(budget)
            .collect::<Vec<_>>()
            .chunks(32)
        {
            let keys = keys
                .iter()
                .filter(|key| {
                    !inner
                        .unavailable_proofs
                        .lock()
                        .unwrap()
                        .contains(&(peer.id.clone(), (*key).clone()))
                })
                .cloned()
                .collect::<Vec<_>>();
            if keys.is_empty() {
                continue;
            }
            let mut stream = connect(inner, peer, started)?;
            write_header(
                &mut stream,
                &Request::Proofs {
                    doc: doc.into(),
                    keys: keys.to_vec(),
                },
            )?;
            let Reply::Proofs(nodes) = read_header(&mut stream)? else {
                return Err("Invalid proof reply".into());
            };
            if nodes.len() > keys.len() || nodes.iter().any(|r| !keys.contains(&r.key())) {
                return Err("Unrelated proof reply".into());
            }
            {
                let returned = nodes.iter().map(Revision::key).collect::<BTreeSet<_>>();
                let mut unavailable = inner.unavailable_proofs.lock().unwrap();
                for key in keys.iter().filter(|key| !returned.contains(*key)) {
                    if unavailable.len() >= 8192 {
                        unavailable.pop_first();
                    }
                    unavailable.insert((peer.id.clone(), key.clone()));
                }
            }
            budget = budget.saturating_sub(keys.len());
            if !nodes.is_empty() {
                advanced = true;
                inner.accept_proofs(doc, nodes, &peer.id, epoch)?;
            }
        }
        if !advanced {
            return Ok(false);
        }
    }
}
pub(super) fn push_proofs(
    inner: &Arc<Inner>,
    peer: &Peer,
    doc: &str,
    head: &Revision,
    started: Instant,
) -> Result<bool, String> {
    let ordered = {
        let s = inner.state.lock().unwrap();
        if !inner.allowed(&s, doc, &peer.id) {
            return Err("Document permission changed".into());
        }
        let mut pending = vec![(head.key(), false)];
        let mut seen = BTreeSet::new();
        let mut ordered = vec![];
        while let Some((key, done)) = pending.pop() {
            if done {
                ordered.push(key);
                continue;
            }
            if !seen.insert(key.clone()) {
                continue;
            }
            if seen.len() > crate::security::limits().proofs as usize {
                return Err("Proof traversal exceeds its quota".into());
            }
            let r = s
                .proofs
                .get(&key)
                .ok_or("Signed dependency unavailable; restoration is required")?;
            pending.push((key, true));
            for id in crate::revision::dependencies(r).into_iter().rev() {
                pending.push((id, false));
            }
        }
        ordered
    };
    let mut budget = 512usize;
    for keys in ordered.chunks(512) {
        let mut stream = connect(inner, peer, started)?;
        write_header(
            &mut stream,
            &Request::HaveProofs {
                doc: doc.into(),
                keys: keys.to_vec(),
            },
        )?;
        let Reply::Known(known) = read_header(&mut stream)? else {
            return Err("Invalid proof receipt reply".into());
        };
        if known.iter().any(|id| !keys.contains(id)) {
            return Err("Unrelated proof receipt".into());
        }
        let missing = keys
            .iter()
            .filter(|id| !known.contains(id))
            .cloned()
            .collect::<Vec<_>>();
        for batch in missing.chunks(32) {
            if batch.len() > budget {
                return Ok(false);
            }
            let nodes = {
                let s = inner.state.lock().unwrap();
                batch
                    .iter()
                    .map(|id| s.proofs.get(id).cloned().ok_or("Proof unavailable"))
                    .collect::<Result<Vec<_>, _>>()?
            };
            let mut stream = connect(inner, peer, started)?;
            write_header(
                &mut stream,
                &Request::ProofPush {
                    doc: doc.into(),
                    nodes,
                },
            )?;
            if !matches!(read_header(&mut stream)?, Reply::Done) {
                return Err("Proof transfer was not acknowledged".into());
            }
            budget -= batch.len();
        }
    }
    Ok(true)
}
