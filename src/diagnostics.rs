//! Bounded, local diagnostics. Callers supply technical events, never document/peer content.
use fltk::dialog;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
const LIMIT: u64 = 512 * 1024;
struct Log {
    path: PathBuf,
    file: File,
}
static LOG: OnceLock<Mutex<Log>> = OnceLock::new();
fn millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
pub fn init(root: &Path) {
    let result = (|| -> io::Result<Log> {
        let dir = root.join("logs");
        crate::private_fs::directory(root)?;
        crate::private_fs::directory(&dir)?;
        let mut old: Vec<_> = fs::read_dir(&dir)?
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name().to_string_lossy().starts_with("EditorTool-")
                    && e.path().extension().is_some_and(|s| s == "log")
            })
            .collect();
        old.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
        let remove = old.len().saturating_sub(7);
        for entry in old.into_iter().take(remove) {
            let _ = fs::remove_file(entry.path());
        }
        let path = dir.join(format!(
            "EditorTool-{}-{}.log",
            millis(),
            std::process::id()
        ));
        let file = crate::private_fs::create(&path)?;
        Ok(Log { path, file })
    })();
    if let Ok(log) = result {
        let _ = LOG.set(Mutex::new(log));
    }
    event(
        "app",
        &format!(
            "start version={} os={} arch={}",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
    );
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Panic payloads may contain document text, so record source location only.
        if let Some(loc) = info.location() {
            event("panic", &format!("{}:{}", loc.file(), loc.line()));
        }
        previous(info);
    }));
}
pub fn event(component: &str, message: &str) {
    let Some(lock) = LOG.get() else {
        return;
    };
    let Ok(mut log) = lock.try_lock() else {
        return;
    };
    if log.file.metadata().is_ok_and(|m| m.len() >= LIMIT) {
        let path = log.path.clone();
        // Retain the preceding segment, with no unbounded file growth.
        if let Ok(bytes) = fs::read(&path) {
            let _ = fs::write(path.with_extension("previous.log"), bytes);
        }
        if let Ok(file) = OpenOptions::new().write(true).truncate(true).open(&path) {
            log.file = file;
        } else {
            return;
        }
    }
    let line = serde_json::json!({"time_unix_ms":millis(),"component":component,"event":message})
        .to_string();
    let _ = writeln!(log.file, "{line}");
}
// Classify internally generated errors without persisting their free-form text.
pub fn failure(component: &str, phase: &str, error: &str) {
    let text = error.to_lowercase();
    let class = if text.contains("permission")
        || text.contains("denied")
        || text.contains("verweigert")
    {
        "permission"
    } else if text.contains("timed out") || text.contains("timeout") || text.contains("zeitübersch")
    {
        "timeout"
    } else if text.contains("refused") || text.contains("abgelehnt") {
        "refused"
    } else if text.contains("certificate")
        || text.contains("cert")
        || text.contains("tls")
        || text.contains("finger")
    {
        "tls_or_identity"
    } else if text.contains("checksum") || text.contains("prüfsumme") {
        "integrity"
    } else if text.contains("429") || text.contains("rate limit") {
        "rate_limit"
    } else if text.contains("disk") || text.contains("speicher") || text.contains("space") {
        "storage"
    } else {
        "other"
    };
    event(component, &format!("{phase} class={class}"));
}
pub fn io_failure(component: &str, phase: &str, error: &io::Error, path: &Path) {
    event(
        component,
        &format!(
            "{phase} kind={:?} os_error={:?} path_chars={}",
            error.kind(),
            error.raw_os_error(),
            path.as_os_str().len()
        ),
    );
}
fn tail(path: &Path) -> io::Result<String> {
    let mut f = File::open(path)?;
    let len = f.metadata()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(64 * 1024)))?;
    let mut bytes = Vec::new();
    f.take(64 * 1024).read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
fn redact(mut text: String) -> String {
    for key in ["USERPROFILE", "HOME"] {
        if let Some(home) = std::env::var_os(key) {
            let home = home.to_string_lossy();
            if !home.is_empty() {
                text = text.replace(home.as_ref(), "<HOME>");
            }
        }
    }
    text
}
pub fn export(root: &Path, runtime: &str) -> io::Result<()> {
    let mut chooser =
        crate::file_dialog::Chooser::new(dialog::NativeFileChooserType::BrowseSaveFile);
    chooser.set_title("Diagnoseprotokoll exportieren");
    chooser.set_filter("Log\t*.log");
    chooser.set_preset_file(&format!("EditorTool-Diagnose-{}.log", millis()));
    chooser.set_option(dialog::FileDialogOptions::SaveAsConfirm);
    chooser.show();
    let destination = chooser.filename();
    if destination.as_os_str().is_empty() {
        return Ok(());
    }
    event("diagnostics", "export_requested");
    let mut report = format!("EditorTool {} diagnostics\nos={} arch={}\n{}\nNo document contents, clipboard, credentials or automatic upload.\n", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH, runtime);
    let mut files: Vec<PathBuf> = fs::read_dir(root.join("logs"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("EditorTool-"))
                && p.extension().is_some_and(|s| s == "log")
        })
        .collect();
    files.sort();
    for path in files.iter().rev().take(8) {
        if let Ok(body) = tail(path) {
            report.push_str(&format!(
                "\n--- {} (last 64 KiB) ---\n{}",
                path.file_name().unwrap().to_string_lossy(),
                body
            ));
        }
    }
    if let Ok(body) = tail(&root.join("updates/last-result.txt.log")) {
        report.push_str("\n--- Update installer (last 64 KiB) ---\n");
        report.push_str(&body);
    }
    fs::write(destination, redact(report))
}
pub fn reveal(path: &Path) -> io::Result<()> {
    use std::process::Command;
    #[cfg(windows)]
    {
        Command::new("explorer.exe")
            .arg("/select,")
            .arg(path)
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("/usr/bin/open").arg("-R").arg(path).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(path.parent().unwrap_or(path))
            .spawn()?;
    }
    Ok(())
}
