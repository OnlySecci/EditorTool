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
    net::{IpAddr, SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::Duration,
};
pub const PORT: u16 = 51473;
const MAX: u64 = 64 * 1024 * 1024;
const HEADER: usize = 512 * 1024;
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
        .map_err(|_| "Bitte eine interne IPv4- oder IPv6-Adresse eingeben.")?;
    if !private(ip) || port == 0 {
        return Err("Nur interne Netze/VPN und ein Port zwischen 1 und 65535 sind erlaubt.".into());
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
struct Listener {
    running: Arc<AtomicBool>,
    port: u16,
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
    transfers: AtomicUsize,
    seen: Mutex<BTreeSet<String>>,
    statuses: Mutex<BTreeMap<String, bool>>,
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
            Event::Changed => None,
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
            return Err("Ungültige Inhaltskennung".into());
        }
        Ok(self.root.join("content").join(format!("{hash}.txt")))
    }
    fn put_content(&self, data: &str) -> Result<String, String> {
        if data.len() as u64 > MAX {
            return Err("Dokument überschreitet 64 MiB.".into());
        }
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
            return Err("Dokument nicht freigegeben.".into());
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
        return Err("Ungültige Versionsdaten.".into());
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
        fs::create_dir_all(root.join("content")).map_err(err)?;
        let (cert, key) = if root.join("identity.der").exists() {
            (
                fs::read(root.join("identity.der")).map_err(err)?,
                fs::read(root.join("identity.key")).map_err(err)?,
            )
        } else {
            let identity =
                rcgen::generate_simple_self_signed(vec!["editortool.local".into()]).map_err(err)?;
            let cert = identity.cert.der().to_vec();
            let key = identity.key_pair.serialize_der();
            for (name, data) in [("identity.der", &cert), ("identity.key", &key)] {
                let mut options = fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let mut file = options.open(root.join(name)).map_err(err)?;
                file.write_all(data).map_err(err)?;
                file.sync_all().map_err(err)?;
            }
            (cert, key)
        };
        let state = if root.join("sharing.json").exists() {
            serde_json::from_slice(&fs::read(root.join("sharing.json")).map_err(err)?)
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
            transfers: AtomicUsize::new(0),
            seen: Mutex::new(BTreeSet::new()),
            statuses: Mutex::new(BTreeMap::new()),
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
        service.configure(config)?;
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
    pub fn status(&self, p: &Peer) -> bool {
        self.inner
            .statuses
            .lock()
            .unwrap()
            .get(&format!("{}:{}", p.host, p.port))
            .copied()
            .unwrap_or(false)
    }
    pub fn configure(&mut self, config: Config) -> Result<(), String> {
        if config.port == 0
            || config.name.trim().is_empty()
            || config.name.len() > 128
            || config.clients.len() > 32
        {
            return Err("Gerätename, Port oder Anzahl der Clients ist ungültig.".into());
        }
        let restart = self
            .listener
            .as_ref()
            .is_some_and(|l| l.port != config.port)
            || !config.enabled;
        if restart {
            self.stop_listener();
        }
        {
            let mut s = self.inner.state.lock().unwrap();
            s.config = config.clone();
            self.inner.save(&s)?;
        }
        if config.enabled && self.listener.is_none() {
            let socket = socket2::Socket::new(
                socket2::Domain::IPV6,
                socket2::Type::STREAM,
                Some(socket2::Protocol::TCP),
            );
            let listener: Result<TcpListener, String> = (|| {
                if let Ok(socket) = socket {
                    socket.set_only_v6(false).map_err(err)?;
                    #[cfg(unix)]
                    socket.set_reuse_address(true).map_err(err)?;
                    socket
                        .bind(&SocketAddr::from(([0u16; 8], config.port)).into())
                        .map_err(err)?;
                    socket.listen(16).map_err(err)?;
                    Ok(socket.into())
                } else {
                    TcpListener::bind(("0.0.0.0", config.port)).map_err(err)
                }
            })();
            let listener = match listener {
                Ok(l) => l,
                Err(e) => {
                    let mut s = self.inner.state.lock().unwrap();
                    s.config.enabled = false;
                    self.inner.enabled.store(false, Ordering::Relaxed);
                    self.inner.save(&s)?;
                    return Err(format!("Lokaler Port konnte nicht geöffnet werden: {e}"));
                }
            };
            let running = Arc::new(AtomicBool::new(true));
            let flag = running.clone();
            let inner = self.inner.clone();
            self.inner.enabled.store(true, Ordering::Relaxed);
            let worker = thread::spawn(move || {
                for connection in listener.incoming() {
                    if !flag.load(Ordering::Relaxed) {
                        break;
                    }
                    let Ok(stream) = connection else {
                        break;
                    };
                    let Ok(remote) = stream.peer_addr() else {
                        continue;
                    };
                    if !private(remote.ip()) {
                        continue;
                    }
                    if inner.transfers.fetch_add(1, Ordering::Relaxed) >= 2 {
                        inner.transfers.fetch_sub(1, Ordering::Relaxed);
                        continue;
                    }
                    let inner = inner.clone();
                    thread::spawn(move || {
                        if let Err(error) = serve(&inner, stream, remote) {
                            crate::diagnostics::failure("sharing", "incoming_failed", &error);
                        }
                        inner.transfers.fetch_sub(1, Ordering::Relaxed);
                    });
                }
            });
            self.listener = Some(Listener {
                running,
                port: config.port,
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
        self.inner.enabled.store(false, Ordering::Relaxed);
        if let Some(listener) = self.listener.take() {
            listener.running.store(false, Ordering::Relaxed);
            let wake = TcpStream::connect_timeout(
                &SocketAddr::from(([127, 0, 0, 1], listener.port)),
                Duration::from_millis(200),
            )
            .or_else(|_| {
                TcpStream::connect_timeout(
                    &SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], listener.port)),
                    Duration::from_millis(200),
                )
            });
            // The accept thread owns the bound socket. Release it before a
            // settings change binds the replacement listener.
            if wake.is_ok() || listener.worker.is_finished() {
                let _ = listener.worker.join();
            }
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
            return Err("Client ist bereits vorhanden.".into());
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
    pub fn trust(&mut self, peer: Peer, accept: bool) -> Result<(), String> {
        if !accept {
            return Ok(());
        }
        if !peer_valid(&peer) {
            return Err("Geräteschlüssel oder Adresse ist ungültig.".into());
        }
        let mut c = self.config();
        if let Some(p) = c.clients.iter_mut().find(|p| {
            p.host == peer.host && (p.port == peer.port || p.cert.is_empty()) || p.id == peer.id
        }) {
            *p = peer;
        } else {
            c.clients.push(peer);
        }
        self.configure(c)
    }
    pub fn body(&self, hash: &str) -> Result<String, String> {
        fs::read_to_string(self.inner.content_path(hash)?).map_err(err)
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
        let d = s.docs.get_mut(id).ok_or("Sync-Dokument fehlt")?;
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
            return Err("Senden/Sync ist nicht aktiviert.".into());
        }
        if peers.is_empty() || peers.len() > 31 || peers.iter().any(|p| !peer_valid(p)) {
            return Err("Bitte mindestens einen freigegebenen Client auswählen.".into());
        }
        if !sync && peers.len() != 1 {
            return Err("Senden benötigt genau einen Client.".into());
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
            return Err("Zu viele ausstehende Anfragen.".into());
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
            .ok_or("Anfrage ist nicht mehr vorhanden.")?;
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
                    return Err("Ungültiger Teilnehmer in der Anfrage.".into());
                }
                if !s.config.clients.iter().any(|c| c.id == p.id) {
                    if s.config.clients.len() >= 32 {
                        return Err("Client-Limit erreicht.".into());
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
        let d = s.docs.get(id).ok_or("Sync-Dokument fehlt.")?;
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
            n.checked_add(1).ok_or("Versionszähler ist voll.")?,
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
        let d = s.docs.get_mut(id).ok_or("Dokument fehlt.")?;
        let heads: Vec<_> = std::iter::once(d.head.clone())
            .chain(d.conflicts.clone())
            .collect();
        let mut chosen = heads
            .iter()
            .find(|r| r.key() == key)
            .cloned()
            .ok_or("Version wurde inzwischen ersetzt. Bitte erneut auswählen.")?;
        let mut clock = Clock::new();
        for r in &heads {
            for (k, n) in &r.clock {
                let e = clock.entry(k.clone()).or_default();
                *e = (*e).max(*n);
            }
        }
        let n = clock.entry(self.inner.id.clone()).or_default();
        *n = n.checked_add(1).ok_or("Versionszähler ist voll.")?;
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
            Err(rustls::Error::General(
                "Gerät ist noch nicht freigegeben.".into(),
            ))
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
            Err(rustls::Error::General(
                "Gerät ist noch nicht freigegeben.".into(),
            ))
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
fn write_header(stream: &mut impl Write, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(err)?;
    if bytes.len() > HEADER {
        return Err("Protokollkopf zu gross.".into());
    }
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .map_err(err)?;
    stream.write_all(&bytes).map_err(err)?;
    stream.flush().map_err(err)
}
fn read_header<T: serde::de::DeserializeOwned>(stream: &mut impl Read) -> Result<T, String> {
    let mut size = [0; 4];
    stream.read_exact(&mut size).map_err(err)?;
    let size = u32::from_be_bytes(size) as usize;
    if size > HEADER {
        return Err("Protokollkopf zu gross.".into());
    }
    let mut data = vec![0; size];
    stream.read_exact(&mut data).map_err(err)?;
    serde_json::from_slice(&data).map_err(err)
}
fn send_body(inner: &Inner, stream: &mut impl Write, hash: &str) -> Result<(), String> {
    let mut file = fs::File::open(inner.content_path(hash)?).map_err(err)?;
    let mut buffer = [0; 65536];
    loop {
        if !inner.enabled.load(Ordering::Relaxed) {
            return Err("Freigabe wurde deaktiviert.".into());
        }
        let n = file.read(&mut buffer).map_err(err)?;
        if n == 0 {
            break;
        }
        stream.write_all(&buffer[..n]).map_err(err)?;
    }
    stream.flush().map_err(err)
}
fn read_body(
    inner: &Inner,
    stream: &mut impl Read,
    revision: &Revision,
    len: u64,
) -> Result<(), String> {
    if len > MAX {
        return Err("Dokument ist zu gross.".into());
    }
    validate_revision(revision)?;
    let path = inner.content_path(&revision.hash)?;
    let temp = inner
        .root
        .join("content")
        .join(format!("{}.pending", session::new_key()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(err)?;
    let mut hash = Sha256::new();
    let mut left = len;
    let mut buffer = [0u8; 65536];
    let result = (|| {
        while left > 0 {
            if !inner.enabled.load(Ordering::Relaxed) {
                return Err("Freigabe wurde deaktiviert.".into());
            }
            let n = stream
                .read(&mut buffer[..(left as usize).min(65536)])
                .map_err(err)?;
            if n == 0 {
                return Err("Übertragung unterbrochen.".into());
            }
            file.write_all(&buffer[..n]).map_err(err)?;
            hash.update(&buffer[..n]);
            left -= n as u64;
        }
        file.sync_all().map_err(err)?;
        drop(file);
        if format!("{:x}", hash.finalize()) != revision.hash {
            return Err("Inhaltsprüfsumme stimmt nicht.".into());
        }
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
fn serve(inner: &Arc<Inner>, mut socket: TcpStream, remote: SocketAddr) -> Result<(), String> {
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
            .ok_or("Geräteschlüssel fehlt.")?
            .as_ref(),
    );
    let mut stream = rustls::StreamOwned::new(connection, socket);
    if !inner.enabled.load(Ordering::Relaxed) {
        return Err("Freigabe ist deaktiviert.".into());
    }
    match read_header::<Request>(&mut stream)? {
        Request::Leave { doc } => {
            let mut s = inner.state.lock().unwrap();
            if !s.docs.contains_key(&doc) {
                if doc.len() > 128 {
                    return Err("Ungültige Dokumentkennung".into());
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
                return Err("Dokument nicht freigegeben".into());
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
                return Err("Ungültige Gerätekennung.".into());
            }
            let mut s = inner.state.lock().unwrap();
            if let Some(client) = s.config.clients.iter_mut().find(|p| p.id == peer) {
                if client.port != info.port || client.name != info.name {
                    client.port = info.port;
                    client.name = info.name;
                    inner.save(&s)?;
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
                return Err("Ungültige Freigabeanfrage.".into());
            }
            validate_revision(&offer.revision)?;
            if offer
                .revision
                .clock
                .keys()
                .any(|id| !offer.members.iter().any(|p| &p.id == id))
            {
                return Err("Versionsautor fehlt in der Freigabe.".into());
            }
            for p in &mut offer.members {
                if p.id == peer {
                    p.host = remote.ip().to_canonical().to_string();
                }
                if !peer_valid(p) {
                    return Err("Ungültiger Teilnehmer.".into());
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
                    return Err("Zu viele ausstehende Anfragen.".into());
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
                    &Reply::Error("Dokument wurde nicht freigegeben.".into()),
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
                return Err("Dokument nicht freigegeben.".into());
            }
            let d = &s.docs[&doc];
            let r = std::iter::once(&d.head)
                .chain(&d.conflicts)
                .find(|r| r.key() == key)
                .cloned()
                .ok_or("Version fehlt.")?;
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
) -> Result<rustls::StreamOwned<rustls::ClientConnection, TcpStream>, String> {
    let socket =
        TcpStream::connect_timeout(&address(&peer.host, peer.port)?, Duration::from_secs(3))
            .map_err(err)?;
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
    Ok(rustls::StreamOwned::new(connection, socket))
}
fn sweep(inner: &Arc<Inner>) {
    let clients = inner.state.lock().unwrap().config.clients.clone();
    for peer in clients {
        if !inner.enabled.load(Ordering::Relaxed) || inner.stop.load(Ordering::Relaxed) {
            break;
        }
        let result = (|| -> Result<(), String> {
            let mut stream = connect(inner, &peer)?;
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
                return Err("Kein kompatibler EditorTool-Client.".into());
            };
            if info.id != peer.id && !peer.id.is_empty() {
                return Err("Gerätekennung stimmt nicht überein.".into());
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
                let mut stream = connect(inner, &peer)?;
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
                    let mut stream = connect(inner, &peer)?;
                    write_header(
                        &mut stream,
                        &Request::Get {
                            doc: d.id.clone(),
                            key: remote.key(),
                        },
                    )?;
                    if let Reply::Data { revision, len } = read_header(&mut stream)? {
                        if !same(&revision, remote) {
                            return Err("Versionsantwort stimmt nicht überein.".into());
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
                    let mut stream = connect(inner, &peer)?;
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
                let mut stream = connect(inner, &peer)?;
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
                        let mut stream = connect(inner, &peer)?;
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
                            "{} hat die Anfrage für {} abgelehnt.",
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
        let key = format!("{}:{}", peer.host, peer.port);
        let mut statuses = inner.statuses.lock().unwrap();
        let changed = statuses.insert(key, result.is_ok()) != Some(result.is_ok());
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
