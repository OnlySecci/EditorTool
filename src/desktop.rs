pub const APP_ID: &str = "com.github.OnlySecci.EditorTool";
/// Package installations ship the same desktop identity. Portable Linux builds
/// register a user-local launcher so Plasma can offer its normal pin action.
#[cfg(target_os = "linux")]
pub fn integrate() -> Result<(), String> {
    use std::{env, fs, path::PathBuf};
    if env::var_os("FLATPAK_ID").is_some() || std::path::Path::new("/.flatpak-info").exists() {
        return Ok(());
    }
    let home = env::var_os("HOME").ok_or("HOME is missing")?;
    let data = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(home).join(".local/share"));
    let mut dirs = vec![data.clone()];
    dirs.extend(
        env::var_os("XDG_DATA_DIRS")
            .map(|v| env::split_paths(&v).collect::<Vec<_>>())
            .unwrap_or_else(|| {
                vec![
                    PathBuf::from("/usr/local/share"),
                    PathBuf::from("/usr/share"),
                ]
            }),
    );
    let entry = data.join("applications").join(format!("{APP_ID}.desktop"));
    for dir in dirs {
        let path = dir.join("applications").join(format!("{APP_ID}.desktop"));
        if path.exists()
            && fs::read_to_string(&path).is_ok_and(|s| !s.contains("X-EditorTool-Portable=true"))
        {
            if path != entry
                && fs::read_to_string(&entry)
                    .is_ok_and(|s| s.contains("X-EditorTool-Portable=true"))
            {
                fs::remove_file(&entry).map_err(|e| e.to_string())?;
            }
            return Ok(());
        }
    }
    let exe = env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.to_str().ok_or("Executable path is not UTF-8")?;
    if exe.contains(['\n', '\r']) {
        return Err("Executable path contains a newline".into());
    }
    let icon = data
        .join("icons/hicolor/256x256/apps")
        .join(format!("{APP_ID}.png"));
    fs::create_dir_all(icon.parent().unwrap()).map_err(|e| e.to_string())?;
    if !icon.exists() {
        fs::write(&icon, include_bytes!("../assets/editortool.png")).map_err(|e| e.to_string())?;
    }
    let quoted = exe
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('\\', "\\\\")
        .replace('%', "%%");
    let content=format!("[Desktop Entry]\nType=Application\nName=EditorTool\nComment=Text, code and images\nExec=\"{quoted}\" %F\nIcon={APP_ID}\nTerminal=false\nCategories=Utility;TextEditor;\nMimeType=application/x-editortool;\nStartupWMClass={APP_ID}\nStartupNotify=true\nX-EditorTool-Portable=true\n");
    fs::create_dir_all(entry.parent().unwrap()).map_err(|e| e.to_string())?;
    if fs::read_to_string(&entry).ok().as_deref() != Some(&content) {
        crate::session::atomic_write(&entry, content.as_bytes()).map_err(|e| e.to_string())?;
    }
    Ok(())
}
