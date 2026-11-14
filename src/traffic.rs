//! Bounded application-protocol metadata, never packet payloads or document contents.
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::VecDeque,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex, OnceLock,
    },
    time::{SystemTime, UNIX_EPOCH},
};
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Entry {
    pub time: u64,
    pub peer: String,
    pub remote: String,
    pub local: String,
    pub direction: String,
    pub event: String,
    pub bytes: u64,
}
#[derive(Clone, Default)]
struct Context {
    peer: String,
    remote: String,
    local: String,
}
thread_local! {static CONTEXT:RefCell<Context>=RefCell::new(Context::default());}
struct Log {
    entries: VecDeque<Entry>,
    path: PathBuf,
    file: File,
}
static LOG: OnceLock<Mutex<Log>> = OnceLock::new();
static GENERATION: AtomicU64 = AtomicU64::new(0);
pub static VISIBLE: AtomicBool = AtomicBool::new(false);
pub fn init(root: &Path) {
    let result = (|| -> std::io::Result<Log> {
        let dir = root.join("traffic");
        crate::private_fs::directory(&dir)?;
        let path = dir.join("communication.jsonl");
        let mut entries = VecDeque::new();
        for old in [dir.join("previous.jsonl"), path.clone()] {
            if old.exists() {
                crate::private_fs::restrict(&old, false)?;
                if let Ok(bytes) = crate::limits::read(&old, 600 * 1024) {
                    for line in bytes.split(|b| *b == b'\n') {
                        if let Ok(entry) = serde_json::from_slice::<Entry>(line) {
                            entries.push_back(entry);
                            if entries.len() > 1500 {
                                entries.pop_front();
                            }
                        }
                    }
                }
            }
        }
        let file = if path.exists() {
            fs::OpenOptions::new().append(true).open(&path)?
        } else {
            crate::private_fs::create(&path)?
        };
        Ok(Log {
            entries,
            path,
            file,
        })
    })();
    match result {
        Ok(log) => {
            let _ = LOG.set(Mutex::new(log));
        }
        Err(e) => crate::diagnostics::io_failure("traffic", "open_log", &e, root),
    }
}
pub fn context(peer: &str, remote: String, local: String) {
    CONTEXT.with(|c| {
        *c.borrow_mut() = Context {
            peer: peer.into(),
            remote,
            local,
        }
    });
}
pub fn identify(peer: &str) {
    CONTEXT.with(|c| c.borrow_mut().peer = peer.into());
}
pub fn local(local: String) {
    CONTEXT.with(|c| c.borrow_mut().local = local);
}
pub fn record(direction: &str, event: &str, bytes: u64) {
    let Some(log) = LOG.get() else { return };
    let Ok(mut log) = log.lock() else { return };
    let context = CONTEXT.with(|c| c.borrow().clone());
    let entry = Entry {
        time: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        peer: context.peer,
        remote: context.remote,
        local: context.local,
        direction: direction.into(),
        event: event.into(),
        bytes,
    };
    if log.file.metadata().is_ok_and(|m| m.len() > 512 * 1024) {
        let path = log.path.clone();
        if let Ok(data) = fs::read(&path) {
            let _ = crate::session::atomic_write(&path.with_file_name("previous.jsonl"), &data);
        }
        if let Ok(file) = fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&path)
        {
            log.file = file;
        }
    }
    if let Ok(line) = serde_json::to_string(&entry) {
        let _ = writeln!(log.file, "{line}");
    }
    log.entries.push_back(entry);
    if log.entries.len() > 1500 {
        log.entries.pop_front();
    }
    drop(log);
    GENERATION.fetch_add(1, Ordering::Relaxed);
    if VISIBLE.load(Ordering::Relaxed) {
        fltk::app::awake();
    }
}
pub fn failure(error: &str) {
    let text = error.to_lowercase();
    let category =
        if text.contains("timed out") || text.contains("timeout") || text.contains("zeitlimit") {
            "error: timeout"
        } else if text.contains("refused") || text.contains("abgelehnt") {
            "error: refused"
        } else if text.contains("certificate")
            || text.contains("freigegeben")
            || text.contains("cert")
            || text.contains("tls")
        {
            "error: TLS / trust"
        } else if text.contains("os error 123") {
            "error: invalid Windows path (123)"
        } else {
            "error: connection / protocol / storage"
        };
    record("ERROR", category, 0);
}
pub fn entries() -> Vec<Entry> {
    LOG.get()
        .and_then(|l| l.lock().ok())
        .map(|l| l.entries.iter().cloned().collect())
        .unwrap_or_default()
}
pub fn generation() -> u64 {
    GENERATION.load(Ordering::Relaxed)
}
pub fn available() -> bool {
    LOG.get().is_some()
}
