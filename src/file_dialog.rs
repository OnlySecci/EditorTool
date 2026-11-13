//! Flatpak file grants are obtained from the desktop portal; other platforms use FLTK.
use fltk::dialog;
use std::path::PathBuf;
pub fn portal_required() -> bool {
    cfg!(target_os = "linux") && std::path::Path::new("/.flatpak-info").exists()
}
pub struct Chooser {
    native: dialog::NativeFileChooser,
    #[cfg(target_os = "linux")]
    save: bool,
    title: String,
    preset: String,
    selected: PathBuf,
}
impl Chooser {
    pub fn new(kind: dialog::NativeFileChooserType) -> Self {
        Self {
            native: dialog::NativeFileChooser::new(kind),
            #[cfg(target_os = "linux")]
            save: matches!(kind, dialog::NativeFileChooserType::BrowseSaveFile),
            title: String::new(),
            preset: String::new(),
            selected: PathBuf::new(),
        }
    }
    pub fn set_title(&mut self, title: &str) {
        self.title = title.into();
        self.native.set_title(title);
    }
    pub fn set_filter(&mut self, filter: &str) {
        self.native.set_filter(filter);
    }
    pub fn set_preset_file(&mut self, name: &str) {
        self.preset = name.into();
        self.native.set_preset_file(name);
    }
    pub fn set_option(&mut self, option: dialog::NativeFileChooserOptions) {
        self.native.set_option(option);
    }
    pub fn show(&mut self) {
        if portal_required() {
            #[cfg(target_os = "linux")]
            {
                let (tx, rx) = std::sync::mpsc::channel();
                let save = self.save;
                let title = self.title.clone();
                let preset = self.preset.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(portal(save, &title, &preset));
                    fltk::app::awake();
                });
                loop {
                    match rx.try_recv() {
                        Ok(Ok(path)) => {
                            self.selected = path.unwrap_or_default();
                            break;
                        }
                        Ok(Err(error)) => {
                            dialog::alert_default(&format!(
                                "Dateiauswahl-Portal nicht verfügbar: {error}"
                            ));
                            break;
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            fltk::app::wait();
                        }
                    }
                }
            }
        } else {
            self.native.show();
            self.selected = self.native.filename();
        }
    }
    pub fn filename(&self) -> PathBuf {
        self.selected.clone()
    }
}
#[cfg(target_os = "linux")]
fn portal(save: bool, title: &str, preset: &str) -> Result<Option<PathBuf>, String> {
    use std::collections::HashMap;
    use zbus::{
        blocking::{Connection, Proxy},
        zvariant::{OwnedObjectPath, OwnedValue, Value},
    };
    let run = || -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
        let connection = Connection::session()?;
        let token = format!("editortool_{}", crate::private_fs::random_token()?);
        let sender = connection
            .unique_name()
            .ok_or("Session-Bus ohne Namen")?
            .as_str()
            .trim_start_matches(':')
            .replace('.', "_");
        let path = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
        let request = Proxy::new(
            &connection,
            "org.freedesktop.portal.Desktop",
            path.as_str(),
            "org.freedesktop.portal.Request",
        )?;
        // Subscribe before opening the chooser so an immediate response cannot be lost.
        let mut responses = request.receive_signal("Response")?;
        let chooser = Proxy::new(
            &connection,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.FileChooser",
        )?;
        let mut options = HashMap::new();
        options.insert("handle_token", Value::from(token.as_str()));
        options.insert("modal", Value::from(true));
        if save && !preset.is_empty() {
            options.insert("current_name", Value::from(preset));
        }
        if !save {
            options.insert("writable", Value::from(true));
            options.insert("multiple", Value::from(false));
        }
        let returned: OwnedObjectPath = chooser.call(
            if save { "SaveFile" } else { "OpenFile" },
            &("", title, options),
        )?;
        if returned.as_str() != path {
            return Err("Unerwartete Portal-Anfragekennung".into());
        }
        let message = responses
            .next()
            .ok_or("Portal hat die Verbindung beendet")?;
        let (code, mut result): (u32, HashMap<String, OwnedValue>) =
            message.body().deserialize()?;
        if code == 1 {
            return Ok(None);
        }
        if code != 0 {
            return Err("Dateiauswahl wurde vom Portal abgelehnt".into());
        }
        let uris = Vec::<String>::try_from(result.remove("uris").ok_or("Dateipfad fehlt")?)?;
        if uris.len() != 1 {
            return Err("Genau eine lokale Datei auswählen".into());
        }
        let path = url::Url::parse(&uris[0])?
            .to_file_path()
            .map_err(|_| "Nur lokale Dateien werden unterstützt")?;
        Ok(Some(path))
    };
    run().map_err(|e| e.to_string())
}
