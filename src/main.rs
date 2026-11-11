#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod chrome;
mod core;
mod ipc;
mod preferences;
mod rich;
mod session;
mod split;
mod syntax;
mod theme;
mod updater;
use rich::RichEditor;
use syntax::Language;
type SyntaxKey = (u64, u64, Language, Language);

use crate::core::{compare, pattern, Comparison, Counts};
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
    ImageAutoSize(bool),
    TabContext(usize, i32, i32),
    CopyPath,
    CloseOtherTabs,
    Split(bool),
    EndSplit,
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
    Compared(usize, u64, Comparison),
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
    status_text: String,
    status_key: String,
    report: String,
    comparing: bool,
    path: Option<PathBuf>,
    saved_revision: u64,
    comparison_visible: bool,
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
        self.left_meta.borrow().revision != self.saved_revision
    }
    fn title(&self) -> String {
        format!(
            "{}{}",
            self.path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().into_owned())
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
    fn refresh(&mut self) {
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
        let key = format!(
            "{}:{cursor}:{:?}:{}:{}",
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
        let text = format!("Zeile {line}, Spalte {col}     ·     {} Zeilen     ·     {} Zeichen     ·     {} Bytes\n{selection}     ·     {}", c.breaks + 1, c.chars, c.bytes, format!("{} · {} Bilder · {}",self.language(self.target_right.get()).name(),if self.target_right.get(){self.right.image_count()}else{self.left.image_count()},self.report));
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
            (
                "&Ansicht/Einstellungen…",
                Shortcut::Command | ',',
                Action::Settings,
            ),
        ];
        menu.add_emit(
            "&Hilfe/Nach Updates suchen…",
            Shortcut::Command | Shortcut::Shift | 'u',
            MenuFlag::Normal,
            tx,
            (wid, Action::CheckUpdates(true)),
        );
        for (label, action) in [
            ("&Ansicht/Split View/Nebeneinander", Action::Split(false)),
            ("&Ansicht/Split View/Übereinander", Action::Split(true)),
            (
                "&Ansicht/Split View/Als zwei Fenster weiterführen",
                Action::EndSplit,
            ),
        ] {
            menu.add_emit(label, Shortcut::None, MenuFlag::Normal, tx, (wid, action));
        }
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
            (theme::Icon::Settings, "Einstellungen", Action::Settings),
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
        if show {
            result.win.show();
            chrome::dark_title(&result.win);
        }
        result
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
        let mut controls = Flex::default().row();
        controls.set_spacing(8);
        controls.set_color(BG);
        button(
            "Vergleichstext öffnen",
            168,
            &mut controls,
            Action::Open(true),
            wid,
            tx,
        );
        button("Vergleichen", 100, &mut controls, Action::Compare, wid, tx);
        Frame::default();
        controls.end();
        rightcol.fixed(&controls, 36);
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
            status_text: String::new(),
            status_key: String::new(),
            comparing: false,
            report: "UTF-8 · Nur Text".into(),
            path: None,
            saved_revision: 0,
            comparison_visible: false,
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
        if let Some(d) = self.docs.get_mut(self.active) {
            d.left.reflow();
            d.right.reflow();
            d.refresh();
            if d.compared.is_none() && !d.syntax_pending && d.syntax_key != Some(d.syntax_key()) {
                d.syntax_pending = true;
                let id = d.id;
                app::add_timeout3(0.25, move |_| tx.send((wid, Action::SyntaxRequest(id))));
            }
            let title = format!("{} — EditorTool", d.title());
            if self.route.get() == wid && self.win.label() != title {
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
        match action {
            Action::TabContext(index, x, y) => {
                if index >= self.docs.len() {
                    return;
                }
                self.action(Action::Select(index), wid, tx);
                Group::set_current(None::<&Group>);
                let mut popup =
                    fltk::menu::MenuButton::new(x - self.win.x(), y - self.win.y(), 1, 1, None);
                self.win.add(&popup);
                popup.set_text_color(FG);
                popup.set_color(PANEL);
                popup.set_text_size(13);
                for (label, action) in [
                    ("Speichern", Action::Save(false)),
                    ("Speichern unter…", Action::Save(true)),
                    ("Dateipfad kopieren", Action::CopyPath),
                    ("Tab schliessen", Action::CloseTab),
                    ("Andere Tabs schliessen", Action::CloseOtherTabs),
                    ("Alle Tabs schliessen", Action::CloseAllTabs),
                    (
                        "In andere Split-Ansicht verschieben",
                        Action::MoveTab(index),
                    ),
                    ("Split View/Nebeneinander", Action::Split(false)),
                    ("Split View/Übereinander", Action::Split(true)),
                    ("Split View/Als zwei Fenster weiterführen", Action::EndSplit),
                    ("Sprache/Automatisch", Action::SetLanguage(None)),
                ] {
                    popup.add_emit(label, Shortcut::None, MenuFlag::Normal, tx, (wid, action));
                }
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
                    popup.add_emit(
                        &format!("Sprache/{}", language.name().replace('/', " · ")),
                        Shortcut::None,
                        MenuFlag::Normal,
                        tx,
                        (wid, Action::SetLanguage(Some(language))),
                    );
                }
                popup.popup();
                self.win.remove(&popup);
                fltk::menu::MenuButton::delete(popup);
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
            Action::Settings => self.settings.show(&self.win),
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
                    if d.compared.is_some() {
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
                    if d.compared.is_some() || key != d.syntax_key() {
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
                self.docs[i].left.take_focus().ok();
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
            Action::Compare => {
                self.show_compare(true);
                let d = &mut self.docs[self.active];
                let id = d.id;
                if d.comparing {
                    return;
                }
                d.comparing = true;
                let revision = d
                    .left_meta
                    .borrow()
                    .revision
                    .wrapping_mul(1_000_000_007)
                    .wrapping_add(d.right_meta.borrow().revision);
                let left = d.left_buf.text();
                let right = d.right_buf.text();
                d.report = "Vergleich läuft …".into();
                std::thread::spawn(move || {
                    tx.send((wid, Action::Compared(id, revision, compare(&left, &right))));
                });
            }
            Action::Compared(id, revision, result) => {
                if let Some(d) = self.docs.iter_mut().find(|d| d.id == id) {
                    d.comparing = false;
                    let rev = (
                        d.left_meta.borrow().revision,
                        d.right_meta.borrow().revision,
                    );
                    if rev.0.wrapping_mul(1_000_000_007).wrapping_add(rev.1) != revision {
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
            d.rightcol.hide();
            d.target_right.set(false);
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
            d.path = tab.path.clone();
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
            let revision = d.left_meta.borrow().revision;
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
        self.layout_splits();
        for (id, w) in self.windows.iter_mut().enumerate() {
            if let Some(w) = w {
                w.refresh(id, self.tx);
            }
        }
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
                                if active.dirty()
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
    let app = app::App::default().with_scheme(app::Scheme::Base);
    let config = settings_path();
    preferences::load(&config);
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
    // Single-shot timers run only after changes. There is no periodic idle polling.
    while app.wait() {
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
