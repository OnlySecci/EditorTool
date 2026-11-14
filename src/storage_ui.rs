use crate::*;
/// Persistent management pages use the same tab surface as the other settings.
pub struct Panel {
    pub win: Window,
    pub page: settings_page::Page,
    text: TextBuffer,
}
impl Panel {
    pub fn new(tx: app::Sender<Message>, permissions: bool) -> Self {
        Group::set_current(None::<&Group>);
        let mut win = Window::default().with_size(660, 620);
        win.set_color(PANEL);
        let mut root = Flex::new(20, 20, 620, 580, None).column();
        root.set_spacing(12);
        let title = theme::heading(
            if permissions {
                "Sharing and Permissions"
            } else {
                "Storage and File History"
            },
            22,
        );
        root.fixed(&title, 36);
        let mut subtitle = Frame::default().with_label(if permissions {
            "Device approval and document consent remain separate.\nSelect the document tab to stop its sync."
        } else {
            "Sizes are calculated only on opening or refreshing.\nActive sessions and sync data are not cleaned up."
        });
        subtitle.set_align(Align::Left | Align::Inside | Align::Wrap);
        subtitle.set_label_color(MUTED);
        subtitle.set_label_size(12);
        root.fixed(&subtitle, 52);
        let text = TextBuffer::default();
        let mut view = fltk::text::TextDisplay::default();
        view.set_buffer(text.clone());
        view.set_frame(FrameType::FlatBox);
        view.set_color(BG);
        view.set_text_color(FG);
        view.set_text_size(13);
        view.set_text_font(Font::Helvetica);
        view.wrap_mode(fltk::text::WrapMode::AtBounds, 0);
        view.set_scrollbar_size(14);
        let mut row = Flex::default().row();
        row.set_spacing(8);
        for (label, action) in if permissions {
            vec![
                ("Refresh", Action::Permissions),
                ("Clients verwalten…", Action::SharingSettings),
            ]
        } else {
            vec![
                ("Refresh", Action::Storage),
                ("Select cleanup category…", Action::StorageClean),
            ]
        } {
            let mut button = Button::default().with_label(label);
            theme::button(&mut button, false, None);
            button.emit(tx, (0, action));
        }
        row.end();
        root.fixed(&row, 36);
        root.end();
        win.end();
        win.set_callback(|w| w.hide());
        let page = settings_page::Page::new(&mut win, &root);
        Self { win, page, text }
    }
    fn set(&mut self, text: &str) {
        self.text.set_text(text);
    }
}
impl Runtime {
    pub fn open_management_tab(&mut self, wid: usize, page: u8) {
        if self.windows.iter().all(Option::is_none) {
            self.handle(0, Action::NewWindow);
        }
        let existing = self.windows.iter().enumerate().find_map(|(wi, w)| {
            w.as_ref().and_then(|w| {
                w.docs
                    .iter()
                    .position(|d| d.page == page)
                    .map(|di| (wi, di))
            })
        });
        let wi = if let Some((wi, di)) = existing {
            self.windows[wi]
                .as_mut()
                .unwrap()
                .action(Action::Select(di), wi, self.tx);
            wi
        } else {
            let wi = if self.windows.get(wid).is_some_and(Option::is_some) {
                wid
            } else {
                self.windows.iter().position(Option::is_some).unwrap()
            };
            self.windows[wi]
                .as_mut()
                .unwrap()
                .open_settings_tab(page, wi, self.tx);
            wi
        };
        if page == 6 {
            let peers = self
                .sharing
                .as_ref()
                .map(|s| s.config().clients)
                .unwrap_or_default();
            self.traffic_panel.refresh(&peers, true);
            let w = self.windows[wi].as_mut().unwrap();
            if !w.embedded {
                w.win.show();
                chrome::dark_title(&w.win);
                shutdown::hook(&w.win, self.weak_self.clone());
            }
            return;
        }
        let details = if page == 4 {
            (|| -> Result<String,String> {
                self.persist(); self.store.flush()?;
                let root=self.config.parent().unwrap(); let mut text=String::new();
                for (dir,label) in [("session","Session including unsaved tabs"),("history","Older file versions"),("sharing","Sharing and sync revisions"),("updates","Update packages"),("logs","Diagnostic logs")] {
                    text.push_str(&format!("{label}\n{:.2} MiB\n\n",storage::size(&root.join(dir)).map_err(|e|e.to_string())? as f64/1048576.0));
                }
                text.push_str("Previous file contents are backed up before saving (up to 10 versions / 256 MiB per file).\nFile → Older file versions opens a version as a new tab.\n\nCleanup is permanent and does not promise secure physical erasure. Personal files are not deleted."); Ok(text)
            })().unwrap_or_else(|e|crate::tr_format!("Could not calculate all storage:\n{e}"))
        } else {
            let mut text=String::from("Connection protection\nTLS 1.3 · AES-256-GCM · X25519 + ML-KEM-768\nDevice identities: classical signatures with fingerprint approval.\n\nUpdates\nEd25519-signed metadata with expiry and version protection.\n\nLocal data\nSessions and history are unencrypted, protected by user permissions. No automatic diagnostic uploads.\n\n");
            if file_dialog::portal_required() {
                text.push_str("Flatpak\nFile access through individual portal grants. Network, X11, tray and the host call for updates remain enabled.\n\n");
            }
            text.push_str(
                &self
                    .sharing
                    .as_ref()
                    .map(peer::Service::permissions)
                    .unwrap_or_else(|| "Send/Sync unavailable.".into()),
            );
            text
        };
        if page == 4 {
            self.storage_panel.set(&details);
        } else {
            self.permissions_panel.set(&details);
        }
        let w = self.windows[wi].as_mut().unwrap();
        if !w.embedded {
            w.win.show();
            chrome::dark_title(&w.win);
            shutdown::hook(&w.win, self.weak_self.clone());
        }
    }
    pub fn storage_dialog(&mut self) {
        let result = (|| -> Result<(), String> {
            self.persist();
            self.store.flush()?;
            let root = self.config.parent().unwrap().to_owned();
            let mut details = String::new();
            for (dir, label) in [
                ("session", "Session including unsaved tabs"),
                ("history", "Older file versions"),
                ("sharing", "Approvals, device identity and sync revisions"),
                ("updates", "Update packages"),
                ("logs", "Diagnostic logs"),
            ] {
                let size = storage::size(&root.join(dir)).map_err(|e| e.to_string())?;
                details.push_str(&format!("{label}: {:.2} MiB\n", size as f64 / 1048576.0));
            }
            let choice = chrome::decision(None, "Storage and cleanup", "Sessions, approvals and device identity are preserved. Cleanup removes only the chosen category; deleted history cannot be recovered.", &details, &["Close", "Orphaned tabs", "File history", "Update packages"], 0);
            let (selected, category) = match choice {
                Some(1) => (
                    self.store
                        .orphan_files(&self.snapshot())
                        .map_err(|e| e.to_string())?,
                    "verwaiste Tab-Sicherungen",
                ),
                Some(2) => (
                    storage::files(&root.join("history")).map_err(|e| e.to_string())?,
                    "older file versions",
                ),
                Some(3) if !self.update_busy => (
                    storage::files(&root.join("updates")).map_err(|e| e.to_string())?,
                    "downloaded update files",
                ),
                Some(3) => return Err("Update files cannot be cleaned up during an update.".into()),
                _ => return Ok(()),
            };
            if selected.is_empty() {
                dialog::message_default("No files in this category.");
                return Ok(());
            }
            let bytes: u64 = selected
                .iter()
                .map(|p| std::fs::symlink_metadata(p).map(|m| m.len()).unwrap_or(0))
                .sum();
            if chrome::decision(None, "Permanently delete files?", &crate::tr_format!("{} files ({:.2} MiB): {category}. User files and current sessions are preserved.", selected.len(), bytes as f64 / 1048576.0), "", &["Cancel", "Delete permanently"], 0) == Some(1) {
                let n = storage::remove_files(&root, &selected).map_err(|e| e.to_string())?;
                dialog::message_default(&crate::tr_format!("Removed {n} files."));
            }
            Ok(())
        })();
        if let Err(error) = result {
            dialog::alert_default(&error);
        }
    }
    pub fn history_dialog(&mut self, wid: usize) {
        let result = (|| -> Result<(), String> {
            let path = self
                .windows
                .get(wid)
                .and_then(Option::as_ref)
                .and_then(|w| w.docs[w.active].path.clone())
                .ok_or("Select a saved file.")?;
            let root = self.config.parent().unwrap().to_owned();
            let versions = storage::versions(&root, &path).map_err(|e| e.to_string())?;
            if versions.is_empty() {
                dialog::message_default("No older file versions yet. Saving backs up the previous file contents locally (up to 10 versions / 256 MiB per file).");
                return Ok(());
            }
            let labels: Vec<String> = versions.iter().map(|p| storage::describe(p)).collect();
            let Some(index) = chrome::select_version(&labels) else {
                return Ok(());
            };
            let text = limits::read_text(&versions[index]).map_err(|e| e.to_string())?;
            rich::validate_snapshot(&text)?;
            let w = self.windows[wid].as_mut().unwrap();
            w.add_tab(wid, self.tx);
            let d = &mut w.docs[w.active];
            d.left.set_cache_text(&text)?;
            d.display_name = Some(format!(
                "Wiederhergestellt – {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ));
            d.saved_revision = u64::MAX;
            d.report = "Older file version · Save as a new file".into();
            Ok(())
        })();
        if let Err(error) = result {
            dialog::alert_default(&error);
        }
    }
}
