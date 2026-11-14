//! Content ancestry is independent of author-counter provenance and wall clocks.
use super::*;
use crate::revision::{Commit, Kind};

pub(super) fn descendant(s: &State, newer: &Revision, older: &Revision) -> bool {
    if !dominates(&newer.clock, &older.clock) {
        return false;
    }
    let target = older.key();
    let mut pending = vec![newer.key()];
    let mut seen = BTreeSet::new();
    while let Some(key) = pending.pop() {
        if key == target {
            return true;
        }
        if !seen.insert(key.clone()) {
            continue;
        }
        if seen.len() > crate::security::limits().proofs as usize {
            return false;
        }
        if let Some(r) = s.proofs.get(&key) {
            pending.extend(content_parents(s, r));
        }
    }
    false
}
pub(super) fn content_parents(s: &State, r: &Revision) -> Vec<String> {
    // V1.5.4 attached an extra own-author proof to ordinary forced-generation
    // edits. That was counter provenance, not an explicit content merge.
    if r.commit.is_none() && r.force.is_some() && r.parents.len() > 1 {
        let inherited = r
            .parents
            .iter()
            .filter_map(|key| s.proofs.get(key)?.force.as_ref())
            .max();
        if r.force.as_ref() == inherited {
            return vec![r.parents[0].clone()];
        }
    }
    r.parents.clone()
}
pub(super) fn root_allowed(d: &Document, r: &Revision) -> bool {
    if !r.parents.is_empty() {
        return true;
    }
    if let Some(c) = &r.commit {
        if c.kind == Kind::Recovered {
            return c.anchor.as_deref() == Some(&d.root);
        }
    }
    r.key() == d.root
}
pub(super) fn heads(d: &Document) -> Vec<Revision> {
    std::iter::once(d.head.clone())
        .chain(d.conflicts.clone())
        .collect()
}
pub(super) fn identical(d: &Document) -> bool {
    d.conflicts
        .iter()
        .all(|r| r.hash == d.head.hash && r.bytes == d.head.bytes)
}
pub(super) fn make(
    s: &State,
    own: &str,
    doc: &Document,
    mut parents: Vec<Revision>,
    hash: String,
    bytes: u64,
    mut commit: Commit,
) -> Result<Revision, String> {
    parents.sort_by_key(Revision::key);
    parents.dedup_by_key(|r| r.key());
    if parents.len() > 64 {
        return Err("Resolve at most 64 branches at once; other branches remain preserved".into());
    }
    let mut dependencies = parents.clone();
    if commit.kind == Kind::Recovered {
        let anchor = s
            .proofs
            .get(&doc.root)
            .ok_or("The approved root is unavailable; repair shared history first")?;
        commit.anchor = Some(doc.root.clone());
        dependencies.push(anchor.clone());
    }
    if let Some(previous) = s
        .proofs
        .values()
        .filter(|r| r.doc == doc.id && r.author == own)
        .max_by_key(|r| r.clock.get(own).copied().unwrap_or(0))
    {
        let known = dependencies
            .iter()
            .map(|r| r.clock.get(own).copied().unwrap_or(0))
            .max()
            .unwrap_or(0);
        if previous.clock.get(own).copied().unwrap_or(0) > known {
            commit.counter_parent = Some(previous.key());
            dependencies.push(previous.clone());
        }
    }
    Ok(Revision {
        commit: Some(commit),
        force: None,
        clock: parent_clock(&dependencies, own)?,
        hash,
        bytes,
        author: own.into(),
        name: s.config.name.clone(),
        doc: doc.id.clone(),
        group: crate::revision::group(&doc.members),
        parents: parents.iter().map(Revision::key).collect(),
        signature: String::new(),
    })
}
pub(super) fn integrate(s: &mut State, doc: &str, revision: Revision) -> Result<bool, String> {
    let d = s.docs.get(doc).ok_or("Document is not shared")?;
    let previous = heads(d);
    if previous.iter().any(|r| descendant(s, r, &revision)) {
        return Ok(false);
    }
    let mut remaining = previous
        .iter()
        .filter(|r| !descendant(s, &revision, r))
        .cloned()
        .collect::<Vec<_>>();
    remaining.push(revision);
    remaining.sort_by_key(Revision::key);
    remaining.dedup_by_key(|r| r.key());
    let old_key = d.head.key();
    // Keep the currently displayed tip when there is a real conflict. The
    // representative is not a winner; every incomparable tip remains visible.
    let index = remaining
        .iter()
        .position(|r| r.key() == old_key)
        .unwrap_or(0);
    let representative = remaining.remove(index);
    let d = s.docs.get_mut(doc).unwrap();
    for r in previous {
        if r.key() != representative.key()
            && !remaining.iter().any(|p| same(p, &r))
            && !d.history.iter().any(|p| same(p, &r))
        {
            d.history.push(r);
        }
    }
    d.head = representative;
    d.conflicts = remaining;
    // The full history remains in the immutable proof/content stores. This is
    // only the small recent-history summary used by the document status model.
    if d.history.len() > 20 {
        d.history.drain(..d.history.len() - 20);
    }
    d.issue = None;
    Ok(true)
}

impl Inner {
    pub(super) fn tree_receive(
        &self,
        doc: &str,
        revision: Revision,
        source: &str,
        epoch: u64,
    ) -> Result<(), String> {
        validate_revision(&revision)?;
        let mut s = self.state.lock().unwrap();
        if self.network_epoch.load(Ordering::Relaxed) != epoch || !self.allowed(&s, doc, source) {
            return Err("Document permission changed; commit refused".into());
        }
        let d = s.docs.get(doc).ok_or("Unknown document")?;
        if s.revoked.contains(&revision.author) || d.departed.contains(&revision.author) {
            return Err("Commit author is revoked or has left this document".into());
        }
        crate::revision::verify(&revision, doc, &d.members)?;
        if !root_allowed(d, &revision) {
            return Err("Unapproved document root".into());
        }
        crate::revision::causal(&revision, &s.proofs)?;
        check_counter(&s, &revision)?;
        if s.proofs.len() + s.unverified.len() + s.quarantine.len()
            >= crate::security::limits().proofs as usize
            && !s.proofs.contains_key(&revision.key())
        {
            return Err("Signed history quota reached; no history was deleted".into());
        }
        let resolution = revision
            .commit
            .as_ref()
            .is_some_and(|c| c.kind == Kind::Resolution);
        s.proofs.insert(revision.key(), revision.clone());
        let changed = integrate(&mut s, doc, revision.clone())?;
        self.save(&s)?;
        let conflict = s
            .docs
            .get(doc)
            .is_some_and(|d| !d.conflicts.is_empty() && !identical(d));
        drop(s);
        if changed {
            crate::traffic::sync(
                doc,
                if resolution {
                    "User main decision committed"
                } else if conflict {
                    "Independent content branches preserved"
                } else {
                    "Content descendant accepted"
                },
                &revision.key(),
                &revision.author,
            );
            self.event(if conflict {
                Event::Conflict(doc.into())
            } else {
                Event::Updated(doc.into())
            });
            let _ = self.wake.send(());
            self.merge_identical(doc)?;
        }
        Ok(())
    }
    pub(super) fn merge_identical(&self, doc: &str) -> Result<(), String> {
        let _writer = self.author_lock.lock().unwrap();
        let revision = {
            let s = self.state.lock().unwrap();
            let Some(d) = s.docs.get(doc) else {
                return Ok(());
            };
            if d.conflicts.is_empty() || !identical(d) || !self.allowed(&s, doc, &self.id) {
                return Ok(());
            }
            if !self.content_path(&d.head.hash)?.is_file() {
                return Ok(());
            }
            // A stable signer avoids an endless race of simultaneous automatic
            // merge commits. Any subsequent edit can also join identical tips.
            let signer = d
                .members
                .iter()
                .filter(|p| !d.departed.contains(&p.id) && !s.revoked.contains(&p.id))
                .map(|p| p.id.as_str())
                .min();
            if signer != Some(&self.id) {
                return Ok(());
            }
            let parents = heads(d).into_iter().take(64).collect();
            let mut r = make(
                &s,
                &self.id,
                d,
                parents,
                d.head.hash.clone(),
                d.head.bytes,
                Commit::new(Kind::IdenticalMerge),
            )?;
            crate::revision::sign(&mut r, &self.key)?;
            r
        };
        let mut s = self.state.lock().unwrap();
        if !self.allowed(&s, doc, &self.id) {
            return Err("Document permission changed before identical merge".into());
        }
        crate::revision::causal(&revision, &s.proofs)?;
        check_counter(&s, &revision)?;
        if s.proofs.len() + s.unverified.len() + s.quarantine.len()
            >= crate::security::limits().proofs as usize
        {
            return Err("Signed history quota reached".into());
        }
        s.proofs.insert(revision.key(), revision.clone());
        integrate(&mut s, doc, revision.clone())?;
        self.save(&s)?;
        drop(s);
        crate::traffic::sync(
            doc,
            "Identical contents joined automatically",
            &revision.key(),
            &revision.author,
        );
        self.event(Event::Updated(doc.into()));
        Ok(())
    }
}
pub(super) fn check_counter(s: &State, r: &Revision) -> Result<(), String> {
    if s.proofs.values().any(|p| {
        p.doc == r.doc
            && p.author == r.author
            && p.clock.get(&r.author) == r.clock.get(&r.author)
            && p.key() != r.key()
    }) {
        return Err(
            "Signed author-counter fork detected; both stored contents remain preserved".into(),
        );
    }
    Ok(())
}

impl Service {
    pub fn base_revision(&self, binding: &Binding) -> Option<Revision> {
        let s = self.inner.state.lock().unwrap();
        exact_base(&s, binding)
    }
    pub fn tree_publish(
        &self,
        binding: &Binding,
        data: &str,
        local_key: &str,
    ) -> Result<Option<Revision>, String> {
        let _writer = self.inner.author_lock.lock().unwrap();
        let (hash, _reservation) = self.inner.put_content(&binding.doc, data)?;
        let mut s = self.inner.state.lock().unwrap();
        if !s.drafts.contains_key(local_key) && s.drafts.len() >= 1024 {
            return Err("Pending draft quota reached; save local text before continuing".into());
        }
        s.drafts.insert(
            local_key.into(),
            Draft {
                key: local_key.into(),
                base: binding.clone(),
                hash: hash.clone(),
                bytes: data.len() as u64,
            },
        );
        self.inner.save(&s)?;
        let Some(d) = s.docs.get(&binding.doc) else {
            return Ok(None);
        };
        if !d.active || !s.proofs.contains_key(&d.root) {
            return Ok(None);
        }
        let Some(base) = exact_base(&s, binding) else {
            repair::request_base(&mut s, binding)?;
            if repair::all_checked(&s, binding, &self.inner.id) {
                let d = &s.docs[&binding.doc];
                let mut r = make(
                    &s,
                    &self.inner.id,
                    d,
                    vec![],
                    hash,
                    data.len() as u64,
                    Commit::new(Kind::Recovered),
                )?;
                crate::revision::sign(&mut r, &self.inner.key)?;
                repair::cancel_base(&mut s, binding);
                self.inner.save(&s)?;
                drop(s);
                self.commit_local(&binding.doc, r.clone())?;
                self.ack_draft(local_key)?;
                return Ok(Some(r));
            }
            self.inner.save(&s)?;
            drop(s);
            self.wake();
            return Ok(None);
        };
        if base.hash == hash {
            let restored = s
                .proofs
                .values()
                .filter(|r| r.hash == hash)
                .map(Revision::key)
                .collect::<BTreeSet<_>>();
            s.want_bodies.retain(|key| !restored.contains(key));
            if let Some(d) = s.docs.get_mut(&binding.doc) {
                if d.head.hash == hash && d.issue.as_deref() == Some("integrity") {
                    d.issue = None;
                }
            }
            s.drafts.remove(local_key);
            self.inner.save(&s)?;
            return Ok(Some(base));
        }
        let d = &s.docs[&binding.doc];
        let mut parents = vec![base.clone()];
        if identical(d) && d.head.hash == base.hash {
            parents.extend(heads(d));
        }
        let mut revision = make(
            &s,
            &self.inner.id,
            d,
            parents,
            hash,
            data.len() as u64,
            Commit::new(Kind::Edit),
        )?;
        crate::revision::sign(&mut revision, &self.inner.key)?;
        drop(s);
        // Local commits are durable even while networking is disabled.
        self.commit_local(&binding.doc, revision.clone())?;
        self.ack_draft(local_key)?;
        Ok(Some(revision))
    }
    pub(super) fn commit_local(&self, doc: &str, revision: Revision) -> Result<(), String> {
        let mut s = self.inner.state.lock().unwrap();
        let d = s.docs.get(doc).ok_or("Unknown document")?;
        if d.departed.contains(&self.inner.id) {
            return Err("This device has left the document".into());
        }
        if s.revoked.contains(&revision.author) || d.departed.contains(&revision.author) {
            return Err("Commit author is revoked or has left the document".into());
        }
        crate::revision::verify(&revision, doc, &d.members)?;
        if !root_allowed(d, &revision) {
            return Err("Unapproved document root".into());
        }
        crate::revision::causal(&revision, &s.proofs)?;
        check_counter(&s, &revision)?;
        if s.proofs.len() + s.unverified.len() + s.quarantine.len()
            >= crate::security::limits().proofs as usize
        {
            return Err("Signed history quota reached; existing branches are preserved".into());
        }
        s.proofs.insert(revision.key(), revision.clone());
        integrate(&mut s, doc, revision.clone())?;
        self.inner.save(&s)?;
        let conflict = s
            .docs
            .get(doc)
            .is_some_and(|d| !d.conflicts.is_empty() && !identical(d));
        drop(s);
        crate::traffic::sync(
            doc,
            "Local tree commit stored",
            &revision.key(),
            &revision.author,
        );
        self.inner.event(if conflict {
            Event::Conflict(doc.into())
        } else {
            Event::Updated(doc.into())
        });
        self.wake();
        Ok(())
    }
    pub fn resolve_main(
        &self,
        doc: &str,
        chosen: Option<&str>,
        expected: &[String],
        edited: Option<&str>,
    ) -> Result<Option<Revision>, String> {
        let _writer = self.inner.author_lock.lock().unwrap();
        let _reservation;
        let edited_body = if let Some(data) = edited {
            let (hash, guard) = self.inner.put_content(doc, data)?;
            _reservation = Some(guard);
            Some((hash, data.len() as u64))
        } else {
            _reservation = None;
            None
        };
        let (mut pending, selected, body_hash, bytes) = {
            let s = self.inner.state.lock().unwrap();
            let d = s.docs.get(doc).ok_or("Document is missing")?;
            if !s.proofs.contains_key(&d.root) {
                return Err("Repair this history before choosing a common main version".into());
            }
            let mut current = heads(d);
            if current.iter().map(Revision::key).collect::<BTreeSet<_>>()
                != expected.iter().cloned().collect()
            {
                return Ok(None);
            }
            let (selected, body_hash, bytes) = if let Some((hash, bytes)) = edited_body {
                (None, hash, bytes)
            } else {
                let r = chosen
                    .and_then(|key| s.proofs.get(key))
                    .filter(|r| r.doc == doc)
                    .ok_or("Select a verified version")?;
                let body = self.revision_body(r)?;
                if body.len() as u64 != r.bytes {
                    return Err("Restore the selected contents before making them main".into());
                }
                if !current.iter().any(|p| same(p, r)) {
                    current.push(r.clone());
                }
                (Some(r.clone()), r.hash.clone(), r.bytes)
            };
            current.sort_by_key(Revision::key);
            current.dedup_by_key(|r| r.key());
            (
                std::collections::VecDeque::from(current),
                selected,
                body_hash,
                bytes,
            )
        };
        let mut previous: Option<Revision> = None;
        // Cover every head from the confirmed snapshot with bounded merge
        // commits. Heads that arrive later are deliberately not added here.
        while !pending.is_empty() {
            let mut parents = vec![];
            let mut commit = Commit::new(Kind::Resolution);
            if let Some(r) = &previous {
                parents.push(r.clone());
                commit.chosen = Some(r.key());
            } else if let Some(r) = &selected {
                parents.push(r.clone());
                commit.chosen = Some(r.key());
                pending.retain(|p| !same(p, r));
            }
            while parents.len() < 64 {
                let Some(r) = pending.pop_front() else {
                    break;
                };
                parents.push(r);
            }
            let revision = {
                let s = self.inner.state.lock().unwrap();
                let d = s.docs.get(doc).ok_or("Document is missing")?;
                let mut r = make(
                    &s,
                    &self.inner.id,
                    d,
                    parents,
                    body_hash.clone(),
                    bytes,
                    commit,
                )?;
                crate::revision::sign(&mut r, &self.inner.key)?;
                r
            };
            self.commit_local(doc,revision.clone()).map_err(|e|format!("Main decision paused; completed commits and remaining branches are preserved: {e}"))?;
            previous = Some(revision);
        }
        if let Some(r) = &previous {
            crate::traffic::sync(
                doc,
                "User main decision stored; all confirmed branches joined",
                &r.key(),
                &r.author,
            );
        }
        Ok(previous)
    }
    pub fn recover_draft(&self, binding: &Binding, data: &str) -> Result<Revision, String> {
        let _writer = self.inner.author_lock.lock().unwrap();
        let (hash, _reservation) = self.inner.put_content(&binding.doc, data)?;
        let revision = {
            let s = self.inner.state.lock().unwrap();
            let d = s
                .docs
                .get(&binding.doc)
                .ok_or("Shared history is missing; start a new approved history")?;
            if !s.proofs.contains_key(&d.root) {
                return Err("No verified root exists; start a new approved history".into());
            }
            let mut r = make(
                &s,
                &self.inner.id,
                d,
                vec![],
                hash,
                data.len() as u64,
                Commit::new(Kind::Recovered),
            )?;
            crate::revision::sign(&mut r, &self.inner.key)?;
            r
        };
        self.commit_local(&binding.doc, revision.clone())?;
        crate::traffic::sync(
            &binding.doc,
            "Unattached local contents recovered as an independent branch",
            &revision.key(),
            &revision.author,
        );
        Ok(revision)
    }
    pub fn is_verified(&self, r: &Revision) -> bool {
        self.inner
            .state
            .lock()
            .unwrap()
            .proofs
            .contains_key(&r.key())
    }
    pub fn versions(&self, doc: &str) -> Vec<Revision> {
        let s = self.inner.state.lock().unwrap();
        let mut versions = s
            .proofs
            .values()
            .filter(|r| r.doc == doc)
            .cloned()
            .collect::<Vec<_>>();
        for r in s.quarantine.values().filter(|r| r.doc == doc) {
            if !versions.iter().any(|p| same(p, r)) {
                versions.push(r.clone());
            }
        }
        if let Some(d) = s.docs.get(doc) {
            for r in heads(d).into_iter().chain(d.history.clone()) {
                if !versions.iter().any(|p| same(p, &r)) {
                    versions.push(r);
                }
            }
        }
        let mut older = BTreeSet::new();
        let mut next = vec![doc.to_string()];
        while let Some(id) = next.pop() {
            if let Some(old) = s
                .docs
                .get(&id)
                .and_then(|d| s.proofs.get(&d.root))
                .and_then(|r| r.commit.as_ref())
                .and_then(|c| c.replaces.as_ref())
            {
                if older.insert(old.clone()) {
                    next.push(old.clone());
                }
            }
        }
        let mut known = versions.iter().map(Revision::key).collect::<BTreeSet<_>>();
        for r in s
            .proofs
            .values()
            .chain(s.quarantine.values())
            .chain(s.unverified.values())
            .filter(|r| older.contains(&r.doc))
        {
            if known.insert(r.key()) {
                versions.push(r.clone());
            }
        }
        for d in s.docs.values().filter(|d| older.contains(&d.id)) {
            for r in heads(d).into_iter().chain(d.history.clone()) {
                if known.insert(r.key()) {
                    versions.push(r);
                }
            }
        }
        versions
    }
    pub fn drafts(&self) -> Vec<Draft> {
        self.inner
            .state
            .lock()
            .unwrap()
            .drafts
            .values()
            .cloned()
            .collect()
    }
    pub fn ack_draft(&self, key: &str) -> Result<(), String> {
        self.ack_drafts(&[key.into()])
    }
    pub fn ack_drafts(&self, keys: &[String]) -> Result<(), String> {
        let mut s = self.inner.state.lock().unwrap();
        for key in keys {
            s.drafts.remove(key);
        }
        self.inner.save(&s)
    }
    pub fn queue_base(&self, b: &Binding) -> Result<(), String> {
        let mut s = self.inner.state.lock().unwrap();
        if s.docs
            .get(&b.doc)
            .is_none_or(|d| !s.proofs.contains_key(&d.root))
        {
            return Ok(());
        }
        repair::request_base(&mut s, b)?;
        self.inner.save(&s)?;
        drop(s);
        self.wake();
        Ok(())
    }
    pub fn note_error(&self, doc: &str, error: &str) {
        transport::report(&self.inner, doc, error);
    }
    pub fn advances(&self, main: &Revision, base: &Revision) -> bool {
        descendant(&self.inner.state.lock().unwrap(), main, base)
    }
    pub fn needs_new_history(&self, doc: &str) -> bool {
        let s = self.inner.state.lock().unwrap();
        s.docs.get(doc).is_none_or(|d| {
            !s.proofs.contains_key(&d.root)
                || !d.members.iter().any(|p| p.id == self.inner.id)
                || d.departed.contains(&self.inner.id)
        })
    }
    pub fn own_replacements(&self) -> Vec<Offer> {
        let s = self.inner.state.lock().unwrap();
        s.docs
            .values()
            .filter(|d| !d.departed.contains(&self.inner.id))
            .filter_map(|d| {
                let r = s.proofs.get(&d.root)?;
                let _old = r.commit.as_ref()?.replaces.as_ref()?;
                (r.author == self.inner.id).then(|| Offer {
                    id: format!("relink-{}", r.key()),
                    doc: d.id.clone(),
                    title: d.title.clone(),
                    sync: true,
                    revision: r.clone(),
                    members: d.members.clone(),
                    len: r.bytes,
                })
            })
            .collect()
    }
    pub fn require_body(&self, r: &Revision) -> Result<(), String> {
        let mut s = self.inner.state.lock().unwrap();
        s.want_bodies.insert(r.key());
        self.inner.save(&s)?;
        drop(s);
        self.wake();
        Ok(())
    }
    pub fn needs_cached_body(&self, b: &Binding) -> bool {
        let s = self.inner.state.lock().unwrap();
        s.docs.get(&b.doc).is_some_and(|d| d.head.hash == b.hash)
            && s.want_bodies
                .iter()
                .filter_map(|key| s.proofs.get(key))
                .any(|r| r.doc == b.doc && r.hash == b.hash)
    }
    pub fn has_body(&self, r: &Revision) -> bool {
        self.inner
            .content_path(&r.hash)
            .is_ok_and(|path| path.is_file())
    }
    pub fn revision_body(&self, r: &Revision) -> Result<String, String> {
        let body = self.body(&r.hash)?;
        if body.len() as u64 != r.bytes && !r.signature.is_empty()
            || hash(body.as_bytes()) != r.hash
        {
            return Err(
                "Version contents do not match the stored hash; recovery is required".into(),
            );
        }
        Ok(body)
    }
    pub fn register_base(&self, b: &Binding, data: &str) -> Result<(), String> {
        let r = self.base_revision(b).ok_or("Base restoration is pending")?;
        if hash(data.as_bytes()) != r.hash {
            return Err("Local contents differ from the verified base".into());
        }
        let (_, _guard) = self.inner.put_content(&b.doc, data)?;
        self.commit_local(&b.doc, r)
    }
}
pub(super) fn exact_base(s: &State, binding: &Binding) -> Option<Revision> {
    if let Some(key) = &binding.revision {
        return s
            .proofs
            .get(key)
            .filter(|r| r.doc == binding.doc && r.hash == binding.hash)
            .cloned();
    }
    let mut matches = s
        .proofs
        .values()
        .filter(|r| r.doc == binding.doc && r.hash == binding.hash && r.clock == binding.clock);
    let first = matches.next()?.clone();
    if matches.next().is_some() {
        None
    } else {
        Some(first)
    }
}
