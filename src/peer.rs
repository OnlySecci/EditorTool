//! Opt-in private-network sharing. Pinned mutual TLS, immutable content blobs,
//! explicit offers, persistent outbox and causal (not timestamp) version order.
use crate::{session, Action, Message};
use base64::{engine::general_purpose::STANDARD, Engine};
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
    pub clock: Clock,
    pub hash: String,
    pub author: String,
    pub name: String,
}
impl Revision {
    pub fn key(&self) -> String {
        hash(format!("{}{:?}", self.hash, self.clock).as_bytes())
    }
}
fn dominates(a: &Clock, b: &Clock) -> bool {
    b.iter()
        .all(|(id, n)| a.get(id).copied().unwrap_or(0) >= *n)
}
fn same(a: &Revision, b: &Revision) -> bool {
    a.hash == b.hash && a.clock == b.clock
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
}
#[derive(Clone, Serialize, Deserialize)]
struct Outbox {
    target: String,
    offer: Offer,
}
#[derive(Default, Serialize, Deserialize)]
struct State {
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
    pub doc: String,
    pub clock: Clock,
    pub hash: String,
    pub autosave: bool,
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
        }
    }
}
impl WireLabel for Reply {
    fn label(&self) -> &'static str {
        match self {
            Self::Hello { .. } => "reply Hello",
            Self::Pending => "reply Pending",
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
    key: Vec<u8>,
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
        let result = session::atomic_write(
            &self.root.join("sharing.json"),
            &serde_json::to_vec(s).map_err(err)?,
        )
        .map_err(err);
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
        }
    }
    fn content_path(&self, hash: &str) -> Result<PathBuf, String> {
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Invalid content identifier".into());
        }
        Ok(self.root.join("content").join(format!("{hash}.txt")))
    }
    fn put_content(&self, data: &str) -> Result<String, String> {
        if data.len() as u64 > MAX {
            return Err("Document exceeds 64 MiB.".into());
        }
        crate::rich::validate_snapshot(data)?;
        let key = hash(data.as_bytes());
        let path = self.content_path(&key)?;
        if !path.exists() {
            session::atomic_write(&path, data.as_bytes()).map_err(err)?;
        }
        Ok(key)
    }
    fn trusted(&self, cert: &CertificateDer<'_>, host: &str, port: u16) -> bool {
        let id = hash(cert.as_ref());
        if cert.len() > 16384 || id == self.id {
            return false;
        }
        if self
            .state
            .lock()
            .unwrap()
            .config
            .clients
            .iter()
            .any(|p| p.id == id && p.cert == STANDARD.encode(cert.as_ref()))
        {
            return true;
        }
        let mut seen = self.seen.lock().unwrap();
        if seen.len() < 32 && seen.insert(id.clone()) {
            self.event(Event::Trust(Peer {
                id,
                name: host.into(),
                host: host.into(),
                port,
                cert: STANDARD.encode(cert.as_ref()),
            }));
        }
        false
    }
    fn receive(&self, doc: &str, revision: Revision) -> Result<(), String> {
        validate_revision(&revision)?;
        let mut s = self.state.lock().unwrap();
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
            return Err("Unbekannter Versionsautor.".into());
        }
        if std::iter::once(&d.head)
            .chain(&d.conflicts)
            .any(|r| dominates(&r.clock, &revision.clock) && r.clock != revision.clock)
        {
            return Ok(());
        }
        let conflict;
        if std::iter::once(&d.head)
            .chain(&d.conflicts)
            .all(|r| dominates(&revision.clock, &r.clock) && revision.clock != r.clock)
        {
            d.history.push(d.head.clone());
            d.history.append(&mut d.conflicts);
            d.head = revision;
            conflict = false;
        } else if revision.hash == d.head.hash && d.conflicts.is_empty() {
            for (id, n) in revision.clock {
                let entry = d.head.clock.entry(id).or_default();
                *entry = (*entry).max(n);
            }
            conflict = false;
        } else {
            d.conflicts
                .retain(|r| !dominates(&revision.clock, &r.clock) || revision.clock == r.clock);
            if d.conflicts.len() >= 32 {
                return Err("Zu viele offene Versionskonflikte.".into());
            }
            d.conflicts.push(revision);
            conflict = true;
        }
        if d.history.len() > 20 {
            d.history.drain(..d.history.len() - 20);
        }
        self.save(&s)?;
        drop(s);
        self.event(if conflict {
            Event::Conflict(doc.into())
        } else {
            Event::Updated(doc.into())
        });
        let _ = self.wake.send(());
        Ok(())
    }
    fn allowed(&self, s: &State, doc: &str, peer: &str) -> bool {
        s.config.clients.iter().any(|p| p.id == peer)
            && s.docs.get(doc).is_some_and(|d| {
                d.active
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
        let (cert, key) = if root.join("identity.der").exists() {
            (
                crate::limits::read(&root.join("identity.der"), 16384).map_err(err)?,
                crate::limits::read(&root.join("identity.key"), 16384).map_err(err)?,
            )
        } else {
            let identity =
                rcgen::generate_simple_self_signed(vec!["editortool.local".into()]).map_err(err)?;
            let cert = identity.cert.der().to_vec();
            let key = identity.key_pair.serialize_der();
            for (name, data) in [("identity.der", &cert), ("identity.key", &key)] {
                let mut file = crate::private_fs::create(&root.join(name)).map_err(err)?;
                file.write_all(data).map_err(err)?;
                file.sync_all().map_err(err)?;
            }
            (cert, key)
        };
        let state = if root.join("sharing.json").exists() {
            serde_json::from_slice(
                &crate::limits::read(&root.join("sharing.json"), 8 * 1024 * 1024).map_err(err)?,
            )
            .map_err(err)?
        } else {
            State::default()
        };
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
                "\nDokument: {} · {}\n",
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
        });
        self.configure(c)
    }
    pub fn remove(&mut self, index: usize) -> Result<(), String> {
        let mut c = self.config();
        if index < c.clients.len() {
            let removed = c.clients.remove(index);
            let mut s = self.inner.state.lock().unwrap();
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
            let active = ids.contains(&d.id) && !d.departed.contains(&self.inner.id);
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
        let key = self.inner.put_content(data)?;
        let id = session::new_key();
        let own = self.inner.own();
        let revision = Revision {
            clock: BTreeMap::from([(own.id.clone(), 1)]),
            hash: key,
            author: own.id.clone(),
            name: own.name.clone(),
        };
        let mut members = peers.clone();
        members.push(own);
        let mut s = self.inner.state.lock().unwrap();
        if s.outbox.len() + peers.len() > 64 {
            return Err("Too many pending requests.".into());
        }
        if sync {
            s.docs.insert(
                id.clone(),
                Document {
                    id: id.clone(),
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
        if accepted && offer.sync {
            for p in offer.members.iter().filter(|p| p.id != self.inner.id) {
                if !peer_valid(p) {
                    return Err("Invalid participant in request.".into());
                }
                if !s.config.clients.iter().any(|c| c.id == p.id) {
                    if s.config.clients.len() >= 32 {
                        return Err("Client limit reached.".into());
                    }
                    s.config.clients.push(p.clone());
                }
            }
        }
        s.decisions.insert(
            id.into(),
            Acceptance {
                accepted,
                path,
                autosave,
            },
        );
        self.inner.save(&s)?;
        drop(s);
        self.wake();
        self.inner.event(Event::Changed);
        Ok(())
    }
    pub fn publish(&self, id: &str, base: &Clock, data: &str) -> Result<Revision, String> {
        let content = self.inner.put_content(data)?;
        let s = self.inner.state.lock().unwrap();
        let name = s.config.name.clone();
        let d = s.docs.get(id).ok_or("Sync document is missing.")?;
        if d.head.hash == content && d.conflicts.is_empty() {
            return Ok(d.head.clone());
        }
        let mut clock = base.clone();
        let n = std::iter::once(&d.head)
            .chain(d.conflicts.iter())
            .map(|r| r.clock.get(&self.inner.id).copied().unwrap_or(0))
            .max()
            .unwrap_or(0)
            .max(clock.get(&self.inner.id).copied().unwrap_or(0));
        clock.insert(
            self.inner.id.clone(),
            n.checked_add(1).ok_or("Revision counter is full.")?,
        );
        let revision = Revision {
            clock,
            hash: content,
            author: self.inner.id.clone(),
            name,
        };
        drop(s);
        self.inner.receive(id, revision.clone())?;
        Ok(revision)
    }
    pub fn resolve(&self, id: &str, key: &str) -> Result<Revision, String> {
        let mut s = self.inner.state.lock().unwrap();
        let name = s.config.name.clone();
        let d = s.docs.get_mut(id).ok_or("Document is missing.")?;
        let heads: Vec<_> = std::iter::once(d.head.clone())
            .chain(d.conflicts.clone())
            .collect();
        let mut chosen = heads
            .iter()
            .find(|r| r.key() == key)
            .cloned()
            .ok_or("Revision has been replaced. Select it again.")?;
        let mut clock = Clock::new();
        for r in &heads {
            for (k, n) in &r.clock {
                let e = clock.entry(k.clone()).or_default();
                *e = (*e).max(*n);
            }
        }
        let n = clock.entry(self.inner.id.clone()).or_default();
        *n = n.checked_add(1).ok_or("Revision counter is full.")?;
        chosen.clock = clock;
        chosen.author = self.inner.id.clone();
        chosen.name = name;
        d.history.extend(heads);
        if d.history.len() > 20 {
            d.history.drain(..d.history.len() - 20);
        }
        d.head = chosen.clone();
        d.conflicts.clear();
        self.inner.save(&s)?;
        drop(s);
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
fn send_body(inner: &Inner, stream: &mut impl Write, hash: &str) -> Result<(), String> {
    let mut file = fs::File::open(inner.content_path(hash)?).map_err(err)?;
    let mut transferred = 0;
    let mut buffer = [0; 65536];
    loop {
        if !inner.enabled.load(Ordering::Relaxed) {
            return Err("Sharing was disabled.".into());
        }
        let n = file.read(&mut buffer).map_err(err)?;
        if n == 0 {
            break;
        }
        stream.write_all(&buffer[..n]).map_err(err)?;
        transferred += n as u64;
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
) -> Result<(), String> {
    if len > MAX {
        return Err("Document is too large.".into());
    }
    validate_revision(revision)?;
    let path = inner.content_path(&revision.hash)?;
    let temp = inner
        .root
        .join("content")
        .join(format!("{}.pending", session::new_key()));
    let mut file = crate::private_fs::create(&temp).map_err(err)?;
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
        file.sync_all().map_err(err)?;
        drop(file);
        let content = crate::limits::read_text(&temp).map_err(err)?;
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
    result
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
            PrivatePkcs8KeyDer::from(inner.key.clone()).into(),
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
            let mut s = inner.state.lock().unwrap();
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
            write_header(&mut stream, &Reply::Ready)?;
            read_body(inner, &mut stream, &revision, len)?;
            if let Some((a, o)) = acceptance {
                let mut s = inner.state.lock().unwrap();
                if o.sync {
                    let departed = s.departures.remove(&doc).unwrap_or_default();
                    s.docs.entry(doc.clone()).or_insert(Document {
                        id: doc.clone(),
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
                inner.receive(&doc, revision)?;
            }
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
            let len = fs::metadata(inner.content_path(&r.hash)?)
                .map_err(err)?
                .len();
            write_header(
                &mut stream,
                &Reply::Data {
                    revision: r.clone(),
                    len,
                },
            )?;
            send_body(inner, &mut stream, &r.hash)?;
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
            PrivatePkcs8KeyDer::from(inner.key.clone()).into(),
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
    let clients = inner.state.lock().unwrap().config.clients.clone();
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
            if info.id != peer.id && !peer.id.is_empty() {
                return Err("Device identifier mismatch.".into());
            }
            for (doc, revision) in &heads {
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
                    if std::iter::once(&d.head).chain(&d.conflicts).any(|r| {
                        same(r, remote)
                            || dominates(&r.clock, &remote.clock) && r.clock != remote.clock
                    }) {
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
                        read_body(inner, &mut stream, &revision, len)?;
                        inner.receive(&d.id, revision)?;
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
                        || heads.iter().filter(|(id, _)| id == &d.id).any(|(_, r)| {
                            same(r, local)
                                || dominates(&r.clock, &local.clock) && r.clock != local.clock
                        })
                    {
                        continue;
                    }
                    let mut stream = connect(inner, &peer, started)?;
                    let len = fs::metadata(inner.content_path(&local.hash)?)
                        .map_err(err)?
                        .len();
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
                        send_body(inner, &mut stream, &local.hash)?;
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
                            send_body(inner, &mut stream, &offer.revision.hash)?;
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
