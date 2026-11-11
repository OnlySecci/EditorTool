#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod caret;
mod chrome;
mod core;
#[cfg(target_os = "linux")]
mod desktop;
mod ipc;
mod peer;
mod preferences;
mod rich;
mod session;
mod settings_page;
mod sharing_runtime;
mod sharing_ui;
mod shortcuts;
mod split;
mod syntax;
mod tab_menu;
mod theme;
mod tray;
mod updater;
use rich::RichEditor;
use syntax::Language;
type SyntaxKey = (u64, u64, Language, Language);

use crate::core::{pattern, Comparison, Counts};
use fltk::{
    app,
    button::Button,
    dialog,
    enums::{Align, Color, Event, Font, FrameType, Shortcut},
    frame::Frame,
    group::{Flex, FlexType, Group, Scroll, ScrollType},
    menu::{MenuBar, MenuFlag},
    prelude::*,
    text::{StyleTableEntry, TextBuffer},
    window::DoubleWindow as Window,
};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
};

const BG: Color = theme::CANVAS;
const PANEL: Color = theme::CHROME;
const FG: Color = theme::TEXT;
const MUTED: Color = theme::SECONDARY;
const ACCENT: Color = Color::from_hex(0x265a97);
const TAB_HEIGHT: i32 = 32;
const TAB_VERTICAL_STEP: i32 = 34;
const TAB_WIDTH: i32 = 174;
const TAB_HORIZONTAL_STEP: i32 = 178;
const ADD_TAB_SIZE: i32 = 30;

#[derive(Clone)]
enum Action {
    SharingSettings,
    ApplySharing,
    KeepTray,
    AddClient,
    RemoveClient,
    ProbeClients,
    Transfer(bool),
    SubmitTransfer,
    StopSync,
    ShowConflicts,
    PreviewConflict,
    ResolveConflict,
    PeerEvent(peer::Event),
    PublishDoc(String, u64),
    SyncCountdown,
    InitTray,
    TrayUnavailable,
    ShowWindows,
    QuitAll,
    NewWindow,
    NewTab,
    Select(usize),
    CloseTab,
    CloseAllTabs,
    WordWrap(bool),
    Design(u8),
    Accent(u8),
    FontSize(i32),
    UpdateCheckOption(bool),
    UpdateInstallOption(bool),
    CheckUpdates(bool),
    UpdateChecked(Result<Option<updater::Release>, String>, bool),
    InstallUpdate,
    UpdateProgress(u8),
    UpdateDownloaded(Result<updater::Download, String>),
    UpdateNotice(String),
    CloseWindow,
    Orientation(bool),
    Open(bool),
    Save(bool),
    Copy,
    Paste,
    ShowFind(bool),
    ToggleReplace,
    Settings,
    KeyboardSettings,
    KeyboardApply(Vec<shortcuts::Binding>),
    Zoom(i32),
    ImageAutoSize(bool),
    TabContext(usize, i32, i32),
    CopyPath,
    CloseOtherTabs,
    Split(bool),
    EndSplit,
    MergeSplit,
    StopCompare,
    CloseCompare,
    RelayoutSplit,
    MoveTab(usize),
    Undo,
    Cut,
    SelectAll,
    NextTab(bool),
    Find,
    Replace,
    ReplaceAll,
    Compare,
    ToggleCompare,
    Compared(String, u64, (u64, u64), Comparison),
    SetLanguage(Option<Language>),
    SyntaxRequest(usize),
    SyntaxReady(usize, SyntaxKey, Option<String>, Option<String>),
    SaveSession,
    CacheError(String),
    InsertImage,
    OpenExternal(Vec<PathBuf>),
    SidebarWidth(i32),
}
type Message = (usize, Action);

#[derive(Default)]
struct Meta {
    counts: Counts,
    revision: u64,
    style: Option<TextBuffer>,
    preview: String,
}

struct Document {
    id: usize,
    group: Flex,
    panes: Flex,
    rightcol: Flex,
    left: RichEditor,
    right: RichEditor,
    target_right: Rc<Cell<bool>>,
    left_buf: TextBuffer,
    right_buf: TextBuffer,
    left_meta: Rc<RefCell<Meta>>,
    right_meta: Rc<RefCell<Meta>>,
    styles: Option<(TextBuffer, TextBuffer)>,
    compared: Option<(u64, u64)>,
    status: Frame,
    sync_status: Frame,
    sync_text: String,
    sync_error: Option<u64>,
    status_text: String,
    status_key: String,
    report: String,
    comparing: bool,
    compare_generation: u64,
    compare_cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    compare_open: Button,
    compare_run: Button,
    compare_stop: Button,
    compare_close: Button,
    path: Option<PathBuf>,
    saved_revision: u64,
    comparison_visible: bool,
    page: u8,
    sharing: Option<peer::Binding>,
    display_name: Option<String>,
    received_offer: Option<String>,
    share_revision: u64,
    cache_key: String,
    right_path: Option<PathBuf>,
    left_language: Option<Language>,
    right_language: Option<Language>,
    syntax_pending: bool,
    syntax_key: Option<SyntaxKey>,
}

impl Document {
    fn language(&self, right: bool) -> Language {
        if right {
            self.right_language
                .unwrap_or_else(|| Language::detect(self.right_path.as_deref()))
        } else {
            self.left_language
                .unwrap_or_else(|| Language::detect(self.path.as_deref()))
        }
    }
    fn syntax_key(&self) -> SyntaxKey {
        (
            self.left_meta.borrow().revision,
            self.right_meta.borrow().revision,
            self.language(false),
            self.language(true),
        )
    }
    fn clear_styles(&mut self) {
        self.left.unset_highlight_data(None::<TextBuffer>);
        self.right.unset_highlight_data(None::<TextBuffer>);
        self.left_meta.borrow_mut().style = None;
        self.right_meta.borrow_mut().style = None;
        self.styles = None;
        self.compared = None;
        self.syntax_key = None;
    }
    fn snapshot(&self) -> session::TabState {
        session::TabState {
            page: self.page,
            zoom: self.left.zoom(),
            right_zoom: self.right.zoom(),
            sharing: self.sharing.clone(),
            display_name: self.display_name.clone(),
            received_offer: self.received_offer.clone(),
            key: self.cache_key.clone(),
            id: self.id,
            path: self.path.clone(),
            right_path: self.right_path.clone(),
            dirty: self.dirty(),
            left_language: self.left_language,
            right_language: self.right_language,
            comparison: self.comparison_visible,
            left_cursor: self.left.insert_position(),
            right_cursor: self.right.insert_position(),
        }
    }

    fn dirty(&self) -> bool {
        if self.page != 0 {
            return false;
        }
        self.left_meta.borrow().revision != self.saved_revision
    }
    fn title(&self) -> String {
        if self.page != 0 {
            return match self.page {
                1 => "Einstellungen",
                2 => "Clients und Sync",
                _ => "Tastenkürzel",
            }
            .into();
        }
        format!(
            "{}{}",
            self.path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().into_owned())
                .or_else(|| self.display_name.clone())
                .unwrap_or_else(|| {
                    let preview = self.left_meta.borrow().preview.clone();
                    if preview.is_empty() {
                        format!("Text {}", self.id + 1)
                    } else {
                        preview
                    }
                }),
            if self.dirty() { " *" } else { "" }
        )
    }
    fn stop_comparison(&mut self) {
        if let Some(cancel) = self.compare_cancel.take() {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.compare_generation = self.compare_generation.wrapping_add(1);
        self.comparing = false;
        self.clear_styles();
        self.report = "Vergleich beendet".into();
    }
    fn rewire_comparison(&mut self, wid: usize, tx: app::Sender<Message>) {
        self.compare_open.emit(tx, (wid, Action::Open(true)));
        self.compare_run.emit(tx, (wid, Action::Compare));
        self.compare_stop.emit(tx, (wid, Action::StopCompare));
        self.compare_close.emit(tx, (wid, Action::CloseCompare));
        if self.comparing {
            self.stop_comparison();
        }
    }
    fn refresh(&mut self) {
        if self.comparing || self.compared.is_some() {
            self.compare_stop.activate();
            self.compare_stop.set_label_color(FG);
        } else {
            self.compare_stop.deactivate();
            self.compare_stop.set_label_color(MUTED);
        }
        if self.comparing {
            self.compare_run.deactivate();
            self.compare_run.set_label("Läuft…");
        } else {
            self.compare_run.activate();
            self.compare_run.set_label("Vergleichen");
        }
        let rev = (
            self.left_meta.borrow().revision,
            self.right_meta.borrow().revision,
        );
        if self.compared.is_some_and(|r| r != rev) {
            self.clear_styles();
            self.report = "Vergleich veraltet – erneut vergleichen".into();
        }
        let (buf, editor, meta) = if self.target_right.get() {
            (&self.right_buf, &self.right, &self.right_meta)
        } else {
            (&self.left_buf, &self.left, &self.left_meta)
        };
        let c = meta.borrow().counts;
        let cursor = editor.insert_position();
        let zoom = if self.target_right.get() {
            self.right.zoom()
        } else {
            self.left.zoom()
        };
        let key = format!(
            "{zoom}:{}:{cursor}:{:?}:{}:{}",
            meta.borrow().revision,
            buf.selection_position(),
            self.target_right.get(),
            format!(
                "{}:{}",
                self.report,
                self.language(self.target_right.get()).name()
            )
        );
        if key == self.status_key {
            return;
        }
        self.status_key = key;
        let line = buf.count_lines(0, cursor) + 1;
        // Only the current line and current selection are read, never the whole document.
        let col = buf
            .text_range(buf.line_start(cursor), cursor)
            .unwrap_or_default()
            .chars()
            .count()
            + 1;
        let selected = buf.selection_text();
        let s = Counts::of(&selected);
        let selection_lines = if selected.is_empty() {
            0
        } else {
            s.breaks + usize::from(!selected.ends_with('\n'))
        };
        let selection = if s.chars > 0 {
            format!(
                "Auswahl  {selection_lines} Zeilen · {} Zeichen · {} Wörter",
                s.chars,
                selected.split_whitespace().count()
            )
        } else {
            "Keine Auswahl".to_owned()
        };
        let text = format!("Zeile {line}, Spalte {col}     ·     {} Zeilen     ·     {} Zeichen     ·     {} Bytes\n{selection}     ·     {}", c.breaks + 1, c.chars, c.bytes, format!("{} · {} Bilder · Zoom {zoom} % · {}",self.language(self.target_right.get()).name(),if self.target_right.get(){self.right.image_count()}else{self.left.image_count()},self.report));
        if text != self.status_text {
            self.status.set_label(&text);
            self.status_text = text;
        }
    }
}

struct EditorWindow {
    win: Window,
    root: Flex,
    workspace: Flex,
    tab_scroll: Scroll,
    divider: Frame,
    sidebar_width: i32,
    bar: Group,
    area: Group,
    find: chrome::SearchPanel,
    settings: chrome::SettingsPanel,
    menu: MenuBar,
    docs: Vec<Document>,
    active: usize,
    next_doc: usize,
    vertical: bool,
    tab_signature: String,
    reveal_tab: bool,
    route: Rc<Cell<usize>>,
    embedded: bool,
    split_key: Option<String>,
    split_stacked: Rc<Cell<bool>>,
    split_ratio: Rc<Cell<i32>>,
}

fn button(
    label: &str,
    width: i32,
    row: &mut Flex,
    action: Action,
    wid: usize,
    tx: app::Sender<Message>,
) {
    let mut b = Button::default().with_label(label);
    theme::button(
        &mut b,
        label == "Weitersuchen" || label == "Vergleichen",
        None,
    );
    row.fixed(&b, width);
    b.emit(tx, (wid, action));
}

fn editor(target: Rc<Cell<bool>>, right: bool) -> (RichEditor, TextBuffer, Rc<RefCell<Meta>>) {
    let ed = RichEditor::new(target, right);
    let mut buf = ed.buffer();
    buf.set_tab_distance(4);
    let meta = Rc::new(RefCell::new(Meta::default()));
    let observed = meta.clone();
    buf.add_modify_callback2(move |b, pos, inserted, deleted, _, removed| {
        if inserted != 0 || deleted != 0 {
            let added = b.text_range(pos, pos + inserted).unwrap_or_default();
            let mut data = observed.borrow_mut();
            data.counts.update(&added, removed);
            data.revision += 1;
            if pos < 512 || data.preview.is_empty() {
                let prefix = b
                    .text_range(0, b.utf8_align(b.length().min(512)))
                    .unwrap_or_default()
                    .replace(rich::IMAGE_MARKER, " ");
                data.preview = prefix
                    .split_whitespace()
                    .take(6)
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(52)
                    .collect();
            }
            if let Some(style) = data.style.as_mut() {
                style.replace(
                    pos,
                    (pos + deleted).min(style.length()),
                    &"A".repeat(inserted as usize),
                );
            }
        }
    });
    (ed, buf, meta)
}

impl EditorWindow {
    fn new(wid: usize, tx: app::Sender<Message>, vertical: bool) -> Self {
        Self::build(wid, tx, vertical, true)
    }
    fn build(wid: usize, tx: app::Sender<Message>, vertical: bool, show: bool) -> Self {
        Group::set_current(None::<&Group>);
        let mut win = Window::default()
            .with_size(1180, 760)
            .with_label("EditorTool — Plain Text");
        win.set_color(PANEL);
        win.size_range(800, 480, 0, 0);
        chrome::set_icon(&mut win);
        let mut root = Flex::new(0, 0, 1180, 760, None).column();
        root.set_spacing(0);
        let mut menurow = Flex::default().row();
        menurow.set_margins(10, 3, 10, 3);
        menurow.set_spacing(5);
        menurow.set_color(PANEL);
        let mut menu = MenuBar::default();
        menu.set_frame(FrameType::FlatBox);
        menu.set_color(PANEL);
        menu.set_text_size(13);
        menu.set_text_color(MUTED);
        let entries = [
            (
                "&Einstellungen/Allgemein…",
                Shortcut::Command | ',',
                Action::Settings,
            ),
            (
                "&Einstellungen/Clients und Sync…",
                Shortcut::None,
                Action::SharingSettings,
            ),
            (
                "&Einstellungen/Tastenkürzel…",
                Shortcut::None,
                Action::KeyboardSettings,
            ),
            ("&Datei/Neuer &Tab", Shortcut::Command | 't', Action::NewTab),
            (
                "&Datei/Neues &Fenster",
                Shortcut::Command | 'n',
                Action::NewWindow,
            ),
            (
                "&Datei/Öffnen…",
                Shortcut::Command | 'o',
                Action::Open(false),
            ),
            (
                "&Datei/Speichern",
                Shortcut::Command | 's',
                Action::Save(false),
            ),
            (
                "&Datei/Speichern unter…",
                Shortcut::Command | Shortcut::Shift | 's',
                Action::Save(true),
            ),
            (
                "&Datei/Tab schließen",
                Shortcut::Command | 'w',
                Action::CloseTab,
            ),
            (
                "&Datei/Alle Tabs schließen",
                Shortcut::Command | Shortcut::Shift | 'w',
                Action::CloseAllTabs,
            ),
            (
                "&Datei/Fenster schließen",
                Shortcut::Alt | fltk::enums::Key::F4,
                Action::CloseWindow,
            ),
            (
                "&Bearbeiten/Rückgängig",
                Shortcut::Command | 'z',
                Action::Undo,
            ),
            (
                "&Bearbeiten/Ausschneiden",
                Shortcut::Command | 'x',
                Action::Cut,
            ),
            (
                "&Bearbeiten/Kopieren (Auswahl oder alles)",
                Shortcut::None,
                Action::Copy,
            ),
            (
                "&Bearbeiten/Einfügen",
                Shortcut::Command | 'v',
                Action::Paste,
            ),
            (
                "&Bearbeiten/Bild einfügen…",
                Shortcut::Command | Shortcut::Shift | 'i',
                Action::InsertImage,
            ),
            (
                "&Bearbeiten/Alles auswählen",
                Shortcut::Command | 'a',
                Action::SelectAll,
            ),
            (
                "&Bearbeiten/Suchen…",
                Shortcut::Command | 'f',
                Action::ShowFind(false),
            ),
            (
                "&Bearbeiten/Ersetzen…",
                Shortcut::Command | 'h',
                Action::ShowFind(true),
            ),
            (
                "&Bearbeiten/Weitersuchen",
                Shortcut::from_key(fltk::enums::Key::F3),
                Action::Find,
            ),
            (
                "&Ansicht/Zoom/Vergrössern",
                Shortcut::Command | '+',
                Action::Zoom(10),
            ),
            (
                "&Ansicht/Zoom/Verkleinern",
                Shortcut::Command | '-',
                Action::Zoom(-10),
            ),
            (
                "&Ansicht/Zoom/100 %",
                Shortcut::Command | '0',
                Action::Zoom(0),
            ),
            (
                "&Ansicht/Vergleich ein oder aus",
                Shortcut::None,
                Action::ToggleCompare,
            ),
            (
                "&Ansicht/Texte vergleichen",
                Shortcut::None,
                Action::Compare,
            ),
            (
                "&Ansicht/Vergleichstext öffnen…",
                Shortcut::None,
                Action::Open(true),
            ),
        ];
        for (label, shortcut, action) in entries {
            menu.add_emit(label, shortcut, MenuFlag::Normal, tx, (wid, action));
        }
        menu.add_emit(
            "&Ansicht/Sprache/Automatisch",
            Shortcut::None,
            MenuFlag::Normal,
            tx,
            (wid, Action::SetLanguage(None)),
        );
        for language in [
            Language::Plain,
            Language::Python,
            Language::PowerShell,
            Language::Rust,
            Language::JavaScript,
            Language::Json,
            Language::Shell,
            Language::Cpp,
            Language::Html,
            Language::Css,
            Language::Config,
            Language::Sql,
        ] {
            let label = format!("&Ansicht/Sprache/{}", language.name().replace('/', " · "));
            menu.add_emit(
                &label,
                Shortcut::None,
                MenuFlag::Normal,
                tx,
                (wid, Action::SetLanguage(Some(language))),
            );
        }
        for (label, action) in [
            ("&Ansicht/Split View/Nebeneinander", Action::Split(false)),
            ("&Ansicht/Split View/Übereinander", Action::Split(true)),
            ("&Ansicht/Split View/Zusammenführen", Action::MergeSplit),
            (
                "&Ansicht/Split View/Als zwei Fenster weiterführen",
                Action::EndSplit,
            ),
            ("&Ansicht/Vergleich stoppen", Action::StopCompare),
            ("&Ansicht/Vergleich schliessen", Action::CloseCompare),
        ] {
            menu.add_emit(label, Shortcut::None, MenuFlag::Normal, tx, (wid, action));
        }
        menu.add_emit(
            "&Hilfe/Nach Updates suchen…",
            Shortcut::Command | Shortcut::Shift | 'u',
            MenuFlag::Normal,
            tx,
            (wid, Action::CheckUpdates(true)),
        );
        shortcuts::apply_menu(&mut menu);
        for (glyph, tooltip, action) in [
            (
                theme::Icon::Search,
                "Suchen (Strg/Cmd+F)",
                Action::ShowFind(false),
            ),
            (
                theme::Icon::Compare,
                "Vergleich ein oder aus",
                Action::ToggleCompare,
            ),
        ] {
            let mut control = Button::default();
            theme::button(&mut control, false, Some(glyph));
            control.set_color(PANEL);
            control.set_tooltip(tooltip);
            control.emit(tx, (wid, action));
            menurow.fixed(&control, 30);
        }
        menurow.end();
        root.fixed(&menurow, 36);
        let mut workspace = Flex::default().column();
        workspace.set_spacing(0);
        let mut tab_scroll = Scroll::default();
        tab_scroll.set_color(PANEL);
        tab_scroll.set_frame(FrameType::FlatBox);
        tab_scroll.set_scrollbar_size(14);
        tab_scroll.super_draw_first(true);
        tab_scroll.draw(|w| theme::draw_scrollbars(w));
        let tab_grab = Cell::new(None);
        tab_scroll.handle(move |w, e| theme::handle_scrollbars(w, e, &tab_grab));
        let mut bar = Group::default();
        bar.set_color(PANEL);
        bar.set_frame(FrameType::FlatBox);
        bar.set_clip_children(true);
        bar.end();
        tab_scroll.end();
        let mut divider = Frame::default();
        divider.set_tooltip("Ziehen, um die Tab-Leiste breiter oder schmaler zu machen");
        let drag_start = Rc::new(Cell::new(0));
        let drag_width = Rc::new(Cell::new(184));
        divider.handle(move |f, event| match event {
            Event::Enter => {
                if let Some(mut w) = f.window() {
                    w.set_cursor(fltk::enums::Cursor::WE);
                }
                true
            }
            Event::Leave => {
                if let Some(mut w) = f.window() {
                    w.set_cursor(fltk::enums::Cursor::Default);
                }
                true
            }
            Event::Push => {
                drag_start.set(app::event_x());
                drag_width.set(f.x() - f.parent().map(|p| p.x()).unwrap_or(0));
                true
            }
            Event::Drag => {
                tx.send((
                    wid,
                    Action::SidebarWidth(drag_width.get() + app::event_x() - drag_start.get()),
                ));
                true
            }
            Event::Released => true,
            _ => false,
        });
        divider.draw(|f| {
            fltk::draw::set_draw_color(PANEL);
            fltk::draw::draw_rectf(f.x(), f.y(), f.w(), f.h());
            fltk::draw::set_draw_color(theme::BORDER);
            fltk::draw::draw_line(
                f.x() + f.w() / 2,
                f.y() + 8,
                f.x() + f.w() / 2,
                f.y() + f.h() - 8,
            );
        });
        workspace.fixed(&divider, 5);
        let area = Group::default();
        area.end();
        workspace.end();
        root.end();
        win.end();
        win.resizable(&root);
        win.set_callback(move |_| tx.send((wid, Action::CloseWindow)));
        let find = chrome::SearchPanel::new(wid, tx);
        let settings = chrome::SettingsPanel::new(wid, tx, vertical);
        let route = Rc::new(Cell::new(wid));
        split::wire_window(&mut win, route.clone(), tx);
        let mut result = Self {
            win,
            root,
            workspace,
            tab_scroll,
            divider,
            sidebar_width: 184,
            bar,
            area,
            find,
            settings,
            menu,
            docs: vec![],
            active: 0,
            next_doc: 0,
            vertical,
            tab_signature: String::new(),
            reveal_tab: true,
            route,
            embedded: false,
            split_key: None,
            split_stacked: Rc::new(Cell::new(false)),
            split_ratio: Rc::new(Cell::new(500)),
        };
        result.layout_tabs(vertical);
        result.add_tab(wid, tx);
        result.wire_root(wid);
        result.refresh_shortcuts();
        if show {
            result.win.show();
            chrome::dark_title(&result.win);
        }
        result
    }

    fn refresh_shortcuts(&mut self) {
        shortcuts::apply_menu(&mut self.menu);
        if let Some(row) = self.root.child(0).and_then(|w| w.as_group()) {
            for i in 0..row.children() {
                if let Some(mut child) = row.child(i) {
                    if let Some(hint) = child.tooltip() {
                        if hint.starts_with("Suchen") {
                            child.set_tooltip(&format!(
                                "Suchen ({})",
                                shortcuts::hint(shortcuts::Command::Find)
                            ));
                        } else if hint.starts_with("Vergleich") {
                            let key = shortcuts::hint(shortcuts::Command::ToggleCompare);
                            child.set_tooltip(&format!("Vergleich ein/aus ({key})"));
                        }
                    }
                }
            }
        }
    }
    fn layout_tabs(&mut self, vertical: bool) {
        self.vertical = vertical;
        self.settings.set_orientation(vertical);
        if vertical {
            self.workspace.insert(&self.tab_scroll, 0);
            self.divider.show();
        } else {
            self.divider.hide();
            self.root.insert(&self.tab_scroll, 1);
            self.root.fixed(&self.tab_scroll, 34);
        }
        self.workspace.set_type(if vertical {
            FlexType::Row
        } else {
            FlexType::Column
        });
        self.workspace.fixed(&self.divider, 5);
        self.workspace.fixed(
            &self.tab_scroll,
            if vertical { self.sidebar_width } else { 34 },
        );
        self.tab_scroll.set_type(if vertical {
            ScrollType::Vertical
        } else {
            ScrollType::Horizontal
        });
        self.tab_scroll.scroll_to(0, 0);
        self.tab_signature.clear();
        self.reveal_tab = true;
        self.workspace.recalc();
        self.root.recalc();
        self.win.redraw();
    }

    fn add_tab(&mut self, wid: usize, tx: app::Sender<Message>) {
        for d in &mut self.docs {
            d.group.hide();
        }
        self.area.begin();
        let mut group = Flex::new(
            self.area.x(),
            self.area.y(),
            self.area.w(),
            self.area.h(),
            None,
        )
        .column();
        group.set_spacing(8);
        group.set_frame(FrameType::RFlatBox);
        group.set_color(BG);
        group.set_margins(0, 4, 0, 0);
        let mut panes = Flex::default().row();
        panes.set_spacing(18);
        panes.set_color(BG);
        let mut leftcol = Flex::default().column();
        leftcol.set_color(BG);
        let target_right = Rc::new(Cell::new(false));
        let (left, left_buf, left_meta) = editor(target_right.clone(), false);
        leftcol.end();
        let mut rightcol = Flex::default().column();
        rightcol.set_color(BG);
        rightcol.set_spacing(12);
        let mut controls = Flex::default().column();
        controls.set_color(PANEL);
        controls.set_frame(FrameType::RFlatBox);
        controls.set_margins(8, 4, 8, 4);
        controls.set_spacing(4);
        let mut header = Flex::default().row();
        header.set_spacing(6);
        theme::heading("Vergleich", 14);
        let mut compare_stop = Button::default().with_label("Stopp");
        theme::button(&mut compare_stop, false, None);
        compare_stop.emit(tx, (wid, Action::StopCompare));
        header.fixed(&compare_stop, 64);
        compare_stop.deactivate();
        compare_stop.set_label_color(MUTED);
        let mut compare_close = Button::default();
        theme::button(&mut compare_close, false, Some(theme::Icon::Close));
        compare_close.set_tooltip("Vergleich schliessen – Text bleibt erhalten");
        compare_close.emit(tx, (wid, Action::CloseCompare));
        header.fixed(&compare_close, 28);
        header.end();
        controls.fixed(&header, 30);
        let mut row = Flex::default().row();
        row.set_spacing(6);
        let mut compare_open = Button::default().with_label("Datei öffnen…");
        theme::button(&mut compare_open, false, None);
        compare_open.emit(tx, (wid, Action::Open(true)));
        let mut compare_run = Button::default().with_label("Vergleichen");
        theme::button(&mut compare_run, true, None);
        compare_run.emit(tx, (wid, Action::Compare));
        row.end();
        controls.fixed(&row, 32);
        controls.end();
        rightcol.fixed(&controls, 78);
        let (right, right_buf, right_meta) = editor(target_right.clone(), true);
        rightcol.end();
        rightcol.hide();
        panes.end();
        // Keep both columns owned by the pane flex. Right column is revealed on demand.
        let mut status = Frame::default();
        status.set_align(Align::Left | Align::Inside);
        status.set_label_size(11);
        status.set_label_color(MUTED);
        group.fixed(&status, 36);
        let mut sync_status = Frame::default();
        sync_status.set_align(Align::Left | Align::Inside);
        sync_status.set_label_color(theme::BLUE);
        sync_status.set_label_size(12);
        group.fixed(&sync_status, 20);
        sync_status.hide();
        group.end();
        self.area.end();
        Group::set_current(None::<&Group>);
        self.area.resizable(&group);
        let id = self.next_doc;
        self.next_doc += 1;
        self.docs.push(Document {
            id,
            group,
            panes,
            rightcol,
            target_right,
            left,
            right,
            left_buf,
            right_buf,
            left_meta,
            right_meta,
            styles: None,
            compared: None,
            status,
            sync_status,
            sync_text: String::new(),
            sync_error: None,
            status_text: String::new(),
            status_key: String::new(),
            comparing: false,
            compare_generation: 0,
            compare_cancel: None,
            compare_open,
            compare_run,
            compare_stop,
            compare_close,
            report: "UTF-8 · Nur Text".into(),
            path: None,
            saved_revision: 0,
            comparison_visible: false,
            page: 0,
            sharing: None,
            display_name: None,
            received_offer: None,
            share_revision: 0,
            cache_key: session::new_key(),
            right_path: None,
            left_language: None,
            right_language: None,
            syntax_pending: false,
            syntax_key: None,
        });
        self.active = self.docs.len() - 1;
        self.tab_signature.clear();
        self.reveal_tab = true;
        self.resize_docs();
        self.docs[self.active].left.take_focus().ok();
    }

    fn open_settings_tab(&mut self, page: u8, wid: usize, tx: app::Sender<Message>) {
        let index = if let Some(index) = self.docs.iter().position(|d| d.page == page) {
            index
        } else {
            self.add_tab(wid, tx);
            let d = &mut self.docs[self.active];
            d.page = page;
            d.panes.hide();
            d.status.hide();
            d.group.set_margins(0, 0, 0, 0);
            self.active
        };
        self.action(Action::Select(index), wid, tx);
        if page == 1 {
            self.settings.sync();
        }
        self.tab_signature.clear();
    }
    fn resize_docs(&mut self) {
        if self.vertical {
            let width = self
                .sidebar_width
                .clamp(120, (self.root.w() - 240).max(120));
            if width != self.sidebar_width {
                self.sidebar_width = width;
                self.workspace.fixed(&self.tab_scroll, width);
                self.workspace.recalc();
            }
        }
        let rect = (
            self.area.x(),
            self.area.y(),
            self.area.w().max(1),
            (self.area.h() - 6).max(1),
        );
        for d in &mut self.docs {
            if (d.group.x(), d.group.y(), d.group.w(), d.group.h()) != rect {
                d.group.resize(rect.0, rect.1, rect.2, rect.3);
                d.group.recalc();
            }
        }
    }

    fn refresh(&mut self, wid: usize, tx: app::Sender<Message>) {
        self.follow_focus(wid);
        self.resize_docs();
        if let Some(d) = self.docs.get_mut(self.active).filter(|d| d.page == 0) {
            d.left.reflow();
            d.right.reflow();
            d.refresh();
            if !d.comparing
                && d.compared.is_none()
                && !d.syntax_pending
                && d.syntax_key != Some(d.syntax_key())
            {
                d.syntax_pending = true;
                let id = d.id;
                app::add_timeout3(0.25, move |_| tx.send((wid, Action::SyntaxRequest(id))));
            }
            let title = format!("{} — EditorTool", d.title());
            if self.route.get() == wid && self.win.label() != title {
                self.win.set_label(&title);
            }
        }
        if self.docs[self.active].page != 0 && self.route.get() == wid {
            let title = format!("{} — EditorTool", self.docs[self.active].title());
            if self.win.label() != title {
                self.win.set_label(&title);
            }
        }
        let signature = format!(
            "{}:{:?}",
            self.active,
            self.docs.iter().map(Document::title).collect::<Vec<_>>()
        );
        if signature != self.tab_signature {
            self.bar.clear();
            self.bar.begin();
            for (i, d) in self.docs.iter().enumerate() {
                let tab = theme::tab(&d.title(), i == self.active, wid, i, tx);
                let _ = tab;
            }
            let mut add = Button::default();
            theme::button(&mut add, false, Some(theme::Icon::Plus));
            add.set_color(PANEL);
            add.set_tooltip("Neuer Tab (Strg/Cmd+T)");
            add.emit(tx, (wid, Action::NewTab));

            self.bar.end();
            Group::set_current(None::<&Group>);
            self.tab_signature = signature;
            self.bar.redraw();
            self.tab_scroll.redraw();
        }
        self.layout_tab_content();
    }

    fn layout_tab_content(&mut self) {
        let count = self.docs.len() as i32;
        let content_length = if self.vertical {
            count * TAB_VERTICAL_STEP + TAB_HEIGHT
        } else {
            count * TAB_HORIZONTAL_STEP + ADD_TAB_SIZE
        };
        if !self.vertical {
            let height = if content_length > self.tab_scroll.w() {
                48
            } else {
                34
            };
            if self.tab_scroll.h() != height {
                self.root.fixed(&self.tab_scroll, height);
                self.root.recalc();
            }
        }
        let viewport_length = if self.vertical {
            self.tab_scroll.h()
        } else {
            self.tab_scroll.w()
        };
        let overflow = content_length > viewport_length;
        let desired_type = if !overflow {
            ScrollType::None
        } else if self.vertical {
            ScrollType::Vertical
        } else {
            ScrollType::Horizontal
        };
        if self.tab_scroll.get_type::<ScrollType>() != desired_type {
            self.tab_scroll.set_type(desired_type);
            self.tab_scroll.redraw();
        }
        let current = if self.vertical {
            self.tab_scroll.yposition()
        } else {
            self.tab_scroll.xposition()
        };
        let maximum = (content_length - viewport_length).max(0);
        let mut position = current.clamp(0, maximum);
        if self.reveal_tab {
            let start = self.active as i32
                * if self.vertical {
                    TAB_VERTICAL_STEP
                } else {
                    TAB_HORIZONTAL_STEP
                };
            let size = if self.vertical { TAB_HEIGHT } else { TAB_WIDTH };
            if start < position {
                position = start;
            } else if start + size > position + viewport_length {
                position = start + size - viewport_length;
            }
            position = position.clamp(0, maximum);
            self.reveal_tab = false;
        }
        let (offset_x, offset_y) = if self.vertical {
            (0, position)
        } else {
            (position, 0)
        };
        if (self.tab_scroll.xposition(), self.tab_scroll.yposition()) != (offset_x, offset_y) {
            self.tab_scroll.scroll_to(offset_x, offset_y);
            self.tab_scroll.redraw();
        }
        // FLTK defines the scroll position as viewport-origin minus content-origin.
        // Padding here would feed back as a negative scroll position on every draw.
        let (width, height) = if self.vertical {
            (
                (self.tab_scroll.w() - if overflow { 14 } else { 0 }).max(1),
                content_length,
            )
        } else {
            (
                content_length,
                (self.tab_scroll.h() - if overflow { 14 } else { 0 }).max(1),
            )
        };
        let rect = (
            self.tab_scroll.x() - offset_x,
            self.tab_scroll.y() - offset_y,
            width,
            height,
        );
        if (self.bar.x(), self.bar.y(), self.bar.w(), self.bar.h()) != rect {
            self.bar.resize(rect.0, rect.1, rect.2, rect.3);
            self.tab_scroll.redraw();
        }
        let mut changed = false;
        for index in 0..self.bar.children() {
            if let Some(mut child) = self.bar.child(index) {
                let child_rect = if self.vertical {
                    (
                        rect.0,
                        rect.1 + index * TAB_VERTICAL_STEP,
                        width,
                        TAB_HEIGHT,
                    )
                } else {
                    (
                        rect.0 + index * TAB_HORIZONTAL_STEP,
                        rect.1,
                        if index == count {
                            ADD_TAB_SIZE
                        } else {
                            TAB_WIDTH
                        },
                        TAB_HEIGHT,
                    )
                };
                if (child.x(), child.y(), child.w(), child.h()) != child_rect {
                    child.resize(child_rect.0, child_rect.1, child_rect.2, child_rect.3);
                    changed = true;
                }
            }
        }
        if changed {
            self.bar.init_sizes();
            self.bar.redraw();
            self.tab_scroll.redraw();
        }
    }

    fn confirm_close(&mut self, index: usize) -> bool {
        if !self.docs[index].dirty() {
            return true;
        }
        match chrome::confirm_save(&self.win, &self.docs[index].title()) {
            1 => true,
            2 => self.save(index, false),
            _ => false,
        }
    }

    fn save(&mut self, index: usize, save_as: bool) -> bool {
        let d = &mut self.docs[index];
        let mut path = if save_as || d.path.is_none() {
            let mut chooser =
                dialog::NativeFileChooser::new(dialog::NativeFileChooserType::BrowseSaveFile);
            chooser.set_title("UTF-8-Text speichern");
            chooser.set_option(dialog::NativeFileChooserOptions::SaveAsConfirm);
            chooser.show();
            let path = chooser.filename();
            if path.as_os_str().is_empty() {
                return false;
            }
            path
        } else {
            d.path.clone().unwrap()
        };
        let rich_file = d.left.has_images() || path.extension().is_some_and(|e| e == "etdoc");
        if rich_file && path.extension().is_none_or(|e| e != "etdoc") {
            let original_language = d.language(false);
            path.set_extension("etdoc");
            if path.exists()
                && dialog::choice2_default(
                    &format!("{} existiert bereits. Überschreiben?", path.display()),
                    "Abbrechen",
                    "Überschreiben",
                    "",
                ) != Some(1)
            {
                return false;
            }
            if d.left_language.is_none() && original_language != Language::Plain {
                d.left_language = Some(original_language);
            }
        }
        let content = if rich_file {
            d.left.document_text()
        } else {
            d.left.plain_text()
        };
        match std::fs::write(&path, content.as_bytes()) {
            Ok(()) => {
                if let Some(b) = &mut d.sharing {
                    b.disk_hash = Some(peer::hash(content.as_bytes()));
                }
                d.path = Some(path);
                d.saved_revision = d.left_meta.borrow().revision;
                d.report = "Gespeichert · UTF-8".into();
                true
            }
            Err(e) => {
                dialog::alert_default(&format!("Speichern fehlgeschlagen: {e}"));
                false
            }
        }
    }

    fn action(&mut self, action: Action, wid: usize, tx: app::Sender<Message>) {
        if self.docs[self.active].page != 0 {
            if matches!(action, Action::Open(false)) {
                self.add_tab(wid, tx);
            } else if matches!(
                action,
                Action::Save(_)
                    | Action::Copy
                    | Action::Paste
                    | Action::ShowFind(_)
                    | Action::InsertImage
                    | Action::Undo
                    | Action::Cut
                    | Action::SelectAll
                    | Action::SetLanguage(_)
                    | Action::Compare
                    | Action::ToggleCompare
                    | Action::StopCompare
                    | Action::CloseCompare
                    | Action::Open(true)
                    | Action::Zoom(_)
            ) {
                return;
            }
        }
        match action {
            Action::TabContext(index, x, y) => {
                if index >= self.docs.len() {
                    return;
                }
                self.action(Action::Select(index), wid, tx);
                if let Some(action) =
                    tab_menu::popup(&self.win, index, self.docs[index].page != 0, x, y)
                {
                    tx.send((wid, action));
                }
            }
            Action::CopyPath => {
                if let Some(path) = &self.docs[self.active].path {
                    app::copy(&path.to_string_lossy());
                } else {
                    dialog::message_default(
                        "Dieses Dokument wurde noch nicht als Datei gespeichert.",
                    );
                }
            }
            Action::CloseOtherTabs => {
                for index in 0..self.docs.len() {
                    if index != self.active && !self.confirm_close(index) {
                        return;
                    }
                }
                let keep = self.docs.remove(self.active);
                for d in self.docs.drain(..) {
                    Flex::delete(d.group);
                }
                self.docs.push(keep);
                self.active = 0;
                self.docs[0].group.show();
                self.tab_signature.clear();
                self.reveal_tab = true;
            }
            Action::SidebarWidth(width) => {
                self.sidebar_width = width.clamp(120, (self.root.w() - 240).max(120));
                if self.vertical {
                    self.workspace.fixed(&self.tab_scroll, self.sidebar_width);
                    self.workspace.recalc();
                    self.resize_docs();
                }
            }
            Action::InsertImage => {
                let mut chooser =
                    dialog::NativeFileChooser::new(dialog::NativeFileChooserType::BrowseFile);
                chooser.set_title("Bild einfügen");
                chooser.set_filter("Bilder\t*.{png,jpg,jpeg,bmp,gif}");
                chooser.show();
                let path = chooser.filename();
                if !path.as_os_str().is_empty() {
                    let d = &mut self.docs[self.active];
                    let editor = if d.target_right.get() {
                        &mut d.right
                    } else {
                        &mut d.left
                    };
                    if let Err(e) = editor.load_image(&path) {
                        dialog::alert_default(&format!("Bild konnte nicht eingefügt werden: {e}"));
                    }
                }
            }
            Action::ShowFind(expanded) => {
                let d = &self.docs[self.active];
                let selected = if d.target_right.get() {
                    d.right_buf.selection_text()
                } else {
                    d.left_buf.selection_text()
                };
                self.find.show(&self.win, expanded, &selected);
            }
            Action::ToggleReplace => self.find.toggle(),
            Action::Settings => self.open_settings_tab(1, wid, tx),
            Action::Zoom(step) => {
                let d = &mut self.docs[self.active];
                let zoom = if step == 0 {
                    100
                } else {
                    (if d.target_right.get() {
                        d.right.zoom()
                    } else {
                        d.left.zoom()
                    }) + step
                };
                d.left.set_zoom(zoom);
                d.right.set_zoom(zoom);
            }
            Action::NextTab(backwards) => {
                let count = self.docs.len();
                let next = (self.active + if backwards { count - 1 } else { 1 }) % count;
                self.action(Action::Select(next), wid, tx);
            }
            Action::Undo | Action::Cut | Action::SelectAll => {
                let d = &mut self.docs[self.active];
                let (ed, buf) = if d.target_right.get() {
                    (&mut d.right, &mut d.right_buf)
                } else {
                    (&mut d.left, &mut d.left_buf)
                };
                match action {
                    Action::Undo => ed.undo(),
                    Action::Cut => {
                        if let Some((start, end)) = buf.selection_position() {
                            app::copy(&buf.selection_text().replace(rich::IMAGE_MARKER, ""));
                            buf.remove(start, end);
                            ed.set_insert_position(start);
                        }
                    }
                    Action::SelectAll => buf.select(0, buf.length()),
                    _ => (),
                }
            }
            Action::SetLanguage(language) => {
                let d = &mut self.docs[self.active];
                if d.target_right.get() {
                    d.right_language = language;
                } else {
                    d.left_language = language;
                }
                d.clear_styles();
            }
            Action::SyntaxRequest(id) => {
                if let Some(d) = self.docs.iter_mut().find(|d| d.id == id) {
                    if d.comparing || d.compared.is_some() {
                        d.syntax_pending = false;
                        return;
                    }
                    let key = d.syntax_key();
                    let left = if key.2 != Language::Plain
                        && d.left_meta.borrow().counts.bytes <= 4 * 1024 * 1024
                    {
                        Some(d.left_buf.text())
                    } else {
                        None
                    };
                    let right = if key.3 != Language::Plain
                        && d.right_meta.borrow().counts.bytes <= 4 * 1024 * 1024
                    {
                        Some(d.right_buf.text())
                    } else {
                        None
                    };
                    std::thread::spawn(move || {
                        tx.send((
                            wid,
                            Action::SyntaxReady(
                                id,
                                key,
                                left.as_deref().and_then(|t| syntax::highlight(t, key.2)),
                                right.as_deref().and_then(|t| syntax::highlight(t, key.3)),
                            ),
                        ))
                    });
                }
            }
            Action::SyntaxReady(id, key, left, right) => {
                if let Some(d) = self.docs.iter_mut().find(|d| d.id == id) {
                    d.syntax_pending = false;
                    if d.comparing || d.compared.is_some() || key != d.syntax_key() {
                        return;
                    }
                    for (ed, meta, text) in [
                        (&mut d.left, &d.left_meta, left),
                        (&mut d.right, &d.right_meta, right),
                    ] {
                        if let Some(text) = text {
                            let mut style = TextBuffer::default();
                            style.set_text(&text);
                            ed.set_highlight_data(style.clone(), syntax::table());
                            meta.borrow_mut().style = Some(style);
                        } else {
                            ed.unset_highlight_data(None::<TextBuffer>);
                            meta.borrow_mut().style = None;
                        }
                    }
                    d.syntax_key = Some(key);
                }
            }
            Action::NewTab => self.add_tab(wid, tx),
            Action::Select(i) if i < self.docs.len() => {
                self.docs[self.active].group.hide();
                self.active = i;
                self.reveal_tab = true;
                self.docs[i].group.show();
                if self.docs[i].page == 0 {
                    self.docs[i].left.take_focus().ok();
                }
            }
            Action::CloseAllTabs => {
                // Confirm every changed document before removing any tabs.
                for index in 0..self.docs.len() {
                    if !self.confirm_close(index) {
                        return;
                    }
                }
                for d in self.docs.drain(..) {
                    Flex::delete(d.group);
                }
                self.active = 0;
                self.add_tab(wid, tx);
            }
            Action::CloseTab => {
                if self.confirm_close(self.active) {
                    let d = self.docs.remove(self.active);
                    Flex::delete(d.group);
                    if self.docs.is_empty() {
                        self.add_tab(wid, tx);
                    } else {
                        self.active = self.active.min(self.docs.len() - 1);
                        self.docs[self.active].group.show();
                    }
                    self.tab_signature.clear();
                    self.reveal_tab = true;
                }
            }
            Action::Save(save_as) => {
                self.save(self.active, save_as);
            }
            Action::Open(right) => {
                if !right && !self.confirm_close(self.active) {
                    return;
                }
                let mut chooser =
                    dialog::NativeFileChooser::new(dialog::NativeFileChooserType::BrowseFile);
                chooser.set_title("UTF-8-Text öffnen");
                chooser.show();
                let path = chooser.filename();
                if path.as_os_str().is_empty() {
                    return;
                }
                match std::fs::read_to_string(&path) {
                    Ok(text) => {
                        if right {
                            self.show_compare(true);
                        }
                        let d = &mut self.docs[self.active];
                        if right {
                            if let Err(e) = d.right.set_cache_text(&text) {
                                dialog::alert_default(&e);
                                return;
                            }
                            d.right_path = Some(path);
                            d.right_language = None;
                        } else {
                            if let Err(e) = d.left.set_cache_text(&text) {
                                dialog::alert_default(&e);
                                return;
                            }
                            d.sharing = None;
                            d.display_name = None;
                            d.path = Some(path);
                            d.left_language = None;
                            d.saved_revision = d.left_meta.borrow().revision;
                        }
                    }
                    Err(e) => dialog::alert_default(&format!(
                        "Datei kann nicht als UTF-8 geöffnet werden: {e}"
                    )),
                }
            }
            Action::ToggleCompare => self.show_compare(!self.docs[self.active].comparison_visible),
            Action::Copy => {
                let d = &self.docs[self.active];
                let b = if d.target_right.get() {
                    &d.right_buf
                } else {
                    &d.left_buf
                };
                let text = if b.selected() {
                    b.selection_text().replace(rich::IMAGE_MARKER, "")
                } else {
                    b.text().replace(rich::IMAGE_MARKER, "")
                };
                app::copy(&text);
            }
            Action::Paste => {
                let d = &mut self.docs[self.active];
                if d.target_right.get() {
                    d.right.take_focus().ok();
                    d.right.paste();
                } else {
                    d.left.take_focus().ok();
                    d.left.paste();
                }
            }
            Action::Find | Action::Replace | Action::ReplaceAll => {
                if self.find.search.value().is_empty() {
                    self.action(Action::ShowFind(false), wid, tx);
                } else {
                    self.search_action(action);
                    self.find.feedback.set_label(&self.docs[self.active].report);
                }
            }
            Action::StopCompare => self.docs[self.active].stop_comparison(),
            Action::CloseCompare => self.show_compare(false),
            Action::Compare => {
                self.show_compare(true);
                let d = &mut self.docs[self.active];
                if d.comparing {
                    return;
                }
                d.stop_comparison();
                d.comparing = true;
                let generation = d.compare_generation;
                let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                d.compare_cancel = Some(cancel.clone());
                let key = d.cache_key.clone();
                let revisions = (
                    d.left_meta.borrow().revision,
                    d.right_meta.borrow().revision,
                );
                let left = d.left_buf.text();
                let right = d.right_buf.text();
                d.report = "Vergleich läuft …".into();
                std::thread::spawn(move || {
                    if let Some(result) = core::compare_interruptible(&left, &right, &cancel) {
                        tx.send((wid, Action::Compared(key, generation, revisions, result)));
                    }
                });
            }
            Action::Compared(key, generation, revisions, result) => {
                if let Some(d) = self.docs.iter_mut().find(|d| {
                    d.cache_key == key && d.compare_generation == generation && d.comparing
                }) {
                    d.comparing = false;
                    d.compare_cancel = None;
                    let rev = (
                        d.left_meta.borrow().revision,
                        d.right_meta.borrow().revision,
                    );
                    if rev != revisions {
                        d.report = "Text verändert – bitte erneut vergleichen".into();
                        return;
                    }
                    let mut a = TextBuffer::default();
                    a.set_text(&result.left_style);
                    let mut b = TextBuffer::default();
                    b.set_text(&result.right_style);
                    let styles = vec![
                        StyleTableEntry {
                            color: FG,
                            font: Font::Courier,
                            size: preferences::get().font_size,
                        },
                        StyleTableEntry {
                            color: Color::from_hex(0xff929d),
                            font: Font::Courier,
                            size: preferences::get().font_size,
                        },
                        StyleTableEntry {
                            color: Color::from_hex(0x83deab),
                            font: Font::Courier,
                            size: preferences::get().font_size,
                        },
                    ];
                    d.left.set_highlight_data(a.clone(), styles.clone());
                    d.right.set_highlight_data(b.clone(), styles);
                    d.left_meta.borrow_mut().style = Some(a.clone());
                    d.right_meta.borrow_mut().style = Some(b.clone());
                    d.styles = Some((a, b));
                    d.compared = Some(rev);
                    d.report = if result.removed == 0 && result.added == 0 {
                        "Texte identisch".into()
                    } else {
                        format!(
                            "Links: {} entfernte Zeilen (rot) · Rechts: {} neue Zeilen (grün)",
                            result.removed, result.added
                        )
                    };
                }
            }
            _ => (),
        }
    }

    fn show_compare(&mut self, show: bool) {
        let d = &mut self.docs[self.active];
        d.comparison_visible = show;
        if show {
            d.rightcol.show();
        } else {
            d.stop_comparison();
            d.rightcol.hide();
            d.target_right.set(false);
            d.left.take_focus().ok();
        }
        d.panes.recalc();
        d.group.recalc();
        self.win.redraw();
    }

    fn search_action(&mut self, action: Action) {
        let Some(re) = pattern(&self.find.search.value(), self.find.case.value()) else {
            return;
        };
        let d = &mut self.docs[self.active];
        let (ed, buf) = if d.target_right.get() {
            (&mut d.right, &mut d.right_buf)
        } else {
            (&mut d.left, &mut d.left_buf)
        };
        let text = buf.text();
        if matches!(action, Action::ReplaceAll) {
            let count = re.find_iter(&text).count();
            if count > 0 {
                buf.replace(
                    0,
                    buf.length(),
                    &re.replace_all(&text, regex::NoExpand(&self.find.replacement.value())),
                );
            }
            d.report = format!("{count} Treffer ersetzt");
            return;
        }
        if matches!(action, Action::Replace) {
            if let Some((start, end)) = buf.selection_position() {
                if re
                    .find(&text[start as usize..end as usize])
                    .is_some_and(|m| m.start() == 0 && m.end() == (end - start) as usize)
                {
                    let value = self.find.replacement.value();
                    buf.replace(start, end, &value);
                    ed.set_insert_position(start + value.len() as i32);
                }
            }
        }
        let text = buf.text();
        let start = (ed.insert_position() as usize).min(text.len());
        if let Some(found) = re.find_at(&text, start).or_else(|| re.find(&text)) {
            buf.select(found.start() as i32, found.end() as i32);
            ed.set_insert_position(found.end() as i32);
            ed.show_insert_position();
            d.report = "Treffer markiert".into();
        } else {
            d.report = "Kein Treffer".into();
        }
    }
}

fn settings_path() -> PathBuf {
    if let Ok(executable) = std::env::current_exe() {
        if let Some(directory) = executable.parent() {
            if directory.join("portable.txt").exists() {
                return directory.join("Data").join("settings.txt");
            }
        }
    }
    if let Some(profile) = std::env::var_os("EDITORTOOL_PROFILE") {
        return PathBuf::from(profile).join("settings.txt");
    }
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
    };
    base.unwrap_or_else(std::env::temp_dir)
        .join("EditorTool")
        .join("settings.txt")
}

impl EditorWindow {
    fn snapshot(&self) -> session::WindowState {
        session::WindowState {
            tabs: self.docs.iter().map(Document::snapshot).collect(),
            active: self.active,
            rect: (self.win.x(), self.win.y(), self.win.w(), self.win.h()),
            sidebar_width: self.sidebar_width,
            split_key: self.split_key.clone(),
            split_stacked: self.split_stacked.get(),
            split_ratio: self.split_ratio.get(),
        }
    }
    fn restore(
        &mut self,
        state: &session::WindowState,
        store: &session::Store,
        wid: usize,
        tx: app::Sender<Message>,
    ) -> bool {
        let mut success = true;
        self.sidebar_width = state.sidebar_width.clamp(120, 600);
        self.split_key = state.split_key.clone();
        self.split_stacked.set(state.split_stacked);
        self.split_ratio.set(state.split_ratio.clamp(200, 800));
        self.layout_tabs(self.vertical);
        for (index, tab) in state.tabs.iter().enumerate() {
            if index > 0 {
                self.add_tab(wid, tx);
            }
            let d = &mut self.docs[index];
            d.id = tab.id;
            d.cache_key = tab.key.clone();
            d.page = tab.page.min(3);
            if d.page != 0 {
                d.panes.hide();
                d.status.hide();
                d.group.set_margins(0, 0, 0, 0);
            }
            d.path = tab.path.clone();
            d.sharing = tab.sharing.clone();
            d.display_name = tab.display_name.clone();
            d.received_offer = tab.received_offer.clone();
            d.right_path = tab.right_path.clone();
            d.left_language = tab.left_language;
            d.right_language = tab.right_language;
            match store.text(&tab.key, false) {
                Ok(text) => {
                    if let Err(e) = d.left.set_cache_text(&text) {
                        success = false;
                        dialog::alert_default(&e);
                    }
                }
                Err(e) => {
                    success = false;
                    dialog::alert_default(&format!(
                        "Tab {} konnte nicht wiederhergestellt werden: {e}",
                        tab.id + 1
                    ));
                }
            }
            match store.text(&tab.key, true) {
                Ok(text) => {
                    if let Err(e) = d.right.set_cache_text(&text) {
                        success = false;
                        dialog::alert_default(&e);
                    }
                }
                Err(e) => {
                    success = false;
                    dialog::alert_default(&format!(
                        "Vergleichstext konnte nicht wiederhergestellt werden: {e}"
                    ));
                }
            }
            d.left.set_zoom(tab.zoom);
            d.right.set_zoom(tab.right_zoom);
            let revision = d.left_meta.borrow().revision;
            d.share_revision = if d
                .sharing
                .as_ref()
                .is_some_and(|b| peer::hash(d.left.cache_text().as_bytes()) != b.hash)
            {
                revision.wrapping_sub(1)
            } else {
                revision
            };
            d.saved_revision = if tab.dirty {
                revision.wrapping_sub(1)
            } else {
                revision
            };
            let left_pos = char_boundary(&d.left_buf, tab.left_cursor);
            let right_pos = char_boundary(&d.right_buf, tab.right_cursor);
            d.left.set_insert_position(left_pos);
            d.right.set_insert_position(right_pos);
            d.comparison_visible = tab.comparison;
            if tab.comparison {
                d.rightcol.show();
            } else {
                d.rightcol.hide();
            }
            d.panes.recalc();
            d.group.hide();
        }
        self.active = state.active.min(self.docs.len() - 1);
        self.docs[self.active].group.show();
        self.docs[self.active].left.take_focus().ok();
        self.next_doc = self.docs.iter().map(|d| d.id).max().unwrap_or(0) + 1;
        self.tab_signature.clear();
        self.reveal_tab = true;
        let (sw, sh) = app::screen_size();
        let width = state.rect.2.clamp(800, (sw as i32).max(800));
        let height = state.rect.3.clamp(480, (sh as i32).max(480));
        self.win.resize(
            state.rect.0.clamp(0, (sw as i32 - width).max(0)),
            state.rect.1.clamp(0, (sh as i32 - height).max(0)),
            width,
            height,
        );
        self.root.resize(0, 0, width, height);
        self.root.recalc();
        self.resize_docs();
        success
    }
}

fn char_boundary(buffer: &TextBuffer, position: i32) -> i32 {
    let text = buffer.text();
    let mut pos = (position.max(0) as usize).min(text.len());
    while !text.is_char_boundary(pos) {
        pos -= 1;
    }
    pos as i32
}

struct Runtime {
    windows: Vec<Option<EditorWindow>>,
    splits: Vec<split::Host>,
    closed: Vec<session::WindowState>,
    store: session::Store,
    tx: app::Sender<Message>,
    config: PathBuf,
    vertical: bool,
    cache_enabled: bool,
    session_pending: bool,
    last_fingerprint: String,
    weak_self: std::rc::Weak<RefCell<Runtime>>,
    updates: chrome::UpdatePanel,
    update_busy: bool,
    update_release: Option<updater::Release>,
    sharing: Option<peer::Service>,
    clients: sharing_ui::Clients,
    keyboard: shortcuts::Panel,
    transfer: sharing_ui::Transfer,
    conflicts: sharing_ui::Conflicts,
    share_timers: std::collections::HashMap<String, sharing_runtime::Pending>,
    countdown_timer: Option<app::TimeoutHandle>,
    conflict_notified: std::collections::BTreeSet<String>,
    tray: Option<tray::Native>,
    quit_requested: bool,
}
impl Runtime {
    fn snapshot(&self) -> session::Snapshot {
        let mut windows = self.closed.clone();
        windows.extend(self.windows.iter().flatten().map(EditorWindow::snapshot));
        session::Snapshot {
            version: 1,
            windows,
        }
    }
    fn fingerprint(&self) -> String {
        let mut key = format!("{:?}", self.snapshot());
        for w in self.windows.iter().flatten() {
            for d in &w.docs {
                key.push_str(&format!(
                    ":{}:{}",
                    d.left_meta.borrow().revision,
                    d.right_meta.borrow().revision
                ));
            }
        }
        key
    }
    fn persist(&mut self) {
        if !self.cache_enabled {
            return;
        }
        let mut texts = Vec::new();
        for window in self.windows.iter().flatten() {
            for d in &window.docs {
                let revision = (
                    d.left_meta.borrow().revision,
                    d.right_meta.borrow().revision,
                );
                let previous = self.store.revisions.get(&d.cache_key).copied();
                if previous.is_none_or(|r| r.0 != revision.0) {
                    texts.push((format!("{}.left.txt", d.cache_key), d.left.cache_text()));
                }
                if previous.is_none_or(|r| r.1 != revision.1) {
                    texts.push((format!("{}.right.txt", d.cache_key), d.right.cache_text()));
                }
                self.store.revisions.insert(d.cache_key.clone(), revision);
            }
        }
        self.store.save(self.snapshot(), texts);
        self.last_fingerprint = self.fingerprint();
    }
    fn update(&mut self) {
        self.sharing_tick();
        self.layout_splits();
        for (id, w) in self.windows.iter_mut().enumerate() {
            if let Some(w) = w {
                if w.win.shown() || w.embedded {
                    w.refresh(id, self.tx);
                }
            }
        }
        self.mount_settings_pages();
        if self.cache_enabled
            && !self.session_pending
            && self.fingerprint() != self.last_fingerprint
        {
            self.session_pending = true;
            let tx = self.tx;
            app::add_timeout3(0.4, move |_| tx.send((0, Action::SaveSession)));
        }
    }
    fn handle(&mut self, wid: usize, action: Action) {
        if matches!(
            action,
            Action::CloseTab
                | Action::CloseOtherTabs
                | Action::CloseAllTabs
                | Action::CloseWindow
                | Action::MoveTab(_)
                | Action::Split(_)
                | Action::EndSplit
                | Action::MergeSplit
                | Action::QuitAll
                | Action::UpdateDownloaded(_)
        ) {
            self.park_settings_pages();
        }
        let action = match self.sharing_action(wid, action) {
            Some(a) => a,
            None => return,
        };
        match action {
            Action::OpenExternal(paths) => {
                let id = if let Some(index) = self.windows.iter().position(Option::is_some) {
                    let active = self.windows[index].as_ref().unwrap().route.get();
                    if self.windows.get(active).is_some_and(Option::is_some) {
                        active
                    } else {
                        index
                    }
                } else {
                    self.handle(0, Action::NewWindow);
                    self.windows.len() - 1
                };
                if let Some(Some(w)) = self.windows.get_mut(id) {
                    for path in paths {
                        match std::fs::read_to_string(&path) {
                            Ok(text) => {
                                let active = &w.docs[w.active];
                                if active.page != 0
                                    || active.dirty()
                                    || active.path.is_some()
                                    || active.left_buf.length() > 0
                                    || active.right_buf.length() > 0
                                {
                                    w.add_tab(id, self.tx);
                                }
                                let d = &mut w.docs[w.active];
                                if let Err(e) = d.left.set_cache_text(&text) {
                                    dialog::alert_default(&e);
                                    continue;
                                }
                                d.path = Some(path);
                                d.left_language = None;
                                d.saved_revision = d.left_meta.borrow().revision;
                            }
                            Err(e) => dialog::alert_default(&format!(
                                "Datei konnte nicht geöffnet werden: {e}"
                            )),
                        }
                    }
                    w.win.show();
                    w.docs[w.active].left.take_focus().ok();
                    w.tab_signature.clear();
                    w.reveal_tab = true;
                }
            }
            Action::SaveSession => {
                self.session_pending = false;
                self.persist();
            }
            Action::CacheError(error) => {
                self.store.revisions.clear();
                self.last_fingerprint.clear();
                dialog::alert_default(&format!(
                    "Die automatische Sitzungssicherung ist fehlgeschlagen: {error}"
                ));
            }
            Action::NewWindow => {
                let id = self.windows.len();
                self.windows
                    .push(Some(EditorWindow::new(id, self.tx, self.vertical)));
                if let Some(w) = &self.windows[id] {
                    shutdown::hook(&w.win, self.weak_self.clone());
                }
            }
            Action::Orientation(_)
            | Action::WordWrap(_)
            | Action::Design(_)
            | Action::Accent(_)
            | Action::FontSize(_)
            | Action::UpdateCheckOption(_)
            | Action::UpdateInstallOption(_)
            | Action::ImageAutoSize(_) => {
                let mut prefs = preferences::get();
                match action {
                    Action::Orientation(value) => prefs.vertical = value,
                    Action::WordWrap(value) => prefs.wrap = value,
                    Action::Design(value) => prefs.design = value.min(2),
                    Action::Accent(value) => prefs.accent = value.min(2),
                    Action::FontSize(value) => prefs.font_size = value.clamp(12, 28),
                    Action::ImageAutoSize(value) => prefs.image_auto_size = value,
                    Action::UpdateCheckOption(value) => {
                        prefs.update_check = value;
                        if !value {
                            prefs.update_install = false;
                        }
                    }
                    Action::UpdateInstallOption(value) => {
                        prefs.update_install = value;
                        if value {
                            prefs.update_check = true;
                        }
                    }
                    _ => (),
                }
                preferences::set(prefs);
                self.vertical = prefs.vertical;
                theme::apply_palette();
                for w in self.windows.iter_mut().flatten() {
                    if w.vertical != prefs.vertical {
                        w.layout_tabs(prefs.vertical);
                    }
                    w.settings.sync();
                    for d in &mut w.docs {
                        d.clear_styles();
                        d.left.configure();
                        d.right.configure();
                    }
                    w.win.redraw();
                    chrome::dark_title(&w.win);
                }
                if let Err(e) = preferences::save(&self.config) {
                    dialog::alert_default(&format!(
                        "Einstellung konnte nicht gespeichert werden: {e}"
                    ));
                }
            }
            Action::CheckUpdates(manual) => {
                if !manual && !preferences::get().update_check {
                    return;
                }
                let owner = self
                    .windows
                    .get(wid)
                    .and_then(Option::as_ref)
                    .map(|w| w.win.clone());
                if self.update_busy {
                    if manual {
                        self.updates.show(owner.as_ref());
                    }
                    return;
                }
                self.update_busy = true;
                self.update_release = None;
                self.updates.state(
                    "Nach Updates suchen",
                    "GitHub wird nach einer neuen Version gefragt…",
                    "",
                    false,
                    true,
                );
                if manual {
                    self.updates.show(owner.as_ref());
                }
                let tx = self.tx;
                std::thread::spawn(move || {
                    tx.send((wid, Action::UpdateChecked(updater::check(), manual)))
                });
            }
            Action::UpdateChecked(result, manual) => {
                self.update_busy = false;
                if self.windows.iter().all(Option::is_none) {
                    return;
                }
                let owner = self.windows.iter().flatten().next().map(|w| w.win.clone());
                match result {
                    Ok(Some(release)) => {
                        self.updates.state(
                            &release.title,
                            &format!(
                                "{} ist verfügbar. Installiert: {}",
                                release.tag,
                                env!("CARGO_PKG_VERSION")
                            ),
                            &release.notes,
                            true,
                            false,
                        );
                        self.update_release = Some(release);
                        self.updates.show(owner.as_ref());
                        if !manual && preferences::get().update_install {
                            self.handle(wid, Action::InstallUpdate);
                        }
                    }
                    Ok(None) => {
                        self.updates.state(
                            "EditorTool ist aktuell",
                            concat!("Installierte Version: ", env!("CARGO_PKG_VERSION")),
                            "Keine neuere stabile Version verfügbar.",
                            false,
                            false,
                        );
                        if manual {
                            self.updates.show(owner.as_ref());
                        }
                    }
                    Err(error) => {
                        self.updates.state(
                            "Update-Suche fehlgeschlagen",
                            "Du kannst die Prüfung später wiederholen.",
                            &error,
                            false,
                            false,
                        );
                        if manual {
                            self.updates.show(owner.as_ref());
                        }
                    }
                }
            }
            Action::InstallUpdate => {
                if self.update_busy {
                    return;
                }
                if let Some(release) = self.update_release.clone() {
                    if !self.cache_enabled {
                        self.updates.state("Update pausiert","Die Sitzungssicherung ist nicht verfügbar.","Bitte zuerst die Dokumente speichern und das Problem mit dem Sitzungscache beheben. Es wurde nichts installiert.",true,false);
                        return;
                    }
                    self.update_busy = true;
                    self.updates.state(
                        &release.title,
                        "Update wird heruntergeladen…",
                        &release.notes,
                        false,
                        true,
                    );
                    let tx = self.tx;
                    let directory = self.config.parent().unwrap().join("updates");
                    std::thread::spawn(move || {
                        let result = updater::download(release, directory, |percent| {
                            tx.send((wid, Action::UpdateProgress(percent)))
                        });
                        tx.send((wid, Action::UpdateDownloaded(result)));
                    });
                }
            }
            Action::UpdateProgress(percent) => self.updates.progress(percent),
            Action::UpdateDownloaded(result) => {
                let result = result.and_then(|download| {
                    self.persist();
                    self.store.flush()?;
                    updater::install(&download, &self.config.parent().unwrap().join("updates"))
                });
                match result {
                    Ok(()) => {
                        self.quit_requested = true;
                        if let Some(s) = &mut self.sharing {
                            s.stop_listener();
                        }
                        self.updates.win.hide();
                        for w in self.windows.iter_mut().flatten() {
                            w.find.win.hide();
                            w.settings.win.hide();
                            w.win.hide();
                        }
                        app::quit();
                    }
                    Err(error) => {
                        self.update_busy = false;
                        self.updates.state(
                            "Aktualisierung fehlgeschlagen",
                            "Die App bleibt geöffnet. Es wurden keine Dokumente geschlossen.",
                            &error,
                            true,
                            false,
                        );
                        let owner = self.windows.iter().flatten().next().map(|w| w.win.clone());
                        self.updates.show(owner.as_ref());
                    }
                }
            }
            Action::UpdateNotice(message) => {
                self.updates.state(
                    "Hinweis zur letzten Aktualisierung",
                    "Das vorherige Update konnte nicht abgeschlossen werden.",
                    &message,
                    false,
                    false,
                );
                let owner = self.windows.iter().flatten().next().map(|w| w.win.clone());
                self.updates.show(owner.as_ref());
            }
            Action::Split(stacked) => self.open_split(wid, stacked),
            Action::MoveTab(index) => self.move_tab(wid, index),
            Action::EndSplit => self.end_split(wid),
            Action::MergeSplit => self.merge_split(wid),
            Action::KeyboardSettings => {
                self.keyboard.reload();
                self.open_keyboard_tab(wid);
            }
            Action::KeyboardApply(values) => match shortcuts::save(&self.config, values) {
                Ok(()) => {
                    for w in self.windows.iter_mut().flatten() {
                        w.refresh_shortcuts();
                    }
                    self.keyboard.reload();
                }
                Err(e) => {
                    let owner = self.windows.iter().flatten().next().map(|w| w.win.clone());
                    chrome::decision(
                        owner.as_ref(),
                        "Tastenkürzel nicht gespeichert",
                        &e,
                        "",
                        &["Schliessen"],
                        0,
                    );
                }
            },
            Action::RelayoutSplit => self.layout_splits(),
            Action::CloseWindow => self.close_editor_window(wid),
            other => {
                if let Some(Some(w)) = self.windows.get_mut(wid) {
                    w.action(other, wid, self.tx);
                }
            }
        }
    }
}

#[cfg(windows)]
mod shutdown {
    use super::*;
    use std::{collections::HashMap, ffi::c_void, rc::Weak};
    type Procedure = unsafe extern "system" fn(*mut c_void, u32, usize, isize) -> isize;
    thread_local! {static WINDOWS:RefCell<HashMap<usize,(isize,Weak<RefCell<Runtime>>)>>=RefCell::new(HashMap::new());}
    #[link(name = "user32")]
    extern "system" {
        fn SetWindowLongPtrW(hwnd: *mut c_void, index: i32, value: isize) -> isize;
        fn CallWindowProcW(
            previous: Option<Procedure>,
            hwnd: *mut c_void,
            message: u32,
            wparam: usize,
            lparam: isize,
        ) -> isize;
        fn DefWindowProcW(hwnd: *mut c_void, message: u32, wparam: usize, lparam: isize) -> isize;
    }
    pub fn hook(window: &Window, state: Weak<RefCell<Runtime>>) {
        let hwnd = window.raw_handle() as *mut c_void;
        if WINDOWS.with(|w| w.borrow().contains_key(&(hwnd as usize))) {
            return;
        }
        let previous = unsafe { SetWindowLongPtrW(hwnd, -4, procedure as *const () as isize) };
        if previous != 0 {
            WINDOWS.with(|w| {
                w.borrow_mut().insert(hwnd as usize, (previous, state));
            });
        }
    }
    unsafe extern "system" fn procedure(
        hwnd: *mut c_void,
        message: u32,
        wparam: usize,
        lparam: isize,
    ) -> isize {
        let entry = WINDOWS.with(|w| w.borrow().get(&(hwnd as usize)).cloned());
        if let Some((previous, state)) = entry {
            if message == 0x0011 || (message == 0x0016 && wparam != 0) {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if let Some(state) = state.upgrade() {
                        if let Ok(mut state) = state.try_borrow_mut() {
                            state.persist();
                            let _ = state.store.flush();
                        }
                    }
                }));
            }
            if message == 0x0082 {
                WINDOWS.with(|w| {
                    w.borrow_mut().remove(&(hwnd as usize));
                });
            }
            return CallWindowProcW(
                Some(std::mem::transmute::<isize, Procedure>(previous)),
                hwnd,
                message,
                wparam,
                lparam,
            );
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }
}
#[cfg(not(windows))]
mod shutdown {
    use super::*;
    pub fn hook(_window: &Window, _state: std::rc::Weak<RefCell<Runtime>>) {}
}

#[cfg(target_os = "macos")]
thread_local! {static MAC_OPEN_TX:RefCell<Option<app::Sender<Message>>>=RefCell::new(None);}
#[cfg(target_os = "macos")]
fn mac_open_file(path: *const std::ffi::c_char) {
    if !path.is_null() {
        let name = unsafe { std::ffi::CStr::from_ptr(path) }
            .to_string_lossy()
            .into_owned();
        MAC_OPEN_TX.with(|s| {
            if let Some(tx) = *s.borrow() {
                tx.send((0, Action::OpenExternal(vec![PathBuf::from(name)])));
            }
        });
    }
}

fn main() {
    app::keyboard_screen_scaling(false);
    let _app = app::App::default().with_scheme(app::Scheme::Base);
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let config = settings_path();
    preferences::load(&config);
    if let Err(e) = shortcuts::load(&config) {
        dialog::alert_default(&format!("Tastenkürzel konnten nicht geladen werden: {e}"));
    }
    #[cfg(target_os = "linux")]
    if let Err(e) = desktop::integrate() {
        dialog::alert_default(&format!("Desktop-Integration: {e}"));
    }
    theme::init();
    let vertical = preferences::get().vertical;
    let (tx, rx) = app::channel::<Message>();
    let input_files = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .map(|p| p.canonicalize().unwrap_or(p))
        .collect::<Vec<_>>();
    let session_directory = config.parent().unwrap().join("session");
    let store = match session::Store::new(session_directory.clone(), tx) {
        Ok(store) => store,
        Err(error) => {
            if ipc::forward(&session_directory, input_files).is_err() {
                dialog::alert_default(&error);
            }
            return;
        }
    };
    if let Err(e) = ipc::listen(&session_directory, tx) {
        dialog::alert_default(&format!(
            "Datei-Übergabe konnte nicht gestartet werden: {e}"
        ));
    }
    #[cfg(target_os = "macos")]
    {
        MAC_OPEN_TX.with(|s| *s.borrow_mut() = Some(tx));
        app::raw_open_callback(Some(mac_open_file));
    }
    let mut cache_enabled = true;
    let saved = match store.load() {
        Ok(saved) => saved,
        Err(e) => {
            dialog::alert_default(&e);
            cache_enabled = false;
            session::Snapshot::default()
        }
    };
    let mut windows = Vec::new();
    for state in &saved.windows {
        if state.tabs.is_empty() {
            continue;
        }
        let id = windows.len();
        let mut window = EditorWindow::build(id, tx, vertical, false);
        if !window.restore(state, &store, id, tx) {
            cache_enabled = false;
        }
        windows.push(Some(window));
    }
    if windows.is_empty() {
        windows.push(Some(EditorWindow::new(0, tx, vertical)));
    }
    let sharing = match peer::Service::new(config.parent().unwrap().join("sharing"), tx) {
        Ok(s) => Some(s),
        Err(e) => {
            dialog::alert_default(&format!("Senden/Sync ist nicht verfügbar: {e}"));
            None
        }
    };
    let state = Rc::new(RefCell::new(Runtime {
        windows,
        splits: vec![],
        closed: vec![],
        store,
        tx,
        config,
        vertical,
        cache_enabled,
        session_pending: false,
        last_fingerprint: String::new(),
        weak_self: std::rc::Weak::new(),
        updates: chrome::UpdatePanel::new(tx),
        update_busy: false,
        update_release: None,
        sharing,
        clients: sharing_ui::Clients::new(tx),
        keyboard: shortcuts::Panel::new(tx),
        transfer: sharing_ui::Transfer::new(tx),
        conflicts: sharing_ui::Conflicts::new(tx),
        share_timers: Default::default(),
        countdown_timer: None,
        conflict_notified: Default::default(),
        tray: None,
        quit_requested: false,
    }));
    {
        let mut runtime = state.borrow_mut();
        runtime.weak_self = Rc::downgrade(&state);
        runtime.restore_splits();
        for w in runtime.windows.iter_mut().flatten().filter(|w| !w.embedded) {
            w.win.show();
            chrome::dark_title(&w.win);
        }
        for w in runtime.windows.iter().flatten().filter(|w| !w.embedded) {
            shutdown::hook(&w.win, Rc::downgrade(&state));
        }
        if !input_files.is_empty() {
            runtime.handle(0, Action::OpenExternal(input_files));
        }
        runtime.update();
        runtime.persist();
    }
    if let Some(error) =
        updater::previous_error(&state.borrow().config.parent().unwrap().join("updates"))
    {
        tx.send((0, Action::UpdateNotice(error)));
    } else if preferences::get().update_check {
        app::add_timeout3(1.2, move |_| tx.send((0, Action::CheckUpdates(false))));
    }
    app::add_timeout3(0.1, move |_| tx.send((0, Action::InitTray)));
    loop {
        if state.borrow().quit_requested || app::should_program_quit() {
            break;
        }
        let background = preferences::get().keep_in_tray && state.borrow().tray.is_some();
        if !background
            && !state
                .borrow()
                .windows
                .iter()
                .flatten()
                .any(|w| !w.embedded && w.win.shown())
        {
            break;
        }
        let _ = app::wait_for(3600.0);
        while let Some((wid, action)) = rx.recv() {
            state.borrow_mut().handle(wid, action);
        }
        state.borrow_mut().update();
    }
    let mut runtime = state.borrow_mut();
    runtime.persist();
    if let Err(error) = runtime.store.flush() {
        dialog::alert_default(&format!("Sitzungssicherung fehlgeschlagen: {error}"));
    }
}
