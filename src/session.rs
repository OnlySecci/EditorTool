use crate::syntax::Language;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TabState {
    #[serde(default)]
    pub page: u8,
    #[serde(default = "default_zoom")]
    pub zoom: i32,
    #[serde(default = "default_zoom")]
    pub right_zoom: i32,
    #[serde(default)]
    pub sharing: Option<crate::peer::Binding>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub received_offer: Option<String>,
    pub key: String,
    pub id: usize,
    pub path: Option<PathBuf>,
    pub right_path: Option<PathBuf>,
    pub dirty: bool,
    pub left_language: Option<Language>,
    pub right_language: Option<Language>,
    pub comparison: bool,
    pub left_cursor: i32,
    pub right_cursor: i32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WindowState {
    pub tabs: Vec<TabState>,
    pub active: usize,
    pub rect: (i32, i32, i32, i32),
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: i32,
    #[serde(default)]
    pub split_key: Option<String>,
    #[serde(default)]
    pub split_stacked: bool,
    #[serde(default = "default_split_ratio")]
    pub split_ratio: i32,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    pub windows: Vec<WindowState>,
    #[serde(default)]
    pub closed: Vec<WindowState>,
}
fn default_zoom() -> i32 {
    100
}
fn default_sidebar_width() -> i32 {
    184
}
fn default_split_ratio() -> i32 {
    500
}

pub type TextWrites = Vec<(String, String)>;
enum Job {
    Save(Snapshot, TextWrites),
    Flush(mpsc::Sender<Result<(), String>>),
}
pub struct Store {
    _lock: File,
    root: PathBuf,
    tx: mpsc::Sender<Job>,
    pub revisions: HashMap<String, (u64, u64)>,
}

pub fn new_key() -> String {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    format!(
        "{:x}-{:x}-{:x}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    )
}
fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.len() < 120 && key.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}

pub(crate) fn atomic_write(path: &Path, data: &[u8]) -> io::Result<()> {
    let tmp = path.with_file_name(format!(
        ".et-{}.pending",
        crate::private_fs::random_token()?
    ));
    struct Pending(PathBuf);
    impl Drop for Pending {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    let _cleanup = Pending(tmp.clone());
    {
        let mut file = crate::private_fs::create(&tmp)?;
        file.write_all(data)?;
        file.sync_all()?;
    }
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
        }
        let from = crate::private_fs::windows_wide(&tmp)?;
        let to = crate::private_fs::windows_wide(path)?;
        if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 1 | 8) } == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    #[cfg(not(windows))]
    {
        fs::rename(&tmp, path)?;
        if let Some(parent) = path.parent() {
            File::open(parent)?.sync_all()?;
        }
    }
    Ok(())
}

impl Store {
    pub fn orphan_files(&self, current: &Snapshot) -> io::Result<Vec<PathBuf>> {
        self.flush().map_err(io::Error::other)?;
        let mut keep = std::collections::BTreeSet::new();
        let mut remember = |snapshot: &Snapshot| {
            for tab in snapshot
                .windows
                .iter()
                .chain(&snapshot.closed)
                .flat_map(|w| &w.tabs)
            {
                keep.insert(format!("{}.left.txt", tab.key));
                keep.insert(format!("{}.right.txt", tab.key));
            }
        };
        remember(current);
        for name in ["session.json", "session.previous.json"] {
            let path = self.root.join(name);
            if path.exists() {
                let snapshot: Snapshot =
                    serde_json::from_slice(&crate::limits::read(&path, 4 * 1024 * 1024)?)?;
                if snapshot.version != 1 {
                    return Err(io::Error::other(
                        "Unknown session version; cleanup blocked.",
                    ));
                }
                remember(&snapshot);
            }
        }
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let key = name
                .strip_suffix(".left.txt")
                .or_else(|| name.strip_suffix(".right.txt"));
            if key.is_some_and(valid_key)
                && !keep.contains(name.as_ref())
                && entry.file_type()?.is_file()
            {
                files.push(entry.path());
            }
        }
        Ok(files)
    }
    pub fn new(root: PathBuf, errors: fltk::app::Sender<crate::Message>) -> Result<Self, String> {
        crate::private_fs::directory(&root).map_err(|e| e.to_string())?;
        for entry in fs::read_dir(&root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if crate::private_fs::is_link(
                &fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?,
            ) {
                return Err("Link in session cache rejected.".into());
            }
            if entry.file_type().map_err(|e| e.to_string())?.is_file() {
                crate::private_fs::restrict(&entry.path(), false).map_err(|e| e.to_string())?;
            }
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("session.lock"))
            .map_err(|e| e.to_string())?;
        lock.try_lock().map_err(|_|"The session cache is already open. Open additional windows using File → New window (Ctrl/Cmd+N).".to_owned())?;
        crate::private_fs::restrict(&root.join("session.lock"), false)
            .map_err(|e| e.to_string())?;
        let (tx, rx) = mpsc::channel();
        let directory = root.clone();
        thread::spawn(move || {
            let mut last_error: Option<String> = None;
            while let Ok(job) = rx.recv() {
                match job {
                    Job::Save(snapshot, texts) => {
                        let result = (|| -> io::Result<()> {
                            fs::create_dir_all(&directory)?;
                            for (name, text) in texts {
                                atomic_write(&directory.join(name), text.as_bytes())?;
                            }
                            let manifest = directory.join("session.json");
                            if let Ok(previous) = crate::limits::read(&manifest, 4 * 1024 * 1024) {
                                atomic_write(&directory.join("session.previous.json"), &previous)?;
                            }
                            atomic_write(&manifest, &serde_json::to_vec(&snapshot)?)
                        })();
                        match result {
                            Ok(()) => last_error = None,
                            Err(e) => {
                                crate::diagnostics::event(
                                    "session",
                                    &format!("save_failed kind={:?}", e.kind()),
                                );
                                let message = e.to_string();
                                if last_error.as_ref() != Some(&message) {
                                    errors.send((0, crate::Action::CacheError(message.clone())));
                                }
                                last_error = Some(message);
                            }
                        }
                    }
                    Job::Flush(reply) => {
                        let _ = reply.send(last_error.clone().map_or(Ok(()), Err));
                    }
                }
            }
        });
        Ok(Self {
            _lock: lock,
            root,
            tx,
            revisions: HashMap::new(),
        })
    }
    pub fn load(&self) -> Result<Snapshot, String> {
        let primary = self.root.join("session.json");
        if !primary.exists() {
            return Ok(Snapshot::default());
        }
        for file in [primary, self.root.join("session.previous.json")] {
            if let Ok(bytes) = crate::limits::read(&file, 4 * 1024 * 1024) {
                if let Ok(snapshot) = serde_json::from_slice::<Snapshot>(&bytes) {
                    if snapshot.version == 1
                        && snapshot
                            .windows
                            .iter()
                            .chain(snapshot.closed.iter())
                            .flat_map(|w| &w.tabs)
                            .all(|t| valid_key(&t.key))
                    {
                        return Ok(snapshot);
                    }
                }
            }
        }
        Err("Could not read the session cache; existing files are preserved.".into())
    }
    pub fn text(&self, key: &str, right: bool) -> io::Result<String> {
        if !valid_key(key) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid cache key",
            ));
        }
        crate::limits::read_text(&self.root.join(format!(
            "{key}.{}.txt",
            if right { "right" } else { "left" }
        )))
    }
    pub fn save(&self, snapshot: Snapshot, texts: TextWrites) {
        let _ = self.tx.send(Job::Save(snapshot, texts));
    }
    pub fn flush(&self) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.tx.send(Job::Flush(tx)).map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?
    }
}
