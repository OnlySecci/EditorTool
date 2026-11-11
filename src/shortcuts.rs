use crate::*;
use fltk::{browser::HoldBrowser, enums::Key, input::Input};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    NewTab,
    NewWindow,
    CloseTab,
    CloseAll,
    Open,
    Save,
    SaveAs,
    Undo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Find,
    Replace,
    FindNext,
    Image,
    Settings,
    Updates,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    NextTab,
    PreviousTab,
    ToggleCompare,
    Compare,
    StopCompare,
    CloseCompare,
    SplitSide,
    SplitStack,
    MergeSplit,
}
pub const ALL: &[Command] = &[
    Command::NewTab,
    Command::NewWindow,
    Command::CloseTab,
    Command::CloseAll,
    Command::Open,
    Command::Save,
    Command::SaveAs,
    Command::Undo,
    Command::Cut,
    Command::Copy,
    Command::Paste,
    Command::SelectAll,
    Command::Find,
    Command::Replace,
    Command::FindNext,
    Command::Image,
    Command::Settings,
    Command::Updates,
    Command::ZoomIn,
    Command::ZoomOut,
    Command::ZoomReset,
    Command::NextTab,
    Command::PreviousTab,
    Command::ToggleCompare,
    Command::Compare,
    Command::StopCompare,
    Command::CloseCompare,
    Command::SplitSide,
    Command::SplitStack,
    Command::MergeSplit,
];
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chord {
    key: i32,
    mods: i32,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Binding {
    command: Command,
    chord: Chord,
}
thread_local! {static CURRENT:RefCell<Vec<Binding>>=RefCell::new(defaults());static CAPTURE:Cell<usize>=const{Cell::new(0)};}
pub fn capturing() -> bool {
    CAPTURE.with(|p| app::focus().is_some_and(|w| w.as_widget_ptr() as usize == p.get()))
}
fn mask() -> i32 {
    (Shortcut::Ctrl | Shortcut::Alt | Shortcut::Shift | Shortcut::Meta).bits()
}
impl Command {
    pub fn label(self) -> &'static str {
        use Command::*;
        match self {
            NewTab => "Neuer Tab",
            NewWindow => "Neues Fenster",
            CloseTab => "Tab schliessen",
            CloseAll => "Alle Tabs schliessen",
            Open => "Datei öffnen",
            Save => "Speichern",
            SaveAs => "Speichern unter",
            Undo => "Rückgängig",
            Cut => "Ausschneiden",
            Copy => "Kopieren",
            Paste => "Einfügen",
            SelectAll => "Alles auswählen",
            Find => "Suchen",
            Replace => "Ersetzen",
            FindNext => "Weitersuchen",
            Image => "Bild einfügen",
            Settings => "Einstellungen",
            Updates => "Nach Updates suchen",
            ZoomIn => "Zoom vergrössern",
            ZoomOut => "Zoom verkleinern",
            ZoomReset => "Zoom 100 %",
            NextTab => "Nächster Tab",
            PreviousTab => "Vorheriger Tab",
            ToggleCompare => "Vergleich ein/aus",
            Compare => "Vergleich starten",
            StopCompare => "Vergleich stoppen",
            CloseCompare => "Vergleich schliessen",
            SplitSide => "Split nebeneinander",
            SplitStack => "Split übereinander",
            MergeSplit => "Split zusammenführen",
        }
    }
    pub fn action(self) -> Action {
        use Command::*;
        match self {
            NewTab => Action::NewTab,
            NewWindow => Action::NewWindow,
            CloseTab => Action::CloseTab,
            CloseAll => Action::CloseAllTabs,
            Open => Action::Open(false),
            Save => Action::Save(false),
            SaveAs => Action::Save(true),
            Undo => Action::Undo,
            Cut => Action::Cut,
            Copy => Action::Copy,
            Paste => Action::Paste,
            SelectAll => Action::SelectAll,
            Find => Action::ShowFind(false),
            Replace => Action::ShowFind(true),
            FindNext => Action::Find,
            Image => Action::InsertImage,
            Settings => Action::Settings,
            Updates => Action::CheckUpdates(true),
            ZoomIn => Action::Zoom(10),
            ZoomOut => Action::Zoom(-10),
            ZoomReset => Action::Zoom(0),
            NextTab => Action::NextTab(false),
            PreviousTab => Action::NextTab(true),
            ToggleCompare => Action::ToggleCompare,
            Compare => Action::Compare,
            StopCompare => Action::StopCompare,
            CloseCompare => Action::CloseCompare,
            SplitSide => Action::Split(false),
            SplitStack => Action::Split(true),
            MergeSplit => Action::MergeSplit,
        }
    }
    pub fn default(self) -> Chord {
        use Command::*;
        let p = Shortcut::Command;
        let shift = p | Shortcut::Shift;
        let (key, mods) = match self {
            NewTab => ('t' as i32, p),
            NewWindow => ('n' as i32, p),
            CloseTab => ('w' as i32, p),
            CloseAll => ('w' as i32, shift),
            Open => ('o' as i32, p),
            Save => ('s' as i32, p),
            SaveAs => ('s' as i32, shift),
            Undo => ('z' as i32, p),
            Cut => ('x' as i32, p),
            Copy => ('c' as i32, p),
            Paste => ('v' as i32, p),
            SelectAll => ('a' as i32, p),
            Find => ('f' as i32, p),
            Replace => ('h' as i32, p),
            FindNext => (Key::F3.bits(), Shortcut::None),
            Image => ('i' as i32, shift),
            Settings => (',' as i32, p),
            Updates => ('u' as i32, shift),
            ZoomIn => ('+' as i32, p),
            ZoomOut => ('-' as i32, p),
            ZoomReset => ('0' as i32, p),
            NextTab => (Key::Tab.bits(), p),
            PreviousTab => (Key::Tab.bits(), shift),
            _ => (0, Shortcut::None),
        };
        Chord {
            key,
            mods: mods.bits(),
        }
    }
    pub fn menu(self) -> Option<&'static str> {
        use Command::*;
        Some(match self {
            NewTab => "&Datei/Neuer &Tab",
            NewWindow => "&Datei/Neues &Fenster",
            CloseTab => "&Datei/Tab schließen",
            CloseAll => "&Datei/Alle Tabs schließen",
            Open => "&Datei/Öffnen…",
            Save => "&Datei/Speichern",
            SaveAs => "&Datei/Speichern unter…",
            Undo => "&Bearbeiten/Rückgängig",
            Cut => "&Bearbeiten/Ausschneiden",
            Copy => "&Bearbeiten/Kopieren (Auswahl oder alles)",
            Paste => "&Bearbeiten/Einfügen",
            SelectAll => "&Bearbeiten/Alles auswählen",
            Find => "&Bearbeiten/Suchen…",
            Replace => "&Bearbeiten/Ersetzen…",
            FindNext => "&Bearbeiten/Weitersuchen",
            Image => "&Bearbeiten/Bild einfügen…",
            Settings => "&Einstellungen/Allgemein…",
            Updates => "&Hilfe/Nach Updates suchen…",
            ZoomIn => "&Ansicht/Zoom/Vergrössern",
            ZoomOut => "&Ansicht/Zoom/Verkleinern",
            ZoomReset => "&Ansicht/Zoom/100 %",
            ToggleCompare => "&Ansicht/Vergleich ein oder aus",
            Compare => "&Ansicht/Texte vergleichen",
            StopCompare => "&Ansicht/Vergleich stoppen",
            CloseCompare => "&Ansicht/Vergleich schliessen",
            SplitSide => "&Ansicht/Split View/Nebeneinander",
            SplitStack => "&Ansicht/Split View/Übereinander",
            MergeSplit => "&Ansicht/Split View/Zusammenführen",
            _ => return None,
        })
    }
}
impl Chord {
    fn event() -> Self {
        let mut key = app::event_key().bits();
        let mut mods = app::event_state().bits() & mask();
        if key == ('=' as i32) || key == ('+' as i32) {
            key = '+' as i32;
            mods &= !Shortcut::Shift.bits();
        }
        Self { key, mods }
    }
    fn allowed(self) -> bool {
        self.key == 0 && self.mods == 0
            || (self.key > 0
                && self.key <= 0x10ffff
                && !(0xffe1..=0xffee).contains(&self.key)
                && self.mods & !mask() == 0
                && (self.mods & (Shortcut::Ctrl | Shortcut::Alt | Shortcut::Meta).bits() != 0
                    || (Key::F1.bits()..=Key::F12.bits()).contains(&self.key)))
    }
    fn shortcut(self) -> Shortcut {
        Shortcut::from_bits_retain(self.key | self.mods)
    }
    fn text(self) -> String {
        if self.key == 0 {
            return "Nicht zugewiesen".into();
        }
        let mut parts = vec![];
        for (flag, label) in [
            (Shortcut::Ctrl, "Ctrl"),
            (Shortcut::Meta, "Cmd"),
            (Shortcut::Alt, "Alt"),
            (Shortcut::Shift, "Shift"),
        ] {
            if self.mods & flag.bits() != 0 {
                parts.push(label.to_string());
            }
        }
        let key = if self.key == Key::Tab.bits() {
            "Tab".into()
        } else if (Key::F1.bits()..=Key::F12.bits()).contains(&self.key) {
            format!("F{}", self.key - Key::F1.bits() + 1)
        } else {
            char::from_u32(self.key as u32)
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_else(|| format!("Taste {}", self.key))
        };
        parts.push(key);
        parts.join("+")
    }
}
fn defaults() -> Vec<Binding> {
    ALL.iter()
        .map(|c| Binding {
            command: *c,
            chord: c.default(),
        })
        .collect()
}
pub fn hint(command: Command) -> String {
    CURRENT.with(|p| {
        p.borrow()
            .iter()
            .find(|b| b.command == command)
            .map(|b| b.chord.text())
            .unwrap_or_default()
    })
}
pub fn action_hint(action: &Action) -> Option<String> {
    use Command::*;
    let command = match action {
        Action::NewTab => NewTab,
        Action::CloseTab => CloseTab,
        Action::CloseAllTabs => CloseAll,
        Action::Save(false) => Save,
        Action::Save(true) => SaveAs,
        Action::Split(false) => SplitSide,
        Action::Split(true) => SplitStack,
        Action::MergeSplit => MergeSplit,
        _ => return None,
    };
    let text = hint(command);
    Some(if text == "Nicht zugewiesen" {
        String::new()
    } else {
        text
    })
}
pub fn resolve() -> Option<Command> {
    let chord = Chord::event();
    CURRENT.with(|p| {
        p.borrow()
            .iter()
            .find(|b| b.chord.key != 0 && b.chord == chord)
            .map(|b| b.command)
    })
}
pub fn is_event(command: Command) -> bool {
    resolve() == Some(command)
}
pub fn block_old_default() -> bool {
    let chord = Chord::event();
    ALL.iter()
        .any(|c| c.default().key != 0 && c.default() == chord)
        && resolve().is_none()
}
pub fn apply_menu(menu: &mut MenuBar) {
    CURRENT.with(|p| {
        for b in p.borrow().iter() {
            if let Some(path) = b.command.menu() {
                if let Some(mut item) = menu.find_item(path) {
                    item.set_shortcut(b.chord.shortcut());
                }
            }
        }
    });
    menu.redraw();
}
fn validate(bindings: &[Binding]) -> Result<(), String> {
    if bindings.len() != ALL.len() {
        return Err("Ungültige Tastenkürzel-Liste".into());
    }
    for (i, b) in bindings.iter().enumerate() {
        if !b.chord.allowed() || bindings[..i].iter().any(|a| a.command == b.command) {
            return Err("Ungültige Tastenkombination".into());
        }
        if b.chord.key != 0 {
            if let Some(other) = bindings[..i].iter().find(|a| a.chord == b.chord) {
                return Err(format!(
                    "{} ist bereits für {} vergeben.",
                    b.chord.text(),
                    other.command.label()
                ));
            }
        }
    }
    Ok(())
}
pub fn load(config: &std::path::Path) -> Result<(), String> {
    let file = config.parent().unwrap().join("shortcuts.json");
    if !file.exists() {
        return Ok(());
    }
    let saved: Vec<serde_json::Value> =
        serde_json::from_slice(&std::fs::read(file).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut values = defaults();
    for b in saved
        .into_iter()
        .filter_map(|v| serde_json::from_value::<Binding>(v).ok())
    {
        if let Some(v) = values.iter_mut().find(|v| v.command == b.command) {
            *v = b;
        }
    }
    validate(&values)?;
    CURRENT.with(|p| *p.borrow_mut() = values);
    Ok(())
}
pub fn save(config: &std::path::Path, values: Vec<Binding>) -> Result<(), String> {
    validate(&values)?;
    crate::session::atomic_write(
        &config.parent().unwrap().join("shortcuts.json"),
        &serde_json::to_vec_pretty(&values).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    CURRENT.with(|p| *p.borrow_mut() = values);
    Ok(())
}
pub struct Panel {
    pub win: Window,
    pub page: crate::settings_page::Page,
    draft: Rc<RefCell<Vec<Binding>>>,
    list: HoldBrowser,
    capture: Input,
    captured: Rc<Cell<Chord>>,
}
fn rows(list: &mut HoldBrowser, values: &[Binding]) {
    let selected = list.value().max(1);
    list.clear();
    for b in values {
        list.add(&format!("{}\t{}", b.command.label(), b.chord.text()));
    }
    list.select(selected);
}
impl Panel {
    pub fn new(tx: app::Sender<Message>) -> Self {
        Group::set_current(None::<&Group>);
        let mut win = Window::default()
            .with_size(660, 680)
            .with_label("Tastenkürzel");
        win.set_color(PANEL);
        chrome::set_icon(&mut win);
        let mut root = Flex::new(20, 20, 620, 640, None).column();
        root.set_spacing(12);
        let title = theme::heading("Tastenkürzel", 22);
        root.fixed(&title, 32);
        let mut info=Frame::default().with_label("Befehl auswählen, neue Kombination im Feld drücken und zuweisen.\nCtrl/Cmd-, Alt- und Funktionstasten sind möglich. Doppelte Belegungen werden angezeigt.");
        info.set_align(Align::Left | Align::Inside | Align::Wrap);
        info.set_label_size(12);
        info.set_label_color(MUTED);
        root.fixed(&info, 52);
        let draft = Rc::new(RefCell::new(defaults()));
        let captured = Rc::new(Cell::new(Chord::default()));
        let mut list = HoldBrowser::default();
        list.set_color(BG);
        list.set_label_color(FG);
        list.set_frame(FrameType::FlatBox);
        list.set_text_size(13);
        list.set_format_char('\0');
        list.set_column_char('\t');
        list.set_column_widths(&[310, 0]);
        rows(&mut list, &draft.borrow());
        let heading = theme::heading("Neue Tastenkombination – hier klicken und drücken", 12);
        root.fixed(&heading, 18);
        let mut capture = Input::default();
        theme::input(&mut capture);
        CAPTURE.with(|p| p.set(capture.as_widget_ptr() as usize));
        root.fixed(&capture, 38);
        let value = captured.clone();
        capture.handle(move |field, event| {
            if event == Event::KeyDown {
                let chord = Chord::event();
                if chord.allowed() && chord.key != 0 {
                    value.set(chord);
                    field.set_value(&chord.text());
                }
                return true;
            }
            false
        });
        let value = captured.clone();
        let mut field = capture.clone();
        let values = draft.clone();
        list.set_callback(move |l| {
            if let Some(b) = values.borrow().get((l.value() - 1).max(0) as usize) {
                value.set(b.chord);
                field.set_value(&b.chord.text());
            }
        });
        let mut row = Flex::default().row();
        row.set_spacing(8);
        let mut assign = Button::default().with_label("Zuweisen");
        theme::button(&mut assign, true, None);
        let values = draft.clone();
        let value = captured.clone();
        let mut browser = list.clone();
        assign.set_callback(move |b| {
            let index = browser.value() - 1;
            if index < 0 {
                return;
            }
            let mut next = values.borrow().clone();
            next[index as usize].chord = value.get();
            match validate(&next) {
                Ok(()) => {
                    *values.borrow_mut() = next;
                    rows(&mut browser, &values.borrow());
                }
                Err(e) => {
                    let owner = b
                        .window()
                        .map(|w| unsafe { Window::from_widget_ptr(w.as_widget_ptr() as _) });
                    chrome::decision(
                        owner.as_ref(),
                        "Tastenkürzel bereits belegt",
                        &e,
                        "",
                        &["Schliessen"],
                        0,
                    );
                }
            }
        });
        let mut clear = Button::default().with_label("Belegung entfernen");
        theme::button(&mut clear, false, None);
        let values = draft.clone();
        let mut browser = list.clone();
        let mut field = capture.clone();
        let value = captured.clone();
        clear.set_callback(move |_| {
            let i = browser.value() - 1;
            if i >= 0 {
                values.borrow_mut()[i as usize].chord = Chord::default();
                rows(&mut browser, &values.borrow());
                field.set_value("Nicht zugewiesen");
                value.set(Chord::default());
            }
        });
        row.end();
        root.fixed(&row, 38);
        let mut row = Flex::default().row();
        row.set_spacing(8);
        let mut reset = Button::default().with_label("Standardbelegung");
        theme::button(&mut reset, false, None);
        let values = draft.clone();
        let mut browser = list.clone();
        reset.set_callback(move |_| {
            *values.borrow_mut() = defaults();
            rows(&mut browser, &values.borrow());
            browser.do_callback();
        });
        let mut apply = Button::default().with_label("Für alle Fenster übernehmen");
        theme::button(&mut apply, true, None);
        let values = draft.clone();
        apply.set_callback(move |_| tx.send((0, Action::KeyboardApply(values.borrow().clone()))));
        row.end();
        root.fixed(&row, 38);
        root.end();
        win.end();
        let page = crate::settings_page::Page::new(&mut win, &root);
        Self {
            win,
            page,
            draft,
            list,
            capture,
            captured,
        }
    }
    pub fn reload(&mut self) {
        let values = CURRENT.with(|p| p.borrow().clone());
        *self.draft.borrow_mut() = values;
        rows(&mut self.list, &self.draft.borrow());
        let index = (self.list.value() - 1).max(0) as usize;
        let chord = self.draft.borrow()[index].chord;
        self.captured.set(chord);
        self.capture.set_value(&chord.text());
    }
}
