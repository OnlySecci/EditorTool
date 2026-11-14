//! Signed document provenance and verifiable causal parents; no clock-only trust.
use crate::peer::{Clock, Peer, Revision};
use std::collections::{BTreeMap, BTreeSet};

pub const PROTOCOL: u16 = 155;
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum Kind {
    Root,
    Edit,
    IdenticalMerge,
    Resolution,
    Recovered,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Commit {
    pub kind: Kind,
    pub counter_parent: Option<String>,
    pub chosen: Option<String>,
    pub anchor: Option<String>,
    pub replaces: Option<String>,
}
impl Commit {
    pub fn new(kind: Kind) -> Self {
        Self {
            kind,
            counter_parent: None,
            chosen: None,
            anchor: None,
            replaces: None,
        }
    }
}
pub fn dependencies(r: &Revision) -> Vec<String> {
    let mut keys = r.parents.clone();
    if let Some(c) = &r.commit {
        for key in [&c.counter_parent, &c.anchor].into_iter().flatten() {
            if !keys.contains(key) {
                keys.push(key.clone());
            }
        }
    }
    keys
}
/// Historical V1.5.4 signature payload only; never a V1.5.5 winner rule.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct ForceMain {
    pub generation: u64,
    pub author: String,
    pub nonce: String,
    pub selected: String,
}
pub fn group(members: &[Peer]) -> String {
    let mut ids = members.iter().map(|p| p.id.as_str()).collect::<Vec<_>>();
    ids.sort();
    crate::peer::hash(&serde_json::to_vec(&ids).expect("serializable group"))
}
pub fn payload(r: &Revision) -> Vec<u8> {
    if let Some(commit) = &r.commit {
        return serde_json::to_vec(&(
            "EditorTool tree commit v1",
            &r.doc,
            &r.group,
            &r.hash,
            r.bytes,
            &r.clock,
            &r.author,
            &r.name,
            &r.parents,
            commit,
        ))
        .expect("serializable tree commit");
    }
    if let Some(force) = &r.force {
        return serde_json::to_vec(&(
            "EditorTool force main revision v1",
            &r.doc,
            &r.group,
            &r.hash,
            r.bytes,
            &r.clock,
            &r.author,
            &r.name,
            &r.parents,
            force,
        ))
        .expect("serializable forced revision");
    }
    serde_json::to_vec(&(
        "EditorTool signed revision v1",
        &r.doc,
        &r.group,
        &r.hash,
        r.bytes,
        &r.clock,
        &r.author,
        &r.name,
        &r.parents,
    ))
    .expect("serializable revision")
}
pub fn sign(r: &mut Revision, key: &[u8]) -> Result<(), String> {
    r.signature = crate::identity::signature(key, &payload(r))?;
    Ok(())
}
pub fn verify(r: &Revision, doc: &str, members: &[Peer]) -> Result<(), String> {
    if r.doc != doc
        || doc.len() != 64
        || !doc.bytes().all(|b| b.is_ascii_hexdigit())
        || r.group != group(members)
        || r.signature.is_empty()
        || r.parents.len() > 64
        || r.bytes > crate::limits::DOCUMENT
    {
        return Err(
            "Unsigned or invalid document revision; compatible V1.5.5 peers are required".into(),
        );
    }
    if members.iter().map(|p| &p.id).collect::<BTreeSet<_>>().len() != members.len() {
        return Err("Duplicate document member".into());
    }
    let author = members
        .iter()
        .find(|p| p.id == r.author)
        .ok_or("Revision author is not a document member")?;
    let cert = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &author.cert)
        .map_err(|_| "Invalid author certificate")?;
    if crate::peer::hash(&cert) != r.author {
        return Err("Author fingerprint mismatch".into());
    }
    crate::identity::verify(&cert, &payload(r), &r.signature)
}
pub fn causal(r: &Revision, parents: &BTreeMap<String, Revision>) -> Result<(), String> {
    let mut expected = Clock::new();
    let mut seen = BTreeSet::new();
    for id in &dependencies(r) {
        if !seen.insert(id) {
            return Err("Duplicate revision parent".into());
        }
        let p = parents
            .get(id)
            .ok_or("Revision parent unavailable; keep the conflict or share a fresh group")?;
        if p.doc != r.doc || p.group != r.group || p.signature.is_empty() {
            return Err("Invalid revision parent".into());
        }
        for (author, n) in &p.clock {
            let value = expected.entry(author.clone()).or_default();
            *value = (*value).max(*n);
        }
    }
    let n = expected.entry(r.author.clone()).or_default();
    *n = n.checked_add(1).ok_or("Revision counter overflow")?;
    if expected != r.clock {
        return Err("Unverifiable revision counters; update refused".into());
    }
    if let Some(c) = &r.commit {
        if r.force.is_some()
            || c.replaces
                .as_ref()
                .is_some_and(|s| s.is_empty() || s.len() > 128)
        {
            return Err("Invalid tree commit metadata".into());
        }
        if let Some(id) = &c.counter_parent {
            if parents.get(id).is_none_or(|p| p.author != r.author) {
                return Err("Counter provenance belongs to another author".into());
            }
        }
        match c.kind {
            Kind::Root
                if r.parents.is_empty()
                    && c.counter_parent.is_none()
                    && c.anchor.is_none()
                    && c.chosen.is_none() =>
            {
                ()
            }
            Kind::Edit
                if !r.parents.is_empty()
                    && c.anchor.is_none()
                    && c.chosen.is_none()
                    && c.replaces.is_none() =>
            {
                let first = &parents[&r.parents[0]];
                if r.parents
                    .iter()
                    .any(|id| parents[id].hash != first.hash || parents[id].bytes != first.bytes)
                {
                    return Err("An edit cannot silently join different contents".into());
                }
            }
            Kind::Recovered
                if r.parents.is_empty()
                    && c.anchor.is_some()
                    && c.chosen.is_none()
                    && c.replaces.is_none() =>
            {
                ()
            }
            Kind::IdenticalMerge
                if r.parents.len() >= 2
                    && c.anchor.is_none()
                    && c.chosen.is_none()
                    && c.replaces.is_none() =>
            {
                if r.parents.iter().any(|id| {
                    parents
                        .get(id)
                        .is_none_or(|p| p.hash != r.hash || p.bytes != r.bytes)
                }) {
                    return Err("Identical merge changed its parent contents".into());
                }
            }
            Kind::Resolution
                if !r.parents.is_empty() && c.anchor.is_none() && c.replaces.is_none() =>
            {
                if let Some(id) = &c.chosen {
                    if !r.parents.contains(id)
                        || parents
                            .get(id)
                            .is_none_or(|p| p.hash != r.hash || p.bytes != r.bytes)
                    {
                        return Err("Resolution does not match the selected parent".into());
                    }
                }
            }
            _ => return Err("Invalid content-parent relationships".into()),
        }
        return Ok(());
    }
    let previous = r
        .parents
        .iter()
        .filter_map(|id| parents.get(id)?.force.as_ref())
        .max();
    if r.force.as_ref() != previous {
        let force = r
            .force
            .as_ref()
            .ok_or("A revision cannot discard force-main authority")?;
        let generation = previous
            .map_or(0, |f| f.generation)
            .checked_add(1)
            .ok_or("Force-main generation overflow")?;
        let selected = parents
            .get(&force.selected)
            .filter(|_| r.parents.contains(&force.selected))
            .ok_or("Force-main choice is not a signed parent")?;
        if r.parents.is_empty()
            || force.generation != generation
            || force.author != r.author
            || force.nonce.len() != 64
            || !force.nonce.bytes().all(|b| b.is_ascii_hexdigit())
            || selected.hash != r.hash
            || selected.bytes != r.bytes
        {
            return Err("Invalid signed force-main decision".into());
        }
    }
    Ok(())
}
