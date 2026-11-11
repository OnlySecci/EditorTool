//! Native mixed text/image flow. Plain documents retain a single native text editor.
use crate::{core::plain_text, ACCENT, BG, FG, MUTED};
use base64::{engine::general_purpose::STANDARD, Engine};
use fltk::{
    app, dialog, draw,
    enums::{ColorDepth, Event, Font, FrameType, Key, Shortcut},
    frame::Frame,
    group::{Group, Scroll, ScrollType},
    image::{RgbImage, SharedImage},
    prelude::*,
    text::{StyleTableEntry, TextBuffer, TextEditor},
};
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    path::Path,
    rc::{Rc, Weak},
};

pub const IMAGE_MARKER: char = '\u{fffc}';
const MAGIC: &str = "EditorTool.Document.v1";
#[derive(Clone, Serialize, Deserialize)]
struct Asset {
    position: usize,
    png: Rc<String>,
}
#[derive(Serialize, Deserialize)]
struct FileDocument {
    format: String,
    text: String,
    images: Vec<Asset>,
}
struct TextPart {
    editor: TextEditor,
    buffer: TextBuffer,
    start: usize,
    style: Option<TextBuffer>,
}
enum Part {
    Text(TextPart),
    Image(Frame, i32, i32),
}
struct State {
    viewport: Scroll,
    content: Group,
    buffer: TextBuffer,
    assets: Vec<Asset>,
    parts: Vec<Part>,
    current: usize,
    target: Rc<Cell<bool>>,
    right: bool,
    guard: Rc<Cell<bool>>,
    style: Option<TextBuffer>,
    table: Vec<StyleTableEntry>,
    geometry: Option<(i32, i32, i32, i32)>,
}
#[derive(Clone)]
pub struct RichEditor {
    scroll: Scroll,
    state: Rc<RefCell<State>>,
}
fltk::widget_extends!(RichEditor, Scroll, scroll);

fn encode(image: &RgbImage) -> Result<String, String> {
    let (w, h) = (image.data_w(), image.data_h());
    if w <= 0 || h <= 0 || w as u64 * h as u64 > 40_000_000 {
        return Err("Bild ist zu groß (maximal 40 Megapixel).".into());
    }
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, w as u32, h as u32);
        encoder.set_color(match image.depth() {
            ColorDepth::L8 => png::ColorType::Grayscale,
            ColorDepth::La8 => png::ColorType::GrayscaleAlpha,
            ColorDepth::Rgb8 => png::ColorType::Rgb,
            _ => png::ColorType::Rgba,
        });
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer
            .write_image_data(&image.to_rgb_data())
            .map_err(|e| e.to_string())?;
    }
    Ok(STANDARD.encode(bytes))
}
fn decode(data: &str) -> Result<RgbImage, String> {
    let bytes = STANDARD.decode(data).map_err(|e| e.to_string())?;
    fltk::image::PngImage::from_data(&bytes)
        .and_then(|i| i.to_rgb())
        .map_err(|e| e.to_string())
}

impl RichEditor {
    pub fn new(target: Rc<Cell<bool>>, right: bool) -> Self {
        let mut scroll = Scroll::default();
        scroll.set_frame(FrameType::FlatBox);
        scroll.set_color(BG);
        scroll.set_scrollbar_size(10);
        let mut content = Group::default();
        content.set_color(BG);
        content.set_frame(FrameType::FlatBox);
        content.set_clip_children(true);
        content.end();
        scroll.end();
        let buffer = TextBuffer::default();
        let guard = Rc::new(Cell::new(false));
        let state = Rc::new(RefCell::new(State {
            viewport: scroll.clone(),
            content,
            buffer: buffer.clone(),
            assets: vec![],
            parts: vec![],
            current: 0,
            target,
            right,
            guard: guard.clone(),
            style: None,
            table: vec![],
            geometry: None,
        }));
        let weak = Rc::downgrade(&state);
        let mut watched = buffer.clone();
        watched.add_modify_callback2(move |_, position, inserted, deleted, _, _| {
            if guard.get() {
                return;
            }
            if let Some(state) = weak.upgrade() {
                if let Ok(mut s) = state.try_borrow_mut() {
                    if inserted != 0 || deleted != 0 {
                        let start = position as usize;
                        let end = start + deleted as usize;
                        s.assets.retain_mut(|asset| {
                            if deleted > 0 && asset.position >= start && asset.position < end {
                                return false;
                            }
                            if asset.position >= end {
                                asset.position =
                                    (asset.position as i64 + inserted as i64 - deleted as i64)
                                        .max(0) as usize;
                            }
                            true
                        });
                        if !s.assets.is_empty() || s.parts.len() > 1 {
                            drop(s);
                            rebuild(&state);
                        }
                    } else if !s.assets.is_empty() {
                        sync_selection(&mut s);
                    }
                }
            }
        });
        rebuild(&state);
        Self { scroll, state }
    }
    pub fn buffer(&self) -> TextBuffer {
        self.state.borrow().buffer.clone()
    }
    pub fn has_images(&self) -> bool {
        !self.state.borrow().assets.is_empty()
    }
    pub fn image_count(&self) -> usize {
        self.state.borrow().assets.len()
    }
    pub fn document_text(&self) -> String {
        let s = self.state.borrow();
        serde_json::to_string(&FileDocument {
            format: MAGIC.into(),
            text: s.buffer.text(),
            images: s.assets.clone(),
        })
        .expect("Document serialization")
    }
    pub fn plain_text(&self) -> String {
        self.buffer().text().replace(IMAGE_MARKER, "")
    }
    pub fn cache_text(&self) -> String {
        let s = self.state.borrow();
        if s.assets.is_empty() {
            return s.buffer.text();
        }
        serde_json::to_string(&FileDocument {
            format: MAGIC.into(),
            text: s.buffer.text(),
            images: s.assets.clone(),
        })
        .expect("Document serialization")
    }
    pub fn set_cache_text(&mut self, text: &str) -> Result<(), String> {
        let parsed = if text.starts_with('{') {
            serde_json::from_str::<FileDocument>(text)
                .ok()
                .filter(|d| d.format == MAGIC)
        } else {
            None
        };
        if let Some(document) = parsed {
            let mut previous = None;
            for asset in &document.images {
                if !document.text.is_char_boundary(asset.position)
                    || !document.text[asset.position..].starts_with(IMAGE_MARKER)
                    || previous.is_some_and(|p| p >= asset.position)
                {
                    return Err("Ungültige Bildposition im Dokument".into());
                }
                decode(&asset.png)?;
                previous = Some(asset.position);
            }
            if document.text.matches(IMAGE_MARKER).count() != document.images.len() {
                return Err("Bildreferenzen und Bilddaten stimmen nicht überein".into());
            }
            let mut s = self.state.borrow_mut();
            s.guard.set(true);
            s.assets = document.images;
            s.buffer.set_text(&document.text);
            s.guard.set(false);
            drop(s);
            rebuild(&self.state);
        } else {
            let mut s = self.state.borrow_mut();
            s.guard.set(true);
            s.assets.clear();
            s.buffer.set_text(
                &plain_text(text.trim_start_matches('\u{feff}')).replace(IMAGE_MARKER, ""),
            );
            s.guard.set(false);
            drop(s);
            rebuild(&self.state);
        }
        Ok(())
    }
    pub fn load_image(&mut self, path: &Path) -> Result<(), String> {
        let image = SharedImage::load(path)
            .and_then(|image| image.to_rgb())
            .map_err(|e| e.to_string())?;
        let png = encode(&image)?;
        insert_image(&self.state, png);
        Ok(())
    }
    pub fn insert_position(&self) -> i32 {
        let s = self.state.borrow();
        match s.parts.get(s.current) {
            Some(Part::Text(t)) => t.start as i32 + t.editor.insert_position(),
            _ => 0,
        }
    }
    pub fn set_insert_position(&mut self, position: i32) {
        let mut s = self.state.borrow_mut();
        let pos = position.max(0) as usize;
        let mut found = 0;
        for (i, part) in s.parts.iter_mut().enumerate() {
            if let Part::Text(t) = part {
                if pos >= t.start {
                    let local = (pos - t.start).min(t.buffer.length() as usize);
                    t.editor.set_insert_position(local as i32);
                    found = i;
                }
            }
        }
        s.current = found;
    }
    pub fn take_focus(&mut self) -> Result<(), fltk::prelude::FltkError> {
        let mut s = self.state.borrow_mut();
        let current = s.current;
        if let Some(Part::Text(t)) = s.parts.get_mut(current) {
            t.editor.take_focus()
        } else {
            Ok(())
        }
    }
    pub fn show_insert_position(&mut self) {
        self.reflow();
        let mut s = self.state.borrow_mut();
        let current = s.current;
        if let Some(Part::Text(t)) = s.parts.get_mut(current) {
            t.editor.show_insert_position();
            let y = t.editor.y();
            if y < s.viewport.y() || y > s.viewport.y() + s.viewport.h() - 30 {
                let offset = s.viewport.yposition() + y - s.viewport.y();
                s.viewport.scroll_to(0, offset.max(0));
                s.viewport.redraw();
            }
        }
    }
    pub fn undo(&self) {
        let s = self.state.borrow();
        if let Some(Part::Text(t)) = s.parts.get(s.current) {
            let ed = t.editor.clone();
            drop(s);
            ed.undo();
        }
    }
    pub fn paste(&mut self) {
        let s = self.state.borrow();
        if let Some(Part::Text(t)) = s.parts.get(s.current) {
            let ed = t.editor.clone();
            drop(s);
            paste_into(&ed);
        }
    }
    pub fn reflow(&mut self) {
        let mut s = self.state.borrow_mut();
        layout(&mut s);
    }
    pub fn set_highlight_data(&mut self, style: TextBuffer, table: Vec<StyleTableEntry>) {
        let mut s = self.state.borrow_mut();
        s.style = Some(style);
        s.table = table;
        apply_styles(&mut s);
    }
    pub fn unset_highlight_data(&mut self, _style: Option<TextBuffer>) {
        let mut s = self.state.borrow_mut();
        s.style = None;
        for part in &mut s.parts {
            if let Part::Text(t) = part {
                t.editor.unset_highlight_data(None::<TextBuffer>);
                t.style = None;
            }
        }
    }
}

fn paste_into(editor: &TextEditor) {
    if app::clipboard_contains(app::ClipboardContent::Image) {
        app::paste_image(editor);
    } else {
        app::paste_text(editor);
    }
}

fn insert_image(state: &Rc<RefCell<State>>, png: String) {
    let position = {
        let s = state.borrow();
        match s.parts.get(s.current) {
            Some(Part::Text(t)) => t.start + t.editor.insert_position() as usize,
            _ => s.buffer.length() as usize,
        }
    };
    let mut s = state.borrow_mut();
    s.guard.set(true);
    for asset in &mut s.assets {
        if asset.position >= position {
            asset.position += IMAGE_MARKER.len_utf8();
        }
    }
    s.assets.push(Asset {
        position,
        png: Rc::new(png),
    });
    s.assets.sort_by_key(|a| a.position);
    s.buffer.insert(position as i32, &IMAGE_MARKER.to_string());
    s.guard.set(false);
    drop(s);
    rebuild(state);
    let mut editor = RichEditor {
        scroll: state.borrow().viewport.clone(),
        state: state.clone(),
    };
    editor.set_insert_position((position + IMAGE_MARKER.len_utf8()) as i32);
    editor.take_focus().ok();
    editor.show_insert_position();
}

fn queue_clipboard(state: Weak<RefCell<State>>, image: RgbImage) {
    match encode(&image) {
        Ok(png) => {
            let mut pending = Some(png);
            app::awake_callback(move || {
                if let (Some(state), Some(png)) = (state.upgrade(), pending.take()) {
                    insert_image(&state, png);
                }
            });
        }
        Err(e) => dialog::alert_default(&e),
    }
}

fn sync_selection(s: &mut State) {
    let selection = s.buffer.selection_position();
    s.guard.set(true);
    for part in &mut s.parts {
        if let Part::Text(t) = part {
            if let Some((a, b)) = selection {
                let start = (a as usize).max(t.start);
                let end = (b as usize).min(t.start + t.buffer.length() as usize);
                if end > start {
                    t.buffer
                        .select((start - t.start) as i32, (end - t.start) as i32);
                } else {
                    t.buffer.unselect();
                }
            } else {
                t.buffer.unselect();
            }
        }
    }
    s.guard.set(false);
}

fn rebuild(state: &Rc<RefCell<State>>) {
    let mut s = state.borrow_mut();
    let guard = s.guard.clone();
    guard.set(true);
    let position = match s.parts.get(s.current) {
        Some(Part::Text(t)) => t.start + t.editor.insert_position() as usize,
        _ => 0,
    };
    s.parts.clear();
    s.content.clear();
    s.content.begin();
    let canonical = s.buffer.clone();
    let assets = s.assets.clone();
    let mut start = 0;
    for asset in assets.iter().map(Some).chain(std::iter::once(None)) {
        let end = asset
            .map(|a| a.position)
            .unwrap_or(canonical.length() as usize);
        let local = if assets.is_empty() {
            canonical.clone()
        } else {
            let mut b = TextBuffer::default();
            b.set_text(
                &canonical
                    .text_range(start as i32, end as i32)
                    .unwrap_or_default(),
            );
            b
        };
        let mut editor = TextEditor::default();
        editor.set_buffer(local.clone());
        editor.set_text_font(Font::Courier);
        editor.set_text_size(16);
        editor.set_text_color(FG);
        editor.set_color(BG);
        editor.set_cursor_color(FG);
        editor.set_selection_color(ACCENT);
        editor.set_frame(FrameType::FlatBox);
        editor.set_linenumber_width(44);
        editor.set_linenumber_size(11);
        editor.set_linenumber_fgcolor(MUTED);
        editor.set_linenumber_bgcolor(BG);
        editor.set_scrollbar_size(10);
        let index = s.parts.len();
        let weak = Rc::downgrade(state);
        let target = s.target.clone();
        let right = s.right;
        let shared_guard = guard.clone();
        let mut watched = local.clone();
        if !assets.is_empty() {
            watched.add_modify_callback2(move |local, pos, inserted, deleted, _, _| {
                if shared_guard.get() {
                    return;
                }
                if let Some(state) = weak.upgrade() {
                    let mut s = state.borrow_mut();
                    if inserted == 0 && deleted == 0 {
                        if let Some((a, b)) = local.selection_position() {
                            let offset = match &s.parts[index] {
                                Part::Text(t) => t.start,
                                _ => 0,
                            };
                            s.guard.set(true);
                            s.buffer.select(offset as i32 + a, offset as i32 + b);
                            s.guard.set(false);
                        } else {
                            s.guard.set(true);
                            s.buffer.unselect();
                            s.guard.set(false);
                        }
                        return;
                    }
                    let offset = match &s.parts[index] {
                        Part::Text(t) => t.start,
                        _ => return,
                    };
                    let global = offset + pos as usize;
                    let added = local.text_range(pos, pos + inserted).unwrap_or_default();
                    s.guard.set(true);
                    s.buffer
                        .replace(global as i32, (global + deleted as usize) as i32, &added);
                    s.guard.set(false);
                    let delta = inserted as i64 - deleted as i64;
                    for asset in &mut s.assets {
                        if asset.position >= global {
                            asset.position = (asset.position as i64 + delta) as usize;
                        }
                    }
                    for (i, part) in s.parts.iter_mut().enumerate() {
                        if let Part::Text(t) = part {
                            if i > index {
                                t.start = (t.start as i64 + delta) as usize;
                            } else if i == index {
                                if let Some(style) = t.style.as_mut() {
                                    style.replace(
                                        pos,
                                        (pos + deleted).min(style.length()),
                                        &"A".repeat(inserted as usize),
                                    );
                                }
                            }
                        }
                    }
                    s.geometry = None;
                }
            });
        }
        let weak = Rc::downgrade(state);
        editor.handle(move |editor, event| {
            if event == Event::Focus {
                target.set(right);
                if let Some(state) = weak.upgrade() {
                    if let Ok(mut s) = state.try_borrow_mut() {
                        s.current = index;
                    }
                }
            }
            if event == Event::KeyDown
                && app::event_state().contains(Shortcut::Command)
                && app::event_key() == Key::from_char('v')
            {
                paste_into(editor);
                return true;
            }
            if event == Event::Paste {
                if let Some(image) = app::event_clipboard_image() {
                    queue_clipboard(weak.clone(), image);
                    return true;
                }
                let value = plain_text(&app::event_text()).replace(IMAGE_MARKER, "");
                if let Some(mut buffer) = editor.buffer() {
                    let (a, b) = buffer
                        .selection_position()
                        .unwrap_or((editor.insert_position(), editor.insert_position()));
                    buffer.replace(a, b, &value);
                    editor.set_insert_position(a + value.len() as i32);
                    editor.show_insert_position();
                }
                return true;
            }
            // Move naturally across image boundaries with the arrow keys.
            if event == Event::KeyDown
                && matches!(
                    app::event_key(),
                    Key::Up | Key::Down | Key::Left | Key::Right
                )
            {
                let backward = matches!(app::event_key(), Key::Up | Key::Left);
                let at_edge = if backward {
                    editor.insert_position() == 0
                } else {
                    editor.insert_position() == editor.buffer().map(|b| b.length()).unwrap_or(0)
                };
                if at_edge {
                    let weak = weak.clone();
                    app::awake_callback(move || {
                        if let Some(state) = weak.upgrade() {
                            let mut s = state.borrow_mut();
                            let next = if backward {
                                (0..index)
                                    .rev()
                                    .find(|&i| matches!(s.parts[i], Part::Text(_)))
                            } else {
                                (index + 1..s.parts.len())
                                    .find(|&i| matches!(s.parts[i], Part::Text(_)))
                            };
                            if let Some(next) = next {
                                s.current = next;
                                if let Part::Text(t) = &mut s.parts[next] {
                                    t.editor.set_insert_position(if backward {
                                        t.buffer.length()
                                    } else {
                                        0
                                    });
                                    t.editor.take_focus().ok();
                                }
                            }
                        }
                    });
                }
            }
            false
        });
        s.parts.push(Part::Text(TextPart {
            editor,
            buffer: local,
            start,
            style: None,
        }));
        if let Some(asset) = asset {
            if let Ok(image) = decode(&asset.png) {
                let (w, h) = (image.data_w(), image.data_h());
                let image = Rc::new(RefCell::new(image));
                let drawn = image.clone();
                let mut frame = Frame::default();
                frame.set_tooltip("Bild · Doppelklick zum Entfernen");
                frame.super_draw(false);
                frame.draw(move |f| {
                    draw::push_clip(f.x(), f.y(), f.w(), f.h());
                    draw::set_draw_color(BG);
                    draw::draw_rectf(f.x(), f.y(), f.w(), f.h());
                    let mut image = drawn.borrow_mut();
                    image.scale((f.w() - 54).max(1), (f.h() - 8).max(1), true, false);
                    let (w, h) = (image.w(), image.h());
                    image.draw(f.x() + 44, f.y() + 4, w, h);
                    draw::pop_clip();
                });
                let weak = Rc::downgrade(state);
                let asset_data = asset.png.clone();
                frame.handle(move |_, event| {
                    if event == Event::Push && app::event_clicks() {
                        let weak = weak.clone();
                        let asset_data = asset_data.clone();
                        app::awake_callback(move || {
                            if let Some(state) = weak.upgrade() {
                                if dialog::choice2_default(
                                    "Bild aus diesem Dokument entfernen?",
                                    "Abbrechen",
                                    "Entfernen",
                                    "",
                                ) == Some(1)
                                {
                                    let item = {
                                        let s = state.borrow();
                                        s.assets
                                            .iter()
                                            .find(|a| Rc::ptr_eq(&a.png, &asset_data))
                                            .map(|a| (s.buffer.clone(), a.position))
                                    };
                                    if let Some((mut buffer, position)) = item {
                                        buffer.remove(
                                            position as i32,
                                            (position + IMAGE_MARKER.len_utf8()) as i32,
                                        );
                                    }
                                }
                            }
                        });
                        true
                    } else {
                        false
                    }
                });
                s.parts.push(Part::Image(frame, w, h));
            }
            start = asset.position + IMAGE_MARKER.len_utf8();
        }
    }
    s.content.end();
    Group::set_current(None::<&Group>);
    let mut restored_current = 0;
    for (index, part) in s.parts.iter_mut().enumerate() {
        if let Part::Text(t) = part {
            if position >= t.start {
                t.editor.set_insert_position(
                    (position - t.start).min(t.buffer.length() as usize) as i32
                );
                restored_current = index;
            }
        }
    }
    s.current = restored_current;
    apply_styles(&mut s);
    s.geometry = None;
    layout(&mut s);
    guard.set(false);
}

fn apply_styles(s: &mut State) {
    let Some(styles) = s.style.clone() else {
        return;
    };
    let table = s.table.clone();
    for part in &mut s.parts {
        if let Part::Text(t) = part {
            if s.assets.is_empty() {
                t.editor.set_highlight_data(styles.clone(), table.clone());
                t.style = Some(styles.clone());
            } else {
                let mut buffer = TextBuffer::default();
                buffer.set_text(
                    &styles
                        .text_range(t.start as i32, t.start as i32 + t.buffer.length())
                        .unwrap_or_default(),
                );
                t.editor.set_highlight_data(buffer.clone(), table.clone());
                t.style = Some(buffer);
            }
        }
    }
}
fn layout(s: &mut State) {
    let rect = (
        s.viewport.x(),
        s.viewport.y(),
        s.viewport.w(),
        s.viewport.h(),
    );
    if s.geometry == Some(rect) {
        return;
    }
    s.geometry = Some(rect);
    let mixed = !s.assets.is_empty();
    let width = (rect.2 - if mixed { 10 } else { 0 }).max(1);
    let heights = s
        .parts
        .iter()
        .map(|part| match part {
            Part::Text(t) => {
                if mixed {
                    ((t.buffer.count_lines(0, t.buffer.length()) + 1) * 21 + 18).max(32)
                } else {
                    rect.3.max(1)
                }
            }
            Part::Image(_, w, h) => {
                let image_width = (*w).min((width - 54).max(1));
                (image_width as f64 * (*h as f64) / (*w as f64)).round() as i32 + 8
            }
        })
        .collect::<Vec<_>>();
    let total = heights.iter().sum::<i32>();
    let overflow = total > rect.3;
    s.viewport.set_type(if mixed && overflow {
        ScrollType::Vertical
    } else {
        ScrollType::None
    });
    let offset = s.viewport.yposition().clamp(0, (total - rect.3).max(0));
    if offset != s.viewport.yposition() {
        s.viewport.scroll_to(0, offset);
    }
    let x = rect.0;
    let y = rect.1 - offset;
    s.content.resize(x, y, width, total.max(1));
    let mut cursor = y;
    for (part, height) in s.parts.iter_mut().zip(heights) {
        match part {
            Part::Text(t) => t.editor.resize(x, cursor, width, height),
            Part::Image(frame, _, _) => frame.resize(x, cursor, width, height),
        }
        cursor += height;
    }
    s.content.init_sizes();
    s.viewport.redraw();
}
