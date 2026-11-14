//! Small vector-drawn controls. No textures, render loop or additional GUI runtime.
use crate::{Action, Message};
use fltk::{
    app,
    button::{Button, CheckButton},
    draw::{self, LineStyle},
    enums::{Align, Color, Event, Font, FrameType},
    frame::Frame,
    input::Input,
    prelude::*,
};
use std::{cell::Cell, rc::Rc};

pub const CANVAS: Color = Color::Background2;
pub const CHROME: Color = Color::Background;
pub const SURFACE: Color = Color::Free;
pub const BORDER: Color = Color::Inactive;
pub const BLUE: Color = Color::Selection;
pub const TEXT: Color = Color::Foreground;
pub const SECONDARY: Color = Color::Dark3;

pub fn apply_palette() {
    let p = crate::preferences::get();
    let palette = match p.design {
        1 => [0xffffff, 0xf2f3f5, 0xe4e6eb, 0xc8cbd2, 0x20232a, 0x5c6270],
        2 => [0x121719, 0x1c2427, 0x2b363a, 0x45565d, 0xe8f1f1, 0xa6b9bd],
        _ => [0x1c1d21, 0x222327, 0x303136, 0x484b54, 0xf0f0f3, 0xa6a9b4],
    };
    for (slot, rgb) in [CANVAS, CHROME, SURFACE, BORDER, TEXT, SECONDARY]
        .into_iter()
        .zip(palette)
    {
        app::set_color(slot, (rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8);
    }
    let (r, g, b) = match p.accent {
        1 => (158, 104, 238),
        2 => (36, 158, 118),
        _ => (10, 132, 255),
    };
    app::set_color(BLUE, r, g, b);
}

pub fn init() {
    app::background(34, 35, 39);
    app::background2(48, 49, 54);
    app::foreground(240, 240, 243);
    let (ui, bold, mono) = if cfg!(target_os = "windows") {
        ("Segoe UI", "Segoe UI Semibold", "Consolas")
    } else if cfg!(target_os = "macos") {
        ("Helvetica Neue", "Helvetica Neue Medium", "Menlo")
    } else {
        ("DejaVu Sans", "DejaVu Sans Bold", "DejaVu Sans Mono")
    };
    Font::set_font(Font::Helvetica, ui);
    Font::set_font(Font::HelveticaBold, bold);
    Font::set_font(Font::Courier, mono);
    app::set_font_size(14);
    app::set_selection_color(38, 90, 151);
    app::set_scrollbar_size(10);
    // Only our inputs use this frame. Native scrollbars and dialogs retain their own metrics.
    app::set_frame_type_cb(
        FrameType::FreeBoxType,
        |x, y, w, h, c| {
            draw::set_draw_color(CHROME);
            draw::draw_rectf(x, y, w, h);
            draw::set_draw_color(c);
            draw::draw_rounded_rectf(x, y, w, h, 8);
            draw::set_draw_color(BORDER);
            draw::draw_rounded_rect(x, y, w - 1, h - 1, 8);
        },
        10,
        5,
        20,
        10,
    );
    app::set_frame_type_cb(
        FrameType::RFlatBox,
        |x, y, w, h, c| {
            draw::set_draw_color(c);
            draw::draw_rounded_rectf(x, y, w, h, 12);
        },
        1,
        1,
        2,
        2,
    );
    fltk::misc::Tooltip::set_color(SURFACE);
    fltk::misc::Tooltip::set_text_color(TEXT);
    fltk::misc::Tooltip::set_font_size(12);
    apply_palette();
}

fn bars<W: WidgetExt>(widget: &W) -> Vec<fltk::valuator::Scrollbar> {
    widget
        .as_group()
        .map(|group| {
            (0..group.children())
                .filter_map(|i| {
                    group
                        .child(i)
                        .and_then(|child| fltk::valuator::Scrollbar::from_dyn_widget(&child))
                })
                .collect()
        })
        .unwrap_or_default()
}
fn thumb(bar: &fltk::valuator::Scrollbar) -> (bool, i32, i32, i32) {
    let vertical = bar.h() > bar.w();
    let length = if vertical { bar.h() } else { bar.w() };
    let size = ((length as f32 * bar.slider_size()) as i32)
        .max(24)
        .min(length);
    let range = bar.maximum() - bar.minimum();
    let ratio = if range > 0.0 {
        ((bar.value() - bar.minimum()) / range).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (
        vertical,
        length,
        size,
        ((length - size) as f64 * ratio) as i32,
    )
}
pub fn draw_scrollbars<W: WidgetExt>(widget: &W) {
    for bar in bars(widget).into_iter().filter(|b| b.visible()) {
        let (vertical, _, size, offset) = thumb(&bar);
        draw::push_clip(bar.x(), bar.y(), bar.w(), bar.h());
        draw::set_draw_color(CANVAS);
        draw::draw_rectf(bar.x(), bar.y(), bar.w(), bar.h());
        let hover = app::event_x() >= bar.x()
            && app::event_x() < bar.x() + bar.w()
            && app::event_y() >= bar.y()
            && app::event_y() < bar.y() + bar.h();
        draw::set_draw_color(if hover { BLUE } else { SECONDARY });
        if vertical {
            draw::draw_rounded_rectf(bar.x() + 3, bar.y() + offset, (bar.w() - 6).max(4), size, 4);
        } else {
            draw::draw_rounded_rectf(bar.x() + offset, bar.y() + 3, size, (bar.h() - 6).max(4), 4);
        }
        draw::pop_clip();
    }
}
pub fn handle_scrollbars<W: WidgetExt>(
    widget: &mut W,
    event: Event,
    grab: &Cell<Option<(usize, i32)>>,
) -> bool {
    if event == Event::Released && grab.get().is_some() {
        grab.set(None);
        return true;
    }
    if !matches!(event, Event::Push | Event::Drag) {
        return false;
    }
    let list = bars(widget);
    for (index, mut bar) in list.into_iter().enumerate().filter(|(_, b)| b.visible()) {
        let inside = app::event_x() >= bar.x()
            && app::event_x() < bar.x() + bar.w()
            && app::event_y() >= bar.y()
            && app::event_y() < bar.y() + bar.h();
        if (event == Event::Push && inside) || grab.get().is_some_and(|g| g.0 == index) {
            let (vertical, length, size, offset) = thumb(&bar);
            let point = if vertical {
                app::event_y() - bar.y()
            } else {
                app::event_x() - bar.x()
            };
            if event == Event::Push {
                grab.set(Some((
                    index,
                    if point >= offset && point <= offset + size {
                        point - offset
                    } else {
                        size / 2
                    },
                )));
            }
            if let Some((_, anchor)) = grab.get() {
                let ratio = (point - anchor).clamp(0, (length - size).max(0)) as f64
                    / (length - size).max(1) as f64;
                bar.set_value(bar.minimum() + ratio * (bar.maximum() - bar.minimum()));
                bar.do_callback();
                widget.redraw();
            }
            return true;
        }
    }
    false
}

fn hover<W: WidgetBase + WidgetExt>(widget: &mut W) -> Rc<Cell<bool>> {
    let hover = Rc::new(Cell::new(false));
    let state = hover.clone();
    widget.handle(move |w, ev| match ev {
        Event::Enter => {
            state.set(true);
            w.redraw();
            true
        }
        Event::Leave => {
            state.set(false);
            w.redraw();
            true
        }
        Event::Move => {
            w.redraw();
            false
        }
        Event::Focus | Event::Unfocus => {
            w.redraw();
            false
        }
        _ => false,
    });
    hover
}

#[derive(Clone, Copy)]
pub enum Icon {
    Search,
    Compare,
    Plus,
    Close,
    Document,
}

pub fn icon(kind: Icon, x: i32, y: i32, color: Color) {
    draw::set_draw_color(color);
    draw::set_line_style(LineStyle::Solid, 2);
    match kind {
        Icon::Search => {
            draw::draw_arc(x + 2, y + 2, 12, 12, 0., 360.);
            draw::draw_line(x + 12, y + 12, x + 18, y + 18);
        }
        Icon::Plus => {
            draw::draw_line(x + 10, y + 3, x + 10, y + 17);
            draw::draw_line(x + 3, y + 10, x + 17, y + 10);
        }
        Icon::Close => {
            draw::draw_line(x + 6, y + 6, x + 14, y + 14);
            draw::draw_line(x + 6, y + 14, x + 14, y + 6);
        }
        Icon::Compare => {
            draw::draw_rounded_rect(x + 1, y + 3, 18, 15, 3);
            draw::draw_line(x + 10, y + 3, x + 10, y + 18);
            draw::draw_line(x + 4, y + 8, x + 7, y + 8);
            draw::draw_line(x + 13, y + 12, x + 16, y + 12);
        }
        Icon::Document => {
            draw::draw_rounded_rect(x + 3, y + 1, 14, 18, 3);
            draw::draw_line(x + 6, y + 7, x + 13, y + 7);
            draw::draw_line(x + 6, y + 11, x + 13, y + 11);
            draw::draw_line(x + 6, y + 15, x + 10, y + 15);
        }
    }
    draw::set_line_style(LineStyle::Solid, 0);
}

pub fn button(b: &mut Button, primary: bool, glyph: Option<Icon>) {
    let label = b.label();
    b.set_label(crate::i18n::tr(&label));
    b.set_frame(FrameType::NoBox);
    b.set_label_size(13);
    b.clear_visible_focus();
    b.set_color(if primary { BLUE } else { SURFACE });
    b.set_label_color(TEXT);
    let hovered = hover(b);
    b.super_draw(false);
    b.draw(move |b| {
        draw::push_clip(b.x(), b.y(), b.w(), b.h());
        let parent = b.parent().map(|p| p.color()).unwrap_or(CHROME);
        draw::set_draw_color(parent);
        draw::draw_rectf(b.x(), b.y(), b.w(), b.h());
        let color = if b.value() {
            b.color().darker()
        } else if hovered.get() {
            b.color().lighter()
        } else {
            b.color()
        };
        draw::set_draw_color(color);
        draw::draw_rounded_rectf(b.x() + 1, b.y() + 2, b.w() - 2, b.h() - 4, 8);
        if b.has_focus() {
            draw::set_draw_color(BLUE);
            draw::draw_rounded_rect(b.x() + 1, b.y() + 2, b.w() - 3, b.h() - 5, 8);
        }
        if let Some(kind) = glyph {
            icon(
                kind,
                b.x() + (b.w() - 20) / 2,
                b.y() + (b.h() - 20) / 2,
                b.label_color(),
            );
        } else {
            draw::set_font(b.label_font(), b.label_size());
            draw::set_draw_color(b.label_color());
            draw::draw_text2(
                &b.label(),
                b.x() + 8,
                b.y(),
                b.w() - 16,
                b.h(),
                Align::Center | Align::Inside,
            );
        }
        draw::pop_clip();
    });
}

pub fn input(input: &mut Input) {
    input.set_frame(FrameType::FreeBoxType);
    input.set_color(SURFACE);
    input.set_text_color(TEXT);
    input.set_cursor_color(BLUE);
    input.set_text_size(14);
    input.set_selection_color(Color::from_hex(0x265a97));
}

pub fn check(check: &mut CheckButton) {
    let label = check.label();
    check.set_label(crate::i18n::tr(&label));
    check.set_frame(FrameType::NoBox);
    check.set_label_color(SECONDARY);
    check.set_label_size(13);
    check.super_draw(false);
    check.draw(|b| {
        draw::push_clip(b.x(), b.y(), b.w(), b.h());
        draw::set_draw_color(b.parent().map(|p| p.color()).unwrap_or(CHROME));
        draw::draw_rectf(b.x(), b.y(), b.w(), b.h());
        let y = b.y() + (b.h() - 16) / 2;
        draw::set_draw_color(if b.value() { BLUE } else { SURFACE });
        draw::draw_rounded_rectf(b.x() + 1, y, 16, 16, 5);
        if b.value() {
            draw::set_draw_color(TEXT);
            draw::set_line_style(LineStyle::Solid, 2);
            draw::draw_line(b.x() + 5, y + 8, b.x() + 8, y + 11);
            draw::draw_line(b.x() + 8, y + 11, b.x() + 13, y + 5);
            draw::set_line_style(LineStyle::Solid, 0);
        } else {
            draw::set_draw_color(BORDER);
            draw::draw_rounded_rect(b.x() + 1, y, 16, 16, 5);
        }
        draw::set_draw_color(SECONDARY);
        draw::set_font(Font::Helvetica, 13);
        draw::draw_text2(
            &b.label(),
            b.x() + 28,
            b.y(),
            b.w() - 28,
            b.h(),
            Align::Left | Align::Inside,
        );
        draw::pop_clip();
    });
}

pub fn heading(label: &str, size: i32) -> Frame {
    let mut f = Frame::default().with_label(crate::i18n::tr(label));
    f.set_label_font(Font::HelveticaBold);
    f.set_label_size(size);
    f.set_label_color(TEXT);
    f.set_align(Align::Left | Align::Inside);
    f
}

pub fn tab(
    title: &str,
    active: bool,
    wid: usize,
    index: usize,
    tx: app::Sender<Message>,
) -> Button {
    let mut b = Button::default().with_label(title);
    b.set_tooltip(title);
    b.set_frame(FrameType::NoBox);
    b.clear_visible_focus();
    let hovered = hover(&mut b);
    let tab_hover = hovered.clone();
    b.super_draw(false);
    b.draw(move |b| {
        draw::push_clip(b.x(), b.y(), b.w(), b.h());
        draw::set_draw_color(CHROME);
        draw::draw_rectf(b.x(), b.y(), b.w(), b.h());
        if active || hovered.get() {
            draw::set_draw_color(if active { SURFACE } else { SURFACE });
            draw::draw_rounded_rectf(b.x() + 2, b.y() + 4, b.w() - 4, b.h() - 8, 9);
            if active {
                draw::set_draw_color(BORDER);
                draw::draw_rounded_rect(b.x() + 2, b.y() + 4, b.w() - 5, b.h() - 9, 9);
            }
        }
        icon(
            Icon::Document,
            b.x() + 13,
            b.y() + (b.h() - 20) / 2,
            if active { BLUE } else { SECONDARY },
        );
        let dirty = b.label().ends_with(" *");
        let mut title = b.label().trim_end_matches(" *").to_owned();
        draw::set_font(Font::Helvetica, 13);
        let max = b.w() - 82;
        if draw::measure(&title, false).0 > max {
            while !title.is_empty() && draw::measure(&format!("{title}…"), false).0 > max {
                title.pop();
            }
            title.push('…');
        }
        draw::set_draw_color(if active { TEXT } else { SECONDARY });
        draw::draw_text2(
            &title,
            b.x() + 40,
            b.y(),
            max,
            b.h(),
            Align::Left | Align::Inside,
        );
        if dirty {
            draw::set_draw_color(SECONDARY);
            draw::draw_pie(b.x() + b.w() - 45, b.y() + b.h() / 2 - 2, 5, 5, 0., 360.);
        }
        icon(
            Icon::Close,
            b.x() + b.w() - 29,
            b.y() + (b.h() - 20) / 2,
            if hovered.get() { TEXT } else { SECONDARY },
        );
        draw::pop_clip();
    });
    let hovered = tab_hover;
    b.handle(move |b, event| match event {
        Event::Push if app::event_mouse_button() == app::MouseButton::Right => {
            tx.send((
                wid,
                Action::TabContext(index, app::event_x_root(), app::event_y_root()),
            ));
            true
        }
        Event::Released if app::event_mouse_button() == app::MouseButton::Right => true,
        Event::Enter => {
            hovered.set(true);
            b.redraw();
            true
        }
        Event::Leave => {
            hovered.set(false);
            b.redraw();
            true
        }
        _ => false,
    });
    b.set_callback(move |b| {
        tx.send((wid, Action::Select(index)));
        if app::event_x() >= b.x() + b.w() - 35 && app::event() == Event::Released {
            tx.send((wid, Action::CloseTab));
        }
    });
    b
}
