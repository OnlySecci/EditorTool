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
        if crate::private_fs::is_link(&metadata) {
            continue;
        }
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            entries += 1;
            if entries > 100_000 {
                return Err(io::Error::other("Too many storage files for safe cleanup."));
            }
            let kind = entry.file_type()?;
            if crate::private_fs::is_link(&fs::symlink_metadata(entry.path())?) {
                continue;
            }
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
    let canonical = match path.canonicalize() {
        Ok(path) => path,
        Err(e) if e.kind() == io::ErrorKind::NotFound => path
            .parent()
            .unwrap_or(Path::new("."))
            .canonicalize()?
            .join(
                path.file_name()
                    .ok_or_else(|| io::Error::other("Filename is missing"))?,
            ),
        Err(e) => return Err(e),
    };
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
            .map_err(|_| io::Error::other("Cleanup outside storage rejected."))?;
        let mut component = root.to_owned();
        for part in relative.components() {
            if !matches!(part, std::path::Component::Normal(_)) {
                return Err(io::Error::other("Invalid cleanup path."));
            }
            component.push(part);
            if crate::private_fs::is_link(&fs::symlink_metadata(&component)?) {
                return Err(io::Error::other("Link rejected during cleanup."));
            }
        }
        let meta = fs::symlink_metadata(path)?;
        if !meta.is_file()
            || crate::private_fs::is_link(&meta)
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
/// User documents retain their ACL/mode. Portal grants cover only the selected
/// file, so a sibling temporary file is not available in that sandbox.
pub fn write_document(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    if bytes.len() as u64 > crate::limits::DOCUMENT {
        return Err(io::Error::other("Document is too large (64 MiB)."));
    }
    let existing = match fs::metadata(path) {
        Ok(m) => Some(m),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    if existing
        .as_ref()
        .is_some_and(|m| !m.is_file() || m.permissions().readonly())
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "File is read-only or is not a regular file.",
        ));
    }
    if crate::file_dialog::portal_required() {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        file.write_all(bytes)?;
        return file.sync_all();
    }
    // Follow a user-selected symlink to its file; never change that link itself.
    let target = if existing.is_some() {
        path.canonicalize()?
    } else {
        path.to_owned()
    };
    let tmp = target.with_file_name(format!(
        ".et-{}.pending",
        crate::private_fs::random_token()?
    ));
    let result = (|| -> io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        #[cfg(unix)]
        if let Some(m) = &existing {
            file.set_permissions(m.permissions())?;
        }
        #[cfg(target_os = "macos")]
        if existing.is_some() {
            use std::{ffi::c_void, os::fd::AsRawFd, ptr};
            unsafe extern "C" {
                fn fcopyfile(from: i32, to: i32, state: *mut c_void, flags: u32) -> i32;
            }
            let source = fs::File::open(&target)?;
            // COPYFILE_ACL | COPYFILE_XATTR; file bytes are written below.
            if unsafe { fcopyfile(source.as_raw_fd(), file.as_raw_fd(), ptr::null_mut(), 5) } != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        #[cfg(windows)]
        {
            use std::{ffi::c_void, ptr};
            #[link(name = "kernel32")]
            unsafe extern "system" {
                fn ReplaceFileW(
                    old: *const u16,
                    new: *const u16,
                    backup: *const u16,
                    flags: u32,
                    exclude: *mut c_void,
                    reserved: *mut c_void,
                ) -> i32;
                fn MoveFileExW(old: *const u16, new: *const u16, flags: u32) -> i32;
            }
            let to = crate::private_fs::windows_wide(&target)?;
            let from = crate::private_fs::windows_wide(&tmp)?;
            let ok = unsafe {
                if existing.is_some() {
                    ReplaceFileW(
                        to.as_ptr(),
                        from.as_ptr(),
                        ptr::null(),
                        0,
                        ptr::null_mut(),
                        ptr::null_mut(),
                    )
                } else {
                    MoveFileExW(from.as_ptr(), to.as_ptr(), 8)
                }
            };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        #[cfg(not(windows))]
        {
            fs::rename(&tmp, &target)?;
            fs::File::open(target.parent().unwrap_or(Path::new(".")))?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}
