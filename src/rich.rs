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
    text::{StyleTableEntry, TextBuffer, TextEditor, WrapMode},
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
    #[serde(default = "full_size")]
    scale_percent: u16,
    #[serde(default)]
    display_width: Option<i32>,
}
fn full_size() -> u16 {
    100
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
    zoom: i32,
    selected_image: Option<Rc<String>>,
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
        scroll.set_scrollbar_size(14);
        scroll.super_draw_first(true);
        scroll.draw(|w| crate::theme::draw_scrollbars(w));
        let scroll_grab = Cell::new(None);
        scroll.handle(move |w, e| crate::theme::handle_scrollbars(w, e, &scroll_grab));
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
            zoom: 100,
            selected_image: None,
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
                        if s.selected_image.as_ref().is_some_and(|selected| {
                            !s.assets.iter().any(|a| Rc::ptr_eq(selected, &a.png))
                        }) {
                            s.selected_image = None;
                        }
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
            s.selected_image = None;
            s.buffer.set_text(&document.text);
            s.guard.set(false);
            drop(s);
            rebuild(&self.state);
        } else {
            let mut s = self.state.borrow_mut();
            s.guard.set(true);
            s.assets.clear();
            s.selected_image = None;
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
        insert_image(&self.state, png, image.data_w());
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
    pub fn zoom(&self) -> i32 {
        self.state.borrow().zoom
    }
    pub fn set_zoom(&mut self, percent: i32) {
        set_zoom(&self.state, percent);
    }
    pub fn configure(&mut self) {
        let mut s = self.state.borrow_mut();
        let prefs = crate::preferences::get();
        let size = scaled_size(prefs.font_size, s.zoom);
        for part in &mut s.parts {
            if let Part::Text(t) = part {
                t.editor.set_text_size(size);
                t.editor.wrap_mode(
                    if prefs.wrap {
                        WrapMode::AtBounds
                    } else {
                        WrapMode::None
                    },
                    0,
                );
            }
        }
        apply_styles(&mut s);
        s.geometry = None;
        layout(&mut s);
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
        s.geometry = None;
    }
    pub fn unset_highlight_data(&mut self, _style: Option<TextBuffer>) {
        let mut s = self.state.borrow_mut();
        s.style = None;
        s.table.clear();
        s.geometry = None;
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

fn insert_image(state: &Rc<RefCell<State>>, png: String, original_width: i32) {
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
    let display_width = if crate::preferences::get().image_auto_size {
        original_width.min((s.viewport.w() - 68).max(48) / 2)
    } else {
        original_width
    };
    s.assets.push(Asset {
        position,
        png: Rc::new(png),
        scale_percent: 100,
        display_width: Some(display_width.max(16)),
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
    // Keep the inserted image in view instead of scrolling to the text after it.
    let mut s = state.borrow_mut();
    let asset_index = s
        .assets
        .iter()
        .position(|a| a.position == position)
        .unwrap_or(0);
    let image_y = s
        .parts
        .iter()
        .filter_map(|part| {
            if let Part::Image(frame, _, _) = part {
                Some(frame.y())
            } else {
                None
            }
        })
        .nth(asset_index);
    if let Some(y) = image_y {
        let offset = (s.viewport.yposition() + y - s.viewport.y() - 8)
            .clamp(0, (s.content.h() - s.viewport.h()).max(0));
        s.viewport.scroll_to(0, offset);
        s.viewport.redraw();
    }
}

fn queue_clipboard(state: Weak<RefCell<State>>, image: RgbImage) {
    let original_width = image.data_w();
    match encode(&image) {
        Ok(png) => {
            let mut pending = Some(png);
            app::awake_callback(move || {
                if let (Some(state), Some(png)) = (state.upgrade(), pending.take()) {
                    insert_image(&state, png, original_width);
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
        let prefs = crate::preferences::get();
        editor.set_text_size(scaled_size(prefs.font_size, s.zoom));
        editor.wrap_mode(
            if prefs.wrap {
                WrapMode::AtBounds
            } else {
                WrapMode::None
            },
            0,
        );
        editor.set_text_color(FG);
        editor.set_color(BG);
        editor.set_cursor_color(FG);
        editor.show_cursor(false);
        editor.set_selection_color(ACCENT);
        editor.set_frame(FrameType::FlatBox);
        editor.set_linenumber_width(44);
        editor.set_linenumber_size(11);
        editor.set_linenumber_fgcolor(MUTED);
        editor.set_linenumber_bgcolor(BG);
        editor.set_scrollbar_size(14);
        editor.super_draw_first(true);
        editor.draw(|w| crate::theme::draw_scrollbars(w));
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
        let scroll_grab = Cell::new(None);
        editor.handle(move |editor, event| {
            if event == Event::Unfocus || event == Event::Hide {
                crate::caret::stop(editor);
            }
            if matches!(event, Event::Focus | Event::Push | Event::KeyDown) {
                crate::caret::reset(editor);
            }
            if event == Event::MouseWheel
                && app::event_state().intersects(Shortcut::Ctrl | Shortcut::Meta)
            {
                if let Some(state) = weak.upgrade() {
                    let delta = match app::event_dy() {
                        app::MouseWheel::Up => 10,
                        app::MouseWheel::Down => -10,
                        _ => 0,
                    };
                    let zoom = state.borrow().zoom;
                    set_zoom(&state, zoom + delta);
                }
                return true;
            }
            if crate::theme::handle_scrollbars(editor, event, &scroll_grab) {
                return true;
            }
            if event == Event::Push && app::event_mouse_button() == app::MouseButton::Left {
                if let Some(state) = weak.upgrade() {
                    if let Ok(mut s) = state.try_borrow_mut() {
                        if s.selected_image.take().is_some() {
                            s.viewport.redraw();
                        }
                    }
                }
            }
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
                frame.set_tooltip(
                    "Bild auswählen und an einer Ecke ziehen · Rechtsklick: Bildaktionen",
                );
                let selected_state = Rc::downgrade(state);
                let selected_data = asset.png.clone();
                frame.super_draw(false);
                frame.draw(move |f| {
                    draw::push_clip(f.x(), f.y(), f.w(), f.h());
                    draw::set_draw_color(BG);
                    draw::draw_rectf(f.x(), f.y(), f.w(), f.h());
                    let mut image = drawn.borrow_mut();
                    image.scale((f.w() - 54).max(1), (f.h() - 16).max(1), true, true);
                    let (w, h) = (image.w(), image.h());
                    image.draw(f.x() + 44, f.y() + 8, w, h);
                    let selected = selected_state.upgrade().is_some_and(|state| {
                        state.try_borrow().is_ok_and(|s| {
                            s.selected_image
                                .as_ref()
                                .is_some_and(|a| Rc::ptr_eq(a, &selected_data))
                        })
                    });
                    if selected {
                        draw::set_draw_color(crate::theme::BLUE);
                        draw::draw_rect(f.x() + 44, f.y() + 8, w, h);
                        for (x, y) in [
                            (f.x() + 44, f.y() + 8),
                            (f.x() + 44 + w, f.y() + 8),
                            (f.x() + 44, f.y() + 8 + h),
                            (f.x() + 44 + w, f.y() + 8 + h),
                        ] {
                            draw::set_draw_color(crate::theme::TEXT);
                            draw::draw_rectf(x - 4, y - 4, 8, 8);
                            draw::set_draw_color(crate::theme::BLUE);
                            draw::draw_rect(x - 4, y - 4, 8, 8);
                        }
                    }
                    draw::pop_clip();
                });
                let weak = Rc::downgrade(state);
                let asset_data = asset.png.clone();
                let dragging = Cell::new(None::<(i32, i32, i32, i32, i32)>);
                let drag_changed = Cell::new(false);
                frame.handle(move |frame, event| {
                    let (actual_w, actual_h) = {
                        let image = image.borrow();
                        (image.w(), image.h())
                    };
                    let corners = [
                        (frame.x() + 44, frame.y() + 8, -1, -1),
                        (frame.x() + 44 + actual_w, frame.y() + 8, 1, -1),
                        (frame.x() + 44, frame.y() + 8 + actual_h, -1, 1),
                        (frame.x() + 44 + actual_w, frame.y() + 8 + actual_h, 1, 1),
                    ];
                    let corner = corners.iter().find(|(x, y, _, _)| {
                        (app::event_x() - x).abs() <= 9 && (app::event_y() - y).abs() <= 9
                    });
                    match event {
                        Event::Move | Event::Enter => {
                            if let Some(mut win) = frame.window() {
                                win.set_cursor(match corner {
                                    Some((_, _, sx, sy)) if sx == sy => fltk::enums::Cursor::NWSE,
                                    Some(_) => fltk::enums::Cursor::NESW,
                                    None => fltk::enums::Cursor::Default,
                                });
                            }
                            true
                        }
                        Event::Leave => {
                            if dragging.get().is_none() {
                                if let Some(mut win) = frame.window() {
                                    win.set_cursor(fltk::enums::Cursor::Default);
                                }
                            }
                            true
                        }
                        Event::Push => {
                            if let Some(state) = weak.upgrade() {
                                let mut s = state.borrow_mut();
                                s.selected_image = Some(asset_data.clone());
                                s.target.set(s.right);
                                let after = s
                                    .assets
                                    .iter()
                                    .find(|a| Rc::ptr_eq(&a.png, &asset_data))
                                    .map(|a| a.position + IMAGE_MARKER.len_utf8());
                                let next = after.and_then(|position| {
                                    s.parts.iter().position(
                                        |part| matches!(part,Part::Text(t) if t.start==position),
                                    )
                                });
                                let mut focus = None;
                                if let Some(index) = next {
                                    s.current = index;
                                    if let Part::Text(t) = &mut s.parts[index] {
                                        t.editor.set_insert_position(0);
                                        focus = Some(t.editor.clone());
                                    }
                                }
                                let mut viewport = s.viewport.clone();
                                let offset = viewport.yposition();
                                let mut buffer = s.buffer.clone();
                                drop(s);
                                if let Some(mut editor) = focus {
                                    editor.take_focus().ok();
                                }
                                viewport.scroll_to(0, offset);
                                buffer.unselect();
                                state.borrow_mut().viewport.redraw();
                                if app::event_mouse_button() == app::MouseButton::Left {
                                    if let Some((_, _, sx, sy)) = corner {
                                        drag_changed.set(false);
                                        dragging.set(Some((
                                            app::event_x(),
                                            app::event_y(),
                                            actual_w,
                                            *sx,
                                            *sy,
                                        )));
                                    }
                                    return true;
                                }
                                if app::event_mouse_button() == app::MouseButton::Right {
                                    let weak = weak.clone();
                                    let asset_data = asset_data.clone();
                                    app::awake_callback(move || {
                                        if let Some(state) = weak.upgrade() {
                                            match dialog::choice2_default(
                                                "Bild bearbeiten",
                                                "Abbrechen",
                                                "An Textfeld anpassen",
                                                "Entfernen",
                                            ) {
                                                Some(1) => {
                                                    let width =
                                                        (state.borrow().viewport.w() - 68).max(24);
                                                    resize_image(&state, &asset_data, width, w, h);
                                                    mark_image_changed(&state, &asset_data);
                                                }
                                                Some(2) => {
                                                    if dialog::choice2_default(
                                                        "Bild entfernen?",
                                                        "Abbrechen",
                                                        "Entfernen",
                                                        "",
                                                    ) == Some(1)
                                                    {
                                                        let item = {
                                                            let s = state.borrow();
                                                            s.assets
                                                                .iter()
                                                                .find(|a| {
                                                                    Rc::ptr_eq(&a.png, &asset_data)
                                                                })
                                                                .map(|a| {
                                                                    (s.buffer.clone(), a.position)
                                                                })
                                                        };
                                                        if let Some((mut buffer, pos)) = item {
                                                            buffer.remove(
                                                                pos as i32,
                                                                (pos + IMAGE_MARKER.len_utf8())
                                                                    as i32,
                                                            );
                                                        }
                                                    }
                                                }
                                                _ => (),
                                            }
                                        }
                                    });
                                    return true;
                                }
                            }
                            false
                        }
                        Event::Drag => {
                            if let (Some((x, y, width, sx, sy)), Some(state)) =
                                (dragging.get(), weak.upgrade())
                            {
                                let dx = (app::event_x() - x) * sx;
                                let dy = ((app::event_y() - y) as f64 * sy as f64 * w as f64
                                    / h.max(1) as f64)
                                    as i32;
                                let delta = if dx.abs() >= dy.abs() { dx } else { dy };
                                resize_image(&state, &asset_data, width + delta, w, h);
                                drag_changed.set(true);
                                return true;
                            }
                            false
                        }
                        Event::Released => {
                            if dragging.take().is_some() {
                                if let Some(state) = weak.upgrade().filter(|_| drag_changed.get()) {
                                    mark_image_changed(&state, &asset_data);
                                }
                                return true;
                            }
                            false
                        }
                        _ => false,
                    }
                });
                let display_width = asset
                    .display_width
                    .unwrap_or(w * asset.scale_percent.clamp(10, 100) as i32 / 100)
                    .clamp(16, 100_000);
                s.parts.push(Part::Image(
                    frame,
                    display_width,
                    ((display_width as f64 * h as f64 / w.max(1) as f64).round() as i32).max(1),
                ));
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
    let mut table = s.table.clone();
    for entry in &mut table {
        entry.size = scaled_size(entry.size, s.zoom);
    }
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
    let width = (rect.2 - if mixed { 14 } else { 0 }).max(1);
    for part in &mut s.parts {
        if let Part::Text(t) = part {
            t.editor.resize(rect.0, t.editor.y(), width, t.editor.h());
        }
    }
    let heights = s
        .parts
        .iter()
        .map(|part| match part {
            Part::Text(t) => {
                if mixed {
                    // Use the same native font metrics as Fl_Text_Display.
                    // Guessing font_size + 5 accumulated a gap on macOS Menlo.
                    draw::set_font(t.editor.text_font(), t.editor.text_size());
                    let mut line_height = draw::height();
                    for style in &s.table {
                        draw::set_font(style.font, scaled_size(style.size, s.zoom));
                        line_height = line_height.max(draw::height());
                    }
                    let horizontal = if !crate::preferences::get().wrap {
                        draw::set_font(t.editor.text_font(), t.editor.text_size());
                        t.buffer
                            .text()
                            .lines()
                            .any(|line| draw::width(line) > (width - 50).max(1) as f64)
                    } else {
                        false
                    };
                    ((t.editor.count_lines(0, t.buffer.length(), true) + 1) * line_height
                        + 2
                        + if horizontal { 14 } else { 0 })
                    .max(line_height + 2)
                } else {
                    rect.3.max(1)
                }
            }
            Part::Image(_, w, h) => {
                let image_width = (*w).min((width - 54).max(1));
                (image_width as f64 * (*h as f64) / (*w as f64)).round() as i32 + 16
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

fn resize_image(
    state: &Rc<RefCell<State>>,
    data: &Rc<String>,
    width: i32,
    original_w: i32,
    original_h: i32,
) {
    let mut s = state.borrow_mut();
    let width = width.clamp(24, (s.viewport.w() - 68).max(24));
    let Some(index) = s.assets.iter().position(|a| Rc::ptr_eq(&a.png, data)) else {
        return;
    };
    s.assets[index].display_width = Some(width);
    let mut image_index = 0;
    for part in &mut s.parts {
        if let Part::Image(_, w, h) = part {
            if image_index == index {
                *w = width;
                *h = ((width as f64 * original_h as f64 / original_w.max(1) as f64).round() as i32)
                    .max(1);
                break;
            }
            image_index += 1;
        }
    }
    s.geometry = None;
    layout(&mut s);
}
fn mark_image_changed(state: &Rc<RefCell<State>>, data: &Rc<String>) {
    let s = state.borrow();
    let Some(position) = s
        .assets
        .iter()
        .find(|a| Rc::ptr_eq(&a.png, data))
        .map(|a| a.position)
    else {
        return;
    };
    s.guard.set(true);
    let mut buffer = s.buffer.clone();
    buffer.replace(
        position as i32,
        (position + IMAGE_MARKER.len_utf8()) as i32,
        &IMAGE_MARKER.to_string(),
    );
    s.guard.set(false);
}

fn scaled_size(base: i32, zoom: i32) -> i32 {
    ((base * zoom + 50) / 100).clamp(6, 112)
}
fn set_zoom(state: &Rc<RefCell<State>>, percent: i32) {
    let mut s = state.borrow_mut();
    let zoom = percent.clamp(50, 300);
    if s.zoom == zoom {
        return;
    }
    s.zoom = zoom;
    let size = scaled_size(crate::preferences::get().font_size, zoom);
    for part in &mut s.parts {
        if let Part::Text(t) = part {
            t.editor.set_text_size(size);
        }
    }
    apply_styles(&mut s);
    s.geometry = None;
    layout(&mut s);
}
