//! Signed document provenance and verifiable causal parents; no clock-only trust.
use crate::peer::{Clock, Peer, Revision};
use std::collections::{BTreeMap, BTreeSet};

pub const PROTOCOL: u16 = 154;
/// Logical force order, independent of device clocks. The signed origin chooses
/// an existing parent; normal descendants retain its exact marker.
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
            "Unsigned or invalid document revision; compatible V1.5.4 peers are required".into(),
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
    for id in &r.parents {
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
