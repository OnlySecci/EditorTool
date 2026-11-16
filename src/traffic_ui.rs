use crate::*;
use fltk::{input::Input, menu::Choice};
pub struct Panel {
    pub win: Window,
    pub page: settings_page::Page,
    device: Choice,
    direction: Choice,
    details: fltk::button::CheckButton,
    search: Input,
    buffer: TextBuffer,
    keys: Vec<String>,
    pub generation: u64,
    anonymous: String,
}
impl Panel {
    pub fn new(tx: app::Sender<Message>) -> Self {
        Group::set_current(None::<&Group>);
        let mut win = Window::default().with_size(800, 650);
        win.set_color(PANEL);
        let mut root = Flex::new(20, 20, 760, 610, None).column();
        root.set_spacing(10);
        let title = theme::heading("Send / Sync — Communication log", 21);
        root.fixed(&title, 32);
        let mut hint=Frame::default().with_label(crate::i18n::tr("Records device/IP metadata and application messages, not text or image contents.\nByte counts describe application data, not all encrypted network packets."));
        hint.set_label_size(12);
        hint.set_label_color(MUTED);
        hint.set_align(Align::Left | Align::Inside | Align::Wrap);
        root.fixed(&hint, 48);
        let mut filters = Flex::default().row();
        filters.set_spacing(8);
        let mut device = Choice::default();
        device.add_choice(crate::i18n::tr("All devices"));
        device.set_value(0);
        let mut direction = Choice::default();
        direction.add_choice(crate::i18n::tr("All activity|Sent|Received|Outgoing connections|Incoming connections|Problems|Synchronization"));
        direction.set_value(0);
        filters.fixed(&direction, 140);
        let mut search = Input::default();
        theme::input(&mut search);
        search.set_tooltip(crate::i18n::tr(
            "Filter by IP, message type or device fingerprint",
        ));
        search.set_trigger(fltk::enums::CallbackTrigger::Changed);
        for choice in [&mut device, &mut direction] {
            choice.set_color(theme::SURFACE);
            choice.set_text_color(FG);
            choice.set_text_size(12);
            choice.emit(tx, (0, Action::TrafficFilter));
        }
        search.emit(tx, (0, Action::TrafficFilter));
        filters.end();
        root.fixed(&filters, 34);
        let mut details = fltk::button::CheckButton::default().with_label(crate::i18n::tr(
            "Show technical details and background messages",
        ));
        theme::check(&mut details);
        details.emit(tx, (0, Action::TrafficFilter));
        root.fixed(&details, 26);
        let buffer = TextBuffer::default();
        let mut display = fltk::text::TextDisplay::default();
        display.set_buffer(buffer.clone());
        display.set_text_size(12);
        display.set_color(BG);
        display.set_text_color(FG);
        display.set_frame(FrameType::FlatBox);
        display.set_scrollbar_size(14);
        display.super_draw_first(true);
        display.draw(|w| theme::draw_scrollbars(w));
        let grab = Cell::new(None);
        display.handle(move |w, e| theme::handle_scrollbars(w, e, &grab));
        let mut row = Flex::default().row();
        row.set_spacing(8);
        for (label, action) in [
            ("Refresh", Action::TrafficFilter),
            ("Export filtered log…", Action::TrafficExport),
            ("Anonymized export…", Action::TrafficExportAnonymous),
        ] {
            let mut button = Button::default().with_label(label);
            theme::button(&mut button, false, None);
            button.emit(tx, (0, action));
        }
        row.end();
        root.fixed(&row, 34);
        root.end();
        win.end();
        let page = settings_page::Page::new(&mut win, &root);
        Self {
            win,
            page,
            device,
            direction,
            details,
            search,
            buffer,
            keys: vec![String::new()],
            generation: 0,
            anonymous: String::new(),
        }
    }
    pub fn refresh(&mut self, peers: &[peer::Peer], reset: bool) {
        if reset {
            let selected = self
                .keys
                .get(self.device.value().max(0) as usize)
                .cloned()
                .unwrap_or_default();
            self.device.clear();
            self.device.add_choice(crate::i18n::tr("All devices"));
            self.keys = vec![String::new()];
            for p in peers {
                let key = if p.id.is_empty() {
                    format!("{}:{}", p.host, p.port)
                } else {
                    p.id.clone()
                };
                self.device.add_choice(&format!(
                    "{} · {}:{}",
                    p.name.replace(['|', '/', '\\'], " "),
                    p.host,
                    p.port
                ));
                self.keys.push(key);
            }
            self.device
                .set_value(self.keys.iter().position(|k| *k == selected).unwrap_or(0) as i32);
        }
        let device = self
            .keys
            .get(self.device.value().max(0) as usize)
            .cloned()
            .unwrap_or_default();
        let direction = ["", "TX", "RX", "OUT", "IN", "ERROR", "SYNC"]
            .get(self.direction.value().max(0) as usize)
            .copied()
            .unwrap_or("");
        let query = self.search.value().to_lowercase();
        let mut text = String::new();
        let mut anonymous=String::from("Pseudonymized communication log. Names, addresses and fingerprints removed. Timestamps and traffic sizes remain.\n");
        let mut aliases = std::collections::BTreeMap::<String, usize>::new();
        for e in traffic::entries() {
            let peer = peers.iter().find(|p| {
                (!e.peer.is_empty() && p.id == e.peer)
                    || format!("{}:{}", p.host, p.port) == e.remote
                    || e.remote
                        .parse::<std::net::SocketAddr>()
                        .ok()
                        .is_some_and(|addr| {
                            p.host.parse::<std::net::IpAddr>().ok()
                                == Some(addr.ip().to_canonical())
                        })
            });
            if !device.is_empty()
                && device != e.peer
                && device != e.force_author
                && device != e.remote
                && !peer
                    .is_some_and(|p| device == p.id || device == format!("{}:{}", p.host, p.port))
            {
                continue;
            }
            if self.direction.value() > 0 && direction != e.direction {
                continue;
            }
            let t = time::OffsetDateTime::from_unix_timestamp(e.time as i64)
                .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
            let mut technical = format!(
                "{:02}:{:02}:{:02} UTC  {:5}  {}  {} -> {}  {}  {} B\n",
                t.hour(),
                t.minute(),
                t.second(),
                e.direction,
                peer.map(|p| p.name.as_str()).unwrap_or("Unlisted device"),
                e.local,
                e.remote,
                e.event,
                e.bytes
            );
            if !e.document.is_empty() {
                if e.revision.is_empty() && e.force_generation > 0 {
                    technical.push_str(&format!(
                        "    Group {} · Legacy force {} · Authority {}\n",
                        e.document, e.force_generation, e.force_author
                    ));
                } else {
                    technical.push_str(&format!(
                        "    Group {} · Commit {} · Author {}\n",
                        e.document, e.revision, e.force_author
                    ));
                }
            }
            let friendly = friendly_event(&e);
            if !self.details.value() && friendly.is_none() {
                continue;
            }
            let name = peer
                .map(|p| p.name.as_str())
                .filter(|name| !name.is_empty())
                .unwrap_or(if e.direction == "SYNC" && e.peer.is_empty() {
                    crate::i18n::tr("This device")
                } else {
                    crate::i18n::tr("Unlisted device")
                });
            let mut line = if let Some((title, explanation)) = friendly {
                format!(
                    "{} {:02}:{:02}:{:02} UTC  ·  {}\n{}\n{}\n\n",
                    t.date(),
                    t.hour(),
                    t.minute(),
                    t.second(),
                    name,
                    crate::i18n::tr(title),
                    crate::i18n::tr(explanation)
                )
            } else {
                String::new()
            };
            if self.details.value() {
                line.push_str(&technical);
                line.push('\n');
            }
            if query.is_empty()
                || line.to_lowercase().contains(&query)
                || technical.to_lowercase().contains(&query)
                || e.peer.to_lowercase().contains(&query)
            {
                text.push_str(&line);
                let identity = if e.peer.is_empty() {
                    e.remote.clone()
                } else {
                    e.peer.clone()
                };
                let next = aliases.len() + 1;
                let id = *aliases.entry(identity).or_insert(next);
                anonymous.push_str(&format!(
                    "{:02}:{:02}:{:02} UTC  {}  Device {}  {}  {} B\n",
                    t.hour(),
                    t.minute(),
                    t.second(),
                    e.direction,
                    id,
                    e.event,
                    e.bytes
                ));
            }
        }
        if !traffic::available() {
            text = "Could not open communication log storage. Check the diagnostic log.".into();
        } else if text.is_empty() {
            text = "No matching events. Up to 1500 events are shown.".into();
        }
        self.buffer.set_text(&text);
        self.anonymous = anonymous;
        self.generation = traffic::generation();
    }
    pub fn export(&self, anonymized: bool) -> std::io::Result<()> {
        let mut chooser =
            file_dialog::Chooser::new(fltk::dialog::NativeFileChooserType::BrowseSaveFile);
        chooser.set_title(crate::i18n::tr(
            "Export communication log (includes device addresses)",
        ));
        chooser.set_preset_file(if anonymized {
            "EditorTool-Communication-anonymized.log"
        } else {
            "EditorTool-Communication.log"
        });
        chooser.set_option(fltk::dialog::NativeFileChooserOptions::SaveAsConfirm);
        chooser.show();
        let path = chooser.filename();
        if !path.as_os_str().is_empty() {
            std::fs::write(
                path,
                if anonymized {
                    self.anonymous.clone()
                } else {
                    self.buffer.text()
                },
            )?;
        }
        Ok(())
    }
}

fn friendly_event(e: &traffic::Entry) -> Option<(&'static str, &'static str)> {
    let event = e.event.to_lowercase();
    if event == "request hello" && e.direction == "TX" {
        return Some((
            "Checking device availability",
            "Asking the other device for its current connection and document status.",
        ));
    }
    if event == "reply hello" && e.direction == "RX" {
        return Some((
            "Device responded",
            "The other device is reachable and returned its document status.",
        ));
    }
    if e.direction == "ERROR" && event.contains("timeout") {
        return Some(("Device did not respond in time", "Check the network or VPN connection and whether EditorTool is running on the other device."));
    }
    if e.direction == "ERROR" && event.contains("refused") {
        return Some(("Connection was refused", "Check that Send / Sync is enabled on the other device and that its address, port and interface are correct."));
    }
    if e.direction == "ERROR" && event.contains("trust") {
        return Some(("Device security check needs attention", "Review device approval and identity on both devices. Do not approve an unfamiliar device."));
    }
    if e.direction == "ERROR" || event == "reply error" {
        return Some(("Communication problem", "The action could not be completed. Check that the other device is online; technical details can help diagnose the cause."));
    }
    if event.contains("immediate sync") {
        return Some(("Sync requested now", "The writing countdown was skipped. Device permissions and conflict decisions still apply."));
    }
    if event.contains("main decision") || event.contains("resolution applied") {
        return Some(("Main version selected", "The chosen contents were recorded in the shared history. Offline devices will receive them when connected."));
    }
    if event.contains("conflict") || event.contains("independent branch") {
        return Some(("A document version needs attention", "Different changes were preserved. Open Document versions to review and choose or merge them."));
    }
    if event.contains("local tree commit") {
        return Some((
            "Local changes saved for sync",
            "Your changes have been recorded locally and are ready for connected devices.",
        ));
    }
    if event.contains("acknowledged") || event == "reply done" {
        return Some(("Other device confirmed a step", "The other device acknowledged the request. This does not mean that all offline devices are up to date."));
    }
    if event == "document body completed" {
        return Some(("Document sent", "The document data was sent. A separate confirmation is needed from the receiving device."));
    }
    if event == "document body validated" {
        return Some((
            "Document received and checked",
            "The received document data passed the content checks.",
        ));
    }
    if event == "request offer" {
        return Some((
            "Document sharing requested",
            "A device is asking to send or synchronise a document. Approval may be required.",
        ));
    }
    if event == "reply accepted" {
        return Some((
            "Sharing approved",
            "The recipient accepted the document request.",
        ));
    }
    if event == "reply rejected" {
        return Some((
            "Sharing declined",
            "The recipient declined the document request.",
        ));
    }
    if event == "request leave" {
        return Some((
            "Document sync stopped",
            "A device has left this document's synchronisation group.",
        ));
    }
    if event.contains("paused") {
        return Some(("Document sync paused", "Your local contents are preserved. Check document status, available storage and device connectivity."));
    }
    if event.contains("restored") || event.contains("recovered") || event.contains("archived") {
        return Some(("Document recovery progressed", "Stored history or contents were preserved or restored. Open Document versions for the available versions."));
    }
    if event.contains("identical contents joined") {
        return Some((
            "Matching versions joined",
            "Versions with the same contents were joined without choosing a winner.",
        ));
    }
    None
}
