//! Document failures are isolated from device connectivity and other documents.
use super::*;
pub(super) fn get_body(
    inner: &Arc<Inner>,
    peer: &Peer,
    doc: &str,
    revision: &Revision,
    started: Instant,
    epoch: u64,
) -> Result<bool, String> {
    let path = inner.content_path(&revision.hash)?;
    if path.is_file() {
        // Authenticate before claiming an existing blob is usable. Damaged data
        // is retained for recovery instead of silently replacing its bytes.
        if crate::limits::read(&path, MAX)
            .is_ok_and(|body| hash(&body) == revision.hash && body.len() as u64 == revision.bytes)
        {
            completed(inner, peer, doc, revision, epoch)?;
            return Ok(true);
        }
        crate::traffic::sync(
            doc,
            "Local snapshot failed verification; requesting an authenticated replacement",
            &revision.key(),
            &revision.author,
        );
    }
    let mut stream = connect(inner, peer, started)?;
    write_header(
        &mut stream,
        &Request::Get {
            doc: doc.into(),
            key: revision.key(),
        },
    )?;
    match read_header(&mut stream)? {
        Reply::Missing => {
            let mut missing = inner.unavailable_bodies.lock().unwrap();
            if missing.len() >= 8192 {
                missing.pop_first();
            }
            missing.insert((peer.id.clone(), revision.key()));
            Ok(false)
        }
        Reply::Data {
            revision: received,
            len,
        } => {
            if !same(&received, revision) {
                return Err("Requested history body does not match its signed commit".into());
            }
            let _guard = read_body(inner, &mut stream, &received, len)?;
            completed(inner, peer, doc, &received, epoch)?;
            Ok(true)
        }
        _ => Err("Unexpected history body reply".into()),
    }
}
fn completed(
    inner: &Arc<Inner>,
    peer: &Peer,
    doc: &str,
    revision: &Revision,
    epoch: u64,
) -> Result<(), String> {
    let may_activate = {
        let s = inner.state.lock().unwrap();
        !s.revoked.contains(&revision.author)
            && s.docs
                .get(doc)
                .is_some_and(|d| !d.departed.contains(&revision.author))
    };
    if may_activate {
        inner.receive(doc, revision.clone(), &peer.id, epoch)?;
    }
    let mut s = inner.state.lock().unwrap();
    if inner.network_epoch.load(Ordering::Relaxed) != epoch || !inner.allowed(&s, doc, &peer.id) {
        return Err("History permission changed during transfer".into());
    }
    s.want_bodies.remove(&revision.key());
    inner.save(&s)?;
    if !may_activate {
        crate::traffic::sync(
            doc,
            "Historical contents restored without reactivating a revoked or departed author",
            &revision.key(),
            &revision.author,
        );
    }
    inner.event(Event::Updated(doc.into()));
    Ok(())
}
pub(super) fn document(
    inner: &Arc<Inner>,
    peer: &Peer,
    doc: &str,
    remote: &[(String, Revision)],
    started: Instant,
    epoch: u64,
) -> Result<(), String> {
    let mut waiting = false;
    // Resume chains learned from a different peer as well as that peer's heads.
    let pending = {
        let s = inner.state.lock().unwrap();
        s.unverified.values().find(|r| r.doc == doc).cloned()
    };
    if let Some(r) = pending {
        if !fetch_proofs(inner, peer, doc, &r, started, epoch)? {
            waiting = true;
        }
    }
    for (_, r) in remote.iter().filter(|(id, _)| id == doc) {
        if !fetch_proofs(inner, peer, doc, r, started, epoch)? {
            waiting = true;
            continue;
        }
        inner.confirmed(&peer.id, doc, r);
        let dominated = {
            let s = inner.state.lock().unwrap();
            tree::heads(&s.docs[doc])
                .iter()
                .any(|local| tree::descendant(&s, local, r))
        };
        if (!dominated || !inner.content_path(&r.hash)?.is_file())
            && !get_body(inner, peer, doc, r, started, epoch)?
        {
            waiting = true;
        }
    }
    let local = {
        let s = inner.state.lock().unwrap();
        tree::heads(&s.docs[doc])
    };
    if remote.iter().any(|(id, _)| id == doc) {
        for r in local {
            let received = {
                let s = inner.state.lock().unwrap();
                remote.iter().filter(|(id, _)| id == doc).any(|(_, head)| {
                    s.proofs.contains_key(&head.key()) && tree::descendant(&s, head, &r)
                })
            };
            if received {
                continue;
            }
            if !push_proofs(inner, peer, doc, &r, started)? {
                waiting = true;
                continue;
            }
            let mut stream = connect(inner, peer, started)?;
            write_header(
                &mut stream,
                &Request::Put {
                    doc: doc.into(),
                    revision: r.clone(),
                    offer: None,
                    len: r.bytes,
                },
            )?;
            if matches!(read_header(&mut stream)?, Reply::Ready) {
                send_body(inner, &mut stream, &r.hash, doc, &peer.id, epoch)?;
                if matches!(read_header(&mut stream)?, Reply::Done) {
                    inner.confirmed(&peer.id, doc, &r);
                }
            } else {
                waiting = true;
            }
        }
    }
    // Historic snapshots are retained and replicated in bounded increments.
    // Foreground heads above always take priority over this background backlog.
    let (historic, history_pending) = {
        let state = inner.state.lock().unwrap();
        let missing = inner.unavailable_bodies.lock().unwrap();
        let pending = state
            .want_bodies
            .iter()
            .filter_map(|id| state.proofs.get(id))
            .filter(|r| r.doc == doc)
            .collect::<Vec<_>>();
        let available = pending
            .iter()
            .filter(|r| !missing.contains(&(peer.id.clone(), r.key())))
            .copied()
            .collect::<Vec<_>>();
        let mut cursors = inner.body_cursor.lock().unwrap();
        let old = cursors.get(&(peer.id.clone(), doc.into()));
        let offset = old
            .and_then(|key| available.iter().position(|r| r.key() > *key))
            .unwrap_or(0);
        let selected = available
            .iter()
            .cycle()
            .skip(offset)
            .take(available.len().min(2))
            .map(|r| (*r).clone())
            .collect::<Vec<_>>();
        if let Some(last) = selected.last() {
            cursors.insert((peer.id.clone(), doc.into()), last.key());
        }
        (selected, !pending.is_empty())
    };
    for r in historic {
        if started.elapsed() > Duration::from_secs(35) {
            break;
        }
        if get_body(inner, peer, doc, &r, started, epoch)? {
            let mut s = inner.state.lock().unwrap();
            s.want_bodies.remove(&r.key());
            inner.save(&s)?;
        }
    }
    inner.merge_identical(doc)?;
    let issue = {
        let mut s = inner.state.lock().unwrap();
        let outstanding = s.bases.values().any(|b| b.doc == doc);
        let issue = if outstanding {
            s.docs[doc].issue.clone().or(Some("waiting_base".into()))
        } else if waiting {
            Some("waiting_history".into())
        } else if history_pending {
            Some("history_pending".into())
        } else {
            None
        };
        let changed = s.docs[doc].issue != issue;
        s.docs.get_mut(doc).unwrap().issue = issue.clone();
        if changed {
            inner.save(&s)?;
            Some(issue.unwrap_or_default())
        } else {
            None
        }
    };
    if let Some(code) = issue {
        inner.event(Event::DocumentStatus(doc.into(), code));
    }
    Ok(())
}
pub(super) fn report(inner: &Inner, doc: &str, error: &str) {
    let code = if error.to_lowercase().contains("quota") {
        "quota"
    } else if error.to_lowercase().contains("authentication")
        || error.to_lowercase().contains("signature")
    {
        "integrity"
    } else if error.to_lowercase().contains("parent") || error.to_lowercase().contains("proof") {
        "waiting_proofs"
    } else {
        "network_wait"
    };
    let mut s = inner.state.lock().unwrap();
    let changed = s
        .docs
        .get(doc)
        .is_some_and(|d| d.issue.as_deref() != Some(code));
    if !changed {
        return;
    }
    if let Some(d) = s.docs.get_mut(doc) {
        d.issue = Some(code.into());
    }
    let _ = inner.save(&s);
    drop(s);
    if changed {
        crate::diagnostics::failure("sharing", "document_sync_paused", error);
        crate::traffic::sync(doc, &format!("Document sync paused: {code}"), "", "");
        inner.event(Event::DocumentStatus(doc.into(), code.into()));
    }
}
