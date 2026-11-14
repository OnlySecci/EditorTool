//! Missing bases are a recoverable document state, never a reason to invent ancestry.
use super::*;
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct BaseLookup {
    pub doc: String,
    pub key: Option<String>,
    pub clock: Clock,
    pub hash: String,
    #[serde(default)]
    pub checked: BTreeSet<String>,
}
pub(super) fn request_base(s: &mut State, b: &Binding) -> Result<(), String> {
    if s.bases.len() >= 1024 {
        return Err("Too many pending recovery requests; existing contents are preserved".into());
    }
    let token = hash(&serde_json::to_vec(&(&b.doc, &b.revision, &b.clock, &b.hash)).map_err(err)?);
    s.bases.entry(token).or_insert(BaseLookup {
        doc: b.doc.clone(),
        key: b.revision.clone(),
        clock: b.clock.clone(),
        hash: b.hash.clone(),
        checked: BTreeSet::new(),
    });
    if let Some(d) = s.docs.get_mut(&b.doc) {
        d.issue = Some("waiting_base".into());
    }
    Ok(())
}
pub(super) fn all_checked(s: &State, b: &Binding, own: &str) -> bool {
    s.bases
        .values()
        .find(|r| r.doc == b.doc && r.key == b.revision && r.hash == b.hash && r.clock == b.clock)
        .is_some_and(|request| {
            s.docs.get(&b.doc).is_some_and(|d| {
                d.members
                    .iter()
                    .filter(|p| {
                        p.id != own && !d.departed.contains(&p.id) && !s.revoked.contains(&p.id)
                    })
                    .all(|p| request.checked.contains(&p.id))
            })
        })
}
pub(super) fn cancel_base(s: &mut State, b: &Binding) {
    s.bases.retain(|_, r| {
        !(r.doc == b.doc && r.key == b.revision && r.hash == b.hash && r.clock == b.clock)
    });
}
pub(super) fn restore_state(root: &std::path::Path, s: &mut State) -> Result<(), String> {
    if s.protocol >= crate::revision::PROTOCOL {
        return Ok(());
    }
    let backup = root.join("recovery-v155");
    crate::private_fs::directory(&backup).map_err(err)?;
    let archive = backup.join("sharing-original.json");
    if !archive.exists() {
        // This is encrypted by the profile vault and never uploaded to GitHub.
        let original = root.join("sharing.json");
        let bytes = if original.exists() {
            crate::limits::read(&original, 32 * 1024 * 1024).map_err(err)?
        } else {
            serde_json::to_vec(s).map_err(err)?
        };
        session::atomic_write(&archive, &bytes).map_err(err)?;
    }
    let mut candidates = std::mem::take(&mut s.proofs);
    candidates.append(&mut s.unverified);
    for d in s.docs.values() {
        for r in std::iter::once(&d.head)
            .chain(&d.conflicts)
            .chain(&d.history)
        {
            if !r.signature.is_empty() {
                candidates.entry(r.key()).or_insert_with(|| r.clone());
            }
        }
    }
    // Restore immutable records where V1.5.3 mutated a displayed vector clock.
    for d in s.docs.values_mut() {
        for r in std::iter::once(&mut d.head)
            .chain(&mut d.conflicts)
            .chain(&mut d.history)
        {
            if let Some(original) = candidates.values().find(|p| {
                p.doc == d.id
                    && p.hash == r.hash
                    && p.signature == r.signature
                    && crate::revision::verify(p, &d.id, &d.members).is_ok()
            }) {
                *r = original.clone();
            }
        }
        if d.root.is_empty() || !candidates.contains_key(&d.root) {
            let roots = candidates
                .values()
                .filter(|r| {
                    r.doc == d.id
                        && r.parents.is_empty()
                        && r.commit
                            .as_ref()
                            .is_none_or(|c| c.kind == crate::revision::Kind::Root)
                        && crate::revision::verify(r, &d.id, &d.members).is_ok()
                })
                .collect::<Vec<_>>();
            if roots.len() == 1 {
                d.root = roots[0].key();
            }
        }
    }
    s.unverified = candidates;
    for id in s.docs.keys().cloned().collect::<Vec<_>>() {
        promote(s, &id)?;
        let root_key = s.docs[&id].root.clone();
        if !s.proofs.contains_key(&root_key) {
            let d = s.docs.get_mut(&id).unwrap();
            d.active = false;
            d.issue = Some("new_history_required".into());
            continue;
        }
        // Reconstruct content tips using parents, including branches that a
        // previous force rank hid. Missing bodies remain recoverable, not deleted.
        let original_heads = std::iter::once(s.docs[&id].head.key())
            .chain(s.docs[&id].conflicts.iter().map(Revision::key))
            .collect::<BTreeSet<_>>();
        let all = s
            .proofs
            .values()
            .filter(|r| {
                r.doc == id
                    && ((!s.revoked.contains(&r.author)
                        && !s.docs[&id].departed.contains(&r.author))
                        || original_heads.contains(&r.key()))
            })
            .cloned()
            .collect::<Vec<_>>();
        let ancestors = all
            .iter()
            .flat_map(|r| tree::content_parents(s, r))
            .collect::<BTreeSet<_>>();
        let mut tips = all
            .into_iter()
            .filter(|r| !ancestors.contains(&r.key()))
            .collect::<Vec<_>>();
        tips.sort_by_key(Revision::key);
        let d = s.docs.get_mut(&id).unwrap();
        if let Some(index) = tips.iter().position(|r| same(r, &d.head)) {
            d.head = tips.remove(index);
        } else if !tips.is_empty() {
            d.head = tips.remove(0);
        }
        d.conflicts = tips;
        d.issue = s
            .unverified
            .values()
            .any(|r| r.doc == id)
            .then(|| "waiting_proofs".into());
    }
    s.protocol = crate::revision::PROTOCOL;
    crate::diagnostics::event(
        "sharing",
        "v155_signed_graph_migration_preserved_original_state",
    );
    Ok(())
}
pub(super) fn promote(s: &mut State, doc: &str) -> Result<bool, String> {
    let Some(d) = s.docs.get(doc) else {
        return Ok(false);
    };
    let members = d.members.clone();
    let mut waits = BTreeMap::<String, usize>::new();
    let mut children = BTreeMap::<String, Vec<String>>::new();
    let mut ready = std::collections::VecDeque::new();
    for (key, r) in s.unverified.iter().filter(|(_, r)| r.doc == doc) {
        let missing = crate::revision::dependencies(r)
            .into_iter()
            .filter(|p| !s.proofs.contains_key(p))
            .collect::<BTreeSet<_>>();
        waits.insert(key.clone(), missing.len());
        if missing.is_empty() {
            ready.push_back(key.clone());
        }
        for p in missing {
            children.entry(p).or_default().push(key.clone());
        }
    }
    let mut changed = false;
    while let Some(key) = ready.pop_front() {
        let Some(r) = s.unverified.get(&key) else {
            continue;
        };
        if validate_revision(r).is_err()
            || crate::revision::verify(r, doc, &members).is_err()
            || !tree::root_allowed(&s.docs[doc], r)
            || crate::revision::causal(r, &s.proofs).is_err()
            || tree::check_counter(s, r).is_err()
        {
            let invalid = s.unverified.remove(&key).unwrap();
            s.quarantine.insert(key, invalid);
            continue;
        }
        let r = s.unverified.remove(&key).unwrap();
        s.proofs.insert(key.clone(), r);
        changed = true;
        for child in children.get(&key).into_iter().flatten() {
            if let Some(n) = waits.get_mut(child) {
                *n = n.saturating_sub(1);
                if *n == 0 {
                    ready.push_back(child.clone());
                }
            }
        }
    }
    Ok(changed)
}
pub(super) fn lookup_round(
    inner: &Arc<Inner>,
    peer: &Peer,
    started: Instant,
    epoch: u64,
) -> Result<(), String> {
    let tickets = {
        let s = inner.state.lock().unwrap();
        s.bases
            .iter()
            .filter(|(_, r)| inner.allowed(&s, &r.doc, &peer.id) && !r.checked.contains(&peer.id))
            .take(4)
            .map(|(id, r)| (id.clone(), r.clone()))
            .collect::<Vec<_>>()
    };
    for (ticket, request) in tickets {
        let mut stream = connect(inner, peer, started)?;
        write_header(
            &mut stream,
            &Request::LookupBase {
                doc: request.doc.clone(),
                key: request.key.clone(),
                clock: request.clock.clone(),
                hash: request.hash.clone(),
            },
        )?;
        let Reply::Base(found) = read_header(&mut stream)? else {
            return Err("Invalid base lookup reply".into());
        };
        if let Some(revision) = found {
            if revision.doc != request.doc
                || revision.hash != request.hash
                || request
                    .key
                    .as_ref()
                    .map_or(revision.clock != request.clock, |key| {
                        &revision.key() != key
                    })
            {
                return Err("Base lookup returned an unrelated revision".into());
            }
            if !fetch_proofs(inner, peer, &request.doc, &revision, started, epoch)? {
                continue;
            }
            let mut s = inner.state.lock().unwrap();
            s.bases.remove(&ticket);
            if let Some(d) = s.docs.get_mut(&request.doc) {
                d.issue = None;
            }
            inner.save(&s)?;
            drop(s);
            crate::traffic::sync(
                &request.doc,
                "Missing signed base restored from peer",
                &revision.key(),
                &revision.author,
            );
            inner.event(Event::BaseReady(request.doc));
        } else {
            let mut s = inner.state.lock().unwrap();
            if let Some(p) = s.bases.get_mut(&ticket) {
                p.checked.insert(peer.id.clone());
            }
            let waiting = s.docs.get(&request.doc).is_some_and(|d| {
                d.members.iter().any(|p| {
                    p.id != inner.id
                        && !d.departed.contains(&p.id)
                        && !s.revoked.contains(&p.id)
                        && !s
                            .bases
                            .get(&ticket)
                            .is_some_and(|r| r.checked.contains(&p.id))
                })
            });
            if let Some(d) = s.docs.get_mut(&request.doc) {
                d.issue = Some(
                    if waiting {
                        "waiting_base"
                    } else {
                        "recover_branch_available"
                    }
                    .into(),
                );
            }
            inner.save(&s)?;
            drop(s);
            inner.event(Event::DocumentStatus(
                request.doc.clone(),
                if waiting {
                    "waiting_base"
                } else {
                    "recover_branch_available"
                }
                .into(),
            ));
            if !waiting {
                inner.event(Event::BaseReady(request.doc.clone()));
            }
        }
    }
    Ok(())
}
