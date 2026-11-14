use crate::*;
use fltk::{
    browser::{CheckBrowser, HoldBrowser},
    input::Input,
    output::Output,
};
pub struct Clients {
    pub win: Window,
    pub page: crate::settings_page::Page,
    pub enabled: fltk::button::CheckButton,
    pub background: fltk::button::CheckButton,
    pub name: Input,
    pub port: Input,
    pub clients: HoldBrowser,
    pub alias: Input,
    pub host: Input,
    pub remote_port: Input,
    pub fingerprint: Output,
    pub status: Frame,
    pub automatic: fltk::button::CheckButton,
    pub interfaces: CheckBrowser,
    pub interface_choices: Vec<network::Binding>,
}
impl Clients {
    pub fn new(tx: app::Sender<Message>) -> Self {
        Group::set_current(None::<&Group>);
        let mut win = Window::default()
            .with_size(760, 900)
            .with_label(crate::i18n::tr("Clients and Sync — EditorTool"));
        win.set_color(PANEL);
        chrome::set_icon(&mut win);
        let mut root = Flex::new(20, 18, 720, 1220, None).column();
        root.set_spacing(10);
        let heading = theme::heading("Clients and Sync", 21);
        root.fixed(&heading, 30);
        let mut enabled = fltk::button::CheckButton::default()
            .with_label(crate::i18n::tr("Enable Send and Sync (open local port)"));
        theme::check(&mut enabled);
        root.fixed(&enabled, 28);
        let mut background = fltk::button::CheckButton::default()
            .with_label(crate::i18n::tr("Keep running in the tray when closing"));
        theme::check(&mut background);
        background.emit(tx, (0, Action::KeepTray));
        root.fixed(&background, 28);
        let mut row = Flex::default().row();
        row.set_spacing(10);
        let mut field = Flex::default().column();
        field.set_spacing(4);
        let label = theme::heading("This device's name", 12);
        field.fixed(&label, 18);
        let mut name = Input::default();
        theme::input(&mut name);
        name.set_tooltip(crate::i18n::tr("This device's name"));
        field.end();
        let mut field = Flex::default().column();
        field.set_spacing(4);
        let label = theme::heading("Local port", 12);
        field.fixed(&label, 18);
        let mut port = Input::default();
        theme::input(&mut port);
        port.set_tooltip(crate::i18n::tr("Local TCP port"));
        field.end();
        row.fixed(&field, 100);
        let mut apply = Button::default().with_label(crate::i18n::tr("Apply"));
        theme::button(&mut apply, true, None);
        apply.emit(tx, (0, Action::ApplySharing));
        row.fixed(&apply, 128);
        row.end();
        root.fixed(&row, 58);
        enabled.emit(tx, (0, Action::ApplySharing));
        let mut automatic = fltk::button::CheckButton::default().with_label(crate::i18n::tr(
            "Select interface automatically (OS routing)",
        ));
        theme::check(&mut automatic);
        automatic.set_value(true);
        root.fixed(&automatic, 28);
        let mut interfaces = CheckBrowser::default();
        interfaces.set_color(BG);
        interfaces.set_text_color(FG);
        interfaces.set_text_size(12);
        interfaces.set_frame(FrameType::FlatBox);
        root.fixed(&interfaces, 130);
        let mut refresh_interfaces =
            Button::default().with_label(crate::i18n::tr("Refresh network adapters / IP / MAC"));
        theme::button(&mut refresh_interfaces, false, None);
        refresh_interfaces.emit(tx, (0, Action::RefreshInterfaces));
        root.fixed(&refresh_interfaces, 30);
        let mut interface_hint = Frame::default().with_label(crate::i18n::tr(
            "For LAN or VPN: disable Automatic, select IP addresses and apply.",
        ));
        interface_hint.set_align(Align::Left | Align::Inside | Align::Wrap);
        interface_hint.set_label_color(MUTED);
        interface_hint.set_label_size(11);
        root.fixed(&interface_hint, 38);
        let mut fingerprint = Output::default();
        fingerprint.set_frame(FrameType::FlatBox);
        fingerprint.set_color(BG);
        fingerprint.set_text_color(MUTED);
        fingerprint.set_text_size(11);
        fingerprint.set_tooltip(crate::i18n::tr(
            "This device's key fingerprint — compare when connecting",
        ));
        root.fixed(&fingerprint, 30);
        let mut crypto=Frame::default().with_label(crate::i18n::tr("Encryption: TLS 1.3 · AES-256-GCM\nHybrid key exchange: X25519 + ML-KEM-768 (post-quantum protection)\nDevice identity: approved SHA-256 fingerprint, classical signature\nRequired on both devices: EditorTool 1.4.1 or newer"));
        crypto.set_label_size(12);
        crypto.set_label_color(FG);
        crypto.set_align(Align::Left | Align::Inside | Align::Wrap);
        root.fixed(&crypto, 82);
        let mut headers = Flex::default().row();
        for (text, width) in [
            ("Client name", 160),
            ("IP address : Port", 260),
            ("Connection status", 0),
        ] {
            let label = theme::heading(text, 12);
            if width > 0 {
                headers.fixed(&label, width);
            }
        }
        headers.end();
        root.fixed(&headers, 20);
        let mut clients = HoldBrowser::default();
        clients.set_frame(FrameType::FlatBox);
        clients.set_color(BG);
        clients.set_label_color(FG);
        clients.set_text_size(13);
        clients.set_format_char('@');
        clients.set_column_char('\t');
        clients.set_column_widths(&[160, 260, 0]);
        clients.emit(tx, (0, Action::SelectClient));
        root.fixed(&clients, 140);
        let mut row = Flex::default().row();
        row.set_spacing(8);
        let mut field = Flex::default().column();
        field.set_spacing(4);
        let label = theme::heading("Client name", 12);
        field.fixed(&label, 18);
        let mut alias = Input::default();
        theme::input(&mut alias);
        alias.set_tooltip(crate::i18n::tr("Client name"));
        field.end();
        row.fixed(&field, 140);
        let mut field = Flex::default().column();
        field.set_spacing(4);
        let label = theme::heading("IP address", 12);
        field.fixed(&label, 18);
        let mut host = Input::default();
        theme::input(&mut host);
        host.set_tooltip(crate::i18n::tr("Private IP address"));
        field.end();
        let mut field = Flex::default().column();
        field.set_spacing(4);
        let label = theme::heading("Port", 12);
        field.fixed(&label, 18);
        let mut remote_port = Input::default();
        theme::input(&mut remote_port);
        remote_port.set_tooltip(crate::i18n::tr("Client port"));
        remote_port.set_value(&peer::PORT.to_string());
        field.end();
        row.fixed(&field, 80);
        let mut add = Button::default().with_label(crate::i18n::tr("Add"));
        theme::button(&mut add, false, None);
        add.emit(tx, (0, Action::AddClient));
        row.fixed(&add, 120);
        row.end();
        root.fixed(&row, 58);
        let mut row = Flex::default().row();
        row.set_spacing(8);
        let mut save = Button::default().with_label(crate::i18n::tr("Save changes"));
        theme::button(&mut save, false, None);
        save.emit(tx, (0, Action::EditClient));
        let mut remove = Button::default().with_label(crate::i18n::tr("Remove client"));
        theme::button(&mut remove, false, None);
        remove.emit(tx, (0, Action::RemoveClient));
        let mut probe = Button::default().with_label(crate::i18n::tr("Check connection / approve"));
        theme::button(&mut probe, false, None);
        probe.emit(tx, (0, Action::ProbeClients));
        row.end();
        root.fixed(&row, 34);
        let mut log = Button::default().with_label(crate::i18n::tr("Communication log…"));
        theme::button(&mut log, false, None);
        log.emit(tx, (0, Action::Traffic));
        root.fixed(&log, 32);
        let mut status = Frame::default();
        status.set_label_color(MUTED);
        status.set_label_size(12);
        status.set_align(Align::Left | Align::Inside | Align::Wrap);
        root.fixed(&status, 48);
        root.end();
        win.end();
        win.set_callback(|w| w.hide());
        let page = crate::settings_page::Page::new(&mut win, &root);
        Self {
            win,
            page,
            enabled,
            background,
            name,
            port,
            clients,
            alias,
            host,
            remote_port,
            fingerprint,
            status,
            automatic,
            interfaces,
            interface_choices: vec![],
        }
    }
    pub fn reload_interfaces(&mut self, config: &peer::Config) -> Result<(), String> {
        self.interfaces.clear();
        let mut choices = network::interfaces().map_err(|e| e.to_string())?;
        for saved in &config.interfaces {
            if !choices
                .iter()
                .any(|b| b.name == saved.name && b.ip == saved.ip)
            {
                choices.push(saved.clone());
            }
        }
        for binding in &choices {
            self.interfaces.add(
                &binding.label(),
                config
                    .interfaces
                    .iter()
                    .any(|b| b.name == binding.name && b.ip == binding.ip),
            );
        }
        self.interface_choices = choices;
        self.automatic.set_value(config.automatic_interfaces);
        Ok(())
    }
    pub fn select_client(&mut self, service: &peer::Service) {
        let index = self.clients.value();
        if index > 0 {
            if let Some(peer) = service.config().clients.get(index as usize - 1) {
                self.alias.set_value(&peer.name);
                self.host.set_value(&peer.host);
                self.remote_port.set_value(&peer.port.to_string());
            }
        }
    }
    pub fn refresh(&mut self, service: &peer::Service, reset: bool) {
        let config = service.config();
        let selected = self.clients.value();
        self.clients.clear();
        for p in &config.clients {
            self.clients.add(&format!(
                "@C{}@.{}\t{}:{}\t{}{}",
                if service.status(p) {
                    Color::Green.bits()
                } else {
                    Color::Red.bits()
                },
                p.name.replace(['\n', '\r', '\t'], " "),
                p.host,
                p.port,
                if service.status(p) {
                    crate::i18n::tr("Connected")
                } else {
                    crate::i18n::tr("Not connected")
                },
                if p.cert.is_empty() {
                    " · Sharing service is missing"
                } else {
                    ""
                }
            ));
        }
        self.clients.select(selected);
        self.enabled.set_value(config.enabled);
        self.background.set_value(preferences::get().keep_in_tray);
        self.fingerprint.set_value(&crate::tr_format!(
            "Own fingerprint: {}",
            service.identity()
        ));
        if reset {
            self.name.set_value(&config.name);
            self.port.set_value(&config.port.to_string());
        }
        let latest = config
            .clients
            .iter()
            .map(|p| service.checked_at(p))
            .max()
            .unwrap_or(0);
        let checked = if latest == 0 {
            "not checked yet".into()
        } else {
            let t = time::OffsetDateTime::from_unix_timestamp(latest as i64)
                .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
            crate::tr_format!(
                "last checked {:02}:{:02}:{:02} UTC",
                t.hour(),
                t.minute(),
                t.second()
            )
        };
        self.status.set_label(&if config.enabled{crate::tr_format!("Automatic: 10 s after each round, 1 s with pending sends.\nRounds may take longer depending on peers · {checked}.\nCheck connection starts a manual round. Approve both devices.")}else{"Send/Sync is off. The sharing port is closed.".into()});
    }
}
pub struct Transfer {
    pub win: Window,
    pub clients: colored_checks::Checks,
    pub autosave: fltk::button::CheckButton,
    pub stop: Button,
    pub context: Option<(usize, usize, bool)>,
    pub choices: Vec<peer::Peer>,
}
impl Transfer {
    pub fn new(tx: app::Sender<Message>) -> Self {
        Group::set_current(None::<&Group>);
        let mut win = Window::default()
            .with_size(560, 390)
            .with_label(crate::i18n::tr("Share document — EditorTool"));
        win.set_color(PANEL);
        chrome::set_icon(&mut win);
        let mut root = Flex::new(20, 18, 520, 354, None).column();
        root.set_spacing(10);
        let label = theme::heading("Select client", 20);
        root.fixed(&label, 28);
        let clients = colored_checks::Checks::new();
        let mut autosave = fltk::button::CheckButton::default()
            .with_label(crate::i18n::tr("Save sync changes to my local file"));
        theme::check(&mut autosave);
        root.fixed(&autosave, 28);
        let mut row = Flex::default().row();
        row.set_spacing(8);
        let mut submit = Button::default().with_label(crate::i18n::tr("Request / Apply"));
        theme::button(&mut submit, true, None);
        submit.emit(tx, (0, Action::SubmitTransfer));
        let mut stop = Button::default().with_label(crate::i18n::tr("Stop sync"));
        theme::button(&mut stop, false, None);
        stop.emit(tx, (0, Action::StopSync));
        let mut log = Button::default().with_label(crate::i18n::tr("Log…"));
        theme::button(&mut log, false, None);
        log.emit(tx, (0, Action::Traffic));
        row.fixed(&log, 70);
        row.end();
        root.fixed(&row, 36);
        root.end();
        win.end();
        win.set_callback(|w| w.hide());
        Self {
            win,
            clients,
            autosave,
            stop,
            context: None,
            choices: vec![],
        }
    }
    pub fn show(
        &mut self,
        service: &peer::Service,
        wid: usize,
        id: usize,
        sync: bool,
        binding: Option<&peer::Binding>,
    ) {
        self.clients.clear();
        self.choices = service
            .config()
            .clients
            .into_iter()
            .filter(|p| !p.cert.is_empty())
            .collect();
        for p in &self.choices {
            self.clients.add(
                &format!(
                    "{} · {}:{} · {}",
                    p.name,
                    p.host,
                    p.port,
                    if service.status(p) {
                        crate::i18n::tr("Connected")
                    } else {
                        crate::i18n::tr("Not connected")
                    }
                ),
                false,
                service.status(p),
            );
        }
        self.context = Some((wid, id, sync));
        self.autosave.set_value(binding.is_some_and(|b| b.autosave));
        if sync {
            self.autosave.show();
            if binding.is_some() {
                self.stop.show();
                if let Some(d) = binding.and_then(|b| service.document(&b.doc)) {
                    for (i, p) in self.choices.iter().enumerate() {
                        if d.members.iter().any(|m| m.id == p.id) && !d.departed.contains(&p.id) {
                            self.clients.set_checked(i as i32 + 1);
                        }
                    }
                }
            } else {
                self.stop.hide();
            }
        } else {
            self.autosave.hide();
            self.stop.hide();
        }
        self.win.set_label(if sync {
            "Synchronise document — EditorTool"
        } else {
            "Send document — EditorTool"
        });
        self.win.show();
        self.clients.layout();
        chrome::dark_title(&self.win);
    }
    pub fn selected(&self) -> Vec<peer::Peer> {
        self.choices
            .iter()
            .enumerate()
            .filter(|(i, _)| self.clients.checked(*i as i32 + 1))
            .map(|(_, p)| p.clone())
            .collect()
    }
}
pub struct Conflicts {
    pub win: Window,
    pub list: HoldBrowser,
    pub text: fltk::text::TextBuffer,
    pub versions: Vec<peer::Revision>,
    pub doc: Option<String>,
}
impl Conflicts {
    pub fn new(tx: app::Sender<Message>) -> Self {
        Group::set_current(None::<&Group>);
        let mut win = Window::default()
            .with_size(680, 510)
            .with_label(crate::i18n::tr("Versionskonflikt — EditorTool"));
        win.set_color(PANEL);
        chrome::set_icon(&mut win);
        let mut root = Flex::new(20, 18, 640, 474, None).column();
        root.set_spacing(10);
        let label = theme::heading("Which revision should be used?", 20);
        root.fixed(&label, 30);
        let mut list = HoldBrowser::default();
        list.set_color(BG);
        list.set_label_color(FG);
        list.set_format_char('\0');
        list.set_text_size(13);
        list.set_frame(FrameType::FlatBox);
        list.emit(tx, (0, Action::PreviewConflict));
        root.fixed(&list, 100);
        let text = fltk::text::TextBuffer::default();
        let mut view = fltk::text::TextDisplay::default();
        view.set_buffer(text.clone());
        view.set_text_font(Font::Courier);
        view.set_text_size(13);
        view.set_text_color(FG);
        view.set_color(BG);
        view.set_frame(FrameType::FlatBox);
        view.wrap_mode(fltk::text::WrapMode::AtBounds, 0);
        view.super_draw_first(true);
        view.draw(|w| theme::draw_scrollbars(w));
        let grab = Cell::new(None);
        view.handle(move |w, e| theme::handle_scrollbars(w, e, &grab));
        let mut row = Flex::default().row();
        row.set_spacing(8);
        let mut resolve = Button::default().with_label(crate::i18n::tr("Use selected revision"));
        theme::button(&mut resolve, true, None);
        resolve.emit(tx, (0, Action::ResolveConflict));
        let mut later = Button::default().with_label(crate::i18n::tr("Decide later"));
        theme::button(&mut later, false, None);
        later.set_callback(|b| {
            if let Some(mut w) = b.window() {
                w.hide()
            }
        });
        row.end();
        root.fixed(&row, 36);
        root.end();
        win.end();
        win.set_callback(|w| w.hide());
        Self {
            win,
            list,
            text,
            versions: vec![],
            doc: None,
        }
    }
    pub fn show(&mut self, document: peer::Document) {
        self.doc = Some(document.id);
        self.versions = std::iter::once(document.head)
            .chain(document.conflicts)
            .collect();
        self.list.clear();
        for r in &self.versions {
            self.list.add(&format!(
                "{} · Stand {} · Inhalt {}",
                r.name,
                r.clock.values().copied().fold(0u64, u64::saturating_add),
                r.hash.get(..12).unwrap_or(&r.hash)
            ));
        }
        self.list.select(1);
        self.win.show();
        chrome::dark_title(&self.win);
    }
}
