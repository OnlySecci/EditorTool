//! On-demand storage accounting and bounded local file history. No idle scan.
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
pub fn files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_owned()];
    let mut entries = 0;
    while let Some(dir) = pending.pop() {
        let metadata = match fs::symlink_metadata(&dir) {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            entries += 1;
            if entries > 100_000 {
                return Err(io::Error::other(
                    "Zu viele Speicherdateien für eine sichere Bereinigung.",
                ));
            }
            let kind = entry.file_type()?;
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                files.push(entry.path());
            }
        }
    }
    Ok(files)
}
pub fn size(root: &Path) -> io::Result<u64> {
    files(root)?.iter().try_fold(0u64, |total, p| {
        Ok(total.saturating_add(fs::symlink_metadata(p)?.len()))
    })
}
fn directory(root: &Path, path: &Path) -> io::Result<PathBuf> {
    let canonical = path.canonicalize()?;
    Ok(root
        .join("history")
        .join(crate::peer::hash(canonical.as_os_str().as_encoded_bytes())))
}
pub fn versions(root: &Path, path: &Path) -> io::Result<Vec<PathBuf>> {
    let dir = directory(root, path)?;
    let mut list = if dir.exists() {
        files(&dir)?
    } else {
        Vec::new()
    };
    list.retain(|p| p.extension().is_some_and(|e| e == "snapshot"));
    list.sort();
    list.reverse();
    Ok(list)
}
/// Preserve the bytes being replaced, before touching the user's file.
pub fn preserve(root: &Path, path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let bytes = crate::limits::read(path, crate::limits::DOCUMENT)?;
    let dir = directory(root, path)?;
    crate::private_fs::directory(&root.join("history"))?;
    crate::private_fs::directory(&dir)?;
    let hash = crate::peer::hash(&bytes);
    let old = versions(root, path)?;
    if old.first().is_some_and(|p| {
        p.file_stem()
            .is_some_and(|n| n.to_string_lossy().ends_with(&hash))
    }) {
        return Ok(());
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    crate::session::atomic_write(&dir.join(format!("{now:030}-{hash}.snapshot")), &bytes)?;
    let mut total = 0u64;
    for (i, entry) in versions(root, path)?.into_iter().enumerate() {
        total = total.saturating_add(fs::symlink_metadata(&entry)?.len());
        if i >= 10 || total > 256 * 1024 * 1024 {
            fs::remove_file(entry)?;
        }
    }
    Ok(())
}
pub fn describe(path: &Path) -> String {
    let stamp = fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let date = time::OffsetDateTime::from_unix_timestamp(stamp as i64)
        .map(|d| {
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02} UTC",
                d.year(),
                u8::from(d.month()),
                d.day(),
                d.hour(),
                d.minute()
            )
        })
        .unwrap_or_default();
    format!(
        "{date} · {:.1} KiB",
        fs::metadata(path).map(|m| m.len()).unwrap_or(0) as f64 / 1024.0
    )
}
pub fn remove_files(root: &Path, selected: &[PathBuf]) -> io::Result<usize> {
    let canonical = root.canonicalize()?;
    let mut count = 0;
    for path in selected {
        let relative = path
            .strip_prefix(root)
            .map_err(|_| io::Error::other("Bereinigung ausserhalb des Speichers abgelehnt."))?;
        let mut component = root.to_owned();
        for part in relative.components() {
            if !matches!(part, std::path::Component::Normal(_)) {
                return Err(io::Error::other("Ungültiger Bereinigungspfad."));
            }
            component.push(part);
            if fs::symlink_metadata(&component)?.file_type().is_symlink() {
                return Err(io::Error::other(
                    "Verknüpfung bei der Bereinigung abgelehnt.",
                ));
            }
        }
        let meta = fs::symlink_metadata(path)?;
        if !meta.is_file()
            || meta.file_type().is_symlink()
            || !path.canonicalize()?.starts_with(&canonical)
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Unsicherer Bereinigungspfad.",
            ));
        }
        fs::remove_file(path)?;
        count += 1;
    }
    Ok(count)
}
