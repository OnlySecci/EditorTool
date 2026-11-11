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
    if let Ok(icon) = RgbImage::new(
        include_bytes!("../assets/editortool.rgba"),
        64,
        64,
        ColorDepth::Rgba8,
    ) {
        win.set_icon(Some(icon));
    }
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
        let enabled: u32 = 1;
        let black: u32 = 0x00272322;
        let white: u32 = 0x00eeeeee;
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
            .set_pos(owner.x() + (owner.w() - 520) / 2, owner.y() + 80);
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
    horizontal: Button,
    vertical: Button,
}
impl SettingsPanel {
    pub fn new(wid: usize, tx: app::Sender<Message>, vertical: bool) -> Self {
        Group::set_current(None::<&Group>);
        let mut win = Window::default()
            .with_size(560, 414)
            .with_label("Einstellungen — EditorTool");
        win.set_color(PANEL);
        set_icon(&mut win);
        let mut root = Flex::new(28, 26, 504, 362, None).column();
        root.set_spacing(16);
        let title = theme::heading("Einstellungen", 26);
        root.fixed(&title, 38);
        let mut section = theme::heading("DARSTELLUNG", 11);
        section.set_label_color(MUTED);
        root.fixed(&section, 18);
        let mut card = Flex::default().column();
        card.set_frame(FrameType::RFlatBox);
        card.set_color(BG);
        card.set_margins(20, 18, 20, 18);
        card.set_spacing(10);
        let title = theme::heading("Tabs", 17);
        card.fixed(&title, 24);
        let mut description =
            Frame::default().with_label("Deine Dokumente. Oben oder in der Seitenleiste.");
        description.set_label_size(13);
        description.set_label_color(MUTED);
        description.set_align(Align::Left | Align::Inside);
        card.fixed(&description, 24);
        let mut segment = Flex::default().row();
        segment.set_color(BG);
        segment.set_spacing(6);
        let mut horizontal = Button::default().with_label("Horizontal");
        theme::button(&mut horizontal, false, None);
        horizontal.emit(tx, (wid, Action::Orientation(false)));
        let mut vertical_button = Button::default().with_label("Vertikal");
        theme::button(&mut vertical_button, false, None);
        vertical_button.emit(tx, (wid, Action::Orientation(true)));
        segment.end();
        card.fixed(&segment, 42);
        card.end();
        root.fixed(&card, 158);
        let mut hint = Frame::default().with_label(
            "Die Auswahl wird automatisch gespeichert\nund für alle Fenster übernommen.",
        );
        hint.set_label_size(13);
        hint.set_label_color(MUTED);
        hint.set_align(Align::Left | Align::Inside);
        root.fixed(&hint, 48);
        let mut footer = Frame::default().with_label("EditorTool     ·     1.0.0");
        footer.set_label_size(11);
        footer.set_label_color(MUTED);
        footer.set_align(Align::Left | Align::Inside);
        root.end();
        win.end();
        win.set_callback(|w| w.hide());
        let mut panel = Self {
            win,
            horizontal,
            vertical: vertical_button,
        };
        panel.set_orientation(vertical);
        panel
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
    pub fn show(&mut self, owner: &Window) {
        self.win
            .set_pos(owner.x() + (owner.w() - 560) / 2, owner.y() + 100);
        self.win.show();
        dark_title(&self.win);
    }
}
