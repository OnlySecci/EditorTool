use crate::{theme, Action, Message, BG, MUTED, PANEL};
use fltk::{
    app,
    button::{Button, CheckButton},
    enums::{Align, CallbackTrigger, ColorDepth, Event, FrameType, Key},
    frame::Frame,
    group::{Flex, Group},
    image::RgbImage,
    input::Input,
    prelude::*,
    window::DoubleWindow as Window,
};
use std::{cell::Cell, rc::Rc};

pub fn set_icon(win: &mut Window) {
    #[cfg(target_os = "linux")]
    win.set_xclass(crate::desktop::APP_ID);
    if let Ok(icon) = RgbImage::new(
        include_bytes!("../assets/editortool.rgba"),
        64,
        64,
        ColorDepth::Rgba8,
    ) {
        win.set_icon(Some(icon));
    }
}

pub fn select_version(labels: &[String]) -> Option<usize> {
    Group::set_current(None::<&Group>);
    let selected = Rc::new(Cell::new(None));
    let mut win = Window::default()
        .with_size(600, 410)
        .with_label("Ältere Dateistände")
        .center_screen();
    win.set_color(PANEL);
    win.make_modal(true);
    set_icon(&mut win);
    let mut root = Flex::new(20, 20, 560, 370, None).column();
    root.set_spacing(12);
    let mut hint = Frame::default().with_label(
        "Stand auswählen und als neuen Tab öffnen. Die aktuelle Datei bleibt erhalten.",
    );
    hint.set_label_color(MUTED);
    hint.set_label_size(13);
    root.fixed(&hint, 36);
    let mut list = fltk::browser::HoldBrowser::default();
    list.set_color(BG);
    list.set_label_color(theme::TEXT);
    list.set_text_size(14);
    list.set_format_char('\0');
    for label in labels {
        list.add(label);
    }
    let mut row = Flex::default().row();
    row.set_spacing(12);
    let mut cancel = Button::default().with_label("Abbrechen");
    theme::button(&mut cancel, false, None);
    let mut owner = win.clone();
    cancel.set_callback(move |_| owner.hide());
    let mut open = Button::default().with_label("Als neuen Tab öffnen");
    theme::button(&mut open, true, None);
    let result = selected.clone();
    let mut owner = win.clone();
    open.set_callback(move |_| {
        let n = list.value();
        if n > 0 {
            result.set(Some(n as usize - 1));
            owner.hide();
        }
    });
    row.end();
    root.fixed(&row, 40);
    root.end();
    win.end();
    win.set_callback(|w| w.hide());
    win.handle(|w, e| {
        if e == Event::KeyDown && app::event_key() == Key::Escape {
            w.hide();
            true
        } else {
            false
        }
    });
    win.show();
    dark_title(&win);
    while win.shown() {
        app::wait();
    }
    let result = selected.get();
    Window::delete(win);
    result
}

pub fn dark_title(win: &Window) {
    #[cfg(target_os = "windows")]
    unsafe {
        use std::ffi::c_void;
        #[link(name = "dwmapi")]
        extern "system" {
            fn DwmSetWindowAttribute(
                hwnd: *mut c_void,
                attribute: u32,
                value: *const c_void,
                size: u32,
            ) -> i32;
        }
        let hwnd = win.raw_handle() as *mut c_void;
        let light = crate::preferences::get().design == 1;
        let enabled: u32 = if light { 0 } else { 1 };
        let black: u32 = if light { 0x00f5f3f2 } else { 0x00272322 };
        let white: u32 = if light { 0x002a2320 } else { 0x00eeeeee };
        // Older Windows versions ignore unsupported attributes and keep their native frame.
        DwmSetWindowAttribute(hwnd, 20, &enabled as *const _ as _, 4);
        DwmSetWindowAttribute(hwnd, 35, &black as *const _ as _, 4);
        DwmSetWindowAttribute(hwnd, 36, &white as *const _ as _, 4);
    }
    #[cfg(not(target_os = "windows"))]
    let _ = win;
}

/// A dedicated modal avoids depending on the geometry of FLTK's shared message box.
/// 0 = cancel, 1 = discard, 2 = save. Closing the dialog always means cancel.
pub fn confirm_save(owner: &Window, title: &str) -> i32 {
    Group::set_current(None::<&Group>);
    let result = Rc::new(Cell::new(0));
    let mut win = Window::default()
        .with_size(560, 254)
        .with_label("Änderungen speichern?");
    win.set_color(PANEL);
    win.make_modal(true);
    set_icon(&mut win);
    let mut root = Flex::new(26, 22, 508, 210, None).column();
    root.set_spacing(16);
    let heading = theme::heading("Änderungen speichern?", 22);
    root.fixed(&heading, 36);
    let mut message = Frame::default().with_label(&format!(
        "„{}“ enthält ungespeicherte Änderungen.\nBeim Verwerfen gehen diese Änderungen verloren.",
        title.trim_end_matches(" *")
    ));
    message.set_label_size(14);
    message.set_label_color(MUTED);
    message.set_align(Align::Left | Align::Inside | Align::Wrap);
    let mut row = Flex::default().row();
    row.set_spacing(10);
    Frame::default();
    for (label, value) in [("Abbrechen", 0), ("Verwerfen", 1), ("Speichern", 2)] {
        let mut button = Button::default().with_label(label);
        theme::button(&mut button, value == 2, None);
        if value == 1 {
            button.set_label_color(fltk::enums::Color::from_hex(0xff9d96));
        }
        row.fixed(&button, 114);
        let state = result.clone();
        let mut dialog = win.clone();
        button.set_callback(move |_| {
            state.set(value);
            dialog.hide();
        });
    }
    row.end();
    root.fixed(&row, 42);
    root.end();
    win.end();
    win.set_callback(|w| w.hide());
    win.handle(|w, event| {
        if event == Event::KeyDown && app::event_key() == Key::Escape {
            w.hide();
            true
        } else {
            false
        }
    });
    win.set_pos(
        owner.x() + (owner.w() - 560) / 2,
        owner.y() + (owner.h() - 254) / 2,
    );
    win.show();
    dark_title(&win);
    while win.shown() {
        app::wait();
    }
    let decision = result.get();
    Window::delete(win);
    decision
}

/// Styled consent sheet. Dismissal never grants permission.
pub fn decision(
    owner: Option<&Window>,
    title: &str,
    message: &str,
    details: &str,
    choices: &[&str],
    primary: usize,
) -> Option<usize> {
    Group::set_current(None::<&Group>);
    let selected = Rc::new(Cell::new(None));
    let detail_height = if details.is_empty() {
        0
    } else {
        (details.lines().count() as i32 * 20 + 32).clamp(108, 240)
    };
    let message_height = (message.chars().count().div_ceil(78) as i32 * 20).clamp(60, 160);
    let height = 160 + message_height + detail_height;
    let mut win = Window::default().with_size(660, height).with_label(title);
    win.set_color(PANEL);
    win.make_modal(true);
    set_icon(&mut win);
    let mut root = Flex::new(24, 22, 612, height - 44, None).column();
    root.set_spacing(14);
    let label = theme::heading(title, 22);
    root.fixed(&label, 32);
    let mut message = Frame::default().with_label(message);
    message.set_label_size(14);
    message.set_label_color(theme::TEXT);
    message.set_align(Align::Left | Align::Inside | Align::Wrap);
    root.fixed(&message, message_height);
    if !details.is_empty() {
        let mut text = fltk::text::TextBuffer::default();
        text.set_text(details);
        let mut view = fltk::text::TextDisplay::default();
        view.set_buffer(text);
        view.set_color(BG);
        view.set_text_color(theme::TEXT);
        view.set_text_font(fltk::enums::Font::Courier);
        view.set_text_size(12);
        view.set_frame(FrameType::RFlatBox);
        view.wrap_mode(fltk::text::WrapMode::AtBounds, 0);
        view.set_scrollbar_size(14);
        view.super_draw_first(true);
        view.draw(|w| theme::draw_scrollbars(w));
        let grab = Cell::new(None);
        view.handle(move |w, e| theme::handle_scrollbars(w, e, &grab));
        root.fixed(&view, detail_height);
    }
    let mut row = Flex::default().row();
    row.set_spacing(10);
    let mut first = None;
    for (i, label) in choices.iter().enumerate() {
        let mut button = Button::default().with_label(label);
        theme::button(&mut button, i == primary, None);
        if i == 0 {
            first = Some(button.clone());
        }
        let value = selected.clone();
        let mut sheet = win.clone();
        button.set_callback(move |_| {
            value.set(Some(i));
            sheet.hide();
        });
    }
    row.end();
    root.fixed(&row, 40);
    root.end();
    win.end();
    win.set_callback(|w| w.hide());
    win.handle(|w, e| {
        if e == Event::KeyDown && app::event_key() == Key::Escape {
            w.hide();
            true
        } else {
            false
        }
    });
    if let Some(owner) = owner {
        win.set_pos(
            owner.x() + (owner.w() - 660) / 2,
            owner.y() + (owner.h() - height) / 2,
        );
    } else {
        win = win.center_screen();
    }
    win.show();
    dark_title(&win);
    if let Some(mut button) = first {
        button.take_focus().ok();
    }
    while win.shown() {
        app::wait();
    }
    let result = selected.get();
    Window::delete(win);
    result
}

pub struct SearchPanel {
    pub win: Window,
    pub search: Input,
    pub replacement: Input,
    pub case: CheckButton,
    pub feedback: Frame,
    replacement_row: Flex,
    root: Flex,
    toggle: Button,
    expanded: bool,
}

impl SearchPanel {
    pub fn new(wid: usize, tx: app::Sender<Message>) -> Self {
        Group::set_current(None::<&Group>);
        let mut win = Window::default()
            .with_size(520, 160)
            .with_label("Suchen — EditorTool");
        win.set_color(PANEL);
        set_icon(&mut win);
        let mut root = Flex::new(14, 12, 492, 136, None).column();
        root.set_spacing(6);
        let mut header = Flex::default().row();
        header.set_spacing(8);
        theme::heading("Suchen", 18);
        let mut toggle = Button::default().with_label("Ersetzen");
        theme::button(&mut toggle, false, None);
        toggle.set_tooltip("Ersetzen ein- oder ausklappen");
        toggle.emit(tx, (wid, Action::ToggleReplace));
        header.fixed(&toggle, 92);
        header.end();
        root.fixed(&header, 28);
        let mut row = Flex::default().row();
        row.set_spacing(8);
        let mut search = Input::default();
        theme::input(&mut search);
        search.set_tooltip("Suchtext");
        search.set_trigger(CallbackTrigger::EnterKeyAlways);
        search.emit(tx, (wid, Action::Find));
        let mut next = Button::default().with_label("Weiter");
        theme::button(&mut next, true, None);
        next.emit(tx, (wid, Action::Find));
        row.fixed(&next, 78);
        row.end();
        root.fixed(&row, 32);
        let mut replacement_row = Flex::default().column();
        replacement_row.set_spacing(6);
        let mut caption = theme::heading("Ersetzen durch", 12);
        caption.set_label_color(MUTED);
        replacement_row.fixed(&caption, 16);
        let mut replace_controls = Flex::default().row();
        replace_controls.set_spacing(8);
        let mut replacement = Input::default();
        theme::input(&mut replacement);
        replacement.set_tooltip("Ersatztext");
        replacement.set_trigger(CallbackTrigger::EnterKeyAlways);
        replacement.emit(tx, (wid, Action::Replace));
        super::button(
            "Ersetzen",
            84,
            &mut replace_controls,
            Action::Replace,
            wid,
            tx,
        );
        super::button(
            "Alle ersetzen",
            104,
            &mut replace_controls,
            Action::ReplaceAll,
            wid,
            tx,
        );
        replace_controls.end();
        replacement_row.end();
        root.fixed(&replacement_row, 54);
        replacement_row.hide();
        let mut case = CheckButton::default().with_label("Groß-/Kleinschreibung beachten");
        theme::check(&mut case);
        root.fixed(&case, 22);
        let mut feedback =
            Frame::default().with_label("Enter  Nächster Treffer     ·     Esc  Schließen");
        feedback.set_label_color(MUTED);
        feedback.set_label_size(11);
        feedback.set_align(Align::Left | Align::Inside);
        root.end();
        win.end();
        win.set_callback(|w| w.hide());
        win.handle(|w, event| {
            if event == Event::KeyDown && app::event_key() == Key::Escape {
                w.hide();
                true
            } else {
                false
            }
        });
        Self {
            win,
            search,
            replacement,
            case,
            feedback,
            replacement_row,
            root,
            toggle,
            expanded: false,
        }
    }
    pub fn expand(&mut self, expanded: bool) {
        self.expanded = expanded;
        if expanded {
            self.replacement_row.show();
        } else {
            self.replacement_row.hide();
        }
        self.toggle.set_color(if expanded {
            theme::BLUE
        } else {
            theme::SURFACE
        });
        self.toggle.set_label(if expanded {
            "Ersetzen −"
        } else {
            "Ersetzen +"
        });
        let height = if expanded { 220 } else { 160 };
        self.win.resize(self.win.x(), self.win.y(), 520, height);
        self.root.resize(14, 12, 492, height - 24);
        self.root.recalc();
        self.win.set_label(if expanded {
            "Suchen und Ersetzen — EditorTool"
        } else {
            "Suchen — EditorTool"
        });
        self.win.redraw();
    }
    pub fn toggle(&mut self) {
        self.expand(!self.expanded);
    }
    pub fn show(&mut self, owner: &Window, expanded: bool, selection: &str) {
        self.expand(expanded);
        if !selection.is_empty() && !selection.contains('\n') {
            self.search.set_value(selection);
        }
        self.win
            .set_pos(owner.x() + (owner.w() - 560) / 2, owner.y() + 80);
        self.win.show();
        self.win.set_opacity(0.95);
        dark_title(&self.win);
        self.search.take_focus().ok();
        self.search
            .set_position(self.search.value().len() as i32)
            .ok();
        self.search.set_mark(0).ok();
    }
}

pub struct SettingsPanel {
    pub win: Window,
    pub page: crate::settings_page::Page,
    horizontal: Button,
    vertical: Button,
    wrap: CheckButton,
    design: fltk::menu::Choice,
    accent: fltk::menu::Choice,
    size: fltk::menu::Choice,
    image_auto: CheckButton,
    update_check: CheckButton,
    update_install: CheckButton,
}
impl SettingsPanel {
    pub fn new(wid: usize, tx: app::Sender<Message>, vertical: bool) -> Self {
        Group::set_current(None::<&Group>);
        let mut win = Window::default()
            .with_size(560, 810)
            .with_label("Einstellungen — EditorTool");
        win.set_color(PANEL);
        set_icon(&mut win);
        let mut root = Flex::new(24, 20, 512, 770, None).column();
        root.set_spacing(12);
        let title = theme::heading("Einstellungen", 23);
        root.fixed(&title, 34);
        let mut segment = Flex::default().row();
        segment.set_spacing(6);
        let mut horizontal = Button::default().with_label("Tabs oben");
        theme::button(&mut horizontal, false, None);
        horizontal.emit(tx, (wid, Action::Orientation(false)));
        let mut vertical_button = Button::default().with_label("Tabs links");
        theme::button(&mut vertical_button, false, None);
        vertical_button.emit(tx, (wid, Action::Orientation(true)));
        segment.end();
        root.fixed(&segment, 36);
        let mut wrap = CheckButton::default().with_label("Zeilenumbruch am Fensterrand");
        theme::check(&mut wrap);
        wrap.set_callback(move |w| tx.send((wid, Action::WordWrap(w.value()))));
        root.fixed(&wrap, 34);
        fn choice(root: &mut Flex, label: &str, values: &str) -> fltk::menu::Choice {
            let heading = theme::heading(label, 12);
            root.fixed(&heading, 18);
            let mut choice = fltk::menu::Choice::default();
            choice.set_frame(FrameType::FlatBox);
            choice.set_color(theme::SURFACE);
            choice.set_text_color(theme::TEXT);
            choice.set_text_size(14);
            choice.add_choice(values);
            root.fixed(&choice, 34);
            choice
        }
        let mut design = choice(&mut root, "Farbschema", "Dunkel|Hell|Graphit");
        design.set_callback(move |c| tx.send((wid, Action::Design(c.value() as u8))));
        let mut accent = choice(&mut root, "Akzentfarbe", "Blau|Violett|Grün");
        accent.set_callback(move |c| tx.send((wid, Action::Accent(c.value() as u8))));
        let mut size = choice(&mut root, "Textgrösse", "12|14|16|18|20|24|28");
        size.set_callback(move |c| {
            if let Some(value) = c.choice().and_then(|s| s.parse().ok()) {
                tx.send((wid, Action::FontSize(value)));
            }
        });
        let mut image_auto = CheckButton::default()
            .with_label("Neue Bilder automatisch auf 50 % der Textfeldbreite verkleinern");
        theme::check(&mut image_auto);
        image_auto.set_callback(move |c| tx.send((wid, Action::ImageAutoSize(c.value()))));
        root.fixed(&image_auto, 30);
        let mut update_check = CheckButton::default().with_label("Beim Start nach Updates suchen");
        theme::check(&mut update_check);
        update_check.set_callback(move |c| tx.send((wid, Action::UpdateCheckOption(c.value()))));
        root.fixed(&update_check, 28);
        let mut update_install =
            CheckButton::default().with_label("Updates beim Start automatisch installieren");
        theme::check(&mut update_install);
        update_install
            .set_callback(move |c| tx.send((wid, Action::UpdateInstallOption(c.value()))));
        root.fixed(&update_install, 28);
        let mut check_now = Button::default().with_label("Jetzt nach Updates suchen…");
        theme::button(&mut check_now, false, None);
        check_now.emit(tx, (wid, Action::CheckUpdates(true)));
        root.fixed(&check_now, 32);
        let mut clients = Button::default().with_label("Clients, Senden und Sync…");
        theme::button(&mut clients, false, None);
        clients.emit(tx, (wid, Action::SharingSettings));
        root.fixed(&clients, 32);
        let mut keyboard = Button::default().with_label("Tastenkürzel anpassen…");
        theme::button(&mut keyboard, false, None);
        keyboard.emit(tx, (wid, Action::KeyboardSettings));
        root.fixed(&keyboard, 32);
        let mut footer = Frame::default().with_label(concat!(
            "Wird für alle Fenster gespeichert.     ·     ",
            env!("CARGO_PKG_VERSION")
        ));
        footer.set_label_color(MUTED);
        footer.set_label_size(11);
        footer.set_align(Align::Left | Align::Inside);
        root.end();
        win.end();
        win.set_callback(|w| w.hide());
        let page = crate::settings_page::Page::new(&mut win, &root);
        let mut panel = Self {
            win,
            page,
            horizontal,
            vertical: vertical_button,
            wrap,
            design,
            accent,
            size,
            image_auto,
            update_check,
            update_install,
        };
        panel.set_orientation(vertical);
        panel.sync();
        panel
    }
    pub fn sync(&mut self) {
        let prefs = crate::preferences::get();
        self.set_orientation(prefs.vertical);
        self.wrap.set_value(prefs.wrap);
        self.design.set_value(prefs.design as i32);
        self.accent.set_value(prefs.accent as i32);
        self.size.set_value(
            [12, 14, 16, 18, 20, 24, 28]
                .iter()
                .position(|&v| v == prefs.font_size)
                .unwrap_or(2) as i32,
        );
        self.image_auto.set_value(prefs.image_auto_size);
        self.update_check.set_value(prefs.update_check);
        self.update_install.set_value(prefs.update_install);
        self.win.redraw();
    }
    pub fn set_orientation(&mut self, vertical: bool) {
        self.horizontal
            .set_color(if vertical { BG } else { theme::SURFACE });
        self.horizontal
            .set_label_color(if vertical { MUTED } else { theme::TEXT });
        self.vertical
            .set_color(if vertical { theme::SURFACE } else { BG });
        self.vertical
            .set_label_color(if vertical { theme::TEXT } else { MUTED });
        self.horizontal.redraw();
        self.vertical.redraw();
    }
}

pub struct UpdatePanel {
    pub win: Window,
    title: Frame,
    status: Frame,
    notes: fltk::text::TextBuffer,
    install: Button,
    check: Button,
}
impl UpdatePanel {
    pub fn new(tx: app::Sender<Message>) -> Self {
        Group::set_current(None::<&Group>);
        let mut win = Window::default()
            .with_size(640, 560)
            .with_label("Updates — EditorTool");
        win.set_color(PANEL);
        set_icon(&mut win);
        let mut root = Flex::new(22, 18, 596, 524, None).column();
        root.set_spacing(12);
        let title = theme::heading("EditorTool aktualisieren", 21);
        root.fixed(&title, 30);
        let mut status = Frame::default();
        status.set_label_color(MUTED);
        status.set_label_size(13);
        status.set_align(Align::Left | Align::Inside | Align::Wrap);
        root.fixed(&status, 46);
        let notes = fltk::text::TextBuffer::default();
        let mut display = fltk::text::TextDisplay::default();
        display.set_buffer(notes.clone());
        display.set_color(BG);
        display.set_text_color(theme::TEXT);
        display.set_text_font(fltk::enums::Font::Helvetica);
        display.set_text_size(14);
        display.set_frame(FrameType::FlatBox);
        display.set_scrollbar_size(14);
        display.wrap_mode(fltk::text::WrapMode::AtBounds, 0);
        display.super_draw_first(true);
        display.draw(|w| theme::draw_scrollbars(w));
        let grab = Cell::new(None);
        display.handle(move |w, e| theme::handle_scrollbars(w, e, &grab));
        let mut hint = Frame::default().with_label(
            "Vor der Installation wird die Sitzung gesichert. Die App startet danach neu.",
        );
        hint.set_label_size(11);
        hint.set_label_color(MUTED);
        hint.set_align(Align::Left | Align::Inside | Align::Wrap);
        root.fixed(&hint, 36);
        let mut controls = Flex::default().row();
        controls.set_spacing(8);
        let mut check = Button::default().with_label("Erneut prüfen");
        theme::button(&mut check, false, None);
        check.emit(tx, (0, Action::CheckUpdates(true)));
        let mut install = Button::default().with_label("Jetzt aktualisieren");
        theme::button(&mut install, true, None);
        install.emit(tx, (0, Action::InstallUpdate));
        install.deactivate();
        let mut close = Button::default().with_label("Schliessen");
        theme::button(&mut close, false, None);
        close.set_callback(|b| {
            if let Some(mut w) = b.window() {
                w.hide()
            }
        });
        controls.end();
        root.fixed(&controls, 38);
        root.end();
        win.end();
        win.set_callback(|w| w.hide());
        Self {
            win,
            title,
            status,
            notes,
            install,
            check,
        }
    }
    pub fn state(&mut self, title: &str, status: &str, notes: &str, available: bool, busy: bool) {
        self.title.set_label(title);
        self.status.set_label(status);
        self.notes.set_text(notes);
        if available && !busy {
            self.install.activate()
        } else {
            self.install.deactivate()
        }
        if busy {
            self.check.deactivate()
        } else {
            self.check.activate()
        }
        self.win.redraw();
    }
    pub fn progress(&mut self, percent: u8) {
        self.status
            .set_label(&format!("Update wird heruntergeladen: {percent} %"));
        self.win.redraw();
    }
    pub fn show(&mut self, owner: Option<&Window>) {
        if !self.win.shown() {
            if let Some(owner) = owner {
                self.win
                    .set_pos(owner.x() + (owner.w() - 640) / 2, owner.y() + 60);
            }
        }
        self.win.show();
        dark_title(&self.win);
    }
}
