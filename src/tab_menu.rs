//! A themed popup with grouped actions and keyboard navigation, independent of native menu styling.
use crate::*;
use fltk::{draw, enums::Key};
#[derive(Clone)]
enum Item {
    Action(&'static str, &'static str, Action),
    Section(&'static str),
    Page(&'static str, u8),
}
fn items(page: u8, settings: bool, index: usize) -> Vec<Item> {
    use Item::*;
    if settings {
        return vec![
            Section("TAB"),
            Action("Tab schliessen", "Strg/Cmd+W", crate::Action::CloseTab),
            Action("Andere Tabs schliessen", "", crate::Action::CloseOtherTabs),
            Action("Alle Tabs schliessen", "", crate::Action::CloseAllTabs),
        ];
    }
    match page {
        1 => vec![
            Page("Zurück", 0),
            Section("SPRACHE"),
            Action("Automatisch erkennen", "", crate::Action::SetLanguage(None)),
        ]
        .into_iter()
        .chain(
            [
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
            ]
            .into_iter()
            .map(|l| Action(l.name(), "", crate::Action::SetLanguage(Some(l)))),
        )
        .collect(),
        2 => vec![
            Page("Zurück", 0),
            Section("SPLIT VIEW"),
            Action("Nebeneinander", "", crate::Action::Split(false)),
            Action("Übereinander", "", crate::Action::Split(true)),
            Action("Split zusammenführen", "", crate::Action::MergeSplit),
            Action(
                "In andere Ansicht verschieben",
                "",
                crate::Action::MoveTab(index),
            ),
            Action("Als zwei Fenster weiterführen", "", crate::Action::EndSplit),
        ],
        _ => vec![
            Section("DATEI"),
            Action("Speichern", "Strg/Cmd+S", crate::Action::Save(false)),
            Action("Speichern unter…", "", crate::Action::Save(true)),
            Action("Dateipfad kopieren", "", crate::Action::CopyPath),
            Page("Sprache", 1),
            Section("SENDEN UND SYNC"),
            Action("Senden an Client…", "", crate::Action::Transfer(false)),
            Action("Synchronisieren…", "", crate::Action::Transfer(true)),
            Action("Versionen / Konflikte…", "", crate::Action::ShowConflicts),
            Page("Split View", 2),
            Section("TABS"),
            Action("Tab schliessen", "Strg/Cmd+W", crate::Action::CloseTab),
            Action("Andere Tabs schliessen", "", crate::Action::CloseOtherTabs),
            Action("Alle Tabs schliessen", "", crate::Action::CloseAllTabs),
        ],
    }
}
pub fn popup(owner: &Window, index: usize, settings: bool, x: i32, y: i32) -> Option<Action> {
    let mut page = 0;
    loop {
        let entries = items(page, settings, index);
        Group::set_current(None::<&Group>);
        let width = 348;
        let height = 16
            + entries
                .iter()
                .map(|i| {
                    if matches!(i, Item::Section(_)) {
                        24
                    } else {
                        32
                    }
                })
                .sum::<i32>();
        let mut win = Window::default().with_size(width, height);
        win.set_border(false);
        win.set_color(PANEL);
        let mut border = Frame::new(0, 0, width, height, None);
        border.set_color(PANEL);
        border.draw(|f| {
            draw::set_draw_color(PANEL);
            draw::draw_rounded_rectf(f.x(), f.y(), f.w(), f.h(), 12);
            draw::set_draw_color(theme::BORDER);
            draw::draw_rounded_rect(f.x(), f.y(), f.w() - 1, f.h() - 1, 12);
        });
        let selected = Rc::new(Cell::new(None::<usize>));
        let hover = Rc::new(Cell::new(None::<usize>));
        let mut row_y = 8;
        let mut buttons = vec![];
        for (i, item) in entries.iter().enumerate() {
            if let Item::Section(label) = item {
                let mut heading = Frame::new(14, row_y, width - 28, 24, Some(*label));
                heading.set_label_color(MUTED);
                heading.set_label_size(10);
                heading.set_align(Align::Left | Align::Inside);
                row_y += 24;
                continue;
            }
            let (label, hint, submenu) = match item {
                Item::Action(l, h, _) => (*l, *h, false),
                Item::Page(l, _) => (*l, if page == 0 { "›" } else { "‹" }, true),
                _ => unreachable!(),
            };
            let hint = if let Item::Action(_, _, action) = item {
                crate::shortcuts::action_hint(action).unwrap_or_else(|| hint.to_owned())
            } else {
                hint.to_owned()
            };
            let mut button = Button::new(8, row_y, width - 16, 32, None);
            button.set_frame(FrameType::NoBox);
            button.clear_visible_focus();
            let state = hover.clone();
            button.draw(move |b| {
                let active = state.get() == Some(i);
                draw::set_draw_color(PANEL);
                draw::draw_rectf(b.x(), b.y(), b.w(), b.h());
                if active {
                    draw::set_draw_color(theme::BLUE);
                    draw::draw_rounded_rectf(b.x(), b.y() + 1, b.w(), b.h() - 2, 7);
                }
                draw::set_font(Font::Helvetica, 13);
                draw::set_draw_color(if active { Color::White } else { FG });
                draw::draw_text2(
                    label,
                    b.x() + 12,
                    b.y(),
                    b.w() - 112,
                    b.h(),
                    Align::Left | Align::Inside,
                );
                draw::set_font(Font::Helvetica, if submenu { 16 } else { 10 });
                draw::set_draw_color(if active { Color::White } else { MUTED });
                draw::draw_text2(
                    &hint,
                    b.x() + b.w() - 108,
                    b.y(),
                    96,
                    b.h(),
                    Align::Right | Align::Inside,
                );
            });
            let state = hover.clone();
            let mut surface = win.clone();
            button.handle(move |_, e| {
                if matches!(e, Event::Enter | Event::Move) {
                    if state.replace(Some(i)) != Some(i) {
                        surface.redraw();
                    }
                    true
                } else {
                    false
                }
            });
            let value = selected.clone();
            let mut surface = win.clone();
            button.set_callback(move |_| {
                value.set(Some(i));
                surface.hide();
            });
            buttons.push(i);
            row_y += 32;
        }
        win.end();
        let choices = buttons.clone();
        let entries_copy = entries.clone();
        let state = hover.clone();
        let value = selected.clone();
        win.handle(move |w, e| {
            if e == Event::Push {
                let (x, y) = (app::event_x_root(), app::event_y_root());
                if x < w.x() || x >= w.x() + w.w() || y < w.y() || y >= w.y() + w.h() {
                    w.hide();
                    return true;
                }
            }
            if e == Event::KeyDown {
                match app::event_key() {
                    Key::Escape => {
                        w.hide();
                        return true;
                    }
                    Key::Left => {
                        if page != 0 {
                            value.set(
                                entries_copy
                                    .iter()
                                    .position(|i| matches!(i, Item::Page(_, 0))),
                            );
                            w.hide();
                        }
                        return true;
                    }
                    Key::Up | Key::Down => {
                        let current = state
                            .get()
                            .and_then(|i| choices.iter().position(|j| *j == i));
                        let next = if app::event_key() == Key::Up {
                            current
                                .map(|p| (p + choices.len() - 1) % choices.len())
                                .unwrap_or(choices.len() - 1)
                        } else {
                            current.map(|p| (p + 1) % choices.len()).unwrap_or(0)
                        };
                        state.set(Some(choices[next]));
                        w.redraw();
                        return true;
                    }
                    Key::Enter | Key::Right => {
                        if let Some(i) = state.get() {
                            if app::event_key() == Key::Enter
                                || matches!(entries_copy[i], Item::Page(..))
                            {
                                value.set(Some(i));
                                w.hide();
                            }
                        }
                        return true;
                    }
                    _ => {}
                }
            }
            false
        });
        let screen = owner.screen_num();
        let (sx, sy, sw, sh) = app::screen_work_area(screen);
        win.set_pos(
            x.clamp(sx, (sx + sw - width).max(sx)),
            y.clamp(sy, (sy + sh - height).max(sy)),
        );
        win.show();
        app::set_grab(Some(win.clone()));
        while win.shown() {
            app::wait();
        }
        app::set_grab(None::<Window>);
        let chosen = selected.get().and_then(|i| entries.get(i).cloned());
        Window::delete(win);
        match chosen {
            Some(Item::Action(_, _, action)) => return Some(action),
            Some(Item::Page(_, next)) => page = next,
            _ => return None,
        }
    }
}
