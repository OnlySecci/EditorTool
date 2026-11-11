use crate::{Action, Message};
#[cfg(not(target_os = "linux"))]
pub struct Native {
    _icon: tray_icon::TrayIcon,
}
#[cfg(not(target_os = "linux"))]
impl Native {
    pub fn new(tx: fltk::app::Sender<Message>) -> Result<Self, String> {
        use tray_icon::{
            menu::{Menu, MenuEvent, MenuItem},
            TrayIconBuilder,
        };
        let menu = Menu::new();
        let show = MenuItem::new("EditorTool öffnen", true, None);
        let clients = MenuItem::new("Clients und Sync", true, None);
        let quit = MenuItem::new("Beenden", true, None);
        menu.append_items(&[&show, &clients, &quit])
            .map_err(|e| e.to_string())?;
        let show_id = show.id().clone();
        let clients_id = clients.id().clone();
        let quit_id = quit.id().clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let action = if event.id == show_id {
                Some(Action::ShowWindows)
            } else if event.id == clients_id {
                Some(Action::SharingSettings)
            } else if event.id == quit_id {
                Some(Action::QuitAll)
            } else {
                None
            };
            if let Some(action) = action {
                tx.send((0, action));
            }
        }));
        let icon = tray_icon::Icon::from_rgba(
            include_bytes!("../assets/editortool.rgba").to_vec(),
            64,
            64,
        )
        .map_err(|e| e.to_string())?;
        let icon = TrayIconBuilder::new()
            .with_icon(icon)
            .with_tooltip("EditorTool – Senden und Sync")
            .with_menu(Box::new(menu))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self { _icon: icon })
    }
}
#[cfg(target_os = "linux")]
pub struct Native {
    handle: ksni::blocking::Handle<LinuxTray>,
}
#[cfg(target_os = "linux")]
struct LinuxTray {
    tx: fltk::app::Sender<Message>,
}
#[cfg(target_os = "linux")]
impl std::fmt::Debug for LinuxTray {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EditorTool tray")
    }
}
#[cfg(target_os = "linux")]
impl ksni::Tray for LinuxTray {
    fn id(&self) -> String {
        "EditorTool".into()
    }
    fn title(&self) -> String {
        "EditorTool".into()
    }
    fn activate(&mut self, _: i32, _: i32) {
        self.tx.send((0, Action::ShowWindows));
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        let mut bytes = include_bytes!("../assets/editortool.rgba").to_vec();
        for pixel in bytes.chunks_exact_mut(4) {
            pixel.rotate_right(1);
        }
        vec![ksni::Icon {
            width: 64,
            height: 64,
            data: bytes,
        }]
    }
    fn watcher_offline(&self, _: ksni::OfflineReason) -> bool {
        self.tx.send((0, Action::TrayUnavailable));
        false
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        vec![
            StandardItem {
                label: "EditorTool öffnen".into(),
                activate: Box::new(|t: &mut Self| t.tx.send((0, Action::ShowWindows))),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Clients und Sync".into(),
                activate: Box::new(|t: &mut Self| t.tx.send((0, Action::SharingSettings))),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Beenden".into(),
                activate: Box::new(|t: &mut Self| t.tx.send((0, Action::QuitAll))),
                ..Default::default()
            }
            .into(),
        ]
    }
}
#[cfg(target_os = "linux")]
impl Native {
    pub fn new(tx: fltk::app::Sender<Message>) -> Result<Self, String> {
        use ksni::blocking::TrayMethods;
        Ok(Self {
            handle: LinuxTray { tx }
                .disable_dbus_name(std::env::var_os("FLATPAK_ID").is_some())
                .spawn()
                .map_err(|e| e.to_string())?,
        })
    }
}
#[cfg(target_os = "linux")]
impl Drop for Native {
    fn drop(&mut self) {
        self.handle.shutdown().wait();
    }
}
