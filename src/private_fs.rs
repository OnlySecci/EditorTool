//! Private application state; never use these permission changes on user documents.
use std::{fs, io, path::Path};
#[cfg(windows)]
pub fn windows_wide(path: &Path) -> io::Result<Vec<u16>> {
    use std::os::windows::ffi::OsStrExt;
    let absolute = if path.exists() {
        path.canonicalize()?
    } else {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        parent.canonicalize()?.join(
            path.file_name()
                .ok_or_else(|| io::Error::other("Dateiname fehlt"))?,
        )
    };
    Ok(absolute.as_os_str().encode_wide().chain(Some(0)).collect())
}
pub fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}
pub fn restrict(path: &Path, directory: bool) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if is_link(&metadata)
        || (directory && !metadata.is_dir())
        || (!directory && !metadata.is_file())
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Verknüpfung im privaten Speicher abgelehnt.",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        unsafe extern "C" {
            fn geteuid() -> u32;
        }
        if fs::symlink_metadata(path)?.uid() != unsafe { geteuid() } {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Privater Speicher gehört einem anderen Benutzer.",
            ));
        }
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(if directory { 0o700 } else { 0o600 }),
        )?;
    }
    #[cfg(windows)]
    {
        use std::{ffi::c_void, ptr};
        #[link(name = "advapi32")]
        unsafe extern "system" {
            fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
                s: *const u16,
                rev: u32,
                out: *mut *mut c_void,
                len: *mut u32,
            ) -> i32;
            fn SetFileSecurityW(path: *const u16, info: u32, descriptor: *const c_void) -> i32;
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn LocalFree(p: *mut c_void) -> *mut c_void;
        }
        let sddl = if directory {
            "D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)"
        } else {
            "D:P(A;;FA;;;OW)(A;;FA;;;SY)"
        };
        let s: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
        // canonicalize returns a verbatim (\\?\ / \\?\UNC\) absolute path on
        // Windows. Raw Win32 calls do not add Rust's long-path prefix for us.
        let path = windows_wide(path)?;
        let mut descriptor = ptr::null_mut();
        unsafe {
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                s.as_ptr(),
                1,
                &mut descriptor,
                ptr::null_mut(),
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            let ok = SetFileSecurityW(path.as_ptr(), 0x80000004, descriptor);
            let error = io::Error::last_os_error();
            LocalFree(descriptor);
            if ok == 0 {
                crate::diagnostics::event(
                    "storage",
                    &format!(
                        "private_acl kind={:?} os_error={:?} path_chars={}",
                        error.kind(),
                        error.raw_os_error(),
                        path.len().saturating_sub(1)
                    ),
                );
                return Err(error);
            }
        }
    }
    Ok(())
}
pub fn directory(path: &Path) -> io::Result<()> {
    if !path.exists() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(path)?;
        }
        #[cfg(not(unix))]
        fs::create_dir_all(path)?;
    }
    restrict(path, true)
}
pub fn create(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    restrict(path, false)?;
    Ok(file)
}
pub fn random_token() -> io::Result<String> {
    use ring::rand::{SecureRandom, SystemRandom};
    let mut bytes = [0u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| io::Error::other("Sichere Zufallsquelle nicht verfügbar."))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
