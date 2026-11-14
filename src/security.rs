//! User-visible resource policies; evaluated on activity, never by an idle scanner.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    sync::{Mutex, RwLock},
};
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    pub total_mib: u32,
    pub peer_mib: u32,
    pub document_mib: u32,
    pub documents: u32,
    pub proofs: u32,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            total_mib: 1024,
            peer_mib: 512,
            document_mib: 256,
            documents: 256,
            proofs: 8192,
        }
    }
}
static READY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn ready() -> bool {
    READY.load(Ordering::Relaxed)
}
static LIMITS: RwLock<Limits> = RwLock::new(Limits {
    total_mib: 1024,
    peer_mib: 512,
    document_mib: 256,
    documents: 256,
    proofs: 8192,
});
pub fn limits() -> Limits {
    *LIMITS.read().unwrap()
}
pub fn load(root: &Path) -> io::Result<()> {
    let path = root.join("security/limits.json");
    if path.exists() {
        let bytes = crate::limits::read(&path, 4096)?;
        let value: Limits = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        validate(value)?;
        *LIMITS.write().unwrap() = value;
    }
    READY.store(true, Ordering::Relaxed);
    Ok(())
}
fn validate(v: Limits) -> io::Result<()> {
    if !(128..=16384).contains(&v.total_mib)
        || !(64..=v.total_mib).contains(&v.peer_mib)
        || !(64..=v.peer_mib).contains(&v.document_mib)
        || !(1..=1024).contains(&v.documents)
        || !(128..=65536).contains(&v.proofs)
    {
        return Err(io::Error::other("Limits: total 128–16384 MiB; peer 64–total; document 64–peer; documents 1–1024; signed versions 128–65536"));
    }
    Ok(())
}
pub fn save(root: &Path, v: Limits) -> io::Result<()> {
    validate(v)?;
    crate::session::atomic_write(
        &root.join("security/limits.json"),
        &serde_json::to_vec(&v).map_err(io::Error::other)?,
    )?;
    *LIMITS.write().unwrap() = v;
    Ok(())
}
struct Pending {
    id: u64,
    doc: String,
    peer: String,
    bytes: u64,
}
static PENDING: Mutex<Vec<Pending>> = Mutex::new(Vec::new());
static SERIAL: AtomicU64 = AtomicU64::new(1);
pub struct Reservation(u64);
pub struct CleanupGuard {
    _guard: std::sync::MutexGuard<'static, Vec<Pending>>,
}
pub fn begin_cleanup() -> Result<CleanupGuard, String> {
    let guard = PENDING.lock().unwrap();
    if !guard.is_empty() {
        return Err("Wait for active transfers before cleanup".into());
    }
    Ok(CleanupGuard { _guard: guard })
}
impl Drop for Reservation {
    fn drop(&mut self) {
        PENDING.lock().unwrap().retain(|p| p.id != self.0);
    }
}
pub fn reserve(
    root: &Path,
    doc: &str,
    peer: &str,
    bytes: u64,
    used_doc: u64,
    used_peer: u64,
) -> Result<Reservation, String> {
    let mut pending = PENDING.lock().unwrap();
    let v = limits();
    let disk = crate::storage::size(&root.join("content"))
        .map_err(|_| "Could not determine sharing storage usage")?;
    let allocated = bytes.saturating_add(bytes / 65536 * 20 + 128);
    let total = disk
        .saturating_add(pending.iter().map(|p| p.bytes).sum::<u64>())
        .saturating_add(allocated);
    let by_doc = used_doc
        .saturating_add(
            pending
                .iter()
                .filter(|p| p.doc == doc)
                .map(|p| p.bytes)
                .sum::<u64>(),
        )
        .saturating_add(allocated);
    let by_peer = used_peer
        .saturating_add(
            pending
                .iter()
                .filter(|p| p.peer == peer)
                .map(|p| p.bytes)
                .sum::<u64>(),
        )
        .saturating_add(allocated);
    if total > v.total_mib as u64 * 1048576
        || by_doc > v.document_mib as u64 * 1048576
        || by_peer > v.peer_mib as u64 * 1048576
    {
        return Err("Sharing storage quota reached. Free unused content or adjust Security limits; active documents are preserved.".into());
    }
    let id = SERIAL.fetch_add(1, Ordering::Relaxed);
    pending.push(Pending {
        id,
        doc: doc.into(),
        peer: peer.into(),
        bytes: allocated,
    });
    Ok(Reservation(id))
}
pub fn retained_bytes<'a>(
    root: &Path,
    revisions: impl Iterator<Item = &'a crate::peer::Revision>,
) -> u64 {
    let mut seen = BTreeSet::new();
    revisions
        .filter(|r| {
            seen.insert(&r.hash)
                && root
                    .join("content")
                    .join(format!("{}.txt", r.hash))
                    .is_file()
        })
        .map(|r| r.bytes)
        .sum()
}
pub fn risky_autosave(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0) {
            return true;
        }
    }
    let extension = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let executable = matches!(
        extension.as_str(),
        "rs" | "c"
            | "h"
            | "cpp"
            | "hpp"
            | "cc"
            | "cs"
            | "go"
            | "java"
            | "kt"
            | "swift"
            | "rb"
            | "php"
            | "pl"
            | "lua"
            | "r"
            | "py"
            | "pyw"
            | "ps1"
            | "psm1"
            | "psd1"
            | "sh"
            | "bash"
            | "zsh"
            | "fish"
            | "bat"
            | "cmd"
            | "vbs"
            | "js"
            | "mjs"
            | "cjs"
            | "hta"
            | "reg"
            | "desktop"
            | "service"
            | "timer"
            | "lnk"
            | "url"
            | "exe"
            | "dll"
            | "com"
            | "scr"
            | "app"
            | "scpt"
    );
    let text = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    executable
        || [
            "/startup/",
            "/autostart/",
            "/launchagents/",
            "/launchdaemons/",
            "/.ssh/",
            "/.config/systemd/",
            "/.git/hooks/",
            "/.github/workflows/",
        ]
        .iter()
        .any(|part| text.contains(part))
        || path.file_name().and_then(|v| v.to_str()).is_some_and(|n| {
            matches!(
                n,
                ".bashrc"
                    | ".bash_profile"
                    | ".zshrc"
                    | ".profile"
                    | ".zprofile"
                    | "Makefile"
                    | "makefile"
                    | "package.json"
            )
        })
}
pub fn target(path: &Path) -> io::Result<std::path::PathBuf> {
    if path.exists() {
        path.canonicalize()
    } else {
        Ok(path
            .parent()
            .unwrap_or(Path::new("."))
            .canonicalize()?
            .join(
                path.file_name()
                    .ok_or_else(|| io::Error::other("Missing target filename"))?,
            ))
    }
}
