//! Opt-in private-network sharing. Pinned mutual TLS, immutable content blobs,
//! explicit offers, persistent outbox and causal (not timestamp) version order.
use crate::{session, Action, Message};
#[path = "peer_security.rs"]
mod protection;
use base64::{engine::general_purpose::STANDARD, Engine};
use protection::{fetch_proofs, push_proofs};
use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, PrivatePkcs8KeyDer, ServerName, UnixTime},
    server::danger::{ClientCertVerified, ClientCertVerifier},
    DigitallySignedStruct, DistinguishedName, SignatureScheme,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    net::{IpAddr, SocketAddr, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
pub const PORT: u16 = 51473;
const MAX: u64 = 64 * 1024 * 1024;
const HEADER: usize = 512 * 1024;
// Includes handshake, headers and body. A trickle cannot reset the deadline.
struct DeadlineSocket {
    socket: TcpStream,
    started: Instant,
    epoch: Arc<AtomicU64>,
    generation: u64,
}
impl DeadlineSocket {
    fn new(socket: TcpStream, epoch: Arc<AtomicU64>) -> Self {
        let generation = epoch.load(Ordering::Relaxed);
        Self {
            socket,
            started: Instant::now(),
            epoch,
            generation,
        }
    }
    fn remaining(&self) -> std::io::Result<Duration> {
        if self.epoch.load(Ordering::Relaxed) != self.generation {
            return Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionAborted,
                "Network settings changed.",
            ));
        }
        Duration::from_secs(45)
            .checked_sub(self.started.elapsed())
            .filter(|d| !d.is_zero())
            .map(|d| d.min(Duration::from_secs(10)))
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::TimedOut, "Sync total deadline reached.")
            })
    }
}
impl std::ops::Deref for DeadlineSocket {
    type Target = TcpStream;
    fn deref(&self) -> &TcpStream {
        &self.socket
    }
}
impl Read for DeadlineSocket {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.socket.set_read_timeout(Some(self.remaining()?))?;
        self.socket.read(bytes)
    }
}
impl Write for DeadlineSocket {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.socket.set_write_timeout(Some(self.remaining()?))?;
        self.socket.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.remaining()?;
        self.socket.flush()
    }
}
struct TransferPermit {
    inner: Arc<Inner>,
    ip: IpAddr,
}
impl Drop for TransferPermit {
    fn drop(&mut self) {
        self.inner.transfers.fetch_sub(1, Ordering::Relaxed);
        if let Ok(mut entries) = self.inner.admissions.lock() {
            if let Some(entry) = entries.get_mut(&self.ip) {
                entry.0 = entry.0.saturating_sub(1);
            }
        }
    }
}
pub type Clock = BTreeMap<String, u64>;
pub fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub fn private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            v.is_private()
                || v.is_loopback()
                || v.is_link_local()
                || (v.octets()[0] == 100 && (64..128).contains(&v.octets()[1]))
        }
        IpAddr::V6(v) => v
            .to_ipv4_mapped()
            .map(|v| private(v.into()))
            .unwrap_or(v.is_loopback() || v.is_unique_local() || v.is_unicast_link_local()),
    }
}
pub fn address(host: &str, port: u16) -> Result<SocketAddr, String> {
    let ip: IpAddr = host
        .trim()
        .trim_matches(['[', ']'])
        .parse()
        .map_err(|_| "Enter a private IPv4 or IPv6 address.")?;
    if !private(ip) || port == 0 {
        return Err("Only private networks/VPN and ports from 1 to 65535 are allowed.".into());
    }
    Ok(SocketAddr::new(ip, port))
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub cert: String,
    #[serde(default)]
    pub protocol: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub enabled: bool,
    pub name: String,
    pub port: u16,
    pub clients: Vec<Peer>,
    #[serde(default = "automatic_interfaces")]
    pub automatic_interfaces: bool,
    #[serde(default)]
    pub interfaces: Vec<crate::network::Binding>,
}
fn automatic_interfaces() -> bool {
    true
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            name: std::env::var("COMPUTERNAME")
                .or_else(|_| std::env::var("HOSTNAME"))
                .unwrap_or("EditorTool".into()),
            port: PORT,
            clients: vec![],
            automatic_interfaces: true,
            interfaces: vec![],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Revision {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub force: Option<crate::revision::ForceMain>,
    pub clock: Clock,
    pub hash: String,
    pub author: String,
    pub name: String,
    #[serde(default)]
    pub doc: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub parents: Vec<String>,
    #[serde(default)]
    pub signature: String,
}
impl Revision {
    pub fn key(&self) -> String {
        if self.signature.is_empty() {
            hash(format!("{}{:?}", self.hash, self.clock).as_bytes())
        } else {
            hash(&crate::revision::payload(self))
        }
    }
}
fn dominates(a: &Clock, b: &Clock) -> bool {
    b.iter()
        .all(|(id, n)| a.get(id).copied().unwrap_or(0) >= *n)
}
fn same(a: &Revision, b: &Revision) -> bool {
    a.key() == b.key()
}
fn supersedes(a: &Revision, b: &Revision) -> bool {
    if a.force != b.force {
        return a.force > b.force;
    }
    dominates(&a.clock, &b.clock) && a.clock != b.clock
}
fn include_local_provenance(s: &State, doc: &str, author: &str, parents: &mut Vec<Revision>) {
    // A forced winner may not know this device's discarded counters. Retain a
    // signed causal parent so subsequent edits never reuse an author counter.
    let own = s
        .proofs
        .values()
        .filter(|r| r.doc == doc && r.author == author)
        .max_by_key(|r| r.clock.get(author).copied().unwrap_or(0));
    let authority = s
        .proofs
        .values()
        .filter(|r| {
            r.doc == doc
                && !s.revoked.contains(&r.author)
                && s.docs
                    .get(doc)
                    .is_some_and(|d| !d.departed.contains(&r.author))
        })
        .max_by(|a, b| a.force.cmp(&b.force));
    let own = own.filter(|r| {
        r.clock.get(author).copied().unwrap_or(0)
            > parents
                .iter()
                .map(|p| p.clock.get(author).copied().unwrap_or(0))
                .max()
                .unwrap_or(0)
    });
    let authority = authority
        .filter(|r| r.force.as_ref() > parents.iter().filter_map(|p| p.force.as_ref()).max());
    for r in [own, authority].into_iter().flatten() {
        if !parents.iter().any(|p| p.key() == r.key()) {
            parents.push(r.clone());
        }
    }
}
fn parent_clock(parents: &[Revision], author: &str) -> Result<Clock, String> {
    let mut clock = Clock::new();
    for r in parents {
        for (id, n) in &r.clock {
            let value = clock.entry(id.clone()).or_default();
            *value = (*value).max(*n);
        }
    }
    let n = clock.entry(author.into()).or_default();
    *n = n.checked_add(1).ok_or("Revision counter is full")?;
    Ok(clock)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub title: String,
    pub head: Revision,
    pub conflicts: Vec<Revision>,
    pub history: Vec<Revision>,
    pub members: Vec<Peer>,
    pub active: bool,
    #[serde(default)]
    pub departed: BTreeSet<String>,
    #[serde(default)]
    pub root: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Offer {
    pub id: String,
    pub doc: String,
    pub title: String,
    pub sync: bool,
    pub revision: Revision,
    pub members: Vec<Peer>,
    pub len: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Acceptance {
    pub accepted: bool,
    pub path: Option<PathBuf>,
    pub autosave: bool,
    #[serde(default)]
    pub approved_target: Option<PathBuf>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Outbox {
    target: String,
    offer: Offer,
}
#[derive(Default, Serialize, Deserialize)]
struct State {
    #[serde(default)]
    protocol: u16,
    #[serde(default)]
    revoked: BTreeSet<String>,
    #[serde(default)]
    proofs: BTreeMap<String, Revision>,
    #[serde(default)]
    locked_clients: BTreeSet<String>,
    #[serde(default)]
    departures: BTreeMap<String, BTreeSet<String>>,
    #[serde(default)]
    denied: BTreeSet<String>,
    #[serde(default)]
    leaves: Vec<(String, String)>,
    #[serde(default)]
    inbox: BTreeMap<String, (Offer, Acceptance)>,
    #[serde(default)]
    config: Config,
    #[serde(default)]
    docs: BTreeMap<String, Document>,
    #[serde(default)]
    offers: BTreeMap<String, Offer>,
    #[serde(default)]
    decisions: BTreeMap<String, Acceptance>,
    #[serde(default)]
    outbox: Vec<Outbox>,
    #[serde(default)]
    completed: BTreeMap<String, String>,
}
#[derive(Clone, Debug)]
pub enum Event {
    Changed,
    Checked,
    Disconnected(String),
    Left(String, String, String),
    Trust(Peer),
    Offer(Offer),
    Received(Offer, Acceptance),
    Updated(String),
    Conflict(String),
    Notice(String),
    Fatal(String),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Binding {
    #[serde(default)]
    pub force: Option<crate::revision::ForceMain>,
    pub doc: String,
    pub clock: Clock,
    pub hash: String,
    pub autosave: bool,
    #[serde(default)]
    pub approved_target: Option<PathBuf>,
    #[serde(default)]
    pub disk_hash: Option<String>,
}
#[derive(Default)]
pub struct Progress {
    pub enabled: bool,
    pub conflict: bool,
    pub clients: usize,
    pub online: usize,
    pub confirmed: usize,
    pub offers: usize,
}
#[derive(Serialize, Deserialize)]
enum Request {
    Leave {
        doc: String,
    },
    Hello {
        info: Peer,
    },
    Offer(Offer),
    Put {
        doc: String,
        revision: Revision,
        offer: Option<String>,
        len: u64,
    },
    Proofs {
        doc: String,
        keys: Vec<String>,
    },
    ProofPush {
        doc: String,
        nodes: Vec<Revision>,
    },
    Get {
        doc: String,
        key: String,
    },
}
#[derive(Serialize, Deserialize)]
enum Reply {
    Hello {
        info: Peer,
        heads: Vec<(String, Revision)>,
    },
    Proofs(Vec<Revision>),
    Pending,
    Accepted,
    Rejected,
    Ready,
    Done,
    Data {
        revision: Revision,
        len: u64,
    },
    Error(String),
}
trait WireLabel {
    fn label(&self) -> &'static str;
}
impl WireLabel for Request {
    fn label(&self) -> &'static str {
        match self {
            Self::Hello { .. } => "request Hello",
            Self::Leave { .. } => "request Leave",
            Self::Offer(_) => "request Offer",
            Self::Put { .. } => "request Put",
            Self::Get { .. } => "request Get",
            Self::Proofs { .. } => "request Proofs",
            Self::ProofPush { .. } => "request ProofPush",
        }
    }
}
impl WireLabel for Reply {
    fn label(&self) -> &'static str {
        match self {
            Self::Hello { .. } => "reply Hello",
            Self::Pending => "reply Pending",
            Self::Proofs(_) => "reply Proofs",
            Self::Accepted => "reply Accepted",
            Self::Rejected => "reply Rejected",
            Self::Ready => "reply Ready",
            Self::Done => "reply Done",
            Self::Data { .. } => "reply Data",
            Self::Error(_) => "reply Error",
        }
    }
}
struct Listener {
    running: Arc<AtomicBool>,
    wake: Arc<mio::Waker>,
    worker: thread::JoinHandle<()>,
}
pub struct Service {
    inner: Arc<Inner>,
    wake: mpsc::Sender<()>,
    listener: Option<Listener>,
}
struct Inner {
    state: Mutex<State>,
    root: PathBuf,
    cert: Vec<u8>,
    key: zeroize::Zeroizing<Vec<u8>>,
    id: String,
    tx: fltk::app::Sender<Message>,
    enabled: AtomicBool,
    stop: AtomicBool,
    network_epoch: Arc<AtomicU64>,
    transfers: AtomicUsize,
    seen: Mutex<BTreeSet<String>>,
    admissions: Mutex<BTreeMap<IpAddr, (usize, u8, Instant)>>,
    statuses: Mutex<BTreeMap<String, bool>>,
    checked: Mutex<BTreeMap<String, u64>>,
    acknowledgements: Mutex<BTreeMap<(String, String), String>>,
    wake: mpsc::Sender<()>,
}
impl std::fmt::Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EditorTool peer service")
    }
}
impl Inner {
    fn confirmed(&self, peer: &str, doc: &str, revision: &Revision) {
        if !self
            .state
            .lock()
            .unwrap()
            .docs
            .get(doc)
            .is_some_and(|d| d.members.iter().any(|m| m.id == peer))
        {
            return;
        }
        let key = (doc.to_string(), peer.to_string());
        let value = revision.key();
        let changed = self
            .acknowledgements
            .lock()
            .unwrap()
            .insert(key, value.clone())
            != Some(value);
        if changed {
            if let Some(force) = &revision.force {
                crate::traffic::force(
                    doc,
                    &format!(
                        "Force sync main revision acknowledged by peer: generation {}",
                        force.generation
                    ),
                    force,
                    0,
                );
            }
            self.event(Event::Changed);
        }
    }
    fn event(&self, event: Event) {
        let label = match &event {
            Event::Changed | Event::Checked => None,
            Event::Disconnected(_) => Some("peer_disconnected"),
            Event::Trust(_) => Some("trust_request"),
            Event::Offer(_) => Some("file_offer"),
            Event::Received(_, _) => Some("file_received"),
            Event::Updated(_) => Some("revision_received"),
            Event::Conflict(_) => Some("version_conflict"),
            Event::Left(_, _, _) => Some("sync_left"),
            Event::Notice(_) => Some("notice"),
            Event::Fatal(_) => Some("storage_failure_service_disabled"),
        };
        if let Some(label) = label {
            crate::diagnostics::event("sharing", label);
        }
        self.tx.send((0, Action::PeerEvent(event)));
    }
    fn save(&self, s: &State) -> Result<(), String> {
        let bytes = serde_json::to_vec(s).map_err(err)?;
        if bytes.len() > 32 * 1024 * 1024 {
            self.enabled.store(false, Ordering::Relaxed);
            self.network_epoch.fetch_add(1, Ordering::Relaxed);
            self.event(Event::Fatal(
                "Sharing metadata quota exceeded; last saved state preserved".into(),
            ));
            return Err("Sharing metadata quota exceeded".into());
        }
        let result = session::atomic_write(&self.root.join("sharing.json"), &bytes).map_err(err);
        if let Err(e) = &result {
            self.enabled.store(false, Ordering::Relaxed);
            self.event(Event::Fatal(e.clone()));
        }
        result
    }
    fn own(&self) -> Peer {
        let s = self.state.lock().unwrap();
        Peer {
            id: self.id.clone(),
            name: s.config.name.clone(),
            host: String::new(),
            port: s.config.port,
            cert: STANDARD.encode(&self.cert),
            protocol: crate::revision::PROTOCOL,
        }
    }
    fn content_path(&self, hash: &str) -> Result<PathBuf, String> {
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Invalid content identifier".into());
        }
        Ok(self.root.join("content").join(format!("{hash}.txt")))
    }
    fn put_content(
        &self,
        doc: &str,
        data: &str,
    ) -> Result<(String, crate::security::Reservation), String> {
        if data.len() as u64 > MAX {
            return Err("Document exceeds 64 MiB.".into());
        }
        crate::rich::validate_snapshot(data)?;
        let key = hash(data.as_bytes());
        let path = self.content_path(&key)?;
        // Reserve the full attribution even when another group already owns the
        // same CAS body; deduplication must not bypass document/author quotas.
        let reservation = self.reserve(doc, &self.id, data.len() as u64)?;
        if !path.exists() {
            session::atomic_write(&path, data.as_bytes()).map_err(err)?;
        }
        Ok((key, reservation))
    }
    fn trusted(&self, cert: &CertificateDer<'_>, host: &str, port: u16) -> bool {
        let id = hash(cert.as_ref());
        if cert.len() > 16384 || id == self.id {
            return false;
        }
        {
            let state = self.state.lock().unwrap();
            if state.revoked.contains(&id) {
                return false;
            }
            let matches = |p: &Peer| p.id == id && p.cert == STANDARD.encode(cert.as_ref());
            if state.config.clients.iter().any(matches)
                || state
                    .docs
                    .values()
                    .any(|d| self.allowed(&state, &d.id, &id) && d.members.iter().any(matches))
                || state.offers.values().any(|o| {
                    state.decisions.get(&o.id).is_some_and(|a| a.accepted)
                        && o.members.iter().any(matches)
                })
            {
                return true;
            }
        }
        let mut seen = self.seen.lock().unwrap();
        if seen.len() < 32 && seen.insert(id.clone()) {
            self.event(Event::Trust(Peer {
                id,
                name: host.into(),
                host: host.into(),
                port,
                cert: STANDARD.encode(cert.as_ref()),
                protocol: 0,
            }));
        }
        false
    }
    fn receive(
        &self,
        doc: &str,
        revision: Revision,
        source: &str,
        epoch: u64,
    ) -> Result<(), String> {
        validate_revision(&revision)?;
        let mut s = self.state.lock().unwrap();
        if self.network_epoch.load(Ordering::Relaxed) != epoch || !self.allowed(&s, doc, source) {
            return Err("Document permission changed; revision refused".into());
        }
        let document = s.docs.get(doc).ok_or("Unknown document")?;
        if s.revoked.contains(&revision.author) || document.departed.contains(&revision.author) {
            return Err("Revision author is revoked or has left this document".into());
        }
        crate::revision::verify(&revision, doc, &document.members)?;
        if revision.parents.is_empty() && revision.key() != document.root {
            return Err("Unapproved revision root".into());
        }
        crate::revision::causal(&revision, &s.proofs)?;
        if s.proofs.len() >= crate::security::limits().proofs as usize
            && !s.proofs.contains_key(&revision.key())
        {
            return Err(
                "Signed history quota reached; share a fresh group or increase its limit".into(),
            );
        }
        if s.proofs.values().any(|known| {
            known.doc == doc
                && known.author == revision.author
                && known.clock.get(&revision.author) == revision.clock.get(&revision.author)
                && known.key() != revision.key()
        }) {
            return Err("Signed device revision fork detected".into());
        }
        s.proofs.insert(revision.key(), revision.clone());
        let Some(d) = s.docs.get_mut(doc) else {
            return Err("Document is not shared.".into());
        };
        if same(&d.head, &revision) || d.conflicts.iter().any(|r| same(r, &revision)) {
            return Ok(());
        }
        if revision
            .clock
            .keys()
            .any(|id| !d.members.iter().any(|p| &p.id == id))
        {
            return Err("Unknown revision author.".into());
        }
        if std::iter::once(&d.head)
            .chain(&d.conflicts)
            .any(|r| supersedes(r, &revision))
        {
            if d.head.force > revision.force {
                crate::traffic::force(
                    doc,
                    "Force sync main: superseded revision ignored",
                    d.head.force.as_ref().unwrap(),
                    0,
                );
            }
            return Ok(());
        }
        let conflict;
        let mut applied_force = None;
        if std::iter::once(&d.head)
            .chain(&d.conflicts)
            .all(|r| supersedes(&revision, r))
        {
            if revision.force > d.head.force {
                applied_force = revision
                    .force
                    .clone()
                    .map(|marker| (marker, revision.bytes));
            }
            d.history.push(d.head.clone());
            d.history.append(&mut d.conflicts);
            d.head = revision;
            conflict = false;
        } else {
            d.conflicts.retain(|r| !supersedes(&revision, r));
            if d.conflicts.len() >= 32 {
                return Err("Too many unresolved revision conflicts.".into());
            }
            d.conflicts.push(revision);
            conflict = true;
        }
        if d.history.len() > 20 {
            d.history.drain(..d.history.len() - 20);
        }
        self.save(&s)?;
        drop(s);
        if let Some((marker, bytes)) = applied_force {
            crate::traffic::force(doc, "Force sync main applied", &marker, bytes);
        }
        self.event(if conflict {
            Event::Conflict(doc.into())
        } else {
            Event::Updated(doc.into())
        });
        let _ = self.wake.send(());
        Ok(())
    }
    fn allowed(&self, s: &State, doc: &str, peer: &str) -> bool {
        self.enabled.load(Ordering::Relaxed)
            && !s.revoked.contains(peer)
            && s.docs.get(doc).is_some_and(|d| {
                d.active
                    && !d.head.signature.is_empty()
                    && d.head.doc == d.id
                    && !d.root.is_empty()
                    && !d.departed.contains(peer)
                    && !d.departed.contains(&self.id)
                    && d.members.iter().any(|p| p.id == peer)
            })
    }
}
fn validate_revision(r: &Revision) -> Result<(), String> {
    if r.clock.is_empty()
        || r.clock.len() > 64
        || r.hash.len() != 64
        || !r.hash.bytes().all(|b| b.is_ascii_hexdigit())
        || r.name.len() > 256
        || !r.clock.contains_key(&r.author)
        || r.clock.iter().any(|(id, n)| {
            id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) || *n > u64::MAX - 1024
        })
    {
        return Err("Invalid revision data.".into());
    }
    Ok(())
}
fn peer_valid(p: &Peer) -> bool {
    p.name.len() <= 256
        && p.id.len() == 64
        && STANDARD
            .decode(&p.cert)
            .is_ok_and(|c| c.len() <= 16384 && hash(&c) == p.id)
        && address(&p.host, p.port).is_ok()
}
impl Service {
    pub fn new(root: PathBuf, tx: fltk::app::Sender<Message>) -> Result<Self, String> {
        if !crate::security::ready() {
            return Err("Security limits could not be loaded; repair settings and restart".into());
        }
        crate::private_fs::directory(&root).map_err(err)?;
        crate::private_fs::directory(&root.join("content")).map_err(err)?;
        for dir in [&root, &root.join("content")] {
            for entry in fs::read_dir(dir).map_err(err)? {
                let entry = entry.map_err(err)?;
                if crate::private_fs::is_link(&fs::symlink_metadata(entry.path()).map_err(err)?) {
                    return Err("Link in sharing storage rejected.".into());
                }
                if entry.file_type().map_err(err)?.is_file() {
                    crate::private_fs::restrict(&entry.path(), false).map_err(err)?;
                }
            }
        }
        let (cert, key) = crate::identity::load(&root)?;
        let mut state: State = if root.join("sharing.json").exists() {
            serde_json::from_slice(
                &crate::limits::read(&root.join("sharing.json"), 32 * 1024 * 1024).map_err(err)?,
            )
            .map_err(err)?
        } else {
            State::default()
        };
        if state.protocol > crate::revision::PROTOCOL {
            return Err("This profile uses a newer sync protocol; downgrade refused".into());
        }
        let legacy = state.protocol < 153;
        if legacy {
            for doc in state.docs.values_mut() {
                doc.active = false;
            }
            state.outbox.clear();
            state.offers.clear();
            state.decisions.clear();
            state.leaves.clear();
            for (_, acceptance) in state.inbox.values_mut() {
                acceptance.autosave = false;
            }
        }
        state.protocol = crate::revision::PROTOCOL;
        let (wake, rx) = mpsc::channel();
        let inner = Arc::new(Inner {
            state: Mutex::new(state),
            root,
            cert: cert.clone(),
            key,
            id: hash(&cert),
            tx,
            enabled: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            network_epoch: Arc::new(AtomicU64::new(0)),
            transfers: AtomicUsize::new(0),
            seen: Mutex::new(BTreeSet::new()),
            admissions: Mutex::new(BTreeMap::new()),
            statuses: Mutex::new(BTreeMap::new()),
            checked: Mutex::new(BTreeMap::new()),
            acknowledgements: Mutex::new(BTreeMap::new()),
            wake: wake.clone(),
        });
        {
            let state = inner.state.lock().unwrap();
            inner.save(&state)?;
        }
        if legacy {
            inner.event(Event::Notice("V1.5.3 protects revision provenance. Existing unsigned groups are paused; update all peers and share a fresh group. Documents and device identities are preserved.".into()));
        }
        let worker = inner.clone();
        thread::spawn(move || loop {
            let pending = {
                let s = worker.state.lock().unwrap();
                !s.outbox.is_empty() || !s.leaves.is_empty()
            };
            let _ = rx.recv_timeout(Duration::from_secs(
                if !worker.enabled.load(Ordering::Relaxed) {
                    3600
                } else if pending {
                    1
                } else {
                    10
                },
            ));
            while rx.try_recv().is_ok() {}
            if worker.stop.load(Ordering::Relaxed) {
                break;
            }
            if !worker.enabled.load(Ordering::Relaxed) {
                continue;
            }
            sweep(&worker);
        });
        let mut service = Self {
            inner,
            wake,
            listener: None,
        };
        let config = service.config();
        if let Err(error) = service.configure(config) {
            service.inner.enabled.store(false, Ordering::Relaxed);
            {
                let mut state = service.inner.state.lock().unwrap();
                state.config.enabled = false;
                service.inner.save(&state)?;
            }
            service.inner.event(Event::Notice(format!(
                "Send/Sync is paused. Check interface settings: {error}"
            )));
        }
        {
            let s = service.inner.state.lock().unwrap();
            for o in s
                .offers
                .values()
                .filter(|o| !s.decisions.contains_key(&o.id))
            {
                service.inner.event(Event::Offer(o.clone()));
            }
            for (o, a) in s.inbox.values() {
                service.inner.event(Event::Received(o.clone(), a.clone()));
            }
        }
        Ok(service)
    }
    pub fn config(&self) -> Config {
        self.inner.state.lock().unwrap().config.clone()
    }
    pub fn identity(&self) -> String {
        self.inner.id.clone()
    }
    pub fn permissions(&self) -> String {
        let state = self.inner.state.lock().unwrap();
        let mut text = format!(
            "Send/Sync: {} · Port {}\nDevice approvals: {} · Pending sends: {}\n",
            if state.config.enabled {
                "enabled"
            } else {
                "off"
            },
            state.config.port,
            state.config.clients.len(),
            state.outbox.len()
        );
        for peer in &state.config.clients {
            text.push_str(&format!(
                "\nDevice: {} · {}:{}\nFingerprint: {}\n",
                peer.name, peer.host, peer.port, peer.id
            ));
        }
        for doc in state.docs.values() {
            text.push_str(&format!(
                "\nDocument: {} · {}\n",
                doc.title,
                if doc.active {
                    "Sync aktiv"
                } else {
                    "Sync beendet"
                }
            ));
            for member in &doc.members {
                text.push_str(&format!(
                    "  {}: {}\n",
                    member.name,
                    if doc.departed.contains(&member.id) {
                        "abgemeldet"
                    } else {
                        "Participant — may synchronise changes"
                    }
                ));
            }
        }
        text
    }
    pub fn status(&self, p: &Peer) -> bool {
        if !self.inner.enabled.load(Ordering::Relaxed) {
            return false;
        }
        self.inner
            .statuses
            .lock()
            .unwrap()
            .get(&format!("{}:{}", p.host, p.port))
            .copied()
            .unwrap_or(false)
    }
    pub fn checked_at(&self, p: &Peer) -> u64 {
        self.inner
            .checked
            .lock()
            .unwrap()
            .get(&format!("{}:{}", p.host, p.port))
            .copied()
            .unwrap_or(0)
    }
    pub fn edit(
        &mut self,
        index: usize,
        name: String,
        host: String,
        port: u16,
    ) -> Result<(), String> {
        address(&host, port)?;
        if name.trim().is_empty() || name.len() > 128 {
            return Err("Client name must contain 1 to 128 characters.".into());
        }
        let mut c = self.config();
        let old = c.clients.get(index).ok_or("Select a client.")?.clone();
        if c.clients
            .iter()
            .enumerate()
            .any(|(i, p)| i != index && p.host == host && p.port == port)
        {
            return Err("Address and port already exist.".into());
        }
        c.clients[index].name = name.trim().into();
        c.clients[index].host = host;
        c.clients[index].port = port;
        let updated = c.clients[index].clone();
        {
            let mut s = self.inner.state.lock().unwrap();
            if !old.id.is_empty() {
                s.locked_clients.insert(old.id.clone());
            }
            for d in s.docs.values_mut() {
                for member in &mut d.members {
                    if !old.id.is_empty() && member.id == old.id {
                        *member = updated.clone();
                    }
                }
            }
            self.inner.save(&s)?;
        }
        self.inner
            .statuses
            .lock()
            .unwrap()
            .remove(&format!("{}:{}", old.host, old.port));
        self.inner
            .checked
            .lock()
            .unwrap()
            .remove(&format!("{}:{}", old.host, old.port));
        self.configure(c)
    }
    pub fn configure(&mut self, config: Config) -> Result<(), String> {
        if config.port == 0
            || config.name.trim().is_empty()
            || config.name.len() > 128
            || config.clients.len() > 32
        {
            return Err("Invalid device name, port or number of clients.".into());
        }
        if config.interfaces.len() > 64
            || (!config.automatic_interfaces && config.interfaces.is_empty())
        {
            return Err("Select at least one interface/IP or enable Automatic.".into());
        }
        self.inner.network_epoch.fetch_add(1, Ordering::Relaxed);
        let old = self.config();
        let restart = old.port != config.port
            || old.automatic_interfaces != config.automatic_interfaces
            || old.interfaces != config.interfaces
            || !config.enabled;
        if restart {
            self.stop_listener();
            self.inner.statuses.lock().unwrap().clear();
            self.inner.checked.lock().unwrap().clear();
        }
        {
            let mut s = self.inner.state.lock().unwrap();
            s.config = config.clone();
            self.inner.save(&s)?;
        }
        if config.enabled && self.listener.is_none() {
            let sockets = match crate::network::listeners(
                config.automatic_interfaces,
                &config.interfaces,
                config.port,
            ) {
                Ok(s) => s,
                Err(e) => {
                    let mut state = self.inner.state.lock().unwrap();
                    state.config.enabled = false;
                    self.inner.enabled.store(false, Ordering::Relaxed);
                    self.inner.save(&state)?;
                    return Err(format!("Could not open selected interfaces: {e}"));
                }
            };
            let mut poll = mio::Poll::new().map_err(err)?;
            let wake =
                Arc::new(mio::Waker::new(poll.registry(), mio::Token(usize::MAX)).map_err(err)?);
            let mut listeners = Vec::new();
            for (index, socket) in sockets.into_iter().enumerate() {
                let mut socket = mio::net::TcpListener::from_std(socket);
                poll.registry()
                    .register(&mut socket, mio::Token(index), mio::Interest::READABLE)
                    .map_err(err)?;
                listeners.push(socket);
            }
            let running = Arc::new(AtomicBool::new(true));
            let flag = running.clone();
            let inner = self.inner.clone();
            self.inner.enabled.store(true, Ordering::Relaxed);
            let worker = thread::spawn(move || {
                let mut events = mio::Events::with_capacity(128);
                while flag.load(Ordering::Relaxed) {
                    if let Err(error) = poll.poll(&mut events, None) {
                        if error.kind() == std::io::ErrorKind::Interrupted {
                            continue;
                        }
                        inner.event(Event::Notice("Network listener stopped.".into()));
                        break;
                    }
                    for event in events.iter() {
                        if event.token() == mio::Token(usize::MAX) {
                            continue;
                        }
                        let Some(listener) = listeners.get(event.token().0) else {
                            continue;
                        };
                        loop {
                            let generation = inner.network_epoch.load(Ordering::Relaxed);
                            let (stream, remote) = match listener.accept() {
                                Ok(value) => value,
                                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                                Err(_) => break,
                            };
                            if !flag.load(Ordering::Relaxed) {
                                break;
                            }
                            let stream: TcpStream = stream.into();
                            if stream.set_nonblocking(false).is_err() || !private(remote.ip()) {
                                continue;
                            }
                            if inner.transfers.fetch_add(1, Ordering::Relaxed) >= 2 {
                                inner.transfers.fetch_sub(1, Ordering::Relaxed);
                                continue;
                            }
                            let ip = remote.ip().to_canonical();
                            {
                                let mut admissions = inner.admissions.lock().unwrap();
                                admissions.retain(|_, (active, _, since)| {
                                    *active > 0 || since.elapsed() < Duration::from_secs(60)
                                });
                                if !admissions.contains_key(&ip) && admissions.len() >= 128 {
                                    inner.transfers.fetch_sub(1, Ordering::Relaxed);
                                    continue;
                                }
                                let entry = admissions.entry(ip).or_insert((0, 0, Instant::now()));
                                if entry.2.elapsed() >= Duration::from_secs(60) {
                                    entry.1 = 0;
                                    entry.2 = Instant::now();
                                }
                                if entry.0 > 0 || entry.1 >= 240 {
                                    inner.transfers.fetch_sub(1, Ordering::Relaxed);
                                    continue;
                                }
                                entry.0 += 1;
                                entry.1 += 1;
                            }
                            let inner = inner.clone();
                            thread::spawn(move || {
                                let _permit = TransferPermit {
                                    inner: inner.clone(),
                                    ip,
                                };
                                if let Err(error) = serve(&inner, stream, remote, generation) {
                                    crate::diagnostics::failure(
                                        "sharing",
                                        "incoming_failed",
                                        &error,
                                    );
                                    crate::traffic::failure(&error);
                                }
                            });
                        }
                    }
                }
            });
            self.listener = Some(Listener {
                running,
                wake,
                worker,
            });
        }
        self.inner.enabled.store(config.enabled, Ordering::Relaxed);
        if !config.enabled {
            self.inner.statuses.lock().unwrap().clear();
        }
        self.wake();
        self.inner.event(Event::Changed);
        Ok(())
    }
    pub fn stop_listener(&mut self) {
        self.inner.network_epoch.fetch_add(1, Ordering::Relaxed);
        self.inner.enabled.store(false, Ordering::Relaxed);
        if let Some(listener) = self.listener.take() {
            listener.running.store(false, Ordering::Relaxed);
            let _ = listener.wake.wake();
            let _ = listener.worker.join();
        }
    }
    pub fn disable_after_error(&mut self) {
        self.stop_listener();
        self.inner.state.lock().unwrap().config.enabled = false;
    }
    pub fn wake(&self) {
        let _ = self.wake.send(());
    }
    pub fn retry(&self) {
        self.inner.seen.lock().unwrap().clear();
        self.wake();
    }
    pub fn add(&mut self, name: String, host: String, port: u16) -> Result<(), String> {
        address(&host, port)?;
        let mut c = self.config();
        if c.clients.iter().any(|p| p.host == host && p.port == port) {
            return Err("Client already exists.".into());
        }
        c.clients.push(Peer {
            id: String::new(),
            name: if name.trim().is_empty() {
                host.clone()
            } else {
                name
            },
            host,
            port,
            cert: String::new(),
            protocol: 0,
        });
        self.configure(c)
    }
    pub fn remove(&mut self, index: usize) -> Result<(), String> {
        let mut c = self.config();
        if index < c.clients.len() {
            let removed = c.clients.remove(index);
            let mut s = self.inner.state.lock().unwrap();
            if !removed.id.is_empty() {
                s.revoked.insert(removed.id.clone());
            }
            s.outbox.retain(|o| o.target != removed.id);
            s.leaves.retain(|(_, target)| target != &removed.id);
            let offers: Vec<_> = s
                .offers
                .values()
                .filter(|o| o.revision.author == removed.id)
                .map(|o| o.id.clone())
                .collect();
            for id in offers {
                s.offers.remove(&id);
                s.decisions.remove(&id);
            }
            self.inner.save(&s)?;
        }
        self.configure(c)
    }
    pub fn trust(&mut self, mut peer: Peer, accept: bool) -> Result<(), String> {
        if !accept {
            return Ok(());
        }
        if !peer_valid(&peer) {
            return Err("Invalid device key or address.".into());
        }
        if self.inner.state.lock().unwrap().revoked.contains(&peer.id) {
            return Err(
                "This fingerprint is revoked. Unblock it explicitly in Security before pairing."
                    .into(),
            );
        }
        let mut c = self.config();
        if let Some(p) = c.clients.iter_mut().find(|p| {
            p.host == peer.host && (p.port == peer.port || p.cert.is_empty()) || p.id == peer.id
        }) {
            let mut state = self.inner.state.lock().unwrap();
            if p.cert.is_empty() || state.locked_clients.contains(&p.id) {
                peer.name = p.name.clone();
                peer.host = p.host.clone();
                peer.port = p.port;
                state.locked_clients.insert(peer.id.clone());
                self.inner.save(&state)?;
            }
            if !p.id.is_empty() && p.id != peer.id {
                self.inner
                    .state
                    .lock()
                    .unwrap()
                    .revoked
                    .insert(p.id.clone());
            }
            *p = peer;
        } else {
            c.clients.push(peer);
        }
        self.configure(c)
    }
    pub fn body(&self, hash: &str) -> Result<String, String> {
        crate::limits::read_text(&self.inner.content_path(hash)?).map_err(err)
    }
    pub fn document(&self, id: &str) -> Option<Document> {
        self.inner.state.lock().unwrap().docs.get(id).cloned()
    }
    pub fn progress(&self, id: &str) -> Progress {
        let s = self.inner.state.lock().unwrap();
        let Some(d) = s.docs.get(id) else {
            return Progress::default();
        };
        let mut p = Progress {
            enabled: self.inner.enabled.load(Ordering::Relaxed),
            conflict: !d.conflicts.is_empty(),
            offers: s
                .outbox
                .iter()
                .filter(|o| o.offer.sync && o.offer.doc == id)
                .count(),
            ..Default::default()
        };
        let acknowledgements = self.inner.acknowledgements.lock().unwrap();
        let statuses = self.inner.statuses.lock().unwrap();
        let key = d.head.key();
        for member in d
            .members
            .iter()
            .filter(|m| m.id != self.inner.id && !d.departed.contains(&m.id))
        {
            p.clients += 1;
            let configured = s.config.clients.iter().find(|c| c.id == member.id);
            if let Some(client) = configured {
                if statuses.get(&format!("{}:{}", client.host, client.port)) == Some(&true) {
                    p.online += 1;
                }
                if acknowledgements.get(&(id.into(), member.id.clone())) == Some(&key) {
                    p.confirmed += 1;
                }
            }
        }
        p
    }
    pub fn delivered(&self, id: &str) -> Result<(), String> {
        let mut s = self.inner.state.lock().unwrap();
        s.inbox.remove(id);
        self.inner.save(&s)
    }
    pub fn leave(&self, id: &str) -> Result<(), String> {
        let mut s = self.inner.state.lock().unwrap();
        let d = s.docs.get_mut(id).ok_or("Sync document is missing")?;
        let targets: Vec<_> = d
            .members
            .iter()
            .filter(|p| p.id != self.inner.id && !d.departed.contains(&p.id))
            .map(|p| p.id.clone())
            .collect();
        d.active = false;
        d.departed.insert(self.inner.id.clone());
        s.outbox.retain(|o| !o.offer.sync || o.offer.doc != id);
        for target in targets {
            if !s
                .leaves
                .iter()
                .any(|(doc, peer)| doc == id && peer == &target)
            {
                s.leaves.push((id.into(), target));
            }
        }
        self.inner.save(&s)?;
        drop(s);
        self.wake();
        Ok(())
    }
    pub fn set_active(&self, ids: &BTreeSet<String>) {
        let mut s = self.inner.state.lock().unwrap();
        let mut changed = false;
        for d in s.docs.values_mut() {
            let active = ids.contains(&d.id)
                && !d.head.signature.is_empty()
                && d.head.doc == d.id
                && d.members.iter().any(|p| p.id == self.inner.id)
                && !d.departed.contains(&self.inner.id);
            if d.active != active {
                d.active = active;
                changed = true;
            }
        }
        if changed {
            if let Err(e) = self.inner.save(&s) {
                self.inner.event(Event::Notice(e));
            }
            drop(s);
            self.wake();
        }
    }
    pub fn begin(
        &self,
        title: String,
        data: &str,
        peers: Vec<Peer>,
        sync: bool,
    ) -> Result<(String, Revision), String> {
        if !self.config().enabled {
            return Err("Send/Sync is not enabled.".into());
        }
        if peers.is_empty() || peers.len() > 31 || peers.iter().any(|p| !peer_valid(p)) {
            return Err("Select at least one approved client.".into());
        }
        if !sync && peers.len() != 1 {
            return Err("Sending requires exactly one client.".into());
        }
        let id = crate::private_fs::random_token().map_err(err)?;
        let (key, _reservation) = self.inner.put_content(&id, data)?;
        let own = self.inner.own();
        let mut revision = Revision {
            force: None,
            doc: id.clone(),
            group: crate::revision::group(
                &peers
                    .iter()
                    .cloned()
                    .chain(std::iter::once(own.clone()))
                    .collect::<Vec<_>>(),
            ),
            bytes: data.len() as u64,
            parents: vec![],
            signature: String::new(),
            clock: BTreeMap::from([(own.id.clone(), 1)]),
            hash: key,
            author: own.id.clone(),
            name: own.name.clone(),
        };
        crate::revision::sign(&mut revision, &self.inner.key)?;
        let mut members = peers.clone();
        members.push(own);
        let mut s = self.inner.state.lock().unwrap();
        if peers.iter().any(|p| s.revoked.contains(&p.id)) {
            return Err("A selected device is revoked".into());
        }
        if s.docs.len() >= crate::security::limits().documents as usize {
            return Err("Document quota reached; remove an inactive group first".into());
        }
        if s.proofs.len() >= crate::security::limits().proofs as usize {
            return Err(
                "Signed history quota reached; clean retired groups or adjust Security limits"
                    .into(),
            );
        }
        if s.outbox.len() + peers.len() > 64 {
            return Err("Too many pending requests.".into());
        }
        s.proofs.insert(revision.key(), revision.clone());
        if sync {
            s.docs.insert(
                id.clone(),
                Document {
                    id: id.clone(),
                    root: revision.key(),
                    title: title.clone(),
                    head: revision.clone(),
                    conflicts: vec![],
                    history: vec![],
                    members: members.clone(),
                    active: true,
                    departed: BTreeSet::new(),
                },
            );
        }
        for peer in peers {
            s.outbox.push(Outbox {
                target: peer.id,
                offer: Offer {
                    id: session::new_key(),
                    doc: id.clone(),
                    title: title.clone(),
                    sync,
                    revision: revision.clone(),
                    members: members.clone(),
                    len: data.len() as u64,
                },
            });
        }
        self.inner.save(&s)?;
        drop(s);
        self.wake();
        Ok((id, revision))
    }
    pub fn approve(
        &self,
        id: &str,
        accepted: bool,
        path: Option<PathBuf>,
        autosave: bool,
        approved_target: Option<PathBuf>,
    ) -> Result<(), String> {
        let mut s = self.inner.state.lock().unwrap();
        let offer = s
            .offers
            .get(id)
            .cloned()
            .ok_or("Request no longer exists.")?;
        if !accepted {
            s.denied.insert(id.into());
            if s.denied.len() > 1024 {
                let oldest = s.denied.iter().next().unwrap().clone();
                s.denied.remove(&oldest);
            }
            s.offers.remove(id);
            s.decisions.remove(id);
            self.inner.save(&s)?;
            return Ok(());
        }
        if offer.members.iter().any(|p| s.revoked.contains(&p.id)) {
            return Err("This offer includes a revoked identity".into());
        }
        if accepted {
            crate::revision::verify(&offer.revision, &offer.doc, &offer.members)?;
            crate::revision::causal(&offer.revision, &BTreeMap::new())?;
        }
        s.decisions.insert(
            id.into(),
            Acceptance {
                accepted,
                path,
                autosave,
                approved_target,
            },
        );
        self.inner.save(&s)?;
        drop(s);
        self.wake();
        self.inner.event(Event::Changed);
        Ok(())
    }
    pub fn publish(&self, id: &str, base: &Clock, data: &str) -> Result<Revision, String> {
        let (content, _reservation) = self.inner.put_content(id, data)?;
        let s = self.inner.state.lock().unwrap();
        let name = s.config.name.clone();
        let d = s.docs.get(id).ok_or("Sync document is missing.")?;
        if d.head.hash == content && d.conflicts.is_empty() {
            return Ok(d.head.clone());
        }
        let parent = s
            .proofs
            .values()
            .find(|r| r.doc == id && &r.clock == base)
            .ok_or("The edited base revision is unavailable; select a current conflict version")?;
        if parent.force < d.head.force {
            return Err("A force-main decision superseded this draft; keep it as a separate copy before editing the current main version".into());
        }
        let mut parents = vec![parent.clone()];
        include_local_provenance(&s, id, &self.inner.id, &mut parents);
        if parents.iter().any(|r| r.force > parent.force) {
            return Err("A newer signed force-main decision is pending; synchronise its main version before publishing".into());
        }
        let clock = parent_clock(&parents, &self.inner.id)?;
        let mut revision = Revision {
            force: parent.force.clone(),
            clock,
            hash: content,
            author: self.inner.id.clone(),
            name,
            doc: id.into(),
            group: parent.group.clone(),
            bytes: data.len() as u64,
            parents: parents.iter().map(Revision::key).collect(),
            signature: String::new(),
        };
        if s.proofs.values().any(|r| {
            r.doc == id
                && r.author == self.inner.id
                && r.clock.get(&self.inner.id) >= revision.clock.get(&self.inner.id)
        }) {
            return Err(
                "A newer local revision exists; resolve the versions before publishing".into(),
            );
        }
        crate::revision::sign(&mut revision, &self.inner.key)?;
        drop(s);
        self.inner.receive(
            id,
            revision.clone(),
            &self.inner.id,
            self.inner.network_epoch.load(Ordering::Relaxed),
        )?;
        Ok(revision)
    }
    pub fn force_main(&self, id: &str, key: &str) -> Result<Revision, String> {
        let mut s = self.inner.state.lock().unwrap();
        if s.proofs.len() >= crate::security::limits().proofs as usize {
            return Err(
                "Signed history quota reached; clean retired groups or adjust Security limits"
                    .into(),
            );
        }
        let name = s.config.name.clone();
        let d = s.docs.get(id).ok_or("Document is missing.")?;
        if !d.active || d.head.signature.is_empty() {
            return Err(
                "This archived group cannot publish revisions; share a fresh group first".into(),
            );
        }
        let mut heads: Vec<_> = std::iter::once(d.head.clone())
            .chain(d.conflicts.clone())
            .collect();
        let mut chosen = heads
            .iter()
            .find(|r| r.key() == key)
            .cloned()
            .ok_or("Revision has been replaced. Select it again.")?;
        include_local_provenance(&s, id, &self.inner.id, &mut heads);
        let generation = heads
            .iter()
            .filter_map(|r| r.force.as_ref())
            .map(|f| f.generation)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("Force-main generation overflow")?;
        chosen.force = Some(crate::revision::ForceMain {
            generation,
            author: self.inner.id.clone(),
            nonce: crate::private_fs::random_token().map_err(err)?,
            selected: key.into(),
        });
        chosen.clock = parent_clock(&heads, &self.inner.id)?;
        chosen.author = self.inner.id.clone();
        chosen.name = name;
        chosen.parents = heads.iter().map(Revision::key).collect();
        crate::revision::sign(&mut chosen, &self.inner.key)?;
        crate::revision::causal(&chosen, &s.proofs)?;
        let chosen_proof = chosen.clone();
        let d = s.docs.get_mut(id).ok_or("Document is missing.")?;
        d.history.extend(heads);
        if d.history.len() > 20 {
            d.history.drain(..d.history.len() - 20);
        }
        d.head = chosen.clone();
        d.conflicts.clear();
        s.proofs.insert(chosen_proof.key(), chosen_proof);
        self.inner.save(&s)?;
        drop(s);
        crate::traffic::force(
            id,
            &format!("Force sync main created: generation {generation}"),
            chosen.force.as_ref().unwrap(),
            chosen.bytes,
        );
        self.wake();
        self.inner.event(Event::Updated(id.into()));
        Ok(chosen)
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.stop_listener();
        self.inner.stop.store(true, Ordering::Relaxed);
        self.wake();
    }
}

#[derive(Debug)]
struct Verifier {
    inner: Arc<Inner>,
    host: String,
    port: u16,
}
fn schemes() -> Vec<SignatureScheme> {
    rustls::crypto::aws_lc_rs::default_provider()
        .signature_verification_algorithms
        .supported_schemes()
}
fn sig12(
    msg: &[u8],
    cert: &CertificateDer<'_>,
    sig: &DigitallySignedStruct,
) -> Result<HandshakeSignatureValid, rustls::Error> {
    rustls::crypto::verify_tls12_signature(
        msg,
        cert,
        sig,
        &rustls::crypto::aws_lc_rs::default_provider().signature_verification_algorithms,
    )
}
fn sig13(
    msg: &[u8],
    cert: &CertificateDer<'_>,
    sig: &DigitallySignedStruct,
) -> Result<HandshakeSignatureValid, rustls::Error> {
    rustls::crypto::verify_tls13_signature(
        msg,
        cert,
        sig,
        &rustls::crypto::aws_lc_rs::default_provider().signature_verification_algorithms,
    )
}
impl ServerCertVerifier for Verifier {
    fn verify_server_cert(
        &self,
        cert: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if self.inner.trusted(cert, &self.host, self.port) {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General("Device is not approved yet.".into()))
        }
    }
    fn verify_tls12_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        s: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        sig12(m, c, s)
    }
    fn verify_tls13_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        s: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        sig13(m, c, s)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        schemes()
    }
}
impl ClientCertVerifier for Verifier {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }
    fn verify_client_cert(
        &self,
        cert: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        if self.inner.trusted(cert, &self.host, self.port) {
            Ok(ClientCertVerified::assertion())
        } else {
            Err(rustls::Error::General("Device is not approved yet.".into()))
        }
    }
    fn verify_tls12_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        s: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        sig12(m, c, s)
    }
    fn verify_tls13_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        s: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        sig13(m, c, s)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        schemes()
    }
}
fn write_header<T: Serialize + WireLabel>(
    stream: &mut impl Write,
    value: &T,
) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(err)?;
    if bytes.len() > HEADER {
        return Err("Protocol header is too large.".into());
    }
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .map_err(err)?;
    stream.write_all(&bytes).map_err(err)?;
    stream.flush().map_err(err)?;
    crate::traffic::record("TX", value.label(), bytes.len() as u64 + 4);
    Ok(())
}
fn read_header<T: serde::de::DeserializeOwned + WireLabel>(
    stream: &mut impl Read,
) -> Result<T, String> {
    let mut size = [0; 4];
    stream.read_exact(&mut size).map_err(err)?;
    let size = u32::from_be_bytes(size) as usize;
    if size > HEADER {
        return Err("Protocol header is too large.".into());
    }
    let mut data = vec![0; size];
    stream.read_exact(&mut data).map_err(err)?;
    let value: T = serde_json::from_slice(&data).map_err(err)?;
    crate::traffic::record("RX", value.label(), size as u64 + 4);
    Ok(value)
}
fn send_body(
    inner: &Inner,
    stream: &mut impl Write,
    hash: &str,
    doc: &str,
    peer: &str,
    epoch: u64,
) -> Result<(), String> {
    let path = inner.content_path(hash)?;
    let mut reader = crate::vault::Reader::new(&path, &path, MAX).map_err(err)?;
    let mut transferred = 0;
    let mut bytes = [0u8; 65536];
    loop {
        if !inner.still_authorized(doc, peer, epoch) {
            return Err("Document transfer permission changed".into());
        }
        let count = reader.read(&mut bytes).map_err(err)?;
        if count == 0 {
            break;
        }
        stream.write_all(&bytes[..count]).map_err(err)?;
        transferred += count as u64;
    }
    if !inner.still_authorized(doc, peer, epoch) {
        return Err("Document transfer permission changed".into());
    }
    stream.flush().map_err(err)?;
    crate::traffic::record("TX", "document body completed", transferred);
    Ok(())
}
fn read_body(
    inner: &Inner,
    stream: &mut impl Read,
    revision: &Revision,
    len: u64,
) -> Result<crate::security::Reservation, String> {
    if len > MAX || len != revision.bytes {
        return Err("Document is too large.".into());
    }
    validate_revision(revision)?;
    let reservation = inner.reserve(&revision.doc, &revision.author, len)?;
    let path = inner.content_path(&revision.hash)?;
    let temp = inner
        .root
        .join("content")
        .join(format!("{}.pending", session::new_key()));
    let file = crate::private_fs::create(&temp).map_err(err)?;
    let mut file = crate::vault::Writer::new(file, &path).map_err(err)?;
    let mut hash = Sha256::new();
    let mut left = len;
    let mut buffer = [0u8; 65536];
    let result = (|| {
        while left > 0 {
            if !inner.enabled.load(Ordering::Relaxed) {
                return Err("Sharing was disabled.".into());
            }
            let n = stream
                .read(&mut buffer[..(left as usize).min(65536)])
                .map_err(err)?;
            if n == 0 {
                return Err("Transfer interrupted.".into());
            }
            file.write_all(&buffer[..n]).map_err(err)?;
            hash.update(&buffer[..n]);
            left -= n as u64;
        }
        file.finish().map_err(err)?;
        let content =
            String::from_utf8(crate::vault::read(&temp, &path, MAX).map_err(err)?).map_err(err)?;
        crate::rich::validate_snapshot(&content)?;
        drop(content);
        if format!("{:x}", hash.finalize()) != revision.hash {
            return Err("Content checksum mismatch.".into());
        }
        crate::traffic::record("RX", "document body validated", len);
        if !path.exists() {
            fs::rename(&temp, &path).map_err(err)?;
        } else {
            let _ = fs::remove_file(&temp);
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result.map(|_| reservation)
}
fn serve(
    inner: &Arc<Inner>,
    socket: TcpStream,
    remote: SocketAddr,
    generation: u64,
) -> Result<(), String> {
    crate::traffic::context(
        "",
        remote.to_string(),
        socket.local_addr().map_err(err)?.to_string(),
    );
    crate::traffic::record("IN", "TCP accepted", 0);
    let mut socket = DeadlineSocket::new(socket, inner.network_epoch.clone());
    socket.generation = generation;
    socket
        .set_read_timeout(Some(Duration::from_secs(15)))
        .map_err(err)?;
    socket
        .set_write_timeout(Some(Duration::from_secs(15)))
        .map_err(err)?;
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(sharing_provider()))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(err)?
        .with_client_cert_verifier(Arc::new(Verifier {
            inner: inner.clone(),
            host: remote.ip().to_canonical().to_string(),
            port: PORT,
        }))
        .with_single_cert(
            vec![CertificateDer::from(inner.cert.clone())],
            PrivatePkcs8KeyDer::from(inner.key.to_vec()).into(),
        )
        .map_err(err)?;
    let mut connection = rustls::ServerConnection::new(Arc::new(config)).map_err(err)?;
    while connection.is_handshaking() {
        connection.complete_io(&mut socket).map_err(err)?;
    }
    let peer = hash(
        connection
            .peer_certificates()
            .and_then(|c| c.first())
            .ok_or("Device key is missing.")?
            .as_ref(),
    );
    let mut stream = rustls::StreamOwned::new(connection, socket);
    crate::traffic::identify(&peer);
    crate::traffic::record("IN", "TLS authenticated", 0);
    if !inner.enabled.load(Ordering::Relaxed) {
        return Err("Sharing is disabled.".into());
    }
    match read_header::<Request>(&mut stream)? {
        Request::Leave { doc } => {
            let mut s = inner.state.lock().unwrap();
            if !s.docs.contains_key(&doc) {
                if doc.len() > 128 {
                    return Err("Invalid document identifier".into());
                }
                s.departures
                    .entry(doc.clone())
                    .or_default()
                    .insert(peer.clone());
                if s.departures.len() > 1024 {
                    let oldest = s.departures.keys().next().unwrap().clone();
                    s.departures.remove(&oldest);
                }
            }
            let cancel: Vec<_> = s
                .offers
                .values()
                .filter(|o| o.doc == doc && o.revision.author == peer)
                .map(|o| o.id.clone())
                .collect();
            for id in cancel {
                s.offers.remove(&id);
                s.decisions.remove(&id);
                s.denied.insert(id);
            }
            let Some(d) = s.docs.get_mut(&doc) else {
                inner.save(&s)?;
                drop(s);
                write_header(&mut stream, &Reply::Done)?;
                return Ok(());
            };
            if !d.members.iter().any(|p| p.id == peer) {
                return Err("Document is not shared".into());
            }
            let name = d
                .members
                .iter()
                .find(|p| p.id == peer)
                .map(|p| p.name.clone())
                .unwrap_or_default();
            let changed = d.departed.insert(peer.clone());
            inner.save(&s)?;
            drop(s);
            if changed {
                inner.event(Event::Left(doc, peer, name));
                inner.event(Event::Changed);
            }
            write_header(&mut stream, &Reply::Done)?;
        }
        Request::Hello { mut info } => {
            info.host = remote.ip().to_canonical().to_string();
            if info.protocol != crate::revision::PROTOCOL {
                return Err(
                    "All sharing devices must use compatible EditorTool V1.5.4 peers".into(),
                );
            }
            if info.id != peer || !peer_valid(&info) {
                return Err("Invalid device identifier.".into());
            }
            let mut s = inner.state.lock().unwrap();
            if !s.locked_clients.contains(&peer) {
                if let Some(client) = s.config.clients.iter_mut().find(|p| p.id == peer) {
                    if client.port != info.port || client.name != info.name {
                        client.port = info.port;
                        client.name = info.name.clone();
                        inner.save(&s)?;
                    }
                }
            }
            let heads = s
                .docs
                .values()
                .filter(|d| inner.allowed(&s, &d.id, &peer))
                .flat_map(|d| {
                    std::iter::once(&d.head)
                        .chain(&d.conflicts)
                        .map(|r| (d.id.clone(), r.clone()))
                })
                .take(256)
                .collect();
            drop(s);
            write_header(
                &mut stream,
                &Reply::Hello {
                    info: inner.own(),
                    heads,
                },
            )?;
        }
        Request::Offer(mut offer) => {
            if offer.id.len() > 128
                || offer.doc.len() > 128
                || offer.title.chars().count() > 256
                || offer.members.len() > 32
                || offer.len > MAX
                || !offer.members.iter().any(|p| p.id == peer)
                || !offer.members.iter().any(|p| p.id == inner.id)
            {
                return Err("Invalid sharing request.".into());
            }
            validate_revision(&offer.revision)?;
            if offer
                .revision
                .clock
                .keys()
                .any(|id| !offer.members.iter().any(|p| &p.id == id))
            {
                return Err("Revision author is missing from the offer.".into());
            }
            for p in &mut offer.members {
                if p.id == peer {
                    p.host = remote.ip().to_canonical().to_string();
                }
                if !peer_valid(p) {
                    return Err("Invalid participant.".into());
                }
            }
            crate::revision::verify(&offer.revision, &offer.doc, &offer.members)?;
            crate::revision::causal(&offer.revision, &BTreeMap::new())?;
            let mut s = inner.state.lock().unwrap();
            if !s.config.clients.iter().any(|p| p.id == peer)
                || s.revoked.contains(&peer)
                || s.revoked
                    .iter()
                    .any(|id| offer.members.iter().any(|p| &p.id == id))
            {
                return Err("Device may not invite new documents".into());
            }
            if s.denied.contains(&offer.id) {
                drop(s);
                write_header(&mut stream, &Reply::Rejected)?;
                return Ok(());
            }
            if s.completed.contains_key(&offer.id) {
                drop(s);
                write_header(&mut stream, &Reply::Done)?;
                return Ok(());
            }
            let decision = s.decisions.get(&offer.id).cloned();
            if decision.is_none() && !s.offers.contains_key(&offer.id) {
                if s.offers.len() >= 64 {
                    return Err("Too many pending requests.".into());
                }
                s.offers.insert(offer.id.clone(), offer.clone());
                inner.save(&s)?;
                inner.event(Event::Offer(offer));
            }
            write_header(
                &mut stream,
                &match decision {
                    Some(a) if a.accepted => Reply::Accepted,
                    Some(_) => Reply::Rejected,
                    None => Reply::Pending,
                },
            )?;
        }
        Request::Put {
            doc,
            revision,
            offer,
            len,
        } => {
            let s = inner.state.lock().unwrap();
            let acceptance = offer.as_ref().and_then(|id| {
                s.decisions
                    .get(id)
                    .filter(|a| a.accepted)
                    .cloned()
                    .zip(s.offers.get(id).cloned())
            });
            let approved = if let Some((_, o)) = &acceptance {
                o.doc == doc
                    && o.revision.key() == revision.key()
                    && o.len == len
                    && o.members.iter().any(|p| p.id == peer)
            } else {
                offer.is_none() && inner.allowed(&s, &doc, &peer)
            };
            drop(s);
            if !approved {
                write_header(
                    &mut stream,
                    &Reply::Error("Document has not been approved.".into()),
                )?;
                return Ok(());
            }
            if acceptance.is_none() {
                let s = inner.state.lock().unwrap();
                let d = s.docs.get(&doc).ok_or("Unknown document")?;
                crate::revision::verify(&revision, &doc, &d.members)?;
                if !s.proofs.contains_key(&revision.key()) {
                    return Err("Send signed provenance before a document body".into());
                }
            } else {
                let (_, o) = acceptance.as_ref().unwrap();
                crate::revision::verify(&revision, &doc, &o.members)?;
            }
            write_header(&mut stream, &Reply::Ready)?;
            let _reservation = read_body(inner, &mut stream, &revision, len)?;
            if let Some((a, o)) = acceptance {
                let mut s = inner.state.lock().unwrap();
                if inner.network_epoch.load(Ordering::Relaxed) != generation
                    || s.revoked.contains(&peer)
                    || !s.decisions.get(&o.id).is_some_and(|a| a.accepted)
                {
                    return Err("Consent changed during transfer".into());
                }
                if (!s.proofs.contains_key(&revision.key())
                    && s.proofs.len() >= crate::security::limits().proofs as usize)
                    || (o.sync
                        && !s.docs.contains_key(&doc)
                        && s.docs.len() >= crate::security::limits().documents as usize)
                {
                    return Err("Sharing quota reached before transfer commit".into());
                }
                s.proofs.insert(revision.key(), revision.clone());
                if o.sync {
                    let departed = s.departures.remove(&doc).unwrap_or_default();
                    s.docs.entry(doc.clone()).or_insert(Document {
                        id: doc.clone(),
                        root: revision.key(),
                        title: o.title.clone(),
                        head: revision,
                        conflicts: vec![],
                        history: vec![],
                        members: o.members.clone(),
                        active: true,
                        departed,
                    });
                }
                s.completed.insert(o.id.clone(), o.revision.hash.clone());
                s.inbox.insert(o.id.clone(), (o.clone(), a.clone()));
                if s.completed.len() > 1024 {
                    let key = s.completed.keys().next().unwrap().clone();
                    s.completed.remove(&key);
                }
                s.offers.remove(&o.id);
                s.decisions.remove(&o.id);
                inner.save(&s)?;
                drop(s);
                inner.event(Event::Received(o, a));
            } else {
                inner.receive(&doc, revision, &peer, generation)?;
            }
            write_header(&mut stream, &Reply::Done)?;
        }
        Request::Proofs { doc, keys } => {
            let s = inner.state.lock().unwrap();
            if keys.len() > 32 || !inner.allowed(&s, &doc, &peer) {
                return Err("Proof access denied".into());
            }
            let nodes = keys
                .iter()
                .map(|key| {
                    s.proofs
                        .get(key)
                        .filter(|r| r.doc == doc)
                        .cloned()
                        .ok_or("Signed parent unavailable; share a fresh group")
                })
                .collect::<Result<Vec<_>, _>>()?;
            drop(s);
            write_header(&mut stream, &Reply::Proofs(nodes))?;
        }
        Request::ProofPush { doc, nodes } => {
            inner.accept_proofs(&doc, nodes, &peer, generation)?;
            write_header(&mut stream, &Reply::Done)?;
        }
        Request::Get { doc, key } => {
            let s = inner.state.lock().unwrap();
            if !inner.allowed(&s, &doc, &peer) {
                return Err("Document is not shared.".into());
            }
            let d = &s.docs[&doc];
            let r = std::iter::once(&d.head)
                .chain(&d.conflicts)
                .find(|r| r.key() == key)
                .cloned()
                .ok_or("Revision is missing.")?;
            drop(s);
            let len = r.bytes;
            write_header(
                &mut stream,
                &Reply::Data {
                    revision: r.clone(),
                    len,
                },
            )?;
            send_body(inner, &mut stream, &r.hash, &doc, &peer, generation)?;
        }
    }
    Ok(())
}
fn connect(
    inner: &Arc<Inner>,
    peer: &Peer,
    started: Instant,
) -> Result<rustls::StreamOwned<rustls::ClientConnection, DeadlineSocket>, String> {
    let remaining = Duration::from_secs(45)
        .checked_sub(started.elapsed())
        .filter(|d| !d.is_zero())
        .ok_or("Sync total deadline per peer reached.")?;
    crate::traffic::context(
        &peer.id,
        format!("{}:{}", peer.host, peer.port),
        String::new(),
    );
    let generation = inner.network_epoch.load(Ordering::Relaxed);
    let config = inner.state.lock().unwrap().config.clone();
    let socket = crate::network::connect(
        address(&peer.host, peer.port)?,
        config.automatic_interfaces,
        &config.interfaces,
        remaining.min(Duration::from_secs(3)),
    )
    .map_err(err)?;
    crate::traffic::context(
        &peer.id,
        socket.peer_addr().map_err(err)?.to_string(),
        socket.local_addr().map_err(err)?.to_string(),
    );
    crate::traffic::record("OUT", "TCP connected", 0);
    socket
        .set_read_timeout(Some(Duration::from_secs(15)))
        .map_err(err)?;
    socket
        .set_write_timeout(Some(Duration::from_secs(15)))
        .map_err(err)?;
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(sharing_provider()))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(err)?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Verifier {
            inner: inner.clone(),
            host: peer.host.clone(),
            port: peer.port,
        }))
        .with_client_auth_cert(
            vec![CertificateDer::from(inner.cert.clone())],
            PrivatePkcs8KeyDer::from(inner.key.to_vec()).into(),
        )
        .map_err(err)?;
    let connection = rustls::ClientConnection::new(
        Arc::new(config),
        ServerName::try_from("editortool.local").unwrap(),
    )
    .map_err(err)?;
    Ok(rustls::StreamOwned::new(
        connection,
        DeadlineSocket {
            socket,
            started,
            epoch: inner.network_epoch.clone(),
            generation,
        },
    ))
}
fn sweep(inner: &Arc<Inner>) {
    let clients = {
        let s = inner.state.lock().unwrap();
        let mut clients = BTreeMap::new();
        for peer in s
            .config
            .clients
            .iter()
            .chain(
                s.docs
                    .values()
                    .filter(|d| d.active)
                    .flat_map(|d| d.members.iter()),
            )
            .chain(
                s.offers
                    .values()
                    .filter(|o| s.decisions.get(&o.id).is_some_and(|a| a.accepted))
                    .flat_map(|o| o.members.iter()),
            )
        {
            if peer.id != inner.id && !s.revoked.contains(&peer.id) {
                clients
                    .entry(peer.id.clone())
                    .or_insert_with(|| peer.clone());
            }
        }
        clients.into_values().collect::<Vec<_>>()
    };
    for peer in clients {
        if !inner.enabled.load(Ordering::Relaxed) || inner.stop.load(Ordering::Relaxed) {
            break;
        }
        let started = Instant::now();
        let checked_generation = inner.network_epoch.load(Ordering::Relaxed);
        let result = (|| -> Result<(), String> {
            let mut stream = connect(inner, &peer, started)?;
            let mut info = inner.own();
            info.host = stream
                .sock
                .local_addr()
                .map_err(err)?
                .ip()
                .to_canonical()
                .to_string();
            write_header(&mut stream, &Request::Hello { info })?;
            let Reply::Hello { info, heads } = read_header(&mut stream)? else {
                return Err("No compatible EditorTool client.".into());
            };
            if info.protocol != crate::revision::PROTOCOL {
                return Err(
                    "All sharing devices must use compatible EditorTool V1.5.4 peers".into(),
                );
            }
            if info.id != peer.id && !peer.id.is_empty() {
                return Err("Device identifier mismatch.".into());
            }
            for (doc, revision) in &heads {
                fetch_proofs(inner, &peer, doc, revision, started, checked_generation)?;
                inner.confirmed(&info.id, doc, revision);
            }
            let leaves = inner
                .state
                .lock()
                .unwrap()
                .leaves
                .iter()
                .filter(|(_, target)| target == &info.id)
                .cloned()
                .collect::<Vec<_>>();
            for (doc, target) in leaves {
                let mut stream = connect(inner, &peer, started)?;
                write_header(&mut stream, &Request::Leave { doc: doc.clone() })?;
                if matches!(read_header(&mut stream)?, Reply::Done) {
                    let mut s = inner.state.lock().unwrap();
                    s.leaves.retain(|(d, p)| d != &doc || p != &target);
                    inner.save(&s)?;
                }
            }
            let docs = inner
                .state
                .lock()
                .unwrap()
                .docs
                .values()
                .filter(|d| {
                    d.active
                        && !d.departed.contains(&info.id)
                        && !d.departed.contains(&inner.id)
                        && d.members.iter().any(|p| p.id == info.id)
                })
                .cloned()
                .collect::<Vec<_>>();
            for d in docs {
                for (_, remote) in heads.iter().filter(|(id, _)| id == &d.id) {
                    if std::iter::once(&d.head)
                        .chain(&d.conflicts)
                        .any(|r| same(r, remote) || supersedes(r, remote))
                    {
                        if d.head.force > remote.force {
                            crate::traffic::force(&d.id, &format!("Force sync main retained: generation {}; superseded peer generation {} ignored", d.head.force.as_ref().unwrap().generation, remote.force.as_ref().map_or(0, |f| f.generation)), d.head.force.as_ref().unwrap(), 0);
                        }
                        continue;
                    }
                    let mut stream = connect(inner, &peer, started)?;
                    write_header(
                        &mut stream,
                        &Request::Get {
                            doc: d.id.clone(),
                            key: remote.key(),
                        },
                    )?;
                    if let Reply::Data { revision, len } = read_header(&mut stream)? {
                        if !same(&revision, remote) {
                            return Err("Revision reply mismatch.".into());
                        }
                        let _reservation = read_body(inner, &mut stream, &revision, len)?;
                        inner.receive(&d.id, revision, &info.id, checked_generation)?;
                    }
                }
                let fresh = inner
                    .state
                    .lock()
                    .unwrap()
                    .docs
                    .get(&d.id)
                    .cloned()
                    .unwrap();
                for local in std::iter::once(&fresh.head).chain(&fresh.conflicts) {
                    if !heads.iter().any(|(id, _)| id == &d.id)
                        || heads
                            .iter()
                            .filter(|(id, _)| id == &d.id)
                            .any(|(_, r)| same(r, local) || supersedes(r, local))
                    {
                        continue;
                    }
                    push_proofs(inner, &peer, &d.id, local, started)?;
                    let mut stream = connect(inner, &peer, started)?;
                    let len = local.bytes;
                    write_header(
                        &mut stream,
                        &Request::Put {
                            doc: d.id.clone(),
                            revision: local.clone(),
                            offer: None,
                            len,
                        },
                    )?;
                    if matches!(read_header(&mut stream)?, Reply::Ready) {
                        send_body(
                            inner,
                            &mut stream,
                            &local.hash,
                            &d.id,
                            &info.id,
                            checked_generation,
                        )?;
                        if matches!(read_header(&mut stream)?, Reply::Done) {
                            inner.confirmed(&info.id, &d.id, local);
                        }
                    }
                }
            }
            let outbox = {
                let s = inner.state.lock().unwrap();
                s.outbox
                    .iter()
                    .filter(|o| {
                        o.target == info.id
                            && (!o.offer.sync || s.docs.get(&o.offer.doc).is_some_and(|d| d.active))
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            };
            for item in outbox {
                let mut stream = connect(inner, &peer, started)?;
                let mut offer = item.offer.clone();
                let host = stream
                    .sock
                    .local_addr()
                    .map_err(err)?
                    .ip()
                    .to_canonical()
                    .to_string();
                for member in &mut offer.members {
                    if member.id == inner.id {
                        member.host = host.clone();
                    }
                }
                write_header(&mut stream, &Request::Offer(offer.clone()))?;
                let reply: Reply = read_header(&mut stream)?;
                let finished = match reply {
                    Reply::Done => true,
                    Reply::Accepted => {
                        let mut stream = connect(inner, &peer, started)?;
                        write_header(
                            &mut stream,
                            &Request::Put {
                                doc: offer.doc.clone(),
                                revision: offer.revision.clone(),
                                offer: Some(offer.id.clone()),
                                len: offer.len,
                            },
                        )?;
                        if matches!(read_header(&mut stream)?, Reply::Ready) {
                            send_body(
                                inner,
                                &mut stream,
                                &offer.revision.hash,
                                &offer.doc,
                                &info.id,
                                checked_generation,
                            )?;
                            matches!(read_header(&mut stream)?, Reply::Done)
                        } else {
                            false
                        }
                    }
                    Reply::Rejected => {
                        inner.event(Event::Notice(format!(
                            "{} rejected the request for {}.",
                            peer.name, offer.title
                        )));
                        true
                    }
                    _ => false,
                };
                if finished {
                    if !matches!(reply, Reply::Rejected) {
                        inner.confirmed(&info.id, &offer.doc, &offer.revision);
                    }
                    let mut s = inner.state.lock().unwrap();
                    s.outbox.retain(|o| o.offer.id != item.offer.id);
                    inner.save(&s)?;
                    inner.event(Event::Changed);
                }
            }
            Ok(())
        })();
        if !inner.enabled.load(Ordering::Relaxed)
            || inner.network_epoch.load(Ordering::Relaxed) != checked_generation
        {
            continue;
        }
        let key = format!("{}:{}", peer.host, peer.port);
        let mut statuses = inner.statuses.lock().unwrap();
        let previous = statuses.insert(key.clone(), result.is_ok());
        let changed = previous != Some(result.is_ok());
        inner.checked.lock().unwrap().insert(
            key,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );
        if previous == Some(true) && result.is_err() {
            inner.event(Event::Disconnected(peer.id.clone()));
        }
        if let Err(error) = &result {
            crate::traffic::failure(error);
        }
        inner.event(Event::Checked);
        drop(statuses);
        if changed {
            if let Err(error) = &result {
                crate::diagnostics::failure("sharing", "outgoing_failed", error);
            }
            crate::diagnostics::event(
                "sharing",
                if result.is_ok() {
                    "peer_connected"
                } else {
                    "peer_disconnected_or_transfer_failed"
                },
            );
            inner.event(Event::Changed);
        }
    }
}

fn sharing_provider() -> rustls::crypto::CryptoProvider {
    let mut provider = rustls::crypto::aws_lc_rs::default_provider();
    provider.kx_groups = vec![rustls::crypto::aws_lc_rs::kx_group::X25519MLKEM768];
    provider.cipher_suites =
        vec![rustls::crypto::aws_lc_rs::cipher_suite::TLS13_AES_256_GCM_SHA384];
    provider
}
