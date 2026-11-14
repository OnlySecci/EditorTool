//! Security policy, recovery and revocation controls share the editor's settings tabs.
use crate::*;
pub struct Panel {
    pub win: Window,
    pub page: settings_page::Page,
    summary: TextBuffer,
    fields: Vec<fltk::input::Input>,
    pub fingerprint: fltk::input::Input,
}
impl Panel {
    pub fn new(tx: app::Sender<Message>) -> Self {
        Group::set_current(None::<&Group>);
        let mut win = Window::default()
            .with_size(760, 900)
            .with_label("Security — EditorTool");
        win.set_color(PANEL);
        let mut root = Flex::new(20, 20, 720, 1090, None).column();
        root.set_spacing(10);
        let title = theme::heading("Security", 22);
        root.fixed(&title, 34);
        let summary = TextBuffer::default();
        let mut view = fltk::text::TextDisplay::default();
        view.set_buffer(summary.clone());
        view.set_color(BG);
        view.set_text_color(FG);
        view.set_text_size(12);
        view.wrap_mode(fltk::text::WrapMode::AtBounds, 0);
        root.fixed(&view, 180);
        let mut export = Button::default().with_label("Export encrypted recovery key…");
        theme::button(&mut export, false, None);
        export.emit(tx, (0, Action::SecurityRecovery));
        root.fixed(&export, 34);
        let caption = theme::heading(
            "Sharing storage limits (MiB; lowering limits never deletes active documents)",
            13,
        );
        root.fixed(&caption, 22);
        let mut fields = vec![];
        for name in [
            "Total stored sync content (MiB)",
            "Stored content per author/device (MiB)",
            "Stored content per document (MiB)",
            "Maximum sharing documents",
            "Maximum signed provenance nodes",
        ] {
            let title = theme::heading(name, 12);
            root.fixed(&title, 18);
            let mut input = fltk::input::Input::default();
            theme::input(&mut input);
            root.fixed(&input, 30);
            fields.push(input);
        }
        let mut save = Button::default().with_label("Apply security limits");
        theme::button(&mut save, true, None);
        save.emit(tx, (0, Action::SecuritySave));
        root.fixed(&save, 34);
        let mut cleanup =
            Button::default().with_label("Clean retired sync groups and unused contents…");
        theme::button(&mut cleanup, false, None);
        cleanup.emit(tx, (0, Action::SecurityClean));
        root.fixed(&cleanup, 34);
        let label = theme::heading(
            "Unblock an exact revoked fingerprint (pairing approval is still required)",
            12,
        );
        root.fixed(&label, 22);
        let mut fingerprint = fltk::input::Input::default();
        theme::input(&mut fingerprint);
        root.fixed(&fingerprint, 32);
        let mut unblock =
            Button::default().with_label("Allow this fingerprint to request pairing…");
        theme::button(&mut unblock, false, None);
        unblock.emit(tx, (0, Action::SecurityUnblock));
        root.fixed(&unblock, 34);
        let mut rotate = Button::default().with_label("Replace this device's identity…");
        theme::button(&mut rotate, false, None);
        rotate.emit(tx, (0, Action::SecurityRotate));
        root.fixed(&rotate, 34);
        let mut footer=Frame::default().with_label("Identity replacement stops local sharing. Other devices must explicitly revoke the old fingerprint and approve the new one. Restart to unlock locked data or apply identity replacement.");
        footer.set_align(Align::Left | Align::Inside | Align::Wrap);
        footer.set_label_size(12);
        footer.set_label_color(MUTED);
        root.end();
        win.end();
        let page = settings_page::Page::new(&mut win, &root);
        Self {
            win,
            page,
            summary,
            fields,
            fingerprint,
        }
    }
    pub fn refresh(&mut self, service: Option<&peer::Service>) {
        let mut text=format!("Local protection\n{}\n\nSuccessfully migrated session cache, local history, sharing state and device key use authenticated encryption. Locked or incomplete migration preserves existing data without enabling sharing. Original saved files retain their normal format. Keep an encrypted recovery key outside this profile.\n\n",vault::status());
        if let Some(service) = service {
            text.push_str(&format!(
                "Device fingerprint\n{}\n\nRevoked fingerprints\n{}",
                service.identity(),
                service.revoked().join("\n")
            ));
        } else {
            text.push_str("Send/Sync is unavailable. Unlock protected data and restart to retry.");
        }
        self.summary.set_text(&text);
        let v = security::limits();
        for (field, value) in self.fields.iter_mut().zip([
            v.total_mib,
            v.peer_mib,
            v.document_mib,
            v.documents,
            v.proofs,
        ]) {
            field.set_value(&value.to_string());
        }
    }
    fn save(&self, root: &std::path::Path) -> Result<(), String> {
        let values = self
            .fields
            .iter()
            .map(|f| {
                f.value()
                    .parse::<u32>()
                    .map_err(|_| "Enter a positive numeric limit".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;
        security::save(
            root,
            security::Limits {
                total_mib: values[0],
                peer_mib: values[1],
                document_mib: values[2],
                documents: values[3],
                proofs: values[4],
            },
        )
        .map_err(|e| e.to_string())
    }
}
impl Runtime {
    pub fn security_action(&mut self, action: Action) -> Result<(), String> {
        match action {
            Action::SecuritySave=>self.security_panel.save(self.config.parent().unwrap())?,
            Action::SecurityRecovery=>vault::export_recovery().map_err(|e|e.to_string())?,
            Action::SecurityUnblock=>{
                let id=self.security_panel.fingerprint.value();
                if chrome::decision(None,"Allow pairing again?","The fingerprint will be removed from the local revocation list. It will not receive automatic trust or document access.",&id,&["Cancel","Allow pairing"],0)==Some(1) {self.sharing.as_mut().ok_or("Sharing service unavailable")?.unblock(&id)?;}
            }
            Action::SecurityRotate=>{
                if chrome::decision(None,"Replace device identity?","Local sync groups will stop. After restarting, this installation has a new fingerprint.","If the old key was compromised, remove/revoke its fingerprint on EVERY other device. A new local identity does not remotely revoke a stolen old key. Existing documents remain available. Save an encrypted recovery key first.",&["Cancel","Replace on next start"],0)==Some(1) {
                    self.persist();self.store.flush()?;self.sharing.as_mut().ok_or("Sharing service unavailable")?.prepare_rotation()?;
                    dialog::message_default("Identity replacement is scheduled. Restart EditorTool, then pair the new fingerprint. Revoke the old fingerprint on other devices.");
                }
            }
            Action::SecurityClean=>{
                if chrome::decision(None,"Clean retired sync groups?","Inactive groups and their sync-only history will be removed, together with unused contents. Active groups and pending offers are preserved.","This is permanent logical deletion, not secure physical erasure. Current editor/session contents and user files are preserved; retired documents can be shared again as fresh groups.",&["Cancel","Clean retired groups"],0)==Some(1) {
                    let count=self.sharing.as_ref().ok_or("Sharing service unavailable")?.collect_unused()?;
                    dialog::message_default(&format!("Removed {count} unused content files."));
                }
            }
            _=>(),
        }
        self.security_panel.refresh(self.sharing.as_ref());
        Ok(())
    }
}
