use crate::*;
use fltk::{input::Input, menu::Choice};
pub struct Panel {
    pub win: Window,
    pub page: settings_page::Page,
    device: Choice,
    direction: Choice,
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
        direction.add_choice(crate::i18n::tr("All directions|TX|RX|OUT|IN|ERROR"));
        direction.add_choice("SYNC");
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
        let direction = self.direction.choice().unwrap_or_default();
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
            let mut line = format!(
                "{:02}:{:02}:{:02} UTC  {:5}  {}  {} -> {}  {}  {} B\n",
                t.hour(),
                t.minute(),
                t.second(),
                e.direction,
                peer.map(|p| p.name.as_str()).unwrap_or("Unbekannt"),
                e.local,
                e.remote,
                e.event,
                e.bytes
            );
            if !e.document.is_empty() {
                line.push_str(&format!(
                    "    Group {} · Force {} · Authority {}\n",
                    e.document, e.force_generation, e.force_author
                ));
            }
            if query.is_empty()
                || line.to_lowercase().contains(&query)
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
