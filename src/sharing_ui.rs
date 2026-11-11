use crate::*;
use fltk::{
    browser::{CheckBrowser, HoldBrowser},
    input::Input,
    output::Output,
};
pub struct Clients {
    pub win: Window,
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
}
impl Clients {
    pub fn new(tx: app::Sender<Message>) -> Self {
        Group::set_current(None::<&Group>);
        let mut win = Window::default()
            .with_size(660, 590)
            .with_label("Clients und Synchronisierung — EditorTool");
        win.set_color(PANEL);
        chrome::set_icon(&mut win);
        let mut root = Flex::new(20, 18, 620, 554, None).column();
        root.set_spacing(10);
        let heading = theme::heading("Clients und Synchronisierung", 21);
        root.fixed(&heading, 30);
        let mut enabled = fltk::button::CheckButton::default()
            .with_label("Senden und Sync aktivieren (lokalen Port öffnen)");
        theme::check(&mut enabled);
        root.fixed(&enabled, 28);
        let mut background =
            fltk::button::CheckButton::default().with_label("Beim Schliessen im Tray weiterlaufen");
        theme::check(&mut background);
        background.emit(tx, (0, Action::KeepTray));
        root.fixed(&background, 28);
        let mut row = Flex::default().row();
        row.set_spacing(10);
        let mut name = Input::default();
        theme::input(&mut name);
        name.set_tooltip("Name dieses Geräts");
        let mut port = Input::default();
        theme::input(&mut port);
        port.set_tooltip("Lokaler TCP-Port");
        row.fixed(&port, 90);
        let mut apply = Button::default().with_label("Übernehmen");
        theme::button(&mut apply, true, None);
        apply.emit(tx, (0, Action::ApplySharing));
        row.fixed(&apply, 128);
        row.end();
        root.fixed(&row, 34);
        enabled.emit(tx, (0, Action::ApplySharing));
        let mut fingerprint = Output::default();
        fingerprint.set_frame(FrameType::FlatBox);
        fingerprint.set_color(BG);
        fingerprint.set_text_color(MUTED);
        fingerprint.set_text_size(11);
        fingerprint.set_tooltip("Eigener Schlüssel-Fingerabdruck – beim Verbinden vergleichen");
        root.fixed(&fingerprint, 30);
        let mut clients = HoldBrowser::default();
        clients.set_frame(FrameType::FlatBox);
        clients.set_color(BG);
        clients.set_label_color(FG);
        clients.set_text_size(13);
        clients.set_format_char('\0');
        let mut row = Flex::default().row();
        row.set_spacing(8);
        let mut alias = Input::default();
        theme::input(&mut alias);
        alias.set_tooltip("Client-Name");
        row.fixed(&alias, 130);
        let mut host = Input::default();
        theme::input(&mut host);
        host.set_tooltip("Interne IP-Adresse");
        let mut remote_port = Input::default();
        theme::input(&mut remote_port);
        remote_port.set_tooltip("Client-Port");
        remote_port.set_value(&peer::PORT.to_string());
        row.fixed(&remote_port, 80);
        let mut add = Button::default().with_label("Hinzufügen");
        theme::button(&mut add, false, None);
        add.emit(tx, (0, Action::AddClient));
        row.fixed(&add, 120);
        row.end();
        root.fixed(&row, 34);
        let mut row = Flex::default().row();
        row.set_spacing(8);
        let mut remove = Button::default().with_label("Client entfernen");
        theme::button(&mut remove, false, None);
        remove.emit(tx, (0, Action::RemoveClient));
        let mut probe = Button::default().with_label("Verbindung prüfen / freigeben");
        theme::button(&mut probe, false, None);
        probe.emit(tx, (0, Action::ProbeClients));
        row.end();
        root.fixed(&row, 34);
        let mut status = Frame::default();
        status.set_label_color(MUTED);
        status.set_label_size(12);
        status.set_align(Align::Left | Align::Inside | Align::Wrap);
        root.fixed(&status, 48);
        root.end();
        win.end();
        win.set_callback(|w| w.hide());
        Self {
            win,
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
        }
    }
    pub fn refresh(&mut self, service: &peer::Service, reset: bool) {
        let config = service.config();
        let selected = self.clients.value();
        self.clients.clear();
        for p in &config.clients {
            self.clients.add(&format!(
                "{}    {}:{}    {}{}",
                p.name,
                p.host,
                p.port,
                if service.status(p) {
                    "Connected"
                } else {
                    "Not connected"
                },
                if p.cert.is_empty() {
                    " · Freigabe fehlt"
                } else {
                    ""
                }
            ));
        }
        self.clients.select(selected);
        self.enabled.set_value(config.enabled);
        self.background.set_value(preferences::get().keep_in_tray);
        self.fingerprint
            .set_value(&format!("Eigener Fingerabdruck: {}", service.identity()));
        if reset {
            self.name.set_value(&config.name);
            self.port.set_value(&config.port.to_string());
        }
        self.status.set_label(if config.enabled{"Interne Netze/VPN · Geräte an beiden Seiten freigeben. Dateien werden erst nach Annahme übertragen."}else{"Senden/Sync ist aus. Der Freigabe-Port ist geschlossen."});
    }
}
pub struct Transfer {
    pub win: Window,
    pub clients: CheckBrowser,
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
            .with_label("Dokument freigeben — EditorTool");
        win.set_color(PANEL);
        chrome::set_icon(&mut win);
        let mut root = Flex::new(20, 18, 520, 354, None).column();
        root.set_spacing(10);
        let label = theme::heading("Client auswählen", 20);
        root.fixed(&label, 28);
        let mut clients = CheckBrowser::default();
        clients.set_color(BG);
        clients.set_text_color(FG);
        clients.set_text_size(13);
        clients.set_frame(FrameType::FlatBox);
        let mut autosave = fltk::button::CheckButton::default()
            .with_label("Sync-Änderungen in meine lokale Datei speichern");
        theme::check(&mut autosave);
        root.fixed(&autosave, 28);
        let mut row = Flex::default().row();
        row.set_spacing(8);
        let mut submit = Button::default().with_label("Anfragen / Übernehmen");
        theme::button(&mut submit, true, None);
        submit.emit(tx, (0, Action::SubmitTransfer));
        let mut stop = Button::default().with_label("Sync beenden");
        theme::button(&mut stop, false, None);
        stop.emit(tx, (0, Action::StopSync));
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
                        "Connected"
                    } else {
                        "Not connected"
                    }
                ),
                false,
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
                        if d.members.iter().any(|m| m.id == p.id) {
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
            "Dokument synchronisieren — EditorTool"
        } else {
            "Dokument senden — EditorTool"
        });
        self.win.show();
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
            .with_label("Versionskonflikt — EditorTool");
        win.set_color(PANEL);
        chrome::set_icon(&mut win);
        let mut root = Flex::new(20, 18, 640, 474, None).column();
        root.set_spacing(10);
        let label = theme::heading("Welche Version soll übernommen werden?", 20);
        root.fixed(&label, 30);
        let mut list = HoldBrowser::default();
        list.set_color(BG);
        list.set_label_color(FG);
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
        let mut resolve = Button::default().with_label("Ausgewählte Version übernehmen");
        theme::button(&mut resolve, true, None);
        resolve.emit(tx, (0, Action::ResolveConflict));
        let mut later = Button::default().with_label("Später entscheiden");
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
                &r.hash[..12]
            ));
        }
        self.list.select(1);
        self.win.show();
        chrome::dark_title(&self.win);
    }
}
